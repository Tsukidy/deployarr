//! `Timestamp` writes its own RFC 3339 text, because `time`'s formatter needs
//! `std`. These tests check it against `time::serde::rfc3339`, which it
//! replaced, so the JSON for a timestamp stays the same.

use deployarr_core::condition::Timestamp;
use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

/// The serde attribute that `Timestamp` used before the crate became `no_std`.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct Before(#[serde(with = "time::serde::rfc3339")] OffsetDateTime);

fn at(year: i32, month: Month, day: u8, time: Time, offset: UtcOffset) -> OffsetDateTime {
    let date = Date::from_calendar_date(year, month, day).expect("a valid date");
    PrimitiveDateTime::new(date, time).assume_offset(offset)
}

fn offset(hours: i8, minutes: i8, seconds: i8) -> UtcOffset {
    UtcOffset::from_hms(hours, minutes, seconds).expect("a valid offset")
}

const NANOSECONDS: [u32; 16] = [
    0,
    1,
    9,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    100_000_001,
    120_000_000,
    123_456_789,
    500_000_000,
    999_999_999,
];

/// Offsets that RFC 3339 can write, and offsets that `time` refuses to write.
fn offsets() -> Vec<UtcOffset> {
    vec![
        UtcOffset::UTC,
        offset(1, 0, 0),
        offset(-1, 0, 0),
        offset(5, 30, 0),
        offset(-5, -30, 0),
        offset(0, 30, 0),
        offset(0, -30, 0),
        offset(23, 59, 0),
        offset(-23, -59, 0),
        offset(24, 0, 0),
        offset(-25, -59, -59),
        offset(1, 0, 30),
        offset(0, 0, -1),
    ]
}

/// Every year, offset, and fraction above, plus a spread of instants from a
/// fixed pseudo-random sequence.
fn values() -> Vec<OffsetDateTime> {
    let mut values = Vec::new();
    for year in [-1, 0, 1, 99, 999, 1601, 1970, 2024, 2038, 9999] {
        for offset in offsets() {
            for nanosecond in NANOSECONDS {
                let time = Time::from_hms_nano(13, 7, 59, nanosecond).expect("a valid time");
                values.push(at(year, Month::June, 15, time, offset));
            }
        }
    }
    values.push(at(
        2024,
        Month::February,
        29,
        Time::MIDNIGHT,
        UtcOffset::UTC,
    ));
    values.push(at(
        1999,
        Month::December,
        31,
        Time::from_hms(23, 59, 59).expect("a valid time"),
        offset(-8, 0, 0),
    ));

    // 0000-01-02 to 9999-12-30 in UTC, so every offset below keeps the year in range.
    let first = at(0, Month::January, 2, Time::MIDNIGHT, UtcOffset::UTC).unix_timestamp_nanos();
    let last = at(9999, Month::December, 30, Time::MIDNIGHT, UtcOffset::UTC).unix_timestamp_nanos();
    // splitmix64. Every output bit is mixed, so `% 4` and `% 1_000` draw evenly.
    // A plain 64-bit LCG repeats its low bits, and `% 4` then picks one branch.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    for _ in 0..20_000 {
        let span = u128::try_from(last - first).expect("last is after first");
        let step = (u128::from(next()) << 64 | u128::from(next())) % span;
        let instant = first + i128::try_from(step).expect("the step fits");
        let minutes = i32::try_from(next() % 2_879).expect("fits") - 1_439;
        let offset = UtcOffset::from_whole_seconds(minutes * 60).expect("a valid offset");
        let mut value = OffsetDateTime::from_unix_timestamp_nanos(instant)
            .expect("a valid instant")
            .to_offset(offset);
        // Whole seconds and short fractions are common in real documents.
        match next() % 4 {
            0 => value = value.replace_nanosecond(0).expect("a valid nanosecond"),
            1 => {
                let millisecond = u16::try_from(next() % 1_000).expect("fits");
                value = value.replace_millisecond(millisecond).expect("valid");
            }
            _ => {}
        }
        values.push(value);
    }
    values
}

