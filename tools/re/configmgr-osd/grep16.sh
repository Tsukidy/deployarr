#!/usr/bin/env bash
# Usage: grep16.sh <root> <pattern>...
# Lists files under <root> containing each literal pattern as ASCII or UTF-16LE.
root="$1"; shift
for p in "$@"; do
  u=$(printf '%s' "$p" | sed 's/./&\\x00/g')
  echo "== $p"
  { grep -rlaF -- "$p" "$root"; grep -rlaP -- "$u" "$root"; } 2>/dev/null | sort -u | head -40
done
