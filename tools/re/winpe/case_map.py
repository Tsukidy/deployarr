#!/usr/bin/env python3
"""Write the upper case map that PathMatchSpecW applies in base WinPE.

Run as: python3 -I case_map.py <l_intl.nls> <out.tsv>

KernelBase's PathMatchSpecW compares one UTF-16 unit at a time after it maps
each unit through LCMapStringEx with LCMAP_UPPERCASE. On the short path that
docs/research/winpe-10.1.26100.9457.md#pathmatchspecw-letter-case records,
LCMapStringEx maps `a` to `z` to `A` to `Z`, keeps every other unit below
U+00C0, and maps a unit from U+00C0 up through the first table in l_intl.nls.

The output has a header line, `unit<TAB>upper`, and then one line for each unit
that the map changes, in ascending order of unit. Both columns are four upper
case hex digits. A unit that has no line maps to itself.

The script checks the properties that the research page states, and stops
with an error when one fails:
- every upper case form maps to itself,
- no two units map to the same other unit,
- no unit from U+D800 to U+DFFF changes or is the result of a change.
"""
import struct
import sys
from pathlib import Path

HEADER = "unit\tupper"


def read_words(data: bytes) -> tuple[int, ...]:
    if len(data) % 2:
        raise ValueError(f"l_intl.nls is {len(data)} bytes, not a whole number of 16-bit words")
    return struct.unpack(f"<{len(data) // 2}H", data)


def upper_table(words: tuple[int, ...]) -> tuple[int, ...]:
    """The table that KernelBase stores at section + 4 (KernelBase!0x1800fa697)."""
    if len(words) < 2 or words[0] != 1:
        raise ValueError("l_intl.nls does not start with the 16-bit word 1")
    lower_offset = words[1]
    if 2 + lower_offset > len(words):
        raise ValueError(f"the second table offset {lower_offset:#x} is past the end of the file")
    return words[2 : 2 + lower_offset]


def table_lookup(table: tuple[int, ...], unit: int) -> int:
    """unit + t[t[t[unit >> 8] + ((unit >> 4) & 0xf)] + (unit & 0xf)], with 16-bit wraparound."""
    level1 = table[unit >> 8]
    level2 = table[level1 + ((unit >> 4) & 0xF)]
    return (unit + table[level2 + (unit & 0xF)]) & 0xFFFF


def short_path_upper(table: tuple[int, ...], unit: int) -> int:
    """LCMapStringEx's loop (KernelBase!0x18001a48e to KernelBase!0x18001a4b4)."""
    if 0x61 <= unit <= 0x7A:
        return unit - 0x20
    if unit < 0xC0:
        return unit
    return table_lookup(table, unit)


def build_map(data: bytes) -> list[tuple[int, int]]:
    table = upper_table(read_words(data))
    upper = [short_path_upper(table, unit) for unit in range(0x10000)]
    changed = [(unit, up) for unit, up in enumerate(upper) if up != unit]
    for unit, up in changed:
        if upper[up] != up:
            raise ValueError(f"U+{unit:04X} maps to U+{up:04X}, which maps to U+{upper[up]:04X}")
        if 0xD800 <= unit <= 0xDFFF or 0xD800 <= up <= 0xDFFF:
            raise ValueError(f"U+{unit:04X} maps to U+{up:04X}, a surrogate unit")
    seen: dict[int, int] = {}
    for unit, up in changed:
        if up in seen:
            raise ValueError(f"U+{seen[up]:04X} and U+{unit:04X} both map to U+{up:04X}")
        seen[up] = unit
    return changed


def main(argv: list[str]) -> None:
    if len(argv) != 3:
        sys.exit(__doc__)
    source, out = Path(argv[1]), Path(argv[2])
    changed = build_map(source.read_bytes())
    lines = [HEADER] + [f"{unit:04X}\t{up:04X}" for unit, up in changed]
    out.write_text("".join(f"{line}\n" for line in lines), encoding="ascii", newline="\n")
    print(f"upper_case_units: {len(changed)}")


if __name__ == "__main__":
    try:
        main(sys.argv)
    except ValueError as error:
        sys.exit(f"case_map.py: {error}")
