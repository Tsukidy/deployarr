//! Evaluates a [`Condition`] against the variables and the machine.
//!
//! The crate does no I/O, so a [`Probe`] answers every question about the
//! machine: the process environment, the registry, files, folders, installed
//! MSI products, and WMI. The agent implements it on Windows. Tests answer
//! from a table.
//!
//! `docs/architecture.md#conditions` holds the rules. A citation such as
//! `TsCore!0x1801a96c0` names the function in MDT 8450's `TsCore.dll` that a
//! rule comes from, as `docs/research/mdt-8450-binaries.md#conditions` does.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;

use time::OffsetDateTime;

use crate::condition::{
    CompareOp, Comparison, Condition, FileVersion, MsiProduct, NonEmptyTemplate, PathTemplate,
    RegData, RegType, RegValueName, RegistryCondition, Test, Timestamp,
};
use crate::store::{Value, Vars};
use crate::substitution::SubstituteError;
use crate::template::Template;
use crate::variable::VarName;

mod upper_case;

/// Answers the questions that a condition asks about the machine.
///
/// Every text argument has had its `%Name%` references substituted, so it can
/// hold a secret value. An implementation must not write it to a log line or
/// put it in an error. No path, key, or query that the evaluator passes is
/// empty, because the evaluator fails a condition whose text substitutes to
/// empty text before it asks. No WMI namespace is empty either, because the
/// parser rejects an empty one and the evaluator does not substitute it.
///
/// A missing file, folder, registry key, or registry value is an answer, not
/// an error. A method returns `Err` only when it cannot tell, such as when
/// access is denied. The evaluator turns that into a [`ConditionError`],
/// never into false.
pub trait Probe {
    /// A process environment variable. [`Template::substitute`] falls back to
    /// it for a name that is not a task sequence variable.
    fn env_var(&self, name: &str) -> Option<String>;

    /// Whether a registry key exists.
    fn registry_key_exists(&self, key: &RegKey) -> Result<bool, ProbeError>;

    /// A named value under a registry key, or `None` when the key or the
    /// value is missing. `name` is never empty.
    fn registry_value(&self, key: &RegKey, name: &str) -> Result<Option<RegValue>, ProbeError>;

    /// Whether a file exists at `path`. A folder is not a file.
    fn file_exists(&self, path: &str) -> Result<bool, ProbeError>;

    /// Whether a folder exists at `path`. A file is not a folder.
    fn folder_exists(&self, path: &str) -> Result<bool, ProbeError>;

    /// The version in a file's version resource, or `None` when the file has
    /// none.
    fn file_version(&self, path: &str) -> Result<Option<FileVersion>, ProbeError>;

    /// The last write time of a file or a folder.
    fn modified(&self, path: &str) -> Result<OffsetDateTime, ProbeError>;

    /// Whether the MSI product with this product code is installed, or any
    /// product with this upgrade code.
    fn installed_msi(&self, product: &MsiProduct) -> Result<bool, ProbeError>;

    /// Whether a WMI query returns at least one row.
    fn wmi_has_rows(&self, namespace: &str, query: &str) -> Result<bool, WmiError>;
}

/// A registry value as a [`Probe`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegValue {
    Dword(u32),
    Qword(u64),
    Sz(String),
    /// A `REG_EXPAND_SZ` value after the probe expands it with
    /// `ExpandEnvironmentStringsW`, as MDT does before it compares one
    /// (`TsCore!0x1801a6bec`).
    ExpandSz(String),
    /// A value of any other type, such as `REG_BINARY`, with its type number.
    Other(u32),
}

impl RegValue {
    /// Whether the value has type `reg_type`. `Other` matches by its number.
    fn has_type(&self, reg_type: RegType) -> bool {
        match self {
            Self::Dword(_) => reg_type == RegType::Dword,
            Self::Qword(_) => reg_type == RegType::Qword,
            Self::Sz(_) => reg_type == RegType::Sz,
            Self::ExpandSz(_) => reg_type == RegType::ExpandSz,
            Self::Other(number) => *number == reg_type.number(),
        }
    }
}

