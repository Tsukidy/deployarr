//! The form language. One [`FormSchema`] describes an add-on's settings, a
//! step kind's config, a join kind's profile, or an action's input. The web UI
//! renders it, the server checks documents with it, and an add-on package
//! ships it as a file.
//!
//! A field is one flat table on the wire: `key`, `label`, `help`, `required`,
//! `kind`, and the keys of that kind, such as `pattern` for `text`:
//!
//! ```toml
//! [[field]]
//! key = "order_no"
//! label = "Order number"
//! kind = "text"
//! pattern = "[A-Z0-9-]{1,40}"
//! ```
//!
//! It parses through a private struct that lists every key of every kind and
//! refuses any other key. A conversion then gives each kind only its own keys,
//! and refuses a key that belongs to another kind. A derived tagged enum with
//! `deny_unknown_fields` accepted `allow_write = true` on a `sql_text` field,
//! because it refuses unknown keys only on the kinds that have keys of their
//! own.

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::num::{NonZeroU32, NonZeroU64};

use regex_automata::meta::Regex;
use regex_syntax::hir::{Hir, Look};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::addon::KindId;
use crate::non_empty::NonEmpty;
use crate::sequence::{TaskSequenceId, TaskSequenceIdError};
use crate::validated_string::validated_string;

/// The longest [`FieldKey`]. It is a default until the owner decides the form
/// language, one of the add-on one-way doors.
pub const FIELD_KEY_MAX_LEN: usize = 63;

validated_string! {
    /// The key that a field's value has in a document, such as `order_no`:
    /// 1 to 63 characters, a lower-case ASCII letter, then lower-case ASCII
    /// letters, digits, or `_`.
    ///
    /// The key names the value in a settings path, such as
    /// `rules/0/statement`, so it holds no `/`.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    FieldKey, FieldKeyError, check_field_key
}

fn check_field_key(text: &str) -> Result<(), FieldKeyError> {
    let mut chars = text.chars();
    match chars.next() {
        None => return Err(FieldKeyError::Empty),
        Some(first) if !first.is_ascii_lowercase() => {
            return Err(FieldKeyError::FirstChar { found: first });
        }
        Some(_) => {}
    }
    if let Some(found) =
        chars.find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
    {
        return Err(FieldKeyError::InvalidChar { found });
    }
    if text.len() > FIELD_KEY_MAX_LEN {
        return Err(FieldKeyError::TooLong { len: text.len() });
    }
    Ok(())
}

/// Why the text is not a [`FieldKey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKeyError {
    Empty,
    FirstChar { found: char },
    InvalidChar { found: char },
    TooLong { len: usize },
}

impl fmt::Display for FieldKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a field key must not be empty"),
            Self::FirstChar { found } => write!(
                f,
                "a field key must start with a lower-case ASCII letter, not {found:?}"
            ),
            Self::InvalidChar { found } => write!(
                f,
                "a field key holds only lower-case ASCII letters, digits, and `_`, not {found:?}"
            ),
            Self::TooLong { len } => write!(
                f,
                "a field key holds at most {FIELD_KEY_MAX_LEN} characters, and this one holds {len}"
            ),
        }
    }
}

impl core::error::Error for FieldKeyError {}

/// A pattern that a whole text value must match, such as `[A-Z0-9-]{1,40}`.
///
/// The syntax is that of the Rust `regex` crate. It has no look-around and no
/// back-references, so a match takes time linear in the text, whoever wrote
/// the pattern. The pattern must match the whole value, as the `pattern`
/// attribute of an HTML form field must. `^` and `$` are allowed and change
/// nothing.
#[derive(Clone)]
pub struct Pattern {
    source: String,
    regex: Regex,
}

impl Pattern {
    pub fn new(source: &str) -> Result<Self, PatternError> {
        let hir = regex_automata::util::syntax::parse(source)
            .map_err(|error| PatternError(error.to_string()))?;
        // Anchoring the parsed pattern, and not its text, keeps a flag or a
        // comment in the text from changing what the anchors apply to.
        let whole = Hir::concat(alloc::vec![
            Hir::look(Look::Start),
            hir,
            Hir::look(Look::End),
        ]);
        let regex = Regex::builder()
            .build_from_hir(&whole)
            .map_err(|error| PatternError(error.to_string()))?;
        Ok(Self {
            source: source.to_owned(),
            regex,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Whether the pattern matches the whole text.
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }
}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Pattern").field(&self.source).finish()
    }
}

