#!/usr/bin/env bash
# Usage: extract_all.sh <media_root> <out_root>
# Extracts every non-empty MSI and CAB under <media_root> (except LanguagePack)
# into its own directory under <out_root>, named after its relative path.
media="$1"; out="$2"
find "$media" -type f \( -iname '*.msi' -o -iname '*.cab' \) -size +0 -not -path '*/LanguagePack/*' | while read -r f; do
  rel="${f#$media/}"
  d="$out/$(echo "$rel" | tr '/' '_')"
  [ -d "$d" ] && continue
  mkdir -p "$d"
  case "${f,,}" in
    *.msi) msiextract -C "$d" "$f" >/dev/null 2>&1 || 7z x -y -o"$d" "$f" >/dev/null 2>&1 ;;
    *.cab) 7z x -y -o"$d" "$f" >/dev/null 2>&1 ;;
  esac
  echo "$(find "$d" -type f | wc -l) files <- $rel"
done
