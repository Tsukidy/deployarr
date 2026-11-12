use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::action::Action;
use crate::condition::Condition;
use crate::validated_string::validated_string;
use crate::variable::VarAssignment;

/// The JSON format this build reads and writes, as the top-level `schema_version`.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSequence {
    pub id: TaskSequenceId,
    /// Every save creates a new version.
    pub version: u32,
    pub name: String,
    pub default_vars: Vec<VarAssignment>,
    pub root: Group,
}

impl TaskSequence {
    /// Parses a task sequence document. The error names the field that failed.
    pub fn from_json(json: &str) -> Result<Self, ParseError> {
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let sequence =
            serde_path_to_error::deserialize(&mut deserializer).map_err(|error| ParseError {
                path: error.path().to_string(),
                message: error.inner().to_string(),
            })?;
        deserializer.end().map_err(|error| ParseError {
            path: ".".to_owned(),
            message: error.to_string(),
        })?;
        Ok(sequence)
    }
}

/// Why a task sequence document did not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    path: String,
    message: String,
}

impl ParseError {
    /// The field that failed, such as `root.children[2].step.action.run_command.exit`.
    /// It is `.` when the document itself is at fault.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { path, message } = self;
        write!(f, "{path}: {message}")
    }
}

impl std::error::Error for ParseError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: SchemaVersion,
    id: TaskSequenceId,
    version: u32,
    name: String,
    default_vars: Vec<VarAssignment>,
    root: Group,
}

#[derive(Serialize)]
struct DocumentRef<'a> {
    schema_version: SchemaVersion,
    id: &'a TaskSequenceId,
    version: u32,
    name: &'a str,
    default_vars: &'a [VarAssignment],
    root: &'a Group,
}

impl<'de> Deserialize<'de> for TaskSequence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Document {
            schema_version: SchemaVersion,
            id,
            version,
            name,
            default_vars,
            root,
        } = Document::deserialize(deserializer)?;
        Ok(Self {
            id,
            version,
            name,
            default_vars,
            root,
        })
    }
}

impl Serialize for TaskSequence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Self {
            id,
            version,
            name,
            default_vars,
            root,
        } = self;
        DocumentRef {
            schema_version: SchemaVersion,
            id,
            version: *version,
            name,
            default_vars,
            root,
        }
        .serialize(serializer)
    }
}

struct SchemaVersion;

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let found = u32::deserialize(deserializer)?;
        if found == SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format_args!(
                "this build reads schema version {SCHEMA_VERSION}, and the document has {found}"
            )))
        }
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(SCHEMA_VERSION)
    }
}

validated_string! {
    /// A task sequence ID, such as `W11-PRO-STD`: one or more of `A` to `Z`,
    /// digits, `_`, or `-`.
    #[derive(PartialEq, Eq, Hash)]
    TaskSequenceId, TaskSequenceIdError, check_task_sequence_id
}

fn check_task_sequence_id(text: &str) -> Result<(), TaskSequenceIdError> {
    if text.is_empty() {
        return Err(TaskSequenceIdError::Empty);
    }
    match text
        .chars()
        .find(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '_' | '-')))
    {
        Some(found) => Err(TaskSequenceIdError::InvalidChar { found }),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSequenceIdError {
    Empty,
    InvalidChar { found: char },
}

impl fmt::Display for TaskSequenceIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a task sequence ID must not be empty"),
            Self::InvalidChar { found } => write!(
                f,
                "a task sequence ID holds only upper-case ASCII letters, digits, `_`, and `-`, not {found:?}"
            ),
        }
    }
}

impl std::error::Error for TaskSequenceIdError {}

/// A node's stable ID. It survives a rename or a move.
///
/// The crate parses and writes IDs and never generates one, because random
/// numbers need platform support that `wasm32-unknown-unknown` lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Group(Group),
    Step(Step),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMeta {
    pub id: NodeId,
    pub name: String,
    pub enabled: bool,
    pub continue_on_error: bool,
    pub condition: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub meta: NodeMeta,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub meta: NodeMeta,
    /// The compiler checks this against the action when the sequence is saved.
    pub run_in: RunIn,
    pub action: Action,
}

/// The environment a step runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunIn {
    WinPe,
    FullOs,
    Either,
}