/// Two patterns are equal when their texts are.
impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for Pattern {}

impl Serialize for Pattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::new(&source).map_err(serde::de::Error::custom)
    }
}

/// Why the text is not a [`Pattern`]. It holds the regex parser's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError(String);

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the pattern is not a valid regular expression: {}",
            self.0
        )
    }
}

impl core::error::Error for PatternError {}

/// The fields of one form. No two fields have the same key.
///
/// It is written as a table whose `field` key holds the fields, so a schema
/// file in TOML holds one `[[field]]` table per field. A form with no fields
/// is `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormSchema(Vec<Field>);

impl FormSchema {
    /// The fields in order, or the first key that two fields share.
    pub fn new(fields: Vec<Field>) -> Result<Self, DuplicateKey> {
        for (index, field) in fields.iter().enumerate() {
            if fields[..index].iter().any(|seen| seen.key == field.key) {
                return Err(DuplicateKey(field.key.clone()));
            }
        }
        Ok(Self(fields))
    }

    pub fn fields(&self) -> &[Field] {
        &self.0
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.0.iter().find(|field| field.key.as_str() == key)
    }

    /// Checks a document, such as a step's config, against the schema. Pure.
    ///
    /// The document is a JSON object with one key for each field that has a
    /// value. It may leave out a field that is not required. Any other key is
    /// an error. Each error names its value with a JSON pointer, such as
    /// `/rules/0/statement`. In each table, the unknown keys come first, in
    /// key order, and then the fields' problems, in the schema's order.
    ///
    /// A secret or secret file field holds `{"set": true}` or
    /// `{"set": false}` in a document, never the secret. The server keeps the
    /// secret with its key, and the web UI and the add-on see only whether it
    /// is set.
    pub fn check(&self, document: &Value) -> Result<(), ValueErrors> {
        let mut errors = Vec::new();
        check_object(&self.0, document, &mut String::new(), &mut errors);
        match NonEmpty::try_from(errors) {
            Ok(errors) => Err(ValueErrors(errors)),
            Err(_) => Ok(()),
        }
    }
}

impl Serialize for FormSchema {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SchemaWire {
            field: self.0.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FormSchema {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let SchemaWire { field } = SchemaWire::deserialize(deserializer)?;
        Self::new(field).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    field: Vec<Field>,
}

/// Two fields of one form, or of one list item, have this key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKey(pub FieldKey);

impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "two fields have the key `{}`", self.0)
    }
}

impl core::error::Error for DuplicateKey {}

/// One field of a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FieldWire", into = "FieldWire")]
pub struct Field {
    pub key: FieldKey,
    pub label: String,
    pub help: Option<String>,
    /// The document must hold a value for the field, and a text value must
    /// not be empty. A required secret must be set, and a required list must
    /// hold at least one item.
    pub required: bool,
    pub kind: FieldKind,
}

/// What a field holds, with the keys of its kind.
///
/// API level 1 freezes this list. A later level can add a kind, and no level
/// changes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Text, at most `max_len` characters, that matches `pattern`.
    Text {
        pattern: Option<Pattern>,
        max_len: Option<NonZeroU32>,
    },
    /// A whole number from `min` to `max`.
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
    Bool,
    /// One of `options`.
    Choice {
        options: NonEmpty<String>,
    },
    /// Text with `%Name%` variable references, which the PC fills in.
    Template,
    /// SQL with named parameters. The value belongs to the admin. A settings
    /// migration cannot change one, add one, or move one.
    SqlText,
    /// A secret that the server stores with its key. The web UI and the
    /// add-on see only whether it is set.
    Secret,
    /// A secret file, such as a bulk enrollment package, of at most
    /// `max_bytes` bytes. Only a built-in action that takes a secret can use
    /// it.
    SecretFile {
        max_bytes: NonZeroU64,
    },
    /// The name of one instance of a connection that the add-on declares.
    ConnectionRef {
        connection: KindId,
    },
    /// A task sequence ID. The server warns before that task sequence is
    /// deleted.
    TaskSequenceRef,
    /// A list of items, each a table of the `item` fields.
    List {
        item: FormSchema,
    },
}