/// A root key that a `registry` condition can name.
///
/// These are the four that MDT accepts. MDT does not accept
/// `HKEY_CLASSES_ROOT` or a short name such as `HKLM` (`TsCore!0x1801ab5c0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
    CurrentConfig,
    Users,
}

impl Hive {
    const ALL: [Self; 4] = [
        Self::LocalMachine,
        Self::CurrentUser,
        Self::CurrentConfig,
        Self::Users,
    ];

    /// The full name, such as `HKEY_LOCAL_MACHINE`.
    pub fn name(self) -> &'static str {
        match self {
            Self::LocalMachine => "HKEY_LOCAL_MACHINE",
            Self::CurrentUser => "HKEY_CURRENT_USER",
            Self::CurrentConfig => "HKEY_CURRENT_CONFIG",
            Self::Users => "HKEY_USERS",
        }
    }
}

/// A registry key: a hive and the path under it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegKey {
    hive: Hive,
    path: String,
}

impl RegKey {
    /// Reads a key that starts with a full hive name and a `\`, such as
    /// `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft`. The hive name ignores case,
    /// and the path after the `\` can be empty (`TsCore!0x1801ab5c0`).
    pub fn parse(text: &str) -> Option<Self> {
        let (hive_name, path) = text.split_once('\\')?;
        let hive = Hive::ALL
            .into_iter()
            .find(|hive| hive.name().eq_ignore_ascii_case(hive_name))?;
        Some(Self {
            hive,
            path: path.into(),
        })
    }

    pub fn hive(&self) -> Hive {
        self.hive
    }

    /// The path under the hive, such as `SOFTWARE\Microsoft`.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Why a [`Probe`] could not answer.
///
/// It holds an error code, such as an HRESULT, and no text, so it cannot leak
/// a secret that a path or a query holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProbeError {
    code: i32,
}

impl ProbeError {
    pub fn new(code: i32) -> Self {
        Self { code }
    }

    pub fn code(self) -> i32 {
        self.code
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Windows prints an HRESULT as its 32 bits in hex.
        write!(f, "error {:#010x}", self.code.cast_unsigned())
    }
}

impl core::error::Error for ProbeError {}

/// Why a WMI query could not run.
///
/// `ignore_failure` covers only a failed query, as MDT's
/// `ignoreQueryFailure` does. A namespace that cannot be opened is always an
/// error (`TsCore!0x1801aa380`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WmiError {
    /// The namespace could not be opened or secured.
    Namespace(ProbeError),
    /// The query failed, or its rows could not be read.
    Query(ProbeError),
}

/// The kind of condition whose question a [`Probe`] could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    Registry,
    File,
    Folder,
    InstalledMsi,
    WmiNamespace,
    WmiQuery,
}

/// A condition's text that must not substitute to empty text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateField {
    /// A `file` condition's path.
    FilePath,
    /// A `folder` condition's path.
    FolderPath,
    /// A `registry` condition's key.
    RegistryKey,
    /// A `wmi` condition's query.
    WmiQuery,
}

impl TemplateField {
    /// What the field is, such as `a file path`.
    fn name(self) -> &'static str {
        match self {
            Self::FilePath => "a file path",
            Self::FolderPath => "a folder path",
            Self::RegistryKey => "a registry key",
            Self::WmiQuery => "a WMI query",
        }
    }
}

