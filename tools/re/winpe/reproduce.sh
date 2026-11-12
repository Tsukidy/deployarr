#!/usr/bin/env bash
# Rebuild the list of DLLs and API sets that base WinPE x64 can load.
#
# Usage: reproduce.sh <workdir> [setup-url] [expected-sha256]
#
# Downloads the WinPE add-on setup program for the Windows ADK into <workdir>,
# reads its Burn manifest, downloads the MSI that carries winpe.wim and the
# cabinet that holds the amd64 image, and writes:
#   <workdir>/out/winpe-x64-dlls.txt  lower-case DLL and API set names, one per line
#   <workdir>/out/system32-dlls.txt   System32 DLLs in stored case
#   <workdir>/out/apisets.tsv         every API set schema entry and whether it resolves
#   <workdir>/out/summary.txt         versions, sizes, hashes, and counts
#
# Needs: curl, 7z, msiinfo (msitools), sha1sum, sha256sum, and python3 (stdlib only).
# Downloaded files are data. Nothing from them runs.
set -euo pipefail

work="${1:?usage: reproduce.sh <workdir> [setup-url] [expected-sha256]}"
url="${2:-https://go.microsoft.com/fwlink/?linkid=2289981}"
expected="${3:-d4de67acc83de253ccc941dd0590808d83726bdd8ea768a374a7beaff7949d6a}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
helper="$here/winpe_dlls.py"
msi_package="package_WindowsPEwimsDesktopEditions_x86_en_us"

py() { python3 -I "$helper" "$@"; }

die() { echo "$*" >&2; exit 1; }

# verify <file> <sha1|sha256> <expected-hex> <label>
verify() {
  local actual
  actual="$("$2sum" "$1" | cut -d' ' -f1)"
  [ "$actual" = "${3,,}" ] || die "$4 $2 mismatch: got $actual, expected ${3,,}"
}

mkdir -p "$work/dl" "$work/burn" "$work/msi" "$work/wim" "$work/out"
setup="$work/dl/adkwinpesetup.exe"

if [ ! -f "$setup" ] || [ ! -f "$work/dl/setup-url.txt" ]; then
  curl -fL --retry 4 --retry-delay 2 -sS -o "$setup" -w '%{url_effective}' "$url" > "$work/dl/setup-url.txt"
fi
effective="$(cat "$work/dl/setup-url.txt")"
base="${effective%/*}"
if [ -n "$expected" ]; then
  verify "$setup" sha256 "$expected" "Setup program"
fi
setup_sha256="$(sha256sum "$setup" | cut -d' ' -f1)"

[ -f "$work/burn/0" ] || 7z x -y -o"$work/burn" "$setup" >/dev/null
manifest="$work/burn/0"
addon_version="$(py version "$manifest")"
payloads="$(py payloads "$manifest" "$msi_package")"

# fetch <FilePath> <sha1> <url path>: downloads if missing or corrupt, verifies, echoes the local path
fetch() {
  local dest="$work/dl/${1##*\\}"
  if [ -f "$dest" ] && [ "$(sha1sum "$dest" | cut -d' ' -f1)" != "$2" ]; then rm -f "$dest"; fi
  if [ ! -f "$dest" ]; then
    curl -fL --retry 4 --retry-delay 2 -sS -o "$dest" "$base/$3"
  fi
  verify "$dest" sha1 "$2" "${1##*\\}"
  echo "$dest"
}

msi_row="$(awk -F'\t' 'tolower($1) ~ /\.msi$/' <<<"$payloads")"
[ "$(wc -l <<<"$msi_row")" -eq 1 ] || die "expected one MSI payload in $msi_package"
IFS=$'\t' read -r msi_fp _ msi_sha1 msi_urlpath <<<"$msi_row"
msi="$(fetch "$msi_fp" "$msi_sha1" "$msi_urlpath")"

if [ ! -f "$work/msi/Media.idt" ]; then
  for t in Directory Component File Media; do msiinfo export "$msi" "$t" > "$work/msi/$t.idt"; done
fi
IFS=$'\t' read -r wim_key cab_name wim_size < <(py locate "$work/msi")

cab_row="$(awk -F'\t' -v c="$cab_name" 'tolower($1) == tolower("Installers\\" c)' <<<"$payloads")"
[ -n "$cab_row" ] || die "cabinet $cab_name is not a payload of $msi_package"
IFS=$'\t' read -r cab_fp _ cab_sha1 cab_urlpath <<<"$cab_row"
cab="$(fetch "$cab_fp" "$cab_sha1" "$cab_urlpath")"

wim="$work/wim/$wim_key"
if [ ! -f "$wim" ] || [ "$(stat -c %s "$wim")" -ne "$wim_size" ]; then
  rm -f "$wim"
  7z x -y -o"$work/wim" "$cab" "$wim_key" >/dev/null
fi
[ "$(stat -c %s "$wim")" -eq "$wim_size" ] || die "winpe.wim is $(stat -c %s "$wim") bytes, the MSI File table says $wim_size"

listing="$work/wim/listing.txt"
[ -s "$listing" ] || 7z l -slt "$wim" > "$listing"
schema_path="$(py schema-path "$listing")"
schema_dll="$work/wim/files/$schema_path"
[ -f "$schema_dll" ] || 7z x -y -o"$work/wim/files" "$wim" "$schema_path" >/dev/null

py build "$listing" "$schema_dll" "$work/out" \
  "addon_version=$addon_version" \
  "setup_url=$effective" \
  "setup_sha256=$setup_sha256" \
  "msi_sha1=$msi_sha1" \
  "cab_name=$cab_name" \
  "cab_sha1=$cab_sha1" \
  "wim_file_key=$wim_key" \
  "wim_size=$wim_size" \
  "wim_sha256=$(sha256sum "$wim" | cut -d' ' -f1)"

cat "$work/out/summary.txt"
echo "Outputs in $work/out"
