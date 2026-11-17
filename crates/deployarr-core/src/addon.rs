//! The names that add-ons give to what they contribute, and the stored form of
//! a step from an add-on.
//!
//! An add-on step is plain data. It loads, saves, shows, and exports whether
//! or not its add-on is installed. See [`crate::lower`] for how the server
//! replaces it with built-in steps before it compiles.

use alloc::boxed::Box;
use core::fmt;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::validated_string::validated_string;

/// The longest publisher or name part of an [`AddOnId`]. It is a default
/// until the owner decides the ID grammar, one of the add-on one-way doors.
pub const ADD_ON_ID_PART_MAX_LEN: usize = 32;

/// The longest [`KindId`]. It is a default until the owner decides the ID
/// grammar.
pub const KIND_ID_MAX_LEN: usize = 63;

validated_string! {
    /// An add-on's ID, `<publisher>.<name>`, such as `deployarr.sql-order-lookup`.
    ///
    /// Each part is 1 to 32 characters: a lower-case ASCII letter, then
    /// lower-case ASCII letters, digits, or `-`. Folder names, URL prefixes,
    /// store file names, and trust scopes derive from it.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    AddOnId, AddOnIdError, check_add_on_id
}

impl AddOnId {
    pub fn publisher(&self) -> &str {
        self.parts().0
    }

    pub fn name(&self) -> &str {
        self.parts().1
    }

    fn parts(&self) -> (&str, &str) {
        self.as_str()
            .split_once('.')
            .expect("an add-on ID holds one `.`")
    }
}

fn check_add_on_id(text: &str) -> Result<(), AddOnIdError> {
    let Some((publisher, name)) = text.split_once('.') else {
        return Err(AddOnIdError::NoDot);
    };
    check_part(publisher, ADD_ON_ID_PART_MAX_LEN).map_err(AddOnIdError::Publisher)?;
    check_part(name, ADD_ON_ID_PART_MAX_LEN).map_err(AddOnIdError::Name)
}

/// Why the text is not an [`AddOnId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddOnIdError {
    /// The text has no `.` between the publisher and the name.
    NoDot,
    Publisher(PartError),
    /// The name part is wrong. A second `.` makes it wrong too.
    Name(PartError),
}

impl fmt::Display for AddOnIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDot => f.write_str(
                "an add-on ID is `<publisher>.<name>`, such as `deployarr.webhooks`, and this one has no `.`",
            ),
            Self::Publisher(problem) => write!(f, "an add-on ID's publisher {problem}"),
            Self::Name(problem) => write!(f, "an add-on ID's name {problem}"),
        }
    }
}

impl core::error::Error for AddOnIdError {}

validated_string! {
    /// The ID of something that one add-on declares, such as a step kind, a
    /// resolver, or a prompt: 1 to 63 characters, a lower-case ASCII letter,
    /// then lower-case ASCII letters, digits, or `-`.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    KindId, PartError, check_kind_id
}

fn check_kind_id(text: &str) -> Result<(), PartError> {
    check_part(text, KIND_ID_MAX_LEN)
}

pub(crate) fn check_part(text: &str, max_len: usize) -> Result<(), PartError> {
    let mut chars = text.chars();
    match chars.next() {
        None => return Err(PartError::Empty),
        Some(first) if !first.is_ascii_lowercase() => {
            return Err(PartError::FirstChar { found: first });
        }
        Some(_) => {}
    }
    if let Some(found) =
        chars.find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
    {
        return Err(PartError::InvalidChar { found });
    }
    if text.len() > max_len {
        return Err(PartError::TooLong {
            len: text.len(),
            max_len,
        });
    }
    Ok(())
}

/// Why the text is not a [`KindId`] or a part of an [`AddOnId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartError {
    Empty,
    FirstChar { found: char },
    InvalidChar { found: char },
    TooLong { len: usize, max_len: usize },
}

impl fmt::Display for PartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("must not be empty"),
            Self::FirstChar { found } => {
                write!(
                    f,
                    "must start with a lower-case ASCII letter, not {found:?}"
                )
            }
            Self::InvalidChar { found } => write!(
                f,
                "holds only lower-case ASCII letters, digits, and `-`, not {found:?}"
            ),
            Self::TooLong { len, max_len } => write!(
                f,
                "holds at most {max_len} characters, and this one holds {len}"
            ),
        }
    }
}

impl core::error::Error for PartError {}

/// Something that an add-on declares, named across add-ons: the add-on's ID
/// and the kind's ID within it.
///
/// It is written as `{"addon": "deployarr.sql-order-lookup", "kind": "by-identity"}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddOnRef {
    pub addon: AddOnId,
    pub kind: KindId,
}

impl fmt::Display for AddOnRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { addon, kind } = self;
        write!(f, "{addon} {kind}")
    }
}

/// A stored step that an add-on provides.
///
/// The config is the JSON text that the editor saved. The step never parses
/// and writes it again, so its key order and its long numbers survive. The
/// envelope refuses unknown fields, as every built-in action does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddOnStep {
    pub addon: AddOnId,
    /// A step kind that the add-on declares.
    pub kind: KindId,
    /// The version of the config's shape when the step was saved. A saved
    /// version never changes it.
    pub config_version: u32,
    /// The config's JSON text. It keeps its bytes only when the step is
    /// parsed from JSON text, as `TaskSequence::from_json` does. Parsed from
    /// a `serde_json::Value`, it is that value's text, whose keys are sorted
    /// and whose long numbers were already lost when the `Value` was made.
    pub config: Box<RawValue>,
}

impl AddOnStep {
    /// The step kind, named across add-ons.
    pub fn step_kind(&self) -> AddOnRef {
        AddOnRef {
            addon: self.addon.clone(),
            kind: self.kind.clone(),
        }
    }
}

/// Two steps are equal when their config texts are equal byte for byte.
impl PartialEq for AddOnStep {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            addon,
            kind,
            config_version,
            config,
        } = self;
        *addon == other.addon
            && *kind == other.kind
            && *config_version == other.config_version
            && config.get() == other.config.get()
    }
}

impl Eq for AddOnStep {}