/// Why a condition could not be evaluated. It is never false.
///
/// No variant holds substituted text, so an error message cannot leak a
/// secret. A variant names the template as written instead, as the logs do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionError {
    /// A template field could not be substituted.
    Substitute {
        template: Template,
        error: SubstituteError,
    },
    /// A `var` comparison's operand substitutes to empty text. MDT makes this
    /// an error too (`TsCore!0x1801a96c0`).
    EmptyOperand { template: Template },
    /// A `file` or `folder` path, a `registry` key, or a `wmi` query
    /// substitutes to empty text. MDT makes each an error before it asks the
    /// machine (`TsCore!0x1801a2c77`, `TsCore!0x1801a3eb6`,
    /// `TsCore!0x1801a747a`, `TsCore!0x1801aa872`). `ignore_failure` does not
    /// cover it, because MDT's check skips the ignore-query-failure flag
    /// (`TsCore!0x1801aae22`).
    EmptyText {
        field: TemplateField,
        template: Template,
    },
    /// A `var` comparison or `Like` test named a variable that holds a list.
    /// `Exists` and `NotExists` work on a list. A list in a `Template` fails
    /// to substitute, so both fail the step (owner decision on #177).
    List { name: VarName },
    /// A `registry` key does not start with a full hive name and a `\`.
    RegistryHive { key: Template },
    /// A `registry` comparison found a value of a type it does not read, such
    /// as `REG_BINARY`. `reg_type` is the type's number.
    RegistryType {
        key: Template,
        value: String,
        reg_type: u32,
    },
    /// A `file` condition compares the version of a file that has no version
    /// resource.
    NoFileVersion { path: Template },
    /// A probe could not answer.
    Probe { kind: ProbeKind, error: ProbeError },
}

impl fmt::Display for ConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Substitute { template, error } => {
                write!(f, "cannot substitute `{}`: {error}", template.as_str())
            }
            Self::EmptyOperand { template } => write!(
                f,
                "`{}` substitutes to empty text, and a comparison needs text to compare against",
                template.as_str()
            ),
            Self::EmptyText { field, template } => write!(
                f,
                "`{}` substitutes to empty text, and {} must not be empty",
                template.as_str(),
                field.name()
            ),
            Self::List { name } => write!(
                f,
                "`{name}` holds a list, and only `exists` and `not_exists` can test a list"
            ),
            Self::RegistryHive { key } => write!(
                f,
                "the registry key `{}` must start with `HKEY_LOCAL_MACHINE\\`, `HKEY_CURRENT_USER\\`, `HKEY_CURRENT_CONFIG\\`, or `HKEY_USERS\\`",
                key.as_str()
            ),
            Self::RegistryType {
                key,
                value,
                reg_type,
            } => write!(
                f,
                "the registry value `{value}` under `{}` has type {reg_type}. A `registry` condition reads only `REG_DWORD`, `REG_QWORD`, `REG_SZ`, and `REG_EXPAND_SZ` values",
                key.as_str()
            ),
            Self::NoFileVersion { path } => {
                write!(f, "the file `{}` has no version resource", path.as_str())
            }
            Self::Probe { kind, error } => {
                let what = match kind {
                    ProbeKind::Registry => "reading the registry",
                    ProbeKind::File => "reading a file",
                    ProbeKind::Folder => "reading a folder",
                    ProbeKind::InstalledMsi => "looking up an installed MSI product",
                    ProbeKind::WmiNamespace => "opening a WMI namespace",
                    ProbeKind::WmiQuery => "running a WMI query",
                };
                write!(f, "{what} failed with {error}")
            }
        }
    }
}

impl core::error::Error for ConditionError {}

/// Evaluates `condition` against `vars` and the machine that `probe` reads.
///
/// Each `Template` field is substituted first, from `vars` and then from the
/// probe's process environment. `All`, `Any`, and `Not` read their children in
/// order and stop at the first child that settles the answer, as MDT does
/// (`TsCore!0x1801a0dc0`, `TsCore!0x1801a55e8`, `TsCore!0x1801a53e4`). A child
/// that fails to evaluate stops them with its error.
///
/// # Errors
///
/// A [`ConditionError`] when a template cannot be substituted, a field holds
/// a value that the condition cannot use, or the probe cannot answer. An
/// error is never false.
pub fn evaluate(
    condition: &Condition,
    vars: &Vars,
    probe: &dyn Probe,
) -> Result<bool, ConditionError> {
    Evaluator { vars, probe }.condition(condition)
}

