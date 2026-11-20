use alloc::boxed::Box;
use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, VariantAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::non_empty::NonEmpty;
use crate::rfc3339;
use crate::template::Template;
use crate::validated_string::validated_string;
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
    /// Tests a registry key, or reads a `REG_DWORD`, `REG_QWORD`, `REG_SZ`, or
    /// `REG_EXPAND_SZ` value.
    Registry(RegistryCondition),
    File {
        path: PathTemplate,
        version: Option<Comparison<FileVersion>>,
        modified: Option<Comparison<Timestamp>>,
    },
    Folder {
        path: PathTemplate,
        modified: Option<Comparison<Timestamp>>,
    },
    InstalledMsi(MsiProduct),
    Wmi {
        namespace: WmiNamespace,
        query: QueryTemplate,
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

impl<T> Test<T> {
    /// Whether the operator takes an operand. Only `Exists` and `NotExists`
    /// take none.
    fn takes_operand(&self) -> bool {
        !matches!(self, Self::Exists | Self::NotExists)
    }
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

/// Defines a [`Template`] type that rejects empty text, and the error that
/// empty text gives. The type is written as a JSON string, as a `Template` is.
macro_rules! non_empty_template {
    (
        $(#[$meta:meta])*
        $name:ident, $error:ident, $message:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
        #[serde(try_from = "Template")]
        pub struct $name(Template);

        impl $name {
            pub fn as_template(&self) -> &Template {
                &self.0
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl TryFrom<Template> for $name {
            type Error = $error;

            fn try_from(text: Template) -> Result<Self, Self::Error> {
                if text.as_str().is_empty() {
                    Err($error)
                } else {
                    Ok(Self(text))
                }
            }
        }

        impl From<$name> for Template {
            fn from(text: $name) -> Self {
                text.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        #[doc = concat!("The text of a [`", stringify!($name), "`] is empty.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $error;

        impl fmt::Display for $error {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($message)
            }
        }

        impl core::error::Error for $error {}
    };
}

non_empty_template! {
    /// The text that a `Var` condition compares against. It is a `Template`
    /// that is not empty, because `not_exists` is the test for an empty
    /// variable.
    NonEmptyTemplate,
    EmptyTemplateError,
    "the text to compare against must not be empty. `not_exists` tests for an empty variable"
}

non_empty_template! {
    /// The path that a `File` or `Folder` condition tests. It is a `Template`
    /// that is not empty. MDT fails a condition whose path is empty after it
    /// expands and trims it (`TsCore!0x1801a2c77`, `TsCore!0x1801a3eb6`).
    PathTemplate,
    EmptyPathError,
    "a file or folder path must not be empty"
}

non_empty_template! {
    /// The key that a `Registry` condition tests, such as
    /// `HKEY_LOCAL_MACHINE\SOFTWARE\Contoso`. It is a `Template` that is not
    /// empty. MDT fails a condition whose key is empty after it expands and
    /// trims it (`TsCore!0x1801a747a`).
    RegKeyTemplate,
    EmptyRegKeyError,
    "a registry key must not be empty"
}

non_empty_template! {
    /// The query that a `Wmi` condition runs. It is a `Template` that is not
    /// empty. MDT fails a condition whose query is empty after it expands and
    /// trims it (`TsCore!0x1801aa872`).
    QueryTemplate,
    EmptyQueryError,
    "a WMI query must not be empty"
}

validated_string! {
    /// The namespace that a `Wmi` condition opens, such as `root\cimv2`. It is
    /// not empty. The agent opens it as written and never substitutes it. MDT
    /// fails a condition whose namespace is empty after it expands and trims
    /// it (`TsCore!0x1801aa7c6`).
    #[derive(PartialEq, Eq, Hash)]
    WmiNamespace, EmptyNamespaceError, check_namespace
}

fn check_namespace(text: &str) -> Result<(), EmptyNamespaceError> {
    if text.is_empty() {
        Err(EmptyNamespaceError)
    } else {
        Ok(())
    }
}

/// The text of a [`WmiNamespace`] is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyNamespaceError;

impl fmt::Display for EmptyNamespaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a WMI namespace must not be empty")
    }
}

impl core::error::Error for EmptyNamespaceError {}

/// A `Registry` condition: a key, the name of a value under it, the type that
/// the value must have, and a test.
///
/// With no value name, `exists` and `not_exists` test the key. Every other
/// test compares the value's data, and a key has no data, so it needs a value
/// name. A key has no type either, so a value type needs a value name too.
/// [`RegistryCondition::new`] and the parser reject a test that takes data,
/// and a value type, without one. MDT fails a comparison without one when it
/// runs, and ignores its `Type` parameter on a test of the key
/// (`TsCore!0x1801a6bec`).
///
/// The JSON is an object with `key`, `value`, `value_type`, and `test`. A
/// `value` that is left out or `null` names no value, and a `value_type` that
/// is left out or `null` names no type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryCondition {
    key: RegKeyTemplate,
    value: Option<RegValueName>,
    value_type: Option<RegType>,
    test: Test<RegData>,
}

impl RegistryCondition {
    /// Fails when `value` names no value and `test` takes data or
    /// `value_type` names a type.
    pub fn new(
        key: RegKeyTemplate,
        value: Option<RegValueName>,
        value_type: Option<RegType>,
        test: Test<RegData>,
    ) -> Result<Self, RegistryConditionError> {
        if value.is_none() {
            if test.takes_operand() {
                return Err(RegistryConditionError::Comparison);
            }
            if value_type.is_some() {
                return Err(RegistryConditionError::ValueType);
            }
        }
        Ok(Self {
            key,
            value,
            value_type,
            test,
        })
    }

    /// The key, such as `HKEY_LOCAL_MACHINE\SOFTWARE\Contoso`. It is a
    /// template, so its hive is known only after substitution.
    pub fn key(&self) -> &RegKeyTemplate {
        &self.key
    }

    /// The value that the test reads, or `None` when it tests the key.
    pub fn value(&self) -> Option<&RegValueName> {
        self.value.as_ref()
    }

    /// The type that the value must have, or `None` for a value of any type.
    /// It is `None` when [`Self::value`] is `None`. A value of another type
    /// counts as missing, as in MDT (`TsCore!0x1801a6bec`).
    pub fn value_type(&self) -> Option<RegType> {
        self.value_type
    }

    /// The test. It is `Exists` or `NotExists` when [`Self::value`] is `None`.
    pub fn test(&self) -> &Test<RegData> {
        &self.test
    }
}

/// A `Registry` condition that names no value has a part that needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryConditionError {
    /// A test that takes data names no value.
    Comparison,
    /// A value type names no value.
    ValueType,
}

impl fmt::Display for RegistryConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Comparison => {
                "a registry comparison needs a value name, because a key has no data to compare. `exists` without `value` tests the key"
            }
            Self::ValueType => {
                "a registry value type needs a value name, because a key has no type. Leave out `value_type` to test the key"
            }
        })
    }
}

