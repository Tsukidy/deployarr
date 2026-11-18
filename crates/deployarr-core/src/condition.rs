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

/// The text that a `Var` condition compares against. It is a `Template` that
/// is not empty, because `not_exists` is the test for an empty variable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "Template")]
pub struct NonEmptyTemplate(Template);

impl NonEmptyTemplate {
    pub fn as_template(&self) -> &Template {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<Template> for NonEmptyTemplate {
    type Error = EmptyTemplateError;

    fn try_from(text: Template) -> Result<Self, Self::Error> {
        if text.as_str().is_empty() {
            Err(EmptyTemplateError)
        } else {
            Ok(Self(text))
        }
    }
}

impl From<NonEmptyTemplate> for Template {
    fn from(text: NonEmptyTemplate) -> Self {
        text.0
    }
}

impl Serialize for NonEmptyTemplate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyTemplateError;

impl fmt::Display for EmptyTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the text to compare against must not be empty. `not_exists` tests for an empty variable",
        )
    }
}

impl core::error::Error for EmptyTemplateError {}

/// A `Registry` condition: a key, the name of a value under it, and a test.
///
/// With no value name, `exists` and `not_exists` test the key. Every other
/// test compares the value's data, and a key has no data, so it needs a value
/// name. [`RegistryCondition::new`] and the parser reject such a test without
/// one. MDT fails a comparison without one when it runs (`TsCore!0x1801a6bec`).
///
/// The JSON is an object with `key`, `value`, and `test`. A `value` that is
/// left out or `null` names no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryCondition {
    key: Template,
    value: Option<RegValueName>,
    test: Test<RegData>,
}

impl RegistryCondition {
    /// Fails when `test` takes data and `value` names no value.
    pub fn new(
        key: Template,
        value: Option<RegValueName>,
        test: Test<RegData>,
    ) -> Result<Self, RegistryConditionError> {
        if value.is_none() && test.takes_operand() {
            return Err(RegistryConditionError);
        }
        Ok(Self { key, value, test })
    }

    /// The key, such as `HKEY_LOCAL_MACHINE\SOFTWARE\Contoso`. It is a
    /// template, so its hive is known only after substitution.
    pub fn key(&self) -> &Template {
        &self.key
    }

    /// The value that the test reads, or `None` when it tests the key.
    pub fn value(&self) -> Option<&RegValueName> {
        self.value.as_ref()
    }

    /// The test. It is `Exists` or `NotExists` when [`Self::value`] is `None`.
    pub fn test(&self) -> &Test<RegData> {
        &self.test
    }
}

/// A `Registry` test that takes data names no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryConditionError;

impl fmt::Display for RegistryConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "a registry comparison needs a value name, because a key has no data to compare. `exists` without `value` tests the key",
        )
    }
}

impl core::error::Error for RegistryConditionError {}

/// Reads a `RegistryCondition` from a map.
///
/// A test that takes data and a `value` of `null` fail at the one that the
/// document holds second: at `value`, or at the operator under `test`, such
/// as `test.equals`. A test that takes data with `value` left out fails at the
/// condition itself.
impl<'de> Deserialize<'de> for RegistryCondition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("RegistryCondition", REGISTRY_FIELDS, RegistryVisitor)
    }
}

const REGISTRY_FIELDS: &[&str] = &["key", "value", "test"];

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum RegistryField {
    Key,
    Value,
    Test,
}

struct RegistryVisitor;

impl<'de> Visitor<'de> for RegistryVisitor {
    type Value = RegistryCondition;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a registry condition, an object with `key`, `value`, and `test`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RegistryCondition, A::Error> {
        let mut key: Option<Template> = None;
        // `Some(None)` is a `value` of `null`.
        let mut value: Option<Option<RegValueName>> = None;
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
                    let needs_name = test.as_ref().is_some_and(Test::takes_operand);
                    value = Some(map.next_value_seed(ValueNameSeed { needs_name })?);
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
        RegistryCondition::new(key, value.flatten(), test).map_err(de::Error::custom)
    }
}

/// Reads `value`. `needs_name` says that the `test` read before it takes data.
struct ValueNameSeed {
    needs_name: bool,
}

impl<'de> DeserializeSeed<'de> for ValueNameSeed {
    type Value = Option<RegValueName>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let name = Option::<RegValueName>::deserialize(deserializer)?;
        if name.is_none() && self.needs_name {
            return Err(de::Error::custom(RegistryConditionError));
        }
        Ok(name)
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
            return Err(de::Error::custom(RegistryConditionError));
        }
        Ok(data)
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
    ProductCode(Uuid),
    UpgradeCode(Uuid),
}
