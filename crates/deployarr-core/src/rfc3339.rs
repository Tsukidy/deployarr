//! RFC 3339 text for `OffsetDateTime`.
//!
//! `time`'s `serde-well-known` and `formatting` features need `std`, so this
//! module replaces `time::serde::rfc3339`. It parses with `time`'s `Rfc3339`
//! parser and writes the same text as `time`'s `Rfc3339` formatter:
//!
//! - `YYYY-MM-DDTHH:MM:SS`
//! - then the fraction of a second, with trailing zeros cut, when it is not zero
//! - then `Z` for a zero offset, or `+HH:MM` or `-HH:MM`

use alloc::format;
use alloc::string::String;
use core::fmt;

use serde::{Deserializer, de};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) fn format(datetime: OffsetDateTime) -> Result<String, FormatError> {
    let year = datetime.year();
    if !(0..10_000).contains(&year) {
        return Err(FormatError::Year(year));
    }
    let offset = datetime.offset();
    let (offset_hours, offset_minutes, offset_seconds) = offset.as_hms();
    if offset_hours.unsigned_abs() > 23 {
        return Err(FormatError::OffsetHours(offset_hours));
    }
    if offset_seconds != 0 {
        return Err(FormatError::OffsetSeconds(offset_seconds));
    }

    let month = u8::from(datetime.month());
    let (day, hour, minute, second) = (
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second(),
    );
    let mut text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");

    let nanosecond = datetime.nanosecond();
    if nanosecond != 0 {
        let fraction = format!(".{nanosecond:09}");
        text.push_str(fraction.trim_end_matches('0'));
    }

    if offset.is_utc() {
        text.push('Z');
    } else {
        let sign = if offset.is_negative() { '-' } else { '+' };
        let (hours, minutes) = (offset_hours.unsigned_abs(), offset_minutes.unsigned_abs());
        text.push_str(&format!("{sign}{hours:02}:{minutes:02}"));
    }
    Ok(text)
}

/// A value that RFC 3339 cannot write. Parsing never yields one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormatError {
    Year(i32),
    OffsetHours(i8),
    OffsetSeconds(i8),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Year(year) => write!(
                f,
                "RFC 3339 writes years from 0 to 9999, and this timestamp's year is {year}"
            ),
            Self::OffsetHours(hours) => write!(
                f,
                "RFC 3339 writes UTC offsets under 24 hours, and this timestamp's offset has {hours} hours"
            ),
            Self::OffsetSeconds(seconds) => write!(
                f,
                "RFC 3339 writes UTC offsets in whole minutes, and this timestamp's offset has {seconds} seconds"
            ),
        }
    }
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<OffsetDateTime, D::Error> {
    deserializer.deserialize_str(Visitor)
}

struct Visitor;

impl de::Visitor<'_> for Visitor {
    type Value = OffsetDateTime;

    // The same text as `time::serde::rfc3339`, so parse errors read as before.
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an RFC3339-formatted `OffsetDateTime`")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<OffsetDateTime, E> {
        OffsetDateTime::parse(value, &Rfc3339).map_err(E::custom)
    }
}