impl core::error::Error for RegistryConditionError {}

/// Reads a `RegistryCondition` from a map.
///
/// A test that takes data and a `value` of `null` fail at the one that the
/// document holds second: at `value`, or at the operator under `test`, such
/// as `test.equals`. A value type and a `value` of `null` do the same: they
/// fail at `value` or at `value_type`. A test that takes data, or a value
/// type, with `value` left out fails at the condition itself.
impl<'de> Deserialize<'de> for RegistryCondition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("RegistryCondition", REGISTRY_FIELDS, RegistryVisitor)
    }
}

const REGISTRY_FIELDS: &[&str] = &["key", "value", "value_type", "test"];

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum RegistryField {
    Key,
    Value,
    ValueType,
    Test,
}

struct RegistryVisitor;

impl<'de> Visitor<'de> for RegistryVisitor {
    type Value = RegistryCondition;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a registry condition, an object with `key`, `value`, `value_type`, and `test`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RegistryCondition, A::Error> {
        let mut key: Option<RegKeyTemplate> = None;
        // `Some(None)` is a `value` or a `value_type` of `null`.
        let mut value: Option<Option<RegValueName>> = None;
        let mut value_type: Option<Option<RegType>> = None;
        let mut test: Option<Test<RegData>> = None;
        while let Some(field) = map.next_key()? {
            match field {
                RegistryField::Key => {
                    if key.is_some() {
                        return Err(de::Error::duplicate_field("key"));
                    }
                    key = Some(map.next_value()?);
                }
                RegistryField::Value => {
                    if value.is_some() {
                        return Err(de::Error::duplicate_field("value"));
                    }
                    let needs_name = if test.as_ref().is_some_and(Test::takes_operand) {
                        Some(RegistryConditionError::Comparison)
                    } else if matches!(value_type, Some(Some(_))) {
                        Some(RegistryConditionError::ValueType)
                    } else {
                        None
                    };
                    value = Some(map.next_value_seed(ValueNameSeed { needs_name })?);
                }
                RegistryField::ValueType => {
                    if value_type.is_some() {
                        return Err(de::Error::duplicate_field("value_type"));
                    }
                    let unnamed = matches!(value, Some(None));
                    value_type = Some(map.next_value_seed(ValueTypeSeed { unnamed })?);
                }
                RegistryField::Test => {
                    if test.is_some() {
                        return Err(de::Error::duplicate_field("test"));
                    }
                    let unnamed = matches!(value, Some(None));
                    test = Some(map.next_value_seed(RegTestSeed { unnamed })?);
                }
            }
        }
        let key = key.ok_or_else(|| de::Error::missing_field("key"))?;
        let test = test.ok_or_else(|| de::Error::missing_field("test"))?;
        RegistryCondition::new(key, value.flatten(), value_type.flatten(), test)
            .map_err(de::Error::custom)
    }
}

