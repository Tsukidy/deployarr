#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR disable=SC2054 # QEMU options use commas.
# Starts one PXE lab VM. Run "sudo tools/lab/network.sh up" first.
# See docs/lab-setup.md.
# Usage: tools/lab/vm.sh uefi|uefi2023|bios [extra QEMU arguments]
# The VM keeps its disk, firmware variables, TPM state, and serial log in
# $LAB_DIR/<name>. LAB_DIR defaults to ~/deployarr-lab.
set -euo pipefail
# shellcheck source=lab.conf
source "$(dirname "$0")/lab.conf"

name=${1:-}
shift || true
if [[ -z $name || -z ${VM_FIRMWARE[$name]:-} ]]; then
    echo "Usage: $0 uefi|uefi2023|bios [extra QEMU arguments]" >&2
    exit 2
fi
firmware=${VM_FIRMWARE[$name]}
dir=$LAB_DIR/$name
mkdir -p "$dir"

if [[ -r /dev/kvm && -w /dev/kvm ]]; then
    accel=(-accel kvm -cpu host)
elif [[ ${LAB_ALLOW_TCG:-} == 1 ]]; then
    echo "No usable /dev/kvm. Running under TCG, which is slow." >&2
    accel=(-accel tcg -cpu max)
else
    echo "No usable /dev/kvm. Turn on virtualization in the host's firmware settings, and add your user to the kvm group." >&2
    exit 1
fi

if [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]]; then
    display=()
else
    vnc=${VM_VNC[$name]}
    display=(-display none -vnc "127.0.0.1:$vnc")
    echo "No desktop session. The VM's screen is on VNC at 127.0.0.1:$((5900 + vnc))." >&2
fi

[[ -f $dir/disk.qcow2 ]] || qemu-img create -q -f qcow2 "$dir/disk.qcow2" 64G

args=(
    -name "deployarr-lab-$name"
    -smp 2 -m 4096
    -uuid "${VM_UUID[$name]}"
    -smbios "type=2,serial=DEPLOYARR-LAB-${name^^}"
    -netdev "tap,id=net0,ifname=${VM_TAP[$name]},script=no,downscript=no"
    -drive "if=none,id=disk0,format=qcow2,file=$dir/disk.qcow2"
    -device "virtio-blk-pci,drive=disk0,bootindex=2"
    -serial "file:$dir/serial.log"
)

if [[ $firmware == uefi ]]; then
    vars=${VM_VARS[$name]}
    if [[ ! -f $dir/vars.fd ]]; then
        if [[ ! -f $vars ]]; then
            echo "Missing $vars. docs/lab-setup.md says how to create it." >&2
            exit 1
        fi
        cp "$vars" "$dir/vars.fd"
    fi
    mkdir -p "$dir/tpm"
    # A Unix socket path must be short, so the socket goes in the runtime folder.
    sock=${XDG_RUNTIME_DIR:-/tmp}/deployarr-lab-$name-swtpm.sock
    rm -f "$sock"
    swtpm socket --tpm2 --tpmstate "dir=$dir/tpm" \
        --ctrl "type=unixio,path=$sock" \
        --terminate --daemon --log "file=$dir/tpm/swtpm.log"
    for _ in $(seq 50); do
        [[ -S $sock ]] && break
        sleep 0.1
    done
    args+=(
        -machine q35,smm=on
        -global driver=cfi.pflash01,property=secure,value=on
        -drive "if=pflash,format=raw,unit=0,readonly=on,file=$OVMF_CODE"
        -drive "if=pflash,format=raw,unit=1,file=$dir/vars.fd"
        -chardev "socket,id=chrtpm,path=$sock"
        -tpmdev emulator,id=tpm0,chardev=chrtpm
        -device tpm-crb,tpmdev=tpm0
        # romfile= leaves out QEMU's unsigned iPXE ROM, so OVMF's own
        # virtio-net driver and PXE client run with Secure Boot on.
        -device "virtio-net-pci,netdev=net0,mac=${VM_MAC[$name]},romfile=,bootindex=1"
    )
else
    args+=(
        # With a 32-bit SMBIOS entry point, the iPXE ROM sends option 97.
        -machine pc,smbios-entry-point-type=32
        -device "e1000,netdev=net0,mac=${VM_MAC[$name]},bootindex=1"
    )
fi

exec qemu-system-x86_64 "${accel[@]}" "${args[@]}" "${display[@]}" "$@"
