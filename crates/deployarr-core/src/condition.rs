use alloc::boxed::Box;
use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::non_empty::NonEmpty;
use crate::rfc3339;
use crate::template::Template;
use crate::variable::VarName;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    All(NonEmpty<Condition>),
    Any(NonEmpty<Condition>),
    Not(Box<Condition>),
    Var {
        name: VarName,
        test: Test<NonEmptyTemplate>,
    },
    /// Reads a `REG_DWORD`, `REG_QWORD`, `REG_SZ`, or `REG_EXPAND_SZ` value.
    Registry {
        key: Template,
        value: Option<String>,
        test: Test<RegData>,
    },
    File {
        path: Template,
        version: Option<Comparison<FileVersion>>,
        modified: Option<Comparison<Timestamp>>,
    },
    Folder {
        path: Template,
        modified: Option<Comparison<Timestamp>>,
    },
    InstalledMsi(MsiProduct),
    Wmi {
        namespace: String,
        query: Template,
        ignore_failure: bool,
    },
}

/// An operator together with the operand it compares against.
///
/// `Exists` and `NotExists` take no operand, and every other operator takes
/// one, so a test can neither lack the operand it needs nor carry one that it
/// ignores. The JSON is `"exists"`, or an object with one key such as
/// `{"equals": "true"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Test<T> {
    Exists,
    NotExists,
    Equals(T),
    NotEquals(T),
    Less(T),
    LessEqual(T),
    Greater(T),
    GreaterEqual(T),
    Like(T),
}

/// The operators that compare a file version or a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Equals,
    NotEquals,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison<T> {
    pub op: CompareOp,
    pub value: T,
}

/// The text that a `Var` condition compares against. It is a `Template` that
/// is not empty, because `not_exists` is the test for an empty variable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "Template")]
pub struct NonEmptyTemplate(Template);

impl NonEmptyTemplate {
    pub fn as_template(&self) -> &Template {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<Template> for NonEmptyTemplate {
    type Error = EmptyTemplateError;

    fn try_from(text: Template) -> Result<Self, Self::Error> {
        if text.as_str().is_empty() {
            Err(EmptyTemplateError)
        } else {
            Ok(Self(text))
        }
    }
}

impl From<NonEmptyTemplate> for Template {
    fn from(text: NonEmptyTemplate) -> Self {
        text.0
    }
}

impl Serialize for NonEmptyTemplate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyTemplateError;

impl fmt::Display for EmptyTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the text to compare against must not be empty. `not_exists` tests for an empty variable",
        )
    }
}

impl core::error::Error for EmptyTemplateError {}

/// The data that a `Registry` condition compares against. `Integer` holds
/// every `REG_DWORD` and `REG_QWORD` value, so a negative number fails to parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    untagged,
    expecting = "registry data is a string or an integer from 0 to 18446744073709551615"
)]
pub enum RegData {
    String(String),
    Integer(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FileVersion {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
}

impl FromStr for FileVersion {
    type Err = FileVersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split('.').map(|part| {
            // `u16::from_str` accepts a leading `+`, which no file version has.
            if part.starts_with('+') {
                return None;
            }
            part.parse::<u16>().ok()
        });
        let mut next = || parts.next().flatten().ok_or(FileVersionError);
        let version = Self {
            major: next()?,
            minor: next()?,
            build: next()?,
            revision: next()?,
        };
        match parts.next() {
            Some(_) => Err(FileVersionError),
            None => Ok(version),
        }
    }
}

impl TryFrom<String> for FileVersion {
    type Error = FileVersionError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for FileVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            major,
            minor,
            build,
            revision,
        } = self;
        write!(f, "{major}.{minor}.{build}.{revision}")
    }
}

impl From<FileVersion> for String {
    fn from(version: FileVersion) -> Self {
        version.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileVersionError;

impl fmt::Display for FileVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a file version is four numbers from 0 to 65535, written as `a.b.c.d`")
    }
}

impl core::error::Error for FileVersionError {}

/// A moment with a UTC offset, written in RFC 3339, such as `2026-01-31T08:00:00Z`.
///
/// The JSON text is the text that `time`'s `Rfc3339` format writes. Writing
/// fails for a year outside 0 to 9999, an offset of 24 hours or more, or an
/// offset that is not a whole number of minutes. Parsing never yields one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub OffsetDateTime);

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = rfc3339::format(self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&text)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        rfc3339::deserialize(deserializer).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsiProduct {
    ProductCode(Uuid),
    UpgradeCode(Uuid),
}
