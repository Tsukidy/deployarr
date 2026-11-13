//! `%Name%` substitution in a [`Template`].

use alloc::borrow::ToOwned;
use alloc::string::String;
use core::fmt;

use crate::store::{Value, Vars};
use crate::template::Template;
use crate::variable::VarName;

impl Template {
    /// Replaces each `%Name%` with a value, and each `%%` with `%`.
    ///
    /// One pass reads the text from left to right. It never scans a value it
    /// inserted, so a value that holds `%Name%` or `%%` comes out exactly as
    /// stored, and no value can make substitution loop.
    ///
    /// A name is everything between one `%` and the next. A variable in
    /// `vars` answers for it first. A name that `vars` does not hold goes to
    /// `env`, which answers for the process environment, as MDT does. A name
    /// that is not a valid [`VarName`], such as `ProgramFiles(x86)`, goes
    /// straight to `env`. It can hold any character but `%`, including `=`
    /// and NUL, so `env` must answer `None` for a name it cannot look up.
    ///
    /// A variable with an empty value substitutes as empty text. It does not
    /// fall back to `env`.
    ///
    /// # Errors
    ///
    /// - [`SubstituteError::Unterminated`] when a `%` has no closing `%`.
    /// - [`SubstituteError::Undefined`] when neither `vars` nor `env` defines
    ///   a name.
    /// - [`SubstituteError::List`] when a name holds a list, which has no one
    ///   text form.
    pub fn substitute<F>(&self, vars: &Vars, mut env: F) -> Result<String, SubstituteError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let text = self.as_str();
        let mut substituted = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('%') {
            substituted.push_str(&rest[..open]);
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('%') else {
                return Err(SubstituteError::Unterminated {
                    offset: text.len() - rest.len() + open,
                });
            };
            let name = &after_open[..close];
            if name.is_empty() {
                substituted.push('%');
            } else if let Some(var) = name.parse::<VarName>().ok().and_then(|n| vars.get(&n)) {
                match var.value() {
                    Value::Scalar(value) => substituted.push_str(value),
                    Value::List(_) => {
                        return Err(SubstituteError::List {
                            name: var.name().clone(),
                        });
                    }
                }
            } else if let Some(value) = env(name) {
                substituted.push_str(&value);
            } else {
                return Err(SubstituteError::Undefined {
                    name: name.to_owned(),
                });
            }
            rest = &after_open[close + 1..];
        }
        substituted.push_str(rest);
        Ok(substituted)
    }
}

/// Why a [`Template`] could not be substituted.
///
/// No variant holds a value, so an error message cannot leak a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstituteError {
    /// The `%` at this byte offset has no closing `%`.
    Unterminated { offset: usize },
    /// Neither the variables nor the process environment define this name.
    Undefined { name: String },
    /// This variable holds a list.
    List { name: VarName },
}

impl fmt::Display for SubstituteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated { offset } => write!(
                f,
                "the `%` at byte {offset} has no closing `%`. Write `%%` for a literal `%`"
            ),
            Self::Undefined { name } => write!(
                f,
                "`%{name}%` names neither a task sequence variable nor an environment variable"
            ),
            Self::List { name } => write!(
                f,
                "`%{name}%` names a list, and a list cannot be substituted into text"
            ),
        }
    }
}

impl core::error::Error for SubstituteError {}
