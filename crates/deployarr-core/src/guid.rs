//! The GUID text that a task sequence accepts.
//!
//! `uuid`'s own `Deserialize` also accepts the URN form, `urn:uuid:` and then a
//! hyphenated GUID, and the simple form of 32 hex digits without hyphens. A task
//! sequence accepts only the hyphenated form of 32 hex digits in groups of
//! 8-4-4-4-12, in any letter case, with or without one pair of braces. Every
//! field that holds a GUID parses through [`deserialize`]. The serializer is
//! `uuid`'s, which writes the hyphenated form in lower case without braces.

use core::fmt;

use serde::{Deserializer, de};
use uuid::Uuid;

/// The length of the hyphenated form without braces.
const HYPHENATED_LEN: usize = 36;

const EXPECTED: &str = "a GUID of 32 hex digits in groups of 8-4-4-4-12 separated by hyphens, \
     with or without braces";

/// Parses a GUID in the hyphenated form, with or without one pair of braces.
pub(crate) fn parse(text: &str) -> Option<Uuid> {
    let hyphenated = text
        .strip_prefix('{')
        .and_then(|inner| inner.strip_suffix('}'))
        .unwrap_or(text);
    // `Uuid::try_parse` reads a 36-byte input as the hyphenated form only. The
    // length check turns away the simple, braced simple, and URN forms.
    if hyphenated.len() != HYPHENATED_LEN {
        return None;
    }
    Uuid::try_parse(hyphenated).ok()
}

/// Deserializes a GUID with [`parse`]. Use it with
/// `#[serde(deserialize_with = "crate::guid::deserialize")]`.
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
    deserializer.deserialize_str(GuidVisitor)
}

struct GuidVisitor;

impl de::Visitor<'_> for GuidVisitor {
    type Value = Uuid;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(EXPECTED)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Uuid, E> {
        parse(text).ok_or_else(|| E::custom(format_args!("expected {EXPECTED}")))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use uuid::Uuid;

    const GUID: Uuid = Uuid::from_u128(0xe2b6a5f1_4c3d_4b2a_9f8e_7d6c5b4a3f21);

    #[test]
    fn the_hyphenated_form_parses_in_any_case_with_or_without_braces() {
        for text in [
            "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21",
            "e2B6a5F1-4c3D-4b2A-9f8E-7d6C5b4A3f21",
            "{e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21}",
            "{E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21}",
        ] {
            assert_eq!(parse(text), Some(GUID), "{text}");
        }
    }

    #[test]
    fn every_other_form_fails() {
        for text in [
            "urn:uuid:e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "URN:UUID:E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21",
            "e2b6a5f14c3d4b2a9f8e7d6c5b4a3f21",
            "{e2b6a5f14c3d4b2a9f8e7d6c5b4a3f21}",
            "{{e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21}}",
            "{e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21}",
            "(e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21)",
            " e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f2",
            "e2b6a5f1-4c3d-4b2a-9f8e7-d6c5b4a3f21",
            "g2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
}
