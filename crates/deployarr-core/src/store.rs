//! The variable store that a deployment's steps read and set.
//!
//! The store holds each variable's value, whether it is secret, and where the
//! value came from. It does not persist itself. The engine saves it with the
//! rest of the run state.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::sequence::NodeId;
use crate::variable::VarName;

mod redaction;

pub use redaction::{MASK, Redactor};

/// A variable's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    /// A real list, where MDT numbers variables such as `Applications001`.
    List(Vec<String>),
}

impl Value {
    /// Whether the value is an empty scalar or a list with no items.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Scalar(text) => text.is_empty(),
            Self::List(items) => items.is_empty(),
        }
    }

    /// The scalar as a one-item slice, or the list's items.
    fn texts(&self) -> &[String] {
        match self {
            Self::Scalar(text) => core::slice::from_ref(text),
            Self::List(items) => items,
        }
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::Scalar(text.into())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Scalar(text)
    }
}

impl From<Vec<String>> for Value {
    fn from(items: Vec<String>) -> Self {
        Self::List(items)
    }
}

/// Whether a value must stay out of logs and events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sensitivity {
    Plain,
    /// The value never reaches a log line or an event. See [`Vars::redactor`].
    Secret,
}

/// Where a variable's value came from. The UI shows it as a trace for each
/// deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// A fact the agent gathered from the machine, such as its serial number.
    Fact,
    /// A rule.
    Rule,
    /// A column of a data source row.
    DataSource,
    /// A technician's answer to a prompt.
    Prompt,
    /// A step, through `SetVariable` or `deployarr-agent var set`.
    Step(NodeId),
    /// The task sequence's `default_vars`, through [`Vars::apply_default`].
    SequenceDefault,
    /// The agent's own bookkeeping, through [`Vars::set_by_agent`].
    Agent,
}

/// One variable in the store.
///
/// Its `Debug` output leaves out a secret value.
#[derive(Clone, PartialEq, Eq)]
pub struct Var {
    name: VarName,
    value: Value,
    sensitivity: Sensitivity,
    source: Source,
}

impl Var {
    /// The name as the latest write spelled it.
    pub fn name(&self) -> &VarName {
        &self.name
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn is_secret(&self) -> bool {
        self.sensitivity == Sensitivity::Secret
    }

    /// Where the latest write came from.
    pub fn source(&self) -> Source {
        self.source
    }
}

impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("Var");
        debug.field("name", &self.name);
        match self.sensitivity {
            Sensitivity::Plain => debug.field("value", &self.value),
            Sensitivity::Secret => debug.field("value", &format_args!("<secret>")),
        };
        debug
            .field("sensitivity", &self.sensitivity)
            .field("source", &self.source)
            .finish()
    }
}

/// A deployment's task sequence variables.
///
/// Names are case-insensitive, by the same rule as [`VarName`]'s equality.
/// Names that start with `_` belong to the agent. Steps, rules, data sources,
/// prompts, and sequence defaults can read them but not set them. Only
/// [`Vars::set_by_agent`] can.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vars {
    vars: BTreeMap<Key, Var>,
}

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &VarName) -> Option<&Var> {
        self.vars.get(&Key::of(name))
    }

    /// Every variable, in name order without regard to case.
    pub fn iter(&self) -> impl Iterator<Item = &Var> {
        self.vars.values()
    }

    /// Sets a variable, replacing any value it had.
    ///
    /// A secret variable stays secret, whatever `sensitivity` says.
    ///
    /// # Errors
    ///
    /// A name that starts with `_` belongs to the agent. The store refuses it
    /// and keeps the old value.
    pub fn set(
        &mut self,
        name: VarName,
        value: impl Into<Value>,
        sensitivity: Sensitivity,
        source: Source,
    ) -> Result<(), ReadOnlyError> {
        refuse_agent_name(&name)?;
        self.write(name, value.into(), sensitivity, source);
        Ok(())
    }

    /// Sets a variable for the agent. This is the only way to set a name that
    /// starts with `_`. The value's source is [`Source::Agent`].
    pub fn set_by_agent(
        &mut self,
        name: VarName,
        value: impl Into<Value>,
        sensitivity: Sensitivity,
    ) {
        self.write(name, value.into(), sensitivity, Source::Agent);
    }

    /// Applies one of the task sequence's `default_vars`.
    ///
    /// A value that is already set beats the default. An empty value counts as
    /// not set, so the default fills it. Returns whether the default filled
    /// the variable.
    ///
    /// # Errors
    ///
    /// A name that starts with `_` belongs to the agent, so a default cannot
    /// set it.
    pub fn apply_default(
        &mut self,
        name: VarName,
        value: impl Into<Value>,
    ) -> Result<bool, ReadOnlyError> {
        refuse_agent_name(&name)?;
        if self.get(&name).is_some_and(|var| !var.value.is_empty()) {
            return Ok(false);
        }
        self.write(
            name,
            value.into(),
            Sensitivity::Plain,
            Source::SequenceDefault,
        );
        Ok(true)
    }

    /// A [`Redactor`] that masks the value of every secret variable, and each
    /// item of a secret list.
    pub fn redactor(&self) -> Redactor {
        Redactor::new(
            self.vars
                .values()
                .filter(|var| var.is_secret())
                .flat_map(|var| var.value.texts())
                .map(String::as_str),
        )
    }

    fn write(&mut self, name: VarName, value: Value, sensitivity: Sensitivity, source: Source) {
        let key = Key::of(&name);
        let sensitivity = match self.vars.get(&key) {
            Some(old) if old.is_secret() => Sensitivity::Secret,
            _ => sensitivity,
        };
        self.vars.insert(
            key,
            Var {
                name,
                value,
                sensitivity,
                source,
            },
        );
    }
}

/// A variable name folded to lower case.
///
/// [`VarName`] holds only ASCII and compares with `eq_ignore_ascii_case`, so
/// two names are equal exactly when their keys are.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key(String);

impl Key {
    fn of(name: &VarName) -> Self {
        Self(name.as_str().to_ascii_lowercase())
    }
}

fn refuse_agent_name(name: &VarName) -> Result<(), ReadOnlyError> {
    if name.as_str().starts_with('_') {
        Err(ReadOnlyError { name: name.clone() })
    } else {
        Ok(())
    }
}

/// A write to a name that starts with `_` from anything but the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlyError {
    name: VarName,
}

impl ReadOnlyError {
    pub fn name(&self) -> &VarName {
        &self.name
    }
}

impl fmt::Display for ReadOnlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "only the agent can set `{}`, because a name that starts with `_` belongs to the agent",
            self.name
        )
    }
}

impl core::error::Error for ReadOnlyError {}