/// Reads `value`. `needs_name` is the error that a `null` gives, when the
/// `test` read before it takes data or the `value_type` read before it names
/// a type.
struct ValueNameSeed {
    needs_name: Option<RegistryConditionError>,
}

impl<'de> DeserializeSeed<'de> for ValueNameSeed {
    type Value = Option<RegValueName>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let name = Option::<RegValueName>::deserialize(deserializer)?;
        match (&name, self.needs_name) {
            (None, Some(error)) => Err(de::Error::custom(error)),
            _ => Ok(name),
        }
    }
}

/// Reads `value_type`. `unnamed` says that the `value` read before it is
/// `null`, so a type fails.
struct ValueTypeSeed {
    unnamed: bool,
}

impl<'de> DeserializeSeed<'de> for ValueTypeSeed {
    type Value = Option<RegType>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let value_type = Option::<RegType>::deserialize(deserializer)?;
        if value_type.is_some() && self.unnamed {
            return Err(de::Error::custom(RegistryConditionError::ValueType));
        }
        Ok(value_type)
    }
}

/// Reads `test` as the derived `Deserialize` of `Test<RegData>` does.
/// `unnamed` says that the `value` read before it is `null`, so an operator
/// that takes data fails.
struct RegTestSeed {
    unnamed: bool,
}

const TEST_VARIANTS: &[&str] = &[
    "exists",
    "not_exists",
    "equals",
    "not_equals",
    "less",
    "less_equal",
    "greater",
    "greater_equal",
    "like",
];

/// The operators of [`Test`], as the JSON names them.
#[derive(Deserialize)]
#[serde(variant_identifier, rename_all = "snake_case")]
enum TestOp {
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

impl<'de> DeserializeSeed<'de> for RegTestSeed {
    type Value = Test<RegData>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_enum("Test", TEST_VARIANTS, self)
    }
}

impl<'de> Visitor<'de> for RegTestSeed {
    type Value = Test<RegData>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The text that the derived `Deserialize` of `Test` gives.
        f.write_str("enum Test")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error> {
        let (op, variant) = data.variant::<TestOp>()?;
        let data = RegDataSeed {
            unnamed: self.unnamed,
        };
        Ok(match op {
            TestOp::Exists => {
                variant.unit_variant()?;
                Test::Exists
            }
            TestOp::NotExists => {
                variant.unit_variant()?;
                Test::NotExists
            }
            TestOp::Equals => Test::Equals(variant.newtype_variant_seed(data)?),
            TestOp::NotEquals => Test::NotEquals(variant.newtype_variant_seed(data)?),
            TestOp::Less => Test::Less(variant.newtype_variant_seed(data)?),
            TestOp::LessEqual => Test::LessEqual(variant.newtype_variant_seed(data)?),
            TestOp::Greater => Test::Greater(variant.newtype_variant_seed(data)?),
            TestOp::GreaterEqual => Test::GreaterEqual(variant.newtype_variant_seed(data)?),
            TestOp::Like => Test::Like(variant.newtype_variant_seed(data)?),
        })
    }
}

