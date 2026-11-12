#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Builds the two TFTP folders that the PXE lab experiments serve, from the
# Windows 11 Enterprise evaluation ISO and the BCD stores from the BCD workflow.
# See docs/lab-setup.md.
#
# Usage: tools/lab/media.sh <Windows ISO> <bcd-stores.zip>
#
# It writes:
#   $LAB_DIR/tftp/2011  boot programs signed with the 2011 certificates
#   $LAB_DIR/tftp/2023  the same, with the _EX boot managers, which are
#                       signed with the Windows UEFI CA 2023
#   $LAB_DIR/tftp/manifest.txt  every file's size, SHA-256, and signer
# Each run rebuilds both folders from the ISO. Never commit these files.
set -euo pipefail
# shellcheck source=lab.conf
source "$(dirname "$0")/lab.conf"

die() {
    echo "$*" >&2
    exit 1
}

[[ $# -eq 2 ]] || die "Usage: $0 <Windows ISO> <bcd-stores.zip>"
iso=$(realpath "$1")
bcd_zip=$(realpath "$2")
[[ -f $iso ]] || die "No ISO at $1"
[[ -f $bcd_zip ]] || die "No BCD zip at $2"
for tool in 7z wiminfo wimexport wimextract osslsigncode python3 sha256sum; do
    command -v "$tool" >/dev/null || die "Missing $tool. Install the packages in docs/lab-setup.md."
done

tftp=$LAB_DIR/tftp
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "Reading $iso"
7z x -y -o"$work/iso" "$iso" sources/boot.wim boot/boot.sdi >/dev/null
boot_wim=$work/iso/sources/boot.wim
[[ -f $boot_wim ]] || die "The ISO has no sources/boot.wim."
[[ -f $work/iso/boot/boot.sdi ]] || die "The ISO has no boot/boot.sdi."

# Index 1 of boot.wim is WinPE. Index 2 is Windows Setup.
pe_name=$(wiminfo "$boot_wim" 1 | sed -n 's/^Name: *//p')
[[ $pe_name == *"Windows PE"* ]] || die "Index 1 of boot.wim is \"$pe_name\", not Windows PE."

echo "Extracting \\Windows\\Boot from boot.wim index 1"
wimextract "$boot_wim" 1 /Windows/Boot --dest-dir="$work/pe" --no-acls >/dev/null 2>&1
b=$work/pe/Boot

rm -rf "$tftp/2011" "$tftp/2023" "$tftp/boot.wim"
mkdir -p "$tftp"
echo "Exporting WinPE as a single-image boot.wim"
wimexport "$boot_wim" 1 "$tftp/boot.wim" --boot >/dev/null

python3 -I -m zipfile -e "$bcd_zip" "$work/bcd"

fill() {
    local dest=$1 bootmgfw=$2 wdsmgfw=$3 fonts=$4
    mkdir -p "$dest/boot/x64" "$dest/boot/fonts"
    cp "$bootmgfw" "$dest/boot/x64/bootmgfw.efi"
    cp "$wdsmgfw" "$dest/boot/x64/wdsmgfw.efi"
    for f in wdsnbp.com pxeboot.n12 pxeboot.com bootmgr.exe abortpxe.com; do
        cp "$b/PXE/$f" "$dest/boot/x64/$f"
    done
    # The _EX font names lose their _EX suffix here (inferred: the boot manager
    # asks for the plain names). The prototype log shows the names it asks for.
    for f in "$fonts"/*.ttf; do
        local name
        name=$(basename "$f")
        cp "$f" "$dest/boot/fonts/${name/_EX.ttf/.ttf}"
    done
    cp "$work/iso/boot/boot.sdi" "$dest/boot/boot.sdi"
    ln "$tftp/boot.wim" "$dest/boot/x64/boot.wim"
    for fw in uefi bios; do
        cp "$work/bcd/out/default/$fw.bcd" "$dest/boot/$fw.bcd"
        cp "$work/bcd/out/no-path/$fw.bcd" "$dest/boot/$fw-nopath.bcd"
        cp "$work/bcd/out/tftp/$fw.bcd" "$dest/boot/$fw-maxtftp.bcd"
    done
}

for f in EFI/bootmgfw.efi PXE/wdsmgfw.efi PXE/wdsnbp.com PXE/pxeboot.n12 PXE/pxeboot.com PXE/bootmgr.exe PXE/abortpxe.com; do
    [[ -f $b/$f ]] || die "boot.wim has no \\Windows\\Boot\\$f."
done
fill "$tftp/2011" "$b/EFI/bootmgfw.efi" "$b/PXE/wdsmgfw.efi" "$b/Fonts"
if [[ -f $b/EFI_EX/bootmgfw_EX.efi && -f $b/PXE_EX/wdsmgfw_EX.efi && -d $b/Fonts_EX ]]; then
    fill "$tftp/2023" "$b/EFI_EX/bootmgfw_EX.efi" "$b/PXE_EX/wdsmgfw_EX.efi" "$b/Fonts_EX"
else
    echo "This boot.wim has no _EX boot managers, so $tftp/2023 is not built." >&2
fi

signer() {
    osslsigncode verify -in "$1" 2>/dev/null |
        sed -n 's/^[[:space:]]*Subject:.*CN=\(.*\)$/\1/p' | grep -E 'CA|PCA' | head -1 || true
}

{
    echo "iso $(basename "$iso") sha256 $(sha256sum "$iso" | cut -d' ' -f1)"
    echo "bcd_zip $(basename "$bcd_zip") sha256 $(sha256sum "$bcd_zip" | cut -d' ' -f1)"
    wiminfo "$tftp/boot.wim" 1 | grep -E '^(Name|Description|Architecture|Build|Service Pack Build|Major Version|Minor Version):'
    echo
    printf '%-40s %10s  %-64s  %s\n' path bytes sha256 signer
    cd "$tftp"
    find 2011 2023 -type f 2>/dev/null | sort | while read -r f; do
        s=""
        [[ $f == *.efi || $f == *.exe ]] && s=$(signer "$f")
        printf '%-40s %10s  %-64s  %s\n' "$f" "$(stat -c %s "$f")" "$(sha256sum "$f" | cut -d' ' -f1)" "${s:--}"
    done
} >"$tftp/manifest.txt"

grep -E '^(Name|Build):' "$tftp/manifest.txt"
grep -E '^20(11|23)/boot/x64/(bootmgfw|wdsmgfw)\.efi ' "$tftp/manifest.txt" |
    awk '{ printf "%-32s", $1; for (i = 4; i <= NF; i++) printf " %s", $i; print "" }'
echo "Wrote $tftp/2011, $tftp/2023, and $tftp/manifest.txt."
