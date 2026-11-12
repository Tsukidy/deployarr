use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::template::Template;
use crate::variable::VarName;

/// A test that decides whether a group or a step runs.
///
/// An empty `All` or `Any` parses. The compiler rejects it when the sequence
/// is saved, so that the error can name the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    Var {
        name: VarName,
        op: CompareOp,
        value: Template,
    },
    Registry {
        key: Template,
        value: Option<String>,
        op: CompareOp,
        data: Option<RegData>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Exists,
    NotExists,
    Equals,
    NotEquals,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Like,
}

/// An operator and the value it compares the found value against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison<T> {
    pub op: CompareOp,
    pub value: T,
}

/// Registry data to compare against. JSON writes it as a bare string or integer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, expecting = "registry data is a string or an integer")]
pub enum RegData {
    String(String),
    Integer(i64),
}

/// A Windows file version. JSON writes it as `major.minor.build.revision`.
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

impl std::error::Error for FileVersionError {}

/// A point in time. JSON writes it in RFC 3339, such as `2026-01-31T08:00:00Z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(#[serde(with = "time::serde::rfc3339")] pub OffsetDateTime);

/// The GUID that identifies an installed MSI product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsiProduct {
    ProductCode(Uuid),
    UpgradeCode(Uuid),
}