/// Reads the data of a test. `unnamed` says that the condition names no value.
struct RegDataSeed {
    unnamed: bool,
}

impl<'de> DeserializeSeed<'de> for RegDataSeed {
    type Value = RegData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<RegData, D::Error> {
        let data = RegData::deserialize(deserializer)?;
        if self.unnamed {
            return Err(de::Error::custom(RegistryConditionError::Comparison));
        }
        Ok(data)
    }
}

/// A registry value type that a `Registry` condition can require. These are
/// the seven names that MDT maps a condition's `Type` to (`TsCore!0x1801ab134`).
///
/// The JSON is the Windows name in snake case, such as `"reg_expand_sz"` for
/// `REG_EXPAND_SZ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegType {
    #[serde(rename = "reg_none")]
    None,
    #[serde(rename = "reg_sz")]
    Sz,
    #[serde(rename = "reg_expand_sz")]
    ExpandSz,
    #[serde(rename = "reg_binary")]
    Binary,
    #[serde(rename = "reg_dword")]
    Dword,
    #[serde(rename = "reg_multi_sz")]
    MultiSz,
    #[serde(rename = "reg_qword")]
    Qword,
}

impl RegType {
    /// The type's number in the Windows API, such as 4 for `REG_DWORD`.
    pub fn number(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Sz => 1,
            Self::ExpandSz => 2,
            Self::Binary => 3,
            Self::Dword => 4,
            Self::MultiSz => 7,
            Self::Qword => 11,
        }
    }
}

validated_string! {
    /// The name of a registry value, such as `CurrentBuildNumber`. It is not
    /// empty. A `Registry` condition that names no value tests the key.
    #[derive(PartialEq, Eq, Hash)]
    RegValueName, EmptyValueNameError, check_value_name
}

fn check_value_name(text: &str) -> Result<(), EmptyValueNameError> {
    if text.is_empty() {
        Err(EmptyValueNameError)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyValueNameError;

impl fmt::Display for EmptyValueNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a registry value name must not be empty. Leave out `value` to test the key")
    }
}

impl core::error::Error for EmptyValueNameError {}

/// The data that a `Registry` condition compares against. `Integer` holds
/// every `REG_DWORD` and `REG_QWORD` value, so a negative number fails to parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum RegData {
    String(RegText),
    Integer(u64),
}

impl<'de> Deserialize<'de> for RegData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(RegDataVisitor)
    }
}

struct RegDataVisitor;

impl Visitor<'_> for RegDataVisitor {
    type Value = RegData;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string or an integer from 0 to 18446744073709551615")
    }

    fn visit_u64<E: de::Error>(self, number: u64) -> Result<RegData, E> {
        Ok(RegData::Integer(number))
    }

    fn visit_i64<E: de::Error>(self, number: i64) -> Result<RegData, E> {
        u64::try_from(number)
            .map(RegData::Integer)
            .map_err(|_| E::invalid_value(de::Unexpected::Signed(number), &self))
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<RegData, E> {
        text.parse().map(RegData::String).map_err(E::custom)
    }
}

validated_string! {
    /// Registry data written as text, such as `Professional`. It is not empty.
    #[derive(PartialEq, Eq, Hash)]
    RegText, EmptyRegTextError, check_reg_text
}

fn check_reg_text(text: &str) -> Result<(), EmptyRegTextError> {
    if text.is_empty() {
        Err(EmptyRegTextError)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyRegTextError;

impl fmt::Display for EmptyRegTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the registry data to compare against must not be empty")
    }
}

impl core::error::Error for EmptyRegTextError {}

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
    ProductCode(#[serde(deserialize_with = "crate::guid::deserialize")] Uuid),
    UpgradeCode(#[serde(deserialize_with = "crate::guid::deserialize")] Uuid),
}
