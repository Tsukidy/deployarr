use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::action::Action;
use crate::condition::Condition;
use crate::validated_string::validated_string;
use crate::variable::{SettableVarName, VarAssignment};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSequence {
    pub id: TaskSequenceId,
    pub version: u32,
    pub name: String,
    pub default_vars: Vec<VarAssignment>,
    pub root: Group,
}

impl TaskSequence {
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

impl core::error::Error for ParseError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: SchemaVersion,
    id: TaskSequenceId,
    version: u32,
    name: String,
    default_vars: DefaultVars,
    root: Group,
}

/// `default_vars` as written. Two entries that name the same variable, in any
/// case, fail, and the error path names the second entry's name, such as
/// `default_vars[1].name`.
struct DefaultVars(Vec<VarAssignment>);

impl<'de> Deserialize<'de> for DefaultVars {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(DefaultVarsVisitor).map(Self)
    }
}

struct DefaultVarsVisitor;

impl<'de> Visitor<'de> for DefaultVarsVisitor {
    type Value = Vec<VarAssignment>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of variable defaults")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut defaults = Vec::new();
        let mut seen = BTreeMap::new();
        while let Some(default) = seq.next_element_seed(DefaultVar {
            earlier: &defaults,
            seen: &seen,
        })? {
            seen.insert(name_key(&default.name), defaults.len());
            defaults.push(default);
        }
        Ok(defaults)
    }
}

/// The key under which `DefaultVarsVisitor` records a name. A `VarName` holds
/// only ASCII, so two names are equal exactly when their keys are.
fn name_key(name: &SettableVarName) -> String {
    name.as_str().to_ascii_lowercase()
}

/// One entry of `default_vars`, read with the entries before it. `seen` maps
/// each earlier name's key to its index in `earlier`.
struct DefaultVar<'a> {
    earlier: &'a [VarAssignment],
    seen: &'a BTreeMap<String, usize>,
}

const DEFAULT_VAR_FIELDS: &[&str] = &["name", "value"];

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum DefaultVarField {
    Name,
    Value,
}

impl<'de> DeserializeSeed<'de> for DefaultVar<'_> {
    type Value = VarAssignment;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_struct("VarAssignment", DEFAULT_VAR_FIELDS, self)
    }
}

impl<'de> Visitor<'de> for DefaultVar<'_> {
    type Value = VarAssignment;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a variable default, an object with `name` and `value`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut name = None;
        let mut value = None;
        while let Some(field) = map.next_key()? {
            match field {
                DefaultVarField::Name => {
                    if name.is_some() {
                        return Err(de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value_seed(UniqueName {
                        earlier: self.earlier,
                        seen: self.seen,
                    })?);
                }
                DefaultVarField::Value => {
                    if value.is_some() {
                        return Err(de::Error::duplicate_field("value"));
                    }
                    value = Some(map.next_value()?);
                }
            }
        }
        Ok(VarAssignment {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            value: value.ok_or_else(|| de::Error::missing_field("value"))?,
        })
    }
}

/// A default's name, which must differ from every earlier default's name.
struct UniqueName<'a> {
    earlier: &'a [VarAssignment],
    seen: &'a BTreeMap<String, usize>,
}

impl<'de> DeserializeSeed<'de> for UniqueName<'_> {
    type Value = SettableVarName;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let name = SettableVarName::deserialize(deserializer)?;
        match self.seen.get(&name_key(&name)) {
            Some(&index) => Err(de::Error::custom(format_args!(
                "default_vars[{index}] already names this variable, as `{}`, and variable names ignore case",
                self.earlier[index].name
            ))),
            None => Ok(name),
        }
    }
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
            default_vars: default_vars.0,
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

impl core::error::Error for TaskSequenceIdError {}

/// A node's stable ID. It survives a rename or a move.
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
    pub run_in: RunIn,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunIn {
    WinPe,
    FullOs,
    Either,
}