impl FieldKind {
    /// The kind's name on the wire, such as `sql_text`.
    pub fn name(&self) -> &'static str {
        self.kind_name().as_str()
    }

    fn kind_name(&self) -> KindName {
        match self {
            Self::Text { .. } => KindName::Text,
            Self::Integer { .. } => KindName::Integer,
            Self::Bool => KindName::Bool,
            Self::Choice { .. } => KindName::Choice,
            Self::Template => KindName::Template,
            Self::SqlText => KindName::SqlText,
            Self::Secret => KindName::Secret,
            Self::SecretFile { .. } => KindName::SecretFile,
            Self::ConnectionRef { .. } => KindName::ConnectionRef,
            Self::TaskSequenceRef => KindName::TaskSequenceRef,
            Self::List { .. } => KindName::List,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KindName {
    Text,
    Integer,
    Bool,
    Choice,
    Template,
    SqlText,
    Secret,
    SecretFile,
    ConnectionRef,
    TaskSequenceRef,
    List,
}

impl KindName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Bool => "bool",
            Self::Choice => "choice",
            Self::Template => "template",
            Self::SqlText => "sql_text",
            Self::Secret => "secret",
            Self::SecretFile => "secret_file",
            Self::ConnectionRef => "connection_ref",
            Self::TaskSequenceRef => "task_sequence_ref",
            Self::List => "list",
        }
    }
}

/// A field as it is written: the common keys, and every key of every kind.
/// [`Field`]'s `TryFrom` gives each kind only its own keys.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldWire {
    key: FieldKey,
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    required: bool,
    kind: KindName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_len: Option<NonZeroU32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    options: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_bytes: Option<NonZeroU64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connection: Option<KindId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item: Option<Vec<Field>>,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl TryFrom<FieldWire> for Field {
    type Error = FieldError;

    fn try_from(mut wire: FieldWire) -> Result<Self, FieldError> {
        let key = wire.key.clone();
        let kind = wire.kind;
        let error = |problem| FieldError {
            key: key.clone(),
            kind: kind.as_str(),
            problem,
        };
        let field_kind = match kind {
            KindName::Text => {
                let pattern = match wire.pattern.take() {
                    Some(source) => {
                        Some(Pattern::new(&source).map_err(|e| error(FieldProblem::Pattern(e)))?)
                    }
                    None => None,
                };
                FieldKind::Text {
                    pattern,
                    max_len: wire.max_len.take(),
                }
            }
            KindName::Integer => {
                let (min, max) = (wire.min.take(), wire.max.take());
                if let (Some(min), Some(max)) = (min, max)
                    && min > max
                {
                    return Err(error(FieldProblem::MinAboveMax { min, max }));
                }
                FieldKind::Integer { min, max }
            }
            KindName::Bool => FieldKind::Bool,
            KindName::Choice => {
                let options = wire
                    .options
                    .take()
                    .ok_or_else(|| error(FieldProblem::Missing("options")))?;
                for (index, option) in options.iter().enumerate() {
                    if option.is_empty() {
                        return Err(error(FieldProblem::EmptyOption));
                    }
                    if options[..index].contains(option) {
                        return Err(error(FieldProblem::DuplicateOption(option.clone())));
                    }
                }
                let options =
                    NonEmpty::try_from(options).map_err(|_| error(FieldProblem::NoOptions))?;
                FieldKind::Choice { options }
            }
            KindName::Template => FieldKind::Template,
            KindName::SqlText => FieldKind::SqlText,
            KindName::Secret => FieldKind::Secret,
            KindName::SecretFile => FieldKind::SecretFile {
                max_bytes: wire
                    .max_bytes
                    .take()
                    .ok_or_else(|| error(FieldProblem::Missing("max_bytes")))?,
            },
            KindName::ConnectionRef => FieldKind::ConnectionRef {
                connection: wire
                    .connection
                    .take()
                    .ok_or_else(|| error(FieldProblem::Missing("connection")))?,
            },
            KindName::TaskSequenceRef => FieldKind::TaskSequenceRef,
            KindName::List => {
                let item = wire
                    .item
                    .take()
                    .ok_or_else(|| error(FieldProblem::Missing("item")))?;
                if item.is_empty() {
                    return Err(error(FieldProblem::EmptyItem));
                }
                let item = FormSchema::new(item)
                    .map_err(|DuplicateKey(key)| error(FieldProblem::DuplicateItemKey(key)))?;
                FieldKind::List { item }
            }
        };
        // Whatever the kind did not take belongs to another kind.
        let FieldWire {
            pattern,
            max_len,
            min,
            max,
            options,
            max_bytes,
            connection,
            item,
            ..
        } = &wire;
        let left = [
            ("pattern", pattern.is_some()),
            ("max_len", max_len.is_some()),
            ("min", min.is_some()),
            ("max", max.is_some()),
            ("options", options.is_some()),
            ("max_bytes", max_bytes.is_some()),
            ("connection", connection.is_some()),
            ("item", item.is_some()),
        ];
        if let Some((name, _)) = left.into_iter().find(|(_, set)| *set) {
            return Err(error(FieldProblem::OtherKindsKey(name)));
        }
        let FieldWire {
            key,
            label,
            help,
            required,
            ..
        } = wire;
        Ok(Self {
            key,
            label,
            help,
            required,
            kind: field_kind,
        })
    }
}