struct Evaluator<'a> {
    vars: &'a Vars,
    probe: &'a dyn Probe,
}

impl Evaluator<'_> {
    fn condition(&self, condition: &Condition) -> Result<bool, ConditionError> {
        match condition {
            Condition::All(children) => {
                for child in children {
                    if !self.condition(child)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Condition::Any(children) => {
                for child in children {
                    if self.condition(child)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Condition::Not(child) => Ok(!self.condition(child)?),
            Condition::Var { name, test } => self.var(name, test),
            Condition::Registry(registry) => self.registry(registry),
            Condition::File {
                path,
                version,
                modified,
            } => self.file(path, version.as_ref(), modified.as_ref()),
            Condition::Folder { path, modified } => self.folder(path, modified.as_ref()),
            Condition::InstalledMsi(product) => self
                .probe
                .installed_msi(product)
                .map_err(failed(ProbeKind::InstalledMsi)),
            Condition::Wmi {
                namespace,
                query,
                ignore_failure,
            } => {
                let query = self.required(query.as_template(), TemplateField::WmiQuery)?;
                match self.probe.wmi_has_rows(namespace.as_str(), &query) {
                    Ok(has_rows) => Ok(has_rows),
                    Err(WmiError::Query(_)) if *ignore_failure => Ok(false),
                    Err(WmiError::Query(error)) => Err(failed(ProbeKind::WmiQuery)(error)),
                    Err(WmiError::Namespace(error)) => Err(failed(ProbeKind::WmiNamespace)(error)),
                }
            }
        }
    }

    fn substitute(&self, template: &Template) -> Result<String, ConditionError> {
        template
            .substitute(self.vars, |name| self.probe.env_var(name))
            .map_err(|error| ConditionError::Substitute {
                template: template.clone(),
                error,
            })
    }

    /// Substitutes a path, a key, or a query, which must not come out empty.
    /// The check comes before any question to the probe, as in MDT.
    fn required(
        &self,
        template: &Template,
        field: TemplateField,
    ) -> Result<String, ConditionError> {
        let text = self.substitute(template)?;
        if text.is_empty() {
            return Err(ConditionError::EmptyText {
                field,
                template: template.clone(),
            });
        }
        Ok(text)
    }

    /// Substitutes a comparison's operand, which must not come out empty.
    fn operand(&self, template: &Template) -> Result<String, ConditionError> {
        let text = self.substitute(template)?;
        if text.is_empty() {
            return Err(ConditionError::EmptyOperand {
                template: template.clone(),
            });
        }
        Ok(text)
    }

    fn var(&self, name: &VarName, test: &Test<NonEmptyTemplate>) -> Result<bool, ConditionError> {
        let value = self.vars.get(name).map(|var| var.value());
        let Some((op, operand)) = Op::split(test) else {
            // A list with at least one item exists.
            let set = value.is_some_and(|value| !value.is_empty());
            return Ok(set == matches!(test, Test::Exists));
        };
        // An unset variable reads as empty text, as in MDT. A list has no one
        // text form to compare, so a comparison or `Like` on it is an error.
        let text = match value {
            None => "",
            Some(Value::Scalar(text)) => text,
            Some(Value::List(_)) => return Err(ConditionError::List { name: name.clone() }),
        };
        Ok(match op {
            Op::Compare(op) => {
                let operand = self.operand(operand.as_template())?;
                compare(op, compare_text(text, &operand))
            }
            // MDT checks for an empty operand only before a comparison.
            Op::Like => like(text, &self.substitute(operand.as_template())?),
        })
    }

    fn registry(&self, registry: &RegistryCondition) -> Result<bool, ConditionError> {
        let key = registry.key().as_template();
        let reg_key = RegKey::parse(&self.required(key, TemplateField::RegistryKey)?)
            .ok_or_else(|| ConditionError::RegistryHive { key: key.clone() })?;
        let name = registry.value().map(RegValueName::as_str);
        let test = registry.test();
        let Some((op, data)) = Op::split(test) else {
            let exists = match name {
                None => self
                    .probe
                    .registry_key_exists(&reg_key)
                    .map_err(failed(ProbeKind::Registry))?,
                Some(name) => self.registry_value(registry, &reg_key, name)?.is_some(),
            };
            return Ok(exists == matches!(test, Test::Exists));
        };
        // `RegistryCondition::new` and the parser reject a test that takes
        // data without a value name.
        let name = name.expect("a registry test that takes data names a value");
        let Some(found) = self.registry_value(registry, &reg_key, name)? else {
            return Ok(false);
        };
        // Both sides compare by the `var` rules, whatever the value's type. A
        // number is its decimal text.
        let text = match found {
            RegValue::Dword(number) => number.to_string(),
            RegValue::Qword(number) => number.to_string(),
            RegValue::Sz(text) | RegValue::ExpandSz(text) => text,
            RegValue::Other(reg_type) => {
                return Err(ConditionError::RegistryType {
                    key: key.clone(),
                    value: name.into(),
                    reg_type,
                });
            }
        };
        let data = match data {
            RegData::String(text) => Cow::Borrowed(text.as_str()),
            RegData::Integer(number) => Cow::Owned(number.to_string()),
        };
        Ok(match op {
            Op::Compare(op) => compare(op, compare_text(&text, &data)),
            Op::Like => like(&text, &data),
        })
    }

    /// Reads the value `name` that `registry` tests, or `None` when it is
    /// missing. With a value type, a value of another type counts as missing
    /// for every test, as in MDT (`TsCore!0x1801a6bec`, `TsCore!0x1801a7c48`).
    fn registry_value(
        &self,
        registry: &RegistryCondition,
        key: &RegKey,
        name: &str,
    ) -> Result<Option<RegValue>, ConditionError> {
        let found = self
            .probe
            .registry_value(key, name)
            .map_err(failed(ProbeKind::Registry))?;
        Ok(found.filter(|found| {
            registry
                .value_type()
                .is_none_or(|value_type| found.has_type(value_type))
        }))
    }

    fn file(
        &self,
        path: &PathTemplate,
        version: Option<&Comparison<FileVersion>>,
        modified: Option<&Comparison<Timestamp>>,
    ) -> Result<bool, ConditionError> {
        let path = path.as_template();
        let text = self.required(path, TemplateField::FilePath)?;
        let file_error = failed(ProbeKind::File);
        if !self.probe.file_exists(&text).map_err(file_error)? {
            return Ok(false);
        }
        if let Some(version) = version {
            let Some(found) = self.probe.file_version(&text).map_err(file_error)? else {
                return Err(ConditionError::NoFileVersion { path: path.clone() });
            };
            if !compare(version.op, found.cmp(&version.value)) {
                return Ok(false);
            }
        }
        match modified {
            None => Ok(true),
            Some(modified) => {
                let found = self.probe.modified(&text).map_err(file_error)?;
                Ok(compare_time(modified, found))
            }
        }
    }

    fn folder(
        &self,
        path: &PathTemplate,
        modified: Option<&Comparison<Timestamp>>,
    ) -> Result<bool, ConditionError> {
        let text = self.required(path.as_template(), TemplateField::FolderPath)?;
        let folder_error = failed(ProbeKind::Folder);
        if !self.probe.folder_exists(&text).map_err(folder_error)? {
            return Ok(false);
        }
        match modified {
            None => Ok(true),
            Some(modified) => {
                let found = self.probe.modified(&text).map_err(folder_error)?;
                Ok(compare_time(modified, found))
            }
        }
    }
}

fn failed(kind: ProbeKind) -> impl Fn(ProbeError) -> ConditionError + Copy {
    move |error| ConditionError::Probe { kind, error }
}

/// What a test with an operand does with it.
#[derive(Debug, Clone, Copy)]
enum Op {
    Compare(CompareOp),
    Like,
}

impl Op {
    /// The operator and the operand of a test, or `None` for `Exists` and
    /// `NotExists`.
    fn split<T>(test: &Test<T>) -> Option<(Self, &T)> {
        let op = match test {
            Test::Exists | Test::NotExists => return None,
            Test::Equals(operand) => (CompareOp::Equals, operand),
            Test::NotEquals(operand) => (CompareOp::NotEquals, operand),
            Test::Less(operand) => (CompareOp::Less, operand),
            Test::LessEqual(operand) => (CompareOp::LessEqual, operand),
            Test::Greater(operand) => (CompareOp::Greater, operand),
            Test::GreaterEqual(operand) => (CompareOp::GreaterEqual, operand),
            Test::Like(operand) => return Some((Self::Like, operand)),
        };
        Some((Self::Compare(op.0), op.1))
    }
}

/// Whether `found op wanted` holds, given how `found` orders against
/// `wanted`.
fn compare(op: CompareOp, ordering: Ordering) -> bool {
    match op {
        CompareOp::Equals => ordering.is_eq(),
        CompareOp::NotEquals => ordering.is_ne(),
        CompareOp::Less => ordering.is_lt(),
        CompareOp::LessEqual => ordering.is_le(),
        CompareOp::Greater => ordering.is_gt(),
        CompareOp::GreaterEqual => ordering.is_ge(),
    }
}

/// Compares a last write time with a condition's time to the whole second,
/// as MDT does (`TsCore!0x1801df900`). Both are instants, so their offsets do
/// not matter.
fn compare_time(comparison: &Comparison<Timestamp>, found: OffsetDateTime) -> bool {
    let ordering = found
        .unix_timestamp()
        .cmp(&comparison.value.0.unix_timestamp());
    compare(comparison.op, ordering)
}

/// Orders two texts as integers when both are integers, and otherwise as text
/// without regard to ASCII case. These are the `var` rules
/// (`TsCore!0x1801a96c0`). A `registry` comparison follows them too, as
/// `RegData` in `docs/architecture.md#parameter-types` says.
fn compare_text(left: &str, right: &str) -> Ordering {
    match (Integer::parse(left), Integer::parse(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => cmp_ignore_ascii_case(left, right),
    }
}

/// Orders two texts by code point after it folds `A` to `Z` to `a` to `z`.
/// Every other character stays as it is, so `Ä` and `ä` differ. MDT compares
/// through `CCM::Utility::String::compare_no_case` (`CcmCore!0x180085f08`),
/// which calls `_wcsicmp`. That `_wcsicmp` runs in the C locale, where only
/// ASCII letters fold, is inferred, as `docs/architecture.md#conditions` says.
fn cmp_ignore_ascii_case(left: &str, right: &str) -> Ordering {
    let left = left.chars().map(|c| c.to_ascii_lowercase());
    let right = right.chars().map(|c| c.to_ascii_lowercase());
    left.cmp(right)
}

/// Whether two characters match in `Like`. MDT's `Like` calls
/// `PathMatchSpecW` (`TsCore!0x1801aa078`), which matches two UTF-16 units when
/// they have the same upper case form. `Like` follows the case map of base
/// WinPE's `l_intl.nls`, as `docs/architecture.md#conditions` says. A
/// character above U+FFFF matches only itself.
fn eq_ignore_case(left: char, right: char) -> bool {
    if left == right {
        return true;
    }
    match (u16::try_from(left), u16::try_from(right)) {
        (Ok(left), Ok(right)) => upper_case::upper(left) == upper_case::upper(right),
        _ => false,
    }
}

/// Whether `text` matches `pattern` without regard to case, by
/// [`eq_ignore_case`]. `*` matches any run of characters, the empty run
/// included, and `?` matches one character. Every other character matches
/// itself.
fn like(text: &str, pattern: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let (mut t, mut p) = (0, 0);
    // The latest `*`, and the length of the run of text it now stands for.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || eq_ignore_case(c, text[t]) => {
                p += 1;
                t += 1;
            }
            _ => match star {
                // Let the `*` take one more character, and try again.
                Some((star_p, star_t)) => {
                    star = Some((star_p, star_t + 1));
                    p = star_p + 1;
                    t = star_t + 1;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// A whole number written as an optional `-` and one or more ASCII digits.
///
/// It can be any length, so two integers always compare exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Integer<'a> {
    negative: bool,
    /// The digits without leading zeros, so zero has none.
    digits: &'a str,
}

impl<'a> Integer<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let digits = digits.trim_start_matches('0');
        Some(Self {
            negative: negative && !digits.is_empty(),
            digits,
        })
    }

    /// Orders the absolute values. With no leading zeros, a longer number is
    /// larger, and numbers of one length order like their text.
    fn cmp_magnitude(self, other: Self) -> Ordering {
        self.digits
            .len()
            .cmp(&other.digits.len())
            .then_with(|| self.digits.cmp(other.digits))
    }
}

impl Ord for Integer<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.cmp_magnitude(*other),
            (true, true) => other.cmp_magnitude(*self),
        }
    }
}

