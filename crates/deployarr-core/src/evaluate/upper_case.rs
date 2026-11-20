//! The upper case map that `PathMatchSpecW` applies in base WinPE, which
//! `Like` uses to match letters in any case.
//!
//! `PathMatchSpecW` compares one UTF-16 unit at a time after it upper-cases
//! each unit through `LCMapStringEx`. In the WinPE image of the add-on for
//! ADK 10.1.26100.9457 (version 10.0.26100.1), that maps `a` to `z` to `A` to
//! `Z`, keeps every other unit below U+00C0, and maps a unit from U+00C0 up
//! through the case table in `l_intl.nls`
//! (`docs/research/winpe-10.1.26100.9457.md#pathmatchspecw-letter-case`).
//!
//! The map is generated data. `tools/re/winpe/case_map.py` writes it from
//! that image's `l_intl.nls`, as `tools/re/README.md#winpe` says. The file has
//! a header line and then one line for each unit that the map changes, in
//! ascending order, with the unit and its upper case form as four hex digits
//! split by a tab. A unit without a line maps to itself. The crate reads the
//! file when it compiles, so a malformed file fails the build.

/// The generated map.
const TSV: &str = include_str!("../../data/winpe-10.1.26100.9457-upper-case-map.tsv");

/// The first line of the file.
const HEADER: &[u8] = b"unit\tupper";

/// The number of units that the map changes.
const LEN: usize = count_rows(TSV.as_bytes());

/// Each unit that the map changes and its upper case form, sorted by unit.
static MAP: [(u16, u16); LEN] = parse(TSV.as_bytes());

/// The upper case form of one UTF-16 unit.
pub(super) fn upper(unit: u16) -> u16 {
    match MAP.binary_search_by_key(&unit, |&(from, _)| from) {
        Ok(index) => MAP[index].1,
        Err(_) => unit,
    }
}

/// The line that starts at `start`, as its end without the line break, and
/// the start of the next line. A `\r` before the `\n` is part of the line
/// break, so a checkout that turns `\n` into `\r\n` reads the same.
const fn line(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut end = start;
    while end < bytes.len() && bytes[end] != b'\n' {
        end += 1;
    }
    assert!(
        end < bytes.len(),
        "the upper case map must end each line with a line break"
    );
    let next = end + 1;
    if end > start && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    (end, next)
}

/// The index after the header line.
const fn skip_header(bytes: &[u8]) -> usize {
    let (end, next) = line(bytes, 0);
    assert!(
        end == HEADER.len(),
        "the upper case map must start with its header line"
    );
    let mut i = 0;
    while i < HEADER.len() {
        assert!(
            bytes[i] == HEADER[i],
            "the upper case map must start with its header line"
        );
        i += 1;
    }
    next
}

const fn count_rows(bytes: &[u8]) -> usize {
    let mut at = skip_header(bytes);
    let mut rows = 0;
    while at < bytes.len() {
        at = line(bytes, at).1;
        rows += 1;
    }
    rows
}

/// Four upper case hex digits at `at`.
const fn hex4(bytes: &[u8], at: usize) -> u16 {
    let mut value: u16 = 0;
    let mut i = 0;
    while i < 4 {
        let digit = match bytes[at + i] {
            byte @ b'0'..=b'9' => byte - b'0',
            byte @ b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("the upper case map must hold four upper case hex digits per unit"),
        };
        value = value * 16 + digit as u16;
        i += 1;
    }
    value
}

const fn parse<const N: usize>(bytes: &[u8]) -> [(u16, u16); N] {
    let mut map = [(0, 0); N];
    let mut at = skip_header(bytes);
    let mut row = 0;
    while row < N {
        let (end, next) = line(bytes, at);
        assert!(
            end - at == 9 && bytes[at + 4] == b'\t',
            "each row of the upper case map must be a unit, a tab, and its upper case form"
        );
        let unit = hex4(bytes, at);
        let upper = hex4(bytes, at + 5);
        assert!(
            unit != upper,
            "the upper case map must list only units that change"
        );
        assert!(
            row == 0 || map[row - 1].0 < unit,
            "the upper case map must list each unit once, in ascending order"
        );
        map[row] = (unit, upper);
        at = next;
        row += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counts and examples in
    /// `docs/research/winpe-10.1.26100.9457.md#what-it-matches`.
    #[test]
    fn the_map_changes_the_units_that_the_research_counts() {
        assert_eq!(MAP.len(), 973);
        let ascii = MAP.iter().filter(|&&(unit, _)| unit < 0x80).count();
        assert_eq!(ascii, 26);
        for unit in 0..0xc0_u16 {
            let expected = if (0x61..=0x7a).contains(&unit) {
                unit - 0x20
            } else {
                unit
            };
            assert_eq!(upper(unit), expected, "U+{unit:04X}");
        }
        for (unit, expected) in [
            (0x00e4, 0x00c4),
            (0x03c3, 0x03a3),
            (0x03c2, 0x03c2),
            (0x00ff, 0x0178),
            (0x00df, 0x00df),
            (0x0131, 0x0131),
            (0x0130, 0x0130),
            (0x212a, 0x212a),
        ] {
            assert_eq!(upper(unit), expected, "U+{unit:04X}");
        }
    }

    /// Each upper case form maps to itself, no two units map to the same
    /// other unit, and no surrogate unit changes, as
    /// `tools/re/winpe/case_map.py` checks when it writes the map.
    #[test]
    fn each_unit_matches_only_its_own_upper_case_form() {
        let mut targets = std::collections::BTreeSet::new();
        for &(unit, up) in &MAP {
            assert_eq!(upper(up), up, "U+{unit:04X} maps to U+{up:04X}");
            assert!(targets.insert(up), "two units map to U+{up:04X}");
            for value in [unit, up] {
                assert!(!(0xd800..=0xdfff).contains(&value), "U+{value:04X}");
            }
        }
    }

    #[test]
    fn a_crlf_checkout_reads_the_same_map() {
        const CRLF: &[u8] = b"unit\tupper\r\n0061\t0041\r\n00E4\t00C4\r\n";
        const ROWS: usize = count_rows(CRLF);
        let map: [(u16, u16); ROWS] = parse(CRLF);
        assert_eq!(map, [(0x61, 0x41), (0xe4, 0xc4)]);
    }
}
