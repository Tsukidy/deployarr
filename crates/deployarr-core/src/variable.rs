use core::fmt;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

use crate::template::Template;
use crate::validated_string::validated_string;

validated_string! {
    /// A task sequence variable name: 1 to 256 ASCII letters, digits, `_`, or `-`.
    ///
    /// Names are case-insensitive, so two names that differ only in case are
    /// equal and hash alike. The spelling the author wrote is kept for display.
    VarName, VarNameError, check_var_name
}

pub const VAR_NAME_MAX_LEN: usize = 256;

fn check_var_name(text: &str) -> Result<(), VarNameError> {
    if text.is_empty() {
        return Err(VarNameError::Empty);
    }
    if text.len() > VAR_NAME_MAX_LEN {
        return Err(VarNameError::TooLong { len: text.len() });
    }
    match text
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-')))
    {
        Some(found) => Err(VarNameError::InvalidChar { found }),
        None => Ok(()),
    }
}

impl PartialEq for VarName {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq_ignore_ascii_case(other.as_str())
    }
}

impl Eq for VarName {}

impl Hash for VarName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for byte in self.as_str().bytes() {
            state.write_u8(byte.to_ascii_lowercase());
        }
        state.write_u8(0xff);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarNameError {
    Empty,
    TooLong { len: usize },
    InvalidChar { found: char },
}

impl fmt::Display for VarNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a variable name must not be empty"),
            Self::TooLong { len } => write!(
                f,
                "a variable name holds at most {VAR_NAME_MAX_LEN} characters, and this one holds {len}"
            ),
            Self::InvalidChar { found } => write!(
                f,
                "a variable name holds only ASCII letters, digits, `_`, and `-`, not {found:?}"
            ),
        }
    }
}

impl core::error::Error for VarNameError {}

/// A variable name that a task sequence can set: a `VarName` that does not
/// start with `_`.
///
/// Names that start with `_` belong to the agent. Conditions and steps can
/// read them, so those take a `VarName`. A `SetVariable` step and a sequence
/// default take this type, so they cannot name one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "VarName")]
pub struct SettableVarName(VarName);

impl SettableVarName {
    pub fn as_var_name(&self) -> &VarName {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<VarName> for SettableVarName {
    type Error = SettableVarNameError;

    fn try_from(name: VarName) -> Result<Self, Self::Error> {
        if name.as_str().starts_with('_') {
            Err(SettableVarNameError::Reserved)
        } else {
            Ok(Self(name))
        }
    }
}

impl FromStr for SettableVarName {
    type Err = SettableVarNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let name: VarName = text.parse().map_err(SettableVarNameError::Invalid)?;
        Self::try_from(name)
    }
}

impl From<SettableVarName> for VarName {
    fn from(name: SettableVarName) -> Self {
        name.0
    }
}

impl fmt::Display for SettableVarName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for SettableVarName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettableVarNameError {
    Invalid(VarNameError),
    Reserved,
}

impl fmt::Display for SettableVarNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(f),
            Self::Reserved => f.write_str(
                "a variable name that starts with `_` belongs to the agent, so a sequence cannot set it",
            ),
        }
    }
}

impl core::error::Error for SettableVarNameError {}

/// One default that a task sequence gives a variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarAssignment {
    pub name: SettableVarName,
    pub value: Template,
}