impl From<Field> for FieldWire {
    fn from(field: Field) -> Self {
        let Field {
            key,
            label,
            help,
            required,
            kind,
        } = field;
        let mut wire = Self {
            key,
            label,
            help,
            required,
            kind: kind.kind_name(),
            pattern: None,
            max_len: None,
            min: None,
            max: None,
            options: None,
            max_bytes: None,
            connection: None,
            item: None,
        };
        match kind {
            FieldKind::Text { pattern, max_len } => {
                wire.pattern = pattern.map(|pattern| pattern.source);
                wire.max_len = max_len;
            }
            FieldKind::Integer { min, max } => {
                wire.min = min;
                wire.max = max;
            }
            FieldKind::Choice { options } => wire.options = Some(options.into_vec()),
            FieldKind::SecretFile { max_bytes } => wire.max_bytes = Some(max_bytes),
            FieldKind::ConnectionRef { connection } => wire.connection = Some(connection),
            FieldKind::List { item } => wire.item = Some(item.0),
            FieldKind::Bool
            | FieldKind::Template
            | FieldKind::SqlText
            | FieldKind::Secret
            | FieldKind::TaskSequenceRef => {}
        }
        wire
    }
}

/// Why a field's table does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub key: FieldKey,
    /// The kind's name, such as `sql_text`.
    pub kind: &'static str,
    pub problem: FieldProblem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldProblem {
    /// The kind needs this key.
    Missing(&'static str),
    /// This key belongs to another kind.
    OtherKindsKey(&'static str),
    Pattern(PatternError),
    MinAboveMax {
        min: i64,
        max: i64,
    },
    NoOptions,
    EmptyOption,
    DuplicateOption(String),
    EmptyItem,
    DuplicateItemKey(FieldKey),
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { key, kind, problem } = self;
        write!(f, "field `{key}` of kind `{kind}`: ")?;
        match problem {
            FieldProblem::Missing(name) => write!(f, "the kind needs `{name}`"),
            FieldProblem::OtherKindsKey(name) => {
                write!(f, "`{name}` belongs to another kind")
            }
            FieldProblem::Pattern(error) => write!(f, "`pattern`: {error}"),
            FieldProblem::MinAboveMax { min, max } => {
                write!(f, "`min` {min} is greater than `max` {max}")
            }
            FieldProblem::NoOptions => f.write_str("`options` must hold at least one option"),
            FieldProblem::EmptyOption => f.write_str("`options` holds an empty option"),
            FieldProblem::DuplicateOption(option) => {
                write!(f, "`options` holds {option:?} twice")
            }
            FieldProblem::EmptyItem => f.write_str("`item` must hold at least one field"),
            FieldProblem::DuplicateItemKey(item_key) => {
                write!(f, "two fields of `item` have the key `{item_key}`")
            }
        }
    }
}

impl core::error::Error for FieldError {}

// Checking documents.

fn check_object(fields: &[Field], value: &Value, path: &mut String, errors: &mut Vec<ValueError>) {
    let Value::Object(object) = value else {
        errors.push(ValueError::new(path, ValueProblem::NotAnObject));
        return;
    };
    for key in object.keys() {
        if !fields.iter().any(|field| field.key.as_str() == key) {
            let mut at = path.clone();
            push_segment(&mut at, key);
            errors.push(ValueError::new(&at, ValueProblem::UnknownKey));
        }
    }
    for field in fields {
        let length = path.len();
        push_segment(path, field.key.as_str());
        check_field(field, object, path, errors);
        path.truncate(length);
    }
}

fn check_field(
    field: &Field,
    object: &Map<String, Value>,
    path: &mut String,
    errors: &mut Vec<ValueError>,
) {
    let Some(value) = object.get(field.key.as_str()) else {
        if field.required {
            errors.push(ValueError::new(path, ValueProblem::Missing));
        }
        return;
    };
    let mut problem = |problem| errors.push(ValueError::new(path, problem));
    match &field.kind {
        FieldKind::Text { pattern, max_len } => {
            let Some(text) = text(value, field, &mut problem) else {
                return;
            };
            let len = text.chars().count();
            if let Some(max_len) = *max_len
                && len > max_len.get() as usize
            {
                problem(ValueProblem::TooLong {
                    len,
                    max_len: max_len.get(),
                });
            }
            if let Some(pattern) = pattern
                && !pattern.is_match(text)
            {
                problem(ValueProblem::NoMatch {
                    pattern: pattern.as_str().to_owned(),
                });
            }
        }
        FieldKind::Integer { min, max } => {
            let Some(number) = value.as_i64() else {
                problem(ValueProblem::WrongType {
                    expected: "a whole number from -2^63 to 2^63 - 1",
                });
                return;
            };
            if let Some(min) = *min
                && number < min
            {
                problem(ValueProblem::BelowMin { min });
            }
            if let Some(max) = *max
                && number > max
            {
                problem(ValueProblem::AboveMax { max });
            }
        }
        FieldKind::Bool => {
            if !value.is_boolean() {
                problem(ValueProblem::WrongType {
                    expected: "true or false",
                });
            }
        }
        FieldKind::Choice { options } => {
            let Some(text) = value.as_str() else {
                problem(ValueProblem::WrongType {
                    expected: "one of the options",
                });
                return;
            };
            if !options.iter().any(|option| option == text) {
                problem(ValueProblem::NotAnOption);
            }
        }
        FieldKind::Template => {
            if let Some(text) = text(value, field, &mut problem)
                && let Some(offset) = unterminated(text)
            {
                problem(ValueProblem::Unterminated { offset });
            }
        }
        FieldKind::SqlText | FieldKind::ConnectionRef { .. } => {
            text(value, field, &mut problem);
        }
        FieldKind::Secret | FieldKind::SecretFile { .. } => match secret_set(value) {
            Some(false) if field.required => problem(ValueProblem::Missing),
            Some(_) => {}
            None => problem(ValueProblem::NotASecretMark),
        },
        FieldKind::TaskSequenceRef => {
            if let Some(text) = value.as_str() {
                if let Err(error) = text.parse::<TaskSequenceId>() {
                    problem(ValueProblem::TaskSequenceId(error));
                }
            } else {
                problem(ValueProblem::WrongType {
                    expected: "a task sequence ID",
                });
            }
        }
        FieldKind::List { item } => {
            let Some(items) = value.as_array() else {
                problem(ValueProblem::WrongType { expected: "a list" });
                return;
            };
            if field.required && items.is_empty() {
                problem(ValueProblem::Empty);
            }
            for (index, entry) in items.iter().enumerate() {
                let length = path.len();
                push_segment(path, &index.to_string());
                check_object(&item.0, entry, path, errors);
                path.truncate(length);
            }
        }
    }
}

/// The value as text, or `None` after it reports why it is not text that the
/// field accepts.
fn text<'v>(
    value: &'v Value,
    field: &Field,
    problem: &mut impl FnMut(ValueProblem),
) -> Option<&'v str> {
    let Some(text) = value.as_str() else {
        problem(ValueProblem::WrongType { expected: "text" });
        return None;
    };
    if field.required && text.is_empty() {
        problem(ValueProblem::Empty);
        return None;
    }
    Some(text)
}

