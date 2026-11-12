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
# Needs: bash 4.4 or later, curl, 7z, msiinfo (msitools), sha1sum, sha256sum, and Python 3.10 or later (stdlib only).
set -euo pipefail
shopt -s inherit_errexit

work="${1:?usage: reproduce.sh <workdir> [setup-url] [expected-sha256]}"
url="${2:-https://go.microsoft.com/fwlink/?linkid=2289981}"
expected="${3:-d4de67acc83de253ccc941dd0590808d83726bdd8ea768a374a7beaff7949d6a}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
helper="$here/winpe_dlls.py"
msi_package="package_WindowsPEwimsDesktopEditions_x86_en_us"

py() { python3 -I "$helper" "$@"; }

die() { echo "$*" >&2; exit 1; }

verify() {
  local file="$1" algo="$2" want="${3,,}" label="$4" actual
  [ -n "$want" ] || die "$label: no expected $algo digest"
  actual="$("${algo}sum" "$file" | cut -d' ' -f1)"
  [ "$actual" = "$want" ] || die "$label $algo mismatch: got $actual, expected $want"
}

download() {
  local dest="$1" src="$2"
  curl -fL --retry 4 --retry-delay 2 -sS -o "$dest.part" -w '%{url_effective}' "$src"
  mv "$dest.part" "$dest"
}

mkdir -p "$work/dl" "$work/msi" "$work/wim" "$work/out"
setup="$work/dl/adkwinpesetup.exe"
setup_url="$work/dl/setup-url.txt"

if [ -f "$setup" ] && [ "$(sha256sum "$setup" | cut -d' ' -f1)" != "${expected,,}" ]; then rm -f "$setup" "$setup_url"; fi
if [ ! -f "$setup" ] || [ ! -f "$setup_url" ]; then
  download "$setup" "$url" > "$setup_url.part"
  mv "$setup_url.part" "$setup_url"
fi
effective="$(cat "$setup_url")"
base="${effective%/*}"
verify "$setup" sha256 "$expected" "Setup program"
setup_sha256="$(sha256sum "$setup" | cut -d' ' -f1)"

rm -rf "$work/burn"
7z x -y -o"$work/burn" "$setup" >/dev/null
manifest="$work/burn/0"
addon_version="$(py version "$manifest")"
payloads="$(py payloads "$manifest" "$msi_package")"

fetch() {
  local manifest_path="$1" sha1="$2" url_path="$3"
  local name="${manifest_path##*\\}"
  local dest="$work/dl/$name"
  if [ -f "$dest" ] && [ "$(sha1sum "$dest" | cut -d' ' -f1)" != "$sha1" ]; then rm -f "$dest"; fi
  if [ ! -f "$dest" ]; then
    download "$dest" "$base/$url_path" >/dev/null
  fi
  verify "$dest" sha1 "$sha1" "$name"
  echo "$dest"
}

msi_row="$(awk -F'\t' 'tolower($1) ~ /\.msi$/' <<<"$payloads")"
IFS=$'\t' read -r msi_fp _ msi_sha1 msi_urlpath <<<"$msi_row"
msi="$(fetch "$msi_fp" "$msi_sha1" "$msi_urlpath")"

for t in Directory Component File Media; do msiinfo export "$msi" "$t" > "$work/msi/$t.idt"; done
IFS=$'\t' read -r wim_key cab_name wim_size < <(py locate "$work/msi")

cab_row="$(awk -F'\t' -v c="$cab_name" 'tolower($1) == tolower("Installers\\" c)' <<<"$payloads")"
[ -n "$cab_row" ] || die "cabinet $cab_name is not a payload of $msi_package"
IFS=$'\t' read -r cab_fp _ cab_sha1 cab_urlpath <<<"$cab_row"
cab="$(fetch "$cab_fp" "$cab_sha1" "$cab_urlpath")"

wim="$work/wim/$cab_sha1/$wim_key"
if [ ! -f "$wim" ] || [ "$(stat -c %s "$wim")" -ne "$wim_size" ]; then
  rm -f "$wim"
  7z x -y -o"$work/wim/$cab_sha1" "$cab" "$wim_key" >/dev/null
fi
[ "$(stat -c %s "$wim")" -eq "$wim_size" ] || die "winpe.wim is $(stat -c %s "$wim") bytes, the MSI File table says $wim_size"

listing="$work/wim/listing.txt"
7z l -slt "$wim" > "$listing"
schema_path="$(py schema-path "$listing")"
schema_dll="$work/wim/files/$schema_path"
7z x -y -o"$work/wim/files" "$wim" "$schema_path" >/dev/null

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