impl PartialOrd for Integer<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_compare_by_value_at_any_length() {
        let cases = [
            ("0", "-0", Ordering::Equal),
            ("007", "7", Ordering::Equal),
            ("-5", "-12", Ordering::Greater),
            ("99", "100", Ordering::Less),
            (
                "123456789012345678901234567890",
                "123456789012345678901234567891",
                Ordering::Less,
            ),
            ("-1", "0", Ordering::Less),
        ];
        for (left, right, expected) in cases {
            let left_int = Integer::parse(left).unwrap();
            let right_int = Integer::parse(right).unwrap();
            assert_eq!(left_int.cmp(&right_int), expected, "{left} vs {right}");
            assert_eq!(
                right_int.cmp(&left_int),
                expected.reverse(),
                "{right} vs {left}"
            );
        }
    }

    #[test]
    fn only_an_optional_minus_and_digits_make_an_integer() {
        for text in ["", "-", "+1", "1.0", " 1", "1 ", "--1", "1-", "0x10", "١"] {
            assert_eq!(Integer::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn like_backtracks_over_stars() {
        assert!(like("abcabd", "*abd"));
        assert!(like("aXbYc", "a*b*c"));
        assert!(!like("aXbYc", "a*b*d"));
        assert!(like("", "*"));
        assert!(like("", ""));
        assert!(!like("a", ""));
        assert!(like("abc", "***"));
        assert!(!like("ab", "a?c"));
    }

    #[test]
    fn a_probe_error_prints_the_bits_of_its_code() {
        let error = ProbeError::new(0x8004_1017_u32.cast_signed());
        assert_eq!(error.to_string(), "error 0x80041017");
    }

    #[test]
    fn registry_keys_need_a_full_hive_and_a_backslash() {
        let key = RegKey::parse(r"hkey_local_machine\SOFTWARE\Contoso").unwrap();
        assert_eq!(key.hive(), Hive::LocalMachine);
        assert_eq!(key.path(), r"SOFTWARE\Contoso");
        assert_eq!(RegKey::parse(r"HKEY_USERS\").unwrap().path(), "");
        for bad in [
            "HKEY_LOCAL_MACHINE",
            r"HKLM\SOFTWARE",
            r"HKEY_CLASSES_ROOT\.txt",
            r"\SOFTWARE",
            "",
        ] {
            assert_eq!(RegKey::parse(bad), None, "{bad}");
        }
    }
}