#[test]
fn timestamps_write_the_same_json_as_time() {
    let values = values();
    // The fixed grid gives 1,950 values with a fraction. The random set must add
    // many more, or the generator has stopped drawing fractions.
    let fractions = values
        .iter()
        .filter(|value| value.nanosecond() != 0)
        .count();
    assert!(
        fractions > 10_000,
        "only {fractions} values have a fraction"
    );

    let mut written = 0;
    let mut refused = 0;
    for value in values {
        let before = serde_json::to_string(&Before(value));
        let after = serde_json::to_string(&Timestamp(value));
        match before {
            Ok(before) => {
                let after = after.unwrap_or_else(|error| {
                    panic!("{value:?}: time wrote {before}, and Timestamp failed with {error}")
                });
                assert_eq!(after, before, "{value:?}");
                written += 1;
            }
            Err(error) => {
                assert!(after.is_err(), "{value:?}: time refused it with {error}");
                refused += 1;
            }
        }
    }
    assert!(
        written > 20_000 && refused > 0,
        "{written} written, {refused} refused"
    );
}

#[test]
fn written_timestamps_parse_back_to_the_same_instant_and_offset() {
    for value in values() {
        let Ok(json) = serde_json::to_string(&Timestamp(value)) else {
            continue;
        };
        let parsed: Timestamp = serde_json::from_str(&json).expect("written text parses");
        assert_eq!(parsed.0, value, "{json}");
        assert_eq!(parsed.0.offset(), value.offset(), "{json}");
    }
}

fn read<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|error| error.to_string())
}

#[test]
fn timestamps_parse_like_time() {
    let inputs = [
        r#""2024-10-01T00:00:00Z""#,
        r#""2030-01-01T00:00:00+01:00""#,
        r#""2030-01-01T00:00:00-00:00""#,
        r#""2024-10-01t00:00:00z""#,
        r#""2024-10-01T00:00:00.5Z""#,
        r#""2024-10-01T00:00:00.123456789-05:30""#,
        r#""2024-10-01T00:00:00.1234567891Z""#,
        r#""1990-12-31T23:59:60Z""#,
        r#""1990-12-31T22:59:60Z""#,
        r#""0000-01-01T00:00:00Z""#,
        r#""9999-12-31T23:59:59.999999999Z""#,
        r#""2024-10-01T00:00:00+24:00""#,
        r#""2024-10-01T00:00:00+23:60""#,
        r#""2024-02-30T00:00:00Z""#,
        r#""2024-10-01 00:00:00Z""#,
        r#""2024-10-01T00:00Z""#,
        r#""2024-10-01T00:00:00""#,
        r#""2024-10-01T00:00:00Z ""#,
        r#""yesterday""#,
        r#""""#,
        "5",
        "null",
        "true",
        "[]",
    ];
    for input in inputs {
        let before = read::<Before>(input).map(|Before(value)| (value, value.offset()));
        let after = read::<Timestamp>(input).map(|Timestamp(value)| (value, value.offset()));
        assert_eq!(after, before, "{input}");
    }
}

#[test]
fn timestamps_write_rfc_3339() {
    let cases = [
        (
            at(2024, Month::October, 1, Time::MIDNIGHT, UtcOffset::UTC),
            "2024-10-01T00:00:00Z",
        ),
        (
            at(2030, Month::January, 1, Time::MIDNIGHT, offset(1, 0, 0)),
            "2030-01-01T00:00:00+01:00",
        ),
        (
            at(
                7,
                Month::March,
                9,
                Time::from_hms_nano(4, 5, 6, 120_000_000).expect("a valid time"),
                offset(0, -30, 0),
            ),
            "0007-03-09T04:05:06.12-00:30",
        ),
    ];
    for (value, text) in cases {
        let json = serde_json::to_string(&Timestamp(value)).expect("it writes");
        assert_eq!(json, format!("\"{text}\""));
    }
}

#[test]
fn a_timestamp_that_rfc_3339_cannot_write_fails_with_the_reason() {
    let noon = Time::from_hms(12, 0, 0).expect("a valid time");
    let cases = [
        (
            at(-1, Month::June, 15, noon, UtcOffset::UTC),
            "RFC 3339 writes years from 0 to 9999, and this timestamp's year is -1",
        ),
        (
            at(2024, Month::June, 15, noon, offset(-24, 0, 0)),
            "RFC 3339 writes UTC offsets under 24 hours, and this timestamp's offset has -24 hours",
        ),
        (
            at(2024, Month::June, 15, noon, offset(1, 0, 30)),
            "RFC 3339 writes UTC offsets in whole minutes, and this timestamp's offset has 30 seconds",
        ),
    ];
    for (value, message) in cases {
        let error = serde_json::to_string(&Timestamp(value)).expect_err("it must not write");
        assert_eq!(error.to_string(), message);
    }
}
