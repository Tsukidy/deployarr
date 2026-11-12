use core::fmt;
use core::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

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

/// One default that a task sequence gives a variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VarAssignment {
    pub name: VarName,
    pub value: Template,
}
