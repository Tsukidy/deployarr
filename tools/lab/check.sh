#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Checks the PXE lab and collects the evidence for issue #25 in one zip.
# See docs/lab-setup.md.
#
# It boots each lab VM to its network boot while the PXE prototype runs, and
# checks that:
# - the VM gets an address from the lab's dnsmasq
# - its PXE DISCOVER carries option 93 and option 97, with the VM's UUID
# - the PXE prototype answers it on port 4011
# - a UEFI VM refuses an unsigned boot program, so Secure Boot is on
#
# Usage: sudo tools/lab/check.sh
# Run "sudo tools/lab/network.sh up" first. LAB_CHECK_SECONDS sets how long
# each VM runs, 60 by default.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

if [[ $(id -u) -ne 0 ]]; then
    echo "Run this with sudo." >&2
    exit 1
fi
user=${SUDO_USER:-root}
HOME=$(getent passwd "$user" | cut -d: -f6)
# shellcheck source=lab.conf
source "$here/lab.conf"

die() {
    echo "$*" >&2
    exit 1
}

for tool in dumpcap tshark qemu-system-x86_64 swtpm python3; do
    command -v "$tool" >/dev/null || die "Missing $tool. Install the packages in docs/lab-setup.md."
done
for vm in "${VMS[@]}"; do
    ip link show "${VM_TAP[$vm]}" >/dev/null 2>&1 || die "Run sudo tools/lab/network.sh up first."
done
if ! [[ -s $RUN_DIR/dnsmasq.pid ]] || ! kill -0 "$(cat "$RUN_DIR/dnsmasq.pid")" 2>/dev/null; then
    die "dnsmasq is not running. Run sudo tools/lab/network.sh up first."
fi
[[ -f $OVMF_VARS_WIN23 ]] || die "Missing $OVMF_VARS_WIN23. docs/lab-setup.md says how to create it."
if pgrep -f 'qemu-system.*deployarr-lab-' >/dev/null; then
    die "A lab VM is running. Stop it first."
fi

seconds=${LAB_CHECK_SECONDS:-60}
out=$LAB_DIR/checks/lab-check-$(date -u +%Y%m%dT%H%M%SZ)
root=$(mktemp -d)
pids=()
cleanup() {
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    rm -rf "$root"
    # The evidence belongs to the user who ran sudo, also after a failed run.
    # The chown in Ubuntu 26.04, from uutils, leaves the group unchanged for
    # "user:", so the group is named.
    chown -R "$user:$(id -gn "$user")" "$LAB_DIR/checks" 2>/dev/null || true
}
trap cleanup EXIT
mkdir -p "$out"

# The two-stage chain sends wdsmgfw.efi to UEFI and wdsnbp.com to BIOS. Only
# the unsigned program is served, so a BIOS VM gets a TFTP error at that point.
mkdir -p "$root/boot/x64"
python3 -I "$here/unsigned_efi.py" "$root/boot/x64/wdsmgfw.efi"
chmod -R a+rX "$root"

echo "Writing the evidence to $out"
# dumpcap drops root's capabilities once it opens the interface, so it cannot
# open a file in a home folder that only its owner can enter. It writes the
# capture to stdout, and this script opens the file.
dumpcap -q -i "$BRIDGE" -f 'udp and udp[8:2] != 3' -w - >"$out/capture.pcapng" 2>"$out/dumpcap.log" &
pids+=($!)
python3 "$repo/tools/pxe-prototype/pxe_prototype.py" \
    --server-ip "$SERVER_IP" --interface "$BRIDGE" --root "$root" \
    --chain two-stage --no-block-log --log-file "$out/prototype.log" >"$out/prototype.out" 2>&1 &
pids+=($!)
sleep 2
for pid in "${pids[@]}"; do
    kill -0 "$pid" 2>/dev/null || die "dumpcap or the PXE prototype failed to start. See $out."
done

echo "Booting ${VMS[*]} for $seconds seconds"
vm_pids=()
for vm in "${VMS[@]}"; do
    rm -f "$LAB_DIR/$vm/serial.log"
    # The VM runs as the user, and root writes its log to the evidence folder.
    # shellcheck disable=SC2024
    sudo -u "$user" env LAB_DIR="$LAB_DIR" LAB_ALLOW_TCG="${LAB_ALLOW_TCG:-}" \
        timeout "$seconds" "$here/vm.sh" "$vm" -display none >"$out/qemu-$vm.log" 2>&1 &
    vm_pids+=($!)
done
wait "${vm_pids[@]}" || true
sleep 1
for pid in "${pids[@]}"; do
    kill -INT "$pid" 2>/dev/null || true
done
wait "${pids[@]}" 2>/dev/null || true
pids=()

for vm in "${VMS[@]}"; do
    cp "$LAB_DIR/$vm/serial.log" "$out/serial-$vm.log" 2>/dev/null || true
done
cp "$RUN_DIR/dnsmasq.log" "$out/dnsmasq.log"
# Ubuntu 26.04's AppArmor profile for tshark lets it open capture files only in
# temporary folders such as /tmp. tshark reads the capture from stdin, which
# this script opens.
read_capture() {
    tshark -r - "$@" <"$out/capture.pcapng" 2>>"$out/tshark.log" ||
        echo "tshark could not read the capture. See $out/tshark.log." >&2
}
read_capture -Y dhcp -V >"$out/dhcp-decode.txt"
read_capture -Y dhcp -T fields -E separator=/t -E occurrence=f \
    -e dhcp.hw.mac_addr -e dhcp.option.dhcp -e dhcp.option.client_system_architecture \
    -e dhcp.client_id.uuid -e dhcp.ip.your \
    >"$out/dhcp-fields.tsv"

{
    echo "date_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "repo_commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "kernel=$(uname -r)"
    echo "os=$(. /etc/os-release && echo "$PRETTY_NAME")"
    echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
    echo "kvm=$([[ -e /dev/kvm ]] && echo yes || echo no)"
    for pkg in qemu-system-x86 ovmf swtpm swtpm-tools dnsmasq-base ipxe-qemu seabios tshark wimtools osslsigncode; do
        echo "package_$pkg=$(dpkg-query -W -f='${Version}' "$pkg" 2>/dev/null || echo missing)"
    done
    echo "virt_firmware=$("$LAB_DIR/venv/bin/pip" show virt-firmware 2>/dev/null | sed -n 's/^Version: //p' || true)"
    for f in "$OVMF_CODE" "$OVMF_VARS_MS" "$OVMF_VARS_WIN23"; do
        echo "sha256 $(sha256sum "$f")"
    done
} >"$out/versions.txt"

summary=$out/summary.txt
python3 -I "$here/check_summary.py" "$out" >"$summary" || true
cat "$summary"

python3 -I -m zipfile -c "$out.zip" "$out"
echo "Attach $out.zip to issue #25."
grep -q '^RESULT PASS' "$summary"
