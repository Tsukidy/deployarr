#!/usr/bin/env bash
# Rebuild the ConfigMgr OSD research outputs from Microsoft's installer.
#
# Usage: reproduce.sh <workdir> [installer-url] [expected-sha256]
#
# Downloads the ConfigMgr baseline installer into <workdir>, unpacks it and
# every nested MSI/CAB, searches for MDT components, and writes:
#   <workdir>/out/mdt-markers.txt         files containing MDT markers
#   <workdir>/out/osd-x64-inventory.tsv   OSD x64 binaries with versions and hashes
#   <workdir>/out/task-sequence-steps.md  step -> executable -> variable catalog
#
# Needs: curl, 7z, msiextract, sha256sum, and a Python with pefile installed.
# Set PYTHON to choose the interpreter (default: python3).
set -euo pipefail

work="${1:?usage: reproduce.sh <workdir> [installer-url] [expected-sha256]}"
url="${2:-https://go.microsoft.com/fwlink/?linkid=2350307&clcid=0x409&culture=en-us&country=us}"
expected="${3:-4cb0380e2b1c43f1e0b2da266beddd1b975dd72537f8d8113c702eadf1e6e313}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
py="${PYTHON:-python3}"

mkdir -p "$work/dl" "$work/media" "$work/nested" "$work/out"
installer="$work/dl/ConfigMgr.exe"

if [ ! -f "$installer" ]; then
  curl -fL --retry 4 --retry-delay 2 -o "$installer" "$url"
fi
actual="$(sha256sum "$installer" | cut -d' ' -f1)"
if [ -n "$expected" ] && [ "$actual" != "$expected" ]; then
  echo "SHA-256 mismatch: got $actual, expected $expected" >&2
  echo "Pass the new hash as the third argument if Microsoft replaced the file." >&2
  exit 1
fi

[ -d "$work/media/SMSSETUP" ] || 7z x -y -o"$work/media" "$installer" >/dev/null
bash "$here/extract_all.sh" "$work/media" "$work/nested"

markers=("Deployment Toolkit" "Microsoft.BDD" "ZTIUtility" "ZTIGather" "DeploymentShare" "MDTIntegration")
{
  echo "# Files whose names look like MDT components"
  find "$work/media" "$work/nested" -type f \( -iname 'ZTI*' -o -iname 'LTI*' -o -iname '*BDD*' -o -iname '*.wsf' \)
  echo "# Files containing MDT markers (ASCII or UTF-16LE)"
  bash "$here/grep16.sh" "$work/media" "${markers[@]}"
  bash "$here/grep16.sh" "$work/nested" "${markers[@]}"
} > "$work/out/mdt-markers.txt"

"$py" -I "$here/peinfo.py" "$work/media/SMSSETUP/OSD/bin/x64" > "$work/out/osd-x64-inventory.tsv"
"$py" -I "$here/mofcatalog.py" "$work/media/SMSSETUP/BIN/X64/_TaskSequenceProvider.mof" > "$work/out/task-sequence-steps.md"

echo "Installer SHA-256: $actual"
echo "Outputs in $work/out"