/// Whether a secret's mark says that it is set, or `None` when the value is
/// not a mark.
fn secret_set(value: &Value) -> Option<bool> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.get("set")?.as_bool()
}

/// The byte offset of a `%` that has no closing `%`, read as
/// [`crate::template::Template::substitute`] reads it: each `%` closes the one
/// before it.
fn unterminated(text: &str) -> Option<usize> {
    let mut open = None;
    for (offset, _) in text.match_indices('%') {
        open = match open {
            None => Some(offset),
            Some(_) => None,
        };
    }
    open
}

fn push_segment(path: &mut String, segment: &str) {
    path.push('/');
    path.push_str(segment);
}

/// The problems that [`FormSchema::check`] found in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueErrors(NonEmpty<ValueError>);

impl ValueErrors {
    pub fn as_slice(&self) -> &[ValueError] {
        &self.0
    }
}

impl fmt::Display for ValueErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl core::error::Error for ValueErrors {}

/// One problem in a document. No variant holds the value, so a message
/// cannot leak a secret that was written where it does not belong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueError {
    /// A JSON pointer to the value, such as `/rules/0/statement`. It is empty
    /// for the document itself.
    pub path: String,
    pub problem: ValueProblem,
}

impl ValueError {
    fn new(path: &str, problem: ValueProblem) -> Self {
        Self {
            path: path.to_owned(),
            problem,
        }
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { path, problem } = self;
        if path.is_empty() {
            write!(f, "the document: {problem}")
        } else {
            write!(f, "`{path}`: {problem}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueProblem {
    /// The document, or a list item, is not a table.
    NotAnObject,
    /// No field has this key.
    UnknownKey,
    /// A required field has no value, or a required secret is not set.
    Missing,
    /// A required text field holds empty text, or a required list holds no
    /// items.
    Empty,
    WrongType {
        expected: &'static str,
    },
    TooLong {
        len: usize,
        max_len: u32,
    },
    NoMatch {
        pattern: String,
    },
    BelowMin {
        min: i64,
    },
    AboveMax {
        max: i64,
    },
    NotAnOption,
    /// The `%` at this byte offset of a template has no closing `%`.
    Unterminated {
        offset: usize,
    },
    /// A secret or secret file field holds something other than
    /// `{"set": true}` or `{"set": false}`.
    NotASecretMark,
    TaskSequenceId(TaskSequenceIdError),
}

impl fmt::Display for ValueProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnObject => f.write_str("must be a table of field values"),
            Self::UnknownKey => f.write_str("no field has this key"),
            Self::Missing => f.write_str("the field is required"),
            Self::Empty => f.write_str("the field is required and must not be empty"),
            Self::WrongType { expected } => write!(f, "must be {expected}"),
            Self::TooLong { len, max_len } => write!(
                f,
                "holds at most {max_len} characters, and this value holds {len}"
            ),
            Self::NoMatch { pattern } => write!(f, "does not match the pattern `{pattern}`"),
            Self::BelowMin { min } => write!(f, "must be at least {min}"),
            Self::AboveMax { max } => write!(f, "must be at most {max}"),
            Self::NotAnOption => f.write_str("is not one of the field's options"),
            Self::Unterminated { offset } => write!(
                f,
                "the `%` at byte {offset} has no closing `%`. Write `%%` for a literal `%`"
            ),
            Self::NotASecretMark => f.write_str(
                "a secret field holds `{\"set\": true}` or `{\"set\": false}`, never the secret",
            ),
            Self::TaskSequenceId(error) => write!(f, "{error}"),
        }
    }
}
