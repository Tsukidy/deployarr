use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::net::Ipv6Addr;
use core::num::{NonZeroU8, NonZeroU32};

use serde::de::value::StrDeserializer;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, forward_to_deserialize_any};
use uuid::Uuid;

use crate::addon::AddOnStep;
use crate::join::JoinStage;
use crate::non_empty::{NonEmpty, NonEmptySeed};
use crate::sequence::RunIn;
use crate::template::Template;
use crate::validated_string::validated_string;
use crate::variable::{SettableVarName, VarName};

/// What a stored step holds. An admin edits it and the server stores it.
///
/// [`crate::lower::lower`] replaces each add-on step and join slot with
/// built-in steps before the server compiles, so the compiler and the agent
/// see only [`BuiltinAction`].
///
/// `Deserialize` is written by hand. It reads the action object's one key and
/// passes the value on without buffering it:
///
/// - `add_on` goes to [`AddOnStep`].
/// - `apply_join_profile` goes to the join slot.
/// - Any other key goes to [`BuiltinAction`] through its enum access.
///
/// A derived enum would need `#[serde(untagged)]` on `Builtin`, so that a
/// built-in action keeps its own key. serde buffers the whole input for an
/// untagged variant, and the add-on step's config then fails to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Written in JSON exactly as the built-in action, such as
    /// `{"run_command": {...}}`.
    Builtin(BuiltinAction),
    /// `{"add_on": {...}}`.
    AddOn(AddOnStep),
    /// `{"apply_join_profile": {"stage": "full_os"}}`. The deployment's join
    /// profile fills this slot when the plan is made.
    ApplyJoinProfile { stage: JoinStage },
}

const ADD_ON: &str = "add_on";
const APPLY_JOIN_PROFILE: &str = "apply_join_profile";

/// The body of an `apply_join_profile` action.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinSlot {
    stage: JoinStage,
}

impl Serialize for Action {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Builtin(action) => action.serialize(serializer),
            Self::AddOn(step) => serializer.serialize_newtype_variant("Action", 1, ADD_ON, step),
            Self::ApplyJoinProfile { stage } => serializer.serialize_newtype_variant(
                "Action",
                2,
                APPLY_JOIN_PROFILE,
                &JoinSlot { stage: *stage },
            ),
        }
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // serde_json does not read this list. An unknown key's error lists
        // every key, the built-in ones included. See `UnknownAction`.
        deserializer.deserialize_enum("Action", &[ADD_ON, APPLY_JOIN_PROFILE], ActionVisitor)
    }
}

struct ActionVisitor;

impl<'de> Visitor<'de> for ActionVisitor {
    type Value = Action;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an action, an object with one key")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Action, A::Error> {
        let (key, variant): (String, _) = data.variant()?;
        match key.as_str() {
            ADD_ON => variant.newtype_variant().map(Action::AddOn),
            APPLY_JOIN_PROFILE => variant
                .newtype_variant()
                .map(|JoinSlot { stage }| Action::ApplyJoinProfile { stage }),
            _ => BuiltinAction::deserialize(BuiltinVariant { key: &key, variant })
                .map(Action::Builtin),
        }
    }
}

/// An action's key, already read, and the access to its value. It hands both
/// to `BuiltinAction`'s derived `Deserialize`, so the value is read in place.
struct BuiltinVariant<'k, A> {
    key: &'k str,
    variant: A,
}

impl<'de, A: VariantAccess<'de>> Deserializer<'de> for BuiltinVariant<'_, A> {
    type Error = A::Error;

    fn deserialize_any<W: Visitor<'de>>(self, visitor: W) -> Result<W::Value, A::Error> {
        visitor.visit_enum(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de, A: VariantAccess<'de>> EnumAccess<'de> for BuiltinVariant<'_, A> {
    type Error = A::Error;
    type Variant = A;

    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, A), A::Error> {
        let key: StrDeserializer<'_, KeyError> = self.key.into_deserializer();
        match seed.deserialize(key) {
            Ok(value) => Ok((value, self.variant)),
            Err(KeyError::Unknown { builtins }) => Err(de::Error::custom(UnknownAction {
                key: self.key,
                builtins,
            })),
            Err(KeyError::Other(message)) => Err(de::Error::custom(message)),
        }
    }
}

/// The error of reading a built-in action's key. It keeps the list of
/// built-in keys that `BuiltinAction`'s derived `Deserialize` passes when the
/// key is unknown.
#[derive(Debug)]
enum KeyError {
    Unknown { builtins: &'static [&'static str] },
    Other(String),
}

impl de::Error for KeyError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Other(message.to_string())
    }

    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self::Unknown { builtins: expected }
    }
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { .. } => f.write_str("unknown action"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for KeyError {}

/// An action key that no action has. Like serde's error for an unknown
/// variant, it lists every key.
struct UnknownAction<'a> {
    key: &'a str,
    builtins: &'static [&'static str],
}

impl fmt::Display for UnknownAction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { key, builtins } = self;
        write!(
            f,
            "unknown variant `{key}`, expected one of `{ADD_ON}`, `{APPLY_JOIN_PROFILE}`"
        )?;
        for builtin in *builtins {
            write!(f, ", `{builtin}`")?;
        }
        Ok(())
    }
}

/// Everything the agent can run. None of these actions knows what an add-on
/// is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BuiltinAction {
    RunCommand {
        command: Template,
        working_dir: Option<Template>,
        #[serde(default)]
        exit: ExitCodes,
        #[serde(default)]
        limits: StepLimits,
        #[serde(default)]
        files: StepFiles,
    },
    #[serde(rename = "run_powershell")]
    RunPowerShell {
        script: ContentRef,
        args: Vec<Template>,
        #[serde(default)]
        exit: ExitCodes,
        #[serde(default)]
        limits: StepLimits,
        #[serde(default)]
        files: StepFiles,
    },
    SetVariable {
        name: SettableVarName,
        value: Template,
    },
    Restart {
        into: RestartTarget,
    },
    PartitionDisk {
        layout: DiskLayoutId,
        #[serde(default)]
        disk: DiskSelector,
    },
    ApplyOsImage {
        image: OsImageRef,
    },
    ApplyDrivers {
        selection: DriverSelection,
    },
    ApplyUnattend {
        template: UnattendTemplateId,
    },
    InstallApplications {
        apps: AppSelection,
    },
    /// Installs software updates from Windows Update or from a WSUS server,
    /// in passes. Every field has a default, so `{"install_updates": {}}` is a
    /// whole step.
    InstallUpdates {
        #[serde(default)]
        source: UpdateSource,
        /// What the step does when updates are still missing after
        /// `max_passes` passes.
        #[serde(default)]
        missing: MissingUpdatePolicy,
        /// How many passes may install updates. The default is 7.
        #[serde(default = "default_max_passes")]
        max_passes: NonZeroU8,
        /// Updates that the step never installs. The default is none.
        #[serde(default)]
        exclude: Vec<UpdateExclusion>,
        #[serde(default)]
        timeouts: UpdateTimeouts,
    },
    VerifyDrivers {
        policy: MissingDriverPolicy,
    },
    /// Turns BitLocker on for the volume of the running Windows, with a TPM
    /// protector and a recovery password. The recovery password stays on the
    /// PC. Only the full OS runs it.
    #[serde(rename = "enable_bitlocker")]
    EnableBitLocker {
        #[serde(default)]
        method: EncryptionMethod,
        /// `false` encrypts used space only, and `true` the whole volume.
        #[serde(default)]
        full_disk: bool,
        /// How long the step waits for the volume to be fully encrypted, in
        /// whole seconds. None ends the step while encryption goes on.
        wait_secs: Option<NonZeroU32>,
        #[serde(default)]
        tpm_failure: TpmFailure,
    },
    /// Turns the key protectors of the volume of the running Windows off for a
    /// number of restarts. It never decrypts. Only the full OS runs it.
    #[serde(rename = "suspend_bitlocker")]
    SuspendBitLocker {
        #[serde(default)]
        restarts: SuspendRestarts,
    },
    CheckReadiness {
        #[serde(default)]
        checks: ReadinessChecks,
    },
    ActivateWindows {
        online: bool,
    },
    /// Sends the named variables to the server and assigns the variables that
    /// come back. The server finds the add-on and its config from the
    /// deployment and the node. The agent refuses to send a variable that is
    /// marked secret.
    ServerCall {
        send: Vec<VarName>,
        #[serde(default)]
        limits: StepLimits,
    },
    /// Adds a provisioning package to Windows with DISM. The agent writes the
    /// package to a private folder, builds the whole command line itself, and
    /// deletes the file after the step. It is the one action that takes a
    /// secret.
    AddProvisioningPackage {
        package: SecretRef,
        #[serde(default)]
        limits: StepLimits,
    },
}

impl BuiltinAction {
    /// The `run_in` values that a step with this action may have. The
    /// compiler rejects a step with any other.
    ///
    /// The steps that check the hardware and build the new Windows run only in
    /// WinPE, and the steps that need the installed Windows run only in the
    /// full OS. The match is exhaustive, so a new action does not compile until
    /// someone decides where it runs.
    pub fn allowed_run_in(&self) -> &'static [RunIn] {
        match self {
            Self::PartitionDisk { .. }
            | Self::ApplyOsImage { .. }
            | Self::ApplyDrivers { .. }
            | Self::ApplyUnattend { .. }
            | Self::CheckReadiness { .. } => &[RunIn::WinPe],
            Self::InstallApplications { .. }
            | Self::InstallUpdates { .. }
            | Self::VerifyDrivers { .. }
            | Self::EnableBitLocker { .. }
            | Self::SuspendBitLocker { .. }
            | Self::ActivateWindows { .. } => &[RunIn::FullOs],
            Self::RunCommand { .. }
            | Self::RunPowerShell { .. }
            | Self::SetVariable { .. }
            | Self::Restart { .. }
            | Self::ServerCall { .. }
            | Self::AddProvisioningPackage { .. } => &[RunIn::WinPe, RunIn::FullOs, RunIn::Either],
        }
    }

    /// Whether an add-on's expansion may hold this action. Six may:
    /// `RunCommand`, `RunPowerShell`, `SetVariable`, `Restart`, `ServerCall`,
    /// and `AddProvisioningPackage`.
    ///
    /// The match is exhaustive, so a new action does not compile until someone
    /// decides whether add-ons may use it.
    pub fn allowed_from_addon(&self) -> bool {
        match self {
            Self::RunCommand { .. }
            | Self::RunPowerShell { .. }
            | Self::SetVariable { .. }
            | Self::Restart { .. }
            | Self::ServerCall { .. }
            | Self::AddProvisioningPackage { .. } => true,
            Self::PartitionDisk { .. }
            | Self::ApplyOsImage { .. }
            | Self::ApplyDrivers { .. }
            | Self::ApplyUnattend { .. }
            | Self::InstallApplications { .. }
            | Self::InstallUpdates { .. }
            | Self::VerifyDrivers { .. }
            | Self::EnableBitLocker { .. }
            | Self::SuspendBitLocker { .. }
            | Self::CheckReadiness { .. }
            | Self::ActivateWindows { .. } => false,
        }
    }
}

/// A file that the agent writes to the step's private folder before the step
/// and deletes after it. The command reads the folder's path from
/// `%_StepDir%`.
///
/// A step file holds content only, such as a script from an add-on's assets.
/// It has no field that can name a secret, because the command's author could
/// send the secret back to the server through `ServerCall`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepFile {
    pub name: FileName,
    pub content: ContentRef,
}

/// A step's files. No two files have names that are equal in any case,
/// because Windows does not assume that file names differ by case alone.
///
/// It is written as a JSON array of files. When two files have one name, the
/// parse error path names the second file's name, such as `files[1].name`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepFiles(Vec<StepFile>);

impl StepFiles {
    /// Fails at the first file whose name an earlier file already has.
    pub fn new(files: Vec<StepFile>) -> Result<Self, StepFilesError> {
        let mut seen = SeenFileNames::default();
        for (index, file) in files.iter().enumerate() {
            if let Some(first) = seen.find(&file.name) {
                return Err(StepFilesError {
                    first,
                    second: index,
                    name: files[first].name.clone(),
                });
            }
            seen.insert(&file.name, index);
        }
        Ok(Self(files))
    }

    pub fn as_slice(&self) -> &[StepFile] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<StepFile> {
        self.0
    }
}

impl TryFrom<Vec<StepFile>> for StepFiles {
    type Error = StepFilesError;

    fn try_from(files: Vec<StepFile>) -> Result<Self, Self::Error> {
        Self::new(files)
    }
}

impl<'a> IntoIterator for &'a StepFiles {
    type Item = &'a StepFile;
    type IntoIter = core::slice::Iter<'a, StepFile>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Two step files have names that are equal in any case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepFilesError {
    /// The index of the earlier file.
    pub first: usize,
    /// The index of the later file. A parse error path names this file.
    pub second: usize,
    /// The earlier file's name, as its author wrote it.
    pub name: FileName,
}

impl fmt::Display for StepFilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { first, name, .. } = self;
        write!(
            f,
            "files[{first}] already has this name, as `{name}`, and file names ignore case"
        )
    }
}

impl core::error::Error for StepFilesError {}

/// The names of the files read so far, each with its index. The key is the
/// name in lower case. A `FileName` holds only ASCII, so two names are equal
/// in any case exactly when their keys are.
#[derive(Default)]
struct SeenFileNames(BTreeMap<String, usize>);

impl SeenFileNames {
    /// The index of the earlier file that has `name`, if one does.
    fn find(&self, name: &FileName) -> Option<usize> {
        self.0.get(&name.as_str().to_ascii_lowercase()).copied()
    }

    fn insert(&mut self, name: &FileName, index: usize) {
        self.0.insert(name.as_str().to_ascii_lowercase(), index);
    }
}

impl Serialize for StepFiles {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StepFiles {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(StepFilesVisitor).map(Self)
    }
}

struct StepFilesVisitor;

impl<'de> Visitor<'de> for StepFilesVisitor {
    type Value = Vec<StepFile>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of step files")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut files = Vec::new();
        let mut seen = SeenFileNames::default();
        while let Some(file) = seq.next_element_seed(StepFileSeed {
            earlier: &files,
            seen: &seen,
        })? {
            seen.insert(&file.name, files.len());
            files.push(file);
        }
        Ok(files)
    }
}

/// One step file, read with the files before it. `seen` holds the names in
/// `earlier`.
struct StepFileSeed<'a> {
    earlier: &'a [StepFile],
    seen: &'a SeenFileNames,
}

const STEP_FILE_FIELDS: &[&str] = &["name", "content"];

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum StepFileField {
    Name,
    Content,
}

impl<'de> DeserializeSeed<'de> for StepFileSeed<'_> {
    type Value = StepFile;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_struct("StepFile", STEP_FILE_FIELDS, self)
    }
}

impl<'de> Visitor<'de> for StepFileSeed<'_> {
    type Value = StepFile;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a step file, an object with `name` and `content`")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut name = None;
        let mut content = None;
        while let Some(field) = map.next_key()? {
            match field {
                StepFileField::Name => {
                    if name.is_some() {
                        return Err(de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value_seed(UniqueFileName {
                        earlier: self.earlier,
                        seen: self.seen,
                    })?);
                }
                StepFileField::Content => {
                    if content.is_some() {
                        return Err(de::Error::duplicate_field("content"));
                    }
                    content = Some(map.next_value()?);
                }
            }
        }
        Ok(StepFile {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            content: content.ok_or_else(|| de::Error::missing_field("content"))?,
        })
    }
}

/// A step file's name, which must differ from every earlier file's name in
/// any case.
struct UniqueFileName<'a> {
    earlier: &'a [StepFile],
    seen: &'a SeenFileNames,
}

impl<'de> DeserializeSeed<'de> for UniqueFileName<'_> {
    type Value = FileName;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let name = FileName::deserialize(deserializer)?;
        match self.seen.find(&name) {
            Some(first) => Err(de::Error::custom(StepFilesError {
                first,
                second: self.earlier.len(),
                name: self.earlier[first].name.clone(),
            })),
            None => Ok(name),
        }
    }
}

/// The exit codes that end a command step with success.
///
/// A code at or above `0x80000000`, such as an HRESULT, is written as its
/// signed value, so `0x80070005` is `-2147024891`. `success` is never empty,
/// and no code is in both lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExitCodes {
    success: NonEmpty<i32>,
    /// Codes that mean success, then a reboot.
    reboot: Vec<i32>,
}

impl ExitCodes {
    pub fn new(success: NonEmpty<i32>, reboot: Vec<i32>) -> Result<Self, ExitCodesError> {
        let sorted_success = sorted(&success);
        match reboot
            .iter()
            .find(|code| sorted_success.binary_search(code).is_ok())
        {
            Some(&code) => Err(ExitCodesError { code }),
            None => Ok(Self { success, reboot }),
        }
    }

    pub fn success(&self) -> &[i32] {
        &self.success
    }

    pub fn reboot(&self) -> &[i32] {
        &self.reboot
    }
}

impl Default for ExitCodes {
    fn default() -> Self {
        Self {
            success: NonEmpty::new(0),
            reboot: vec![3010, 1641],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCodesError {
    pub code: i32,
}

impl fmt::Display for ExitCodesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { code } = self;
        write!(
            f,
            "exit code {code} is in both `success` and `reboot`, and a code means only one of them"
        )
    }
}

impl core::error::Error for ExitCodesError {}

/// Reads `ExitCodes` from a map. An omitted list takes its default.
///
/// Each code is checked against the other list as it is read, so a code that
/// is in both fails at the second one, such as `exit.reboot[0]`. A code that is
/// in a written list and in the other list's default fails at `exit`.
impl<'de> Deserialize<'de> for ExitCodes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("ExitCodes", EXIT_CODES_FIELDS, ExitCodesVisitor)
    }
}

const EXIT_CODES_FIELDS: &[&str] = &["success", "reboot"];

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum ExitCodesField {
    Success,
    Reboot,
}

struct ExitCodesVisitor;

impl<'de> Visitor<'de> for ExitCodesVisitor {
    type Value = ExitCodes;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("exit codes, an object with `success` and `reboot` lists")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ExitCodes, A::Error> {
        let mut success: Option<NonEmpty<i32>> = None;
        let mut reboot: Option<Vec<i32>> = None;
        while let Some(field) = map.next_key()? {
            match field {
                ExitCodesField::Success => {
                    if success.is_some() {
                        return Err(de::Error::duplicate_field("success"));
                    }
                    let other = reboot.as_deref().map(sorted);
                    let other = other.as_deref();
                    success = Some(map.next_value_seed(NonEmptySeed(CodeList { other }))?);
                }
                ExitCodesField::Reboot => {
                    if reboot.is_some() {
                        return Err(de::Error::duplicate_field("reboot"));
                    }
                    let other = success.as_deref().map(sorted);
                    let other = other.as_deref();
                    reboot = Some(map.next_value_seed(CodeList { other })?);
                }
            }
        }
        let defaults = ExitCodes::default();
        // The list the document omits, if it omits one, and that list's default.
        let (success, reboot, omitted) = match (success, reboot) {
            (Some(success), Some(reboot)) => (success, reboot, None),
            (Some(success), None) => (
                success,
                defaults.reboot.clone(),
                Some(("reboot", defaults.reboot())),
            ),
            (None, Some(reboot)) => (
                defaults.success.clone(),
                reboot,
                Some(("success", defaults.success())),
            ),
            (None, None) => return Ok(defaults),
        };
        ExitCodes::new(success, reboot).map_err(|error| match omitted {
            Some((list, codes)) => {
                de::Error::custom(format_args!("{error}. An omitted `{list}` is {codes:?}"))
            }
            None => de::Error::custom(error),
        })
    }
}

/// A sorted copy of a list of codes, so a lookup in it takes a binary search
/// and a document with long lists cannot make the check quadratic.
fn sorted(codes: &[i32]) -> Vec<i32> {
    let mut codes = codes.to_vec();
    codes.sort_unstable();
    codes
}

/// One list of exit codes, checked code by code against the list read before
/// it. `other` is that list, sorted.
struct CodeList<'a> {
    other: Option<&'a [i32]>,
}

impl<'de> DeserializeSeed<'de> for CodeList<'_> {
    type Value = Vec<i32>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Vec<i32>, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for CodeList<'_> {
    type Value = Vec<i32>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of exit codes")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<i32>, A::Error> {
        let mut codes = Vec::new();
        while let Some(code) = seq.next_element_seed(Code { other: self.other })? {
            codes.push(code);
        }
        Ok(codes)
    }
}

struct Code<'a> {
    other: Option<&'a [i32]>,
}

impl<'de> DeserializeSeed<'de> for Code<'_> {
    type Value = i32;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<i32, D::Error> {
        let code = i32::deserialize(deserializer)?;
        match self.other {
            Some(other) if other.binary_search(&code).is_ok() => {
                Err(de::Error::custom(ExitCodesError { code }))
            }
            _ => Ok(code),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StepLimits {
    pub max_runtime_secs: Option<NonZeroU32>,
    pub rerun_if_interrupted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartTarget {
    WinPe,
    InstalledOs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskSelector {
    /// The first internal, non-removable disk.
    #[default]
    FirstInternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverSelection {
    Auto,
    Profile,
    ProfileAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingDriverPolicy {
    Fail,
    Warn,
    Ignore,
}

/// What `InstallUpdates` does when its last search, after `max_passes`
/// passes, still finds updates to install.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingUpdatePolicy {
    /// The step fails.
    Fail,
    /// The step succeeds with a warning.
    #[default]
    Warn,
    /// The step succeeds.
    Ignore,
}

/// Where `InstallUpdates` gets updates.
///
/// `Deserialize` is written by hand, so that no error repeats a string that
/// the author wrote in `source`. serde's error for an unknown variant repeats
/// it, and an author can write the URL itself as the source, such as
/// `"source": "https://admin:pass@wsus.example.com"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateSource {
    /// Microsoft's Windows Update service over the internet.
    #[default]
    WindowsUpdate,
    /// The WSUS server at this URL.
    Wsus(WsusUrl),
}

const WINDOWS_UPDATE: &str = "windows_update";
const WSUS: &str = "wsus";

impl<'de> Deserialize<'de> for UpdateSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_enum("UpdateSource", &[WINDOWS_UPDATE, WSUS], UpdateSourceVisitor)
    }
}

struct UpdateSourceVisitor;

impl<'de> Visitor<'de> for UpdateSourceVisitor {
    type Value = UpdateSource;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an update source")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<UpdateSource, A::Error> {
        let (kind, variant) = data.variant()?;
        match kind {
            UpdateSourceKind::WindowsUpdate => {
                // serde's error for `{"windows_update": "..."}` repeats the
                // string.
                variant
                    .unit_variant()
                    .map_err(|_| de::Error::custom("the `windows_update` source takes no value"))?;
                Ok(UpdateSource::WindowsUpdate)
            }
            UpdateSourceKind::Wsus => variant.newtype_variant().map(UpdateSource::Wsus),
        }
    }
}

/// The key of an [`UpdateSource`].
enum UpdateSourceKind {
    WindowsUpdate,
    Wsus,
}

impl<'de> Deserialize<'de> for UpdateSourceKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_identifier(UpdateSourceKindVisitor)
    }
}

struct UpdateSourceKindVisitor;

impl Visitor<'_> for UpdateSourceKindVisitor {
    type Value = UpdateSourceKind;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("`windows_update` or `wsus`")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<UpdateSourceKind, E> {
        match key {
            WINDOWS_UPDATE => Ok(UpdateSourceKind::WindowsUpdate),
            WSUS => Ok(UpdateSourceKind::Wsus),
            // Unlike serde's error, this one does not repeat the key.
            _ => Err(E::custom(
                "unknown update source, expected `windows_update` or `wsus`",
            )),
        }
    }
}

/// An update that `InstallUpdates` never installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateExclusion {
    /// A KB article number without the `KB` prefix, such as `5034441`.
    Kb(NonZeroU32),
    /// An update ID. It is written as a GUID in lower case without braces.
    UpdateId(Uuid),
}

fn default_max_passes() -> NonZeroU8 {
    const DEFAULT: NonZeroU8 = NonZeroU8::new(7).unwrap();
    DEFAULT
}

/// How long one phase of one pass of `InstallUpdates` may take, in whole
/// seconds. No limit is zero, because a zero limit fails every pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateTimeouts {
    /// The default is 3600.
    pub search_secs: NonZeroU32,
    /// The default is 7200.
    pub download_secs: NonZeroU32,
    /// The default is 7200.
    pub install_secs: NonZeroU32,
}

impl Default for UpdateTimeouts {
    fn default() -> Self {
        const DEFAULT: UpdateTimeouts = UpdateTimeouts {
            search_secs: NonZeroU32::new(3600).unwrap(),
            download_secs: NonZeroU32::new(7200).unwrap(),
            install_secs: NonZeroU32::new(7200).unwrap(),
        };
        DEFAULT
    }
}

validated_string! {
    /// The URL of a WSUS server, such as `https://wsus.example.com:8531`.
    ///
    /// It starts with `http://` or `https://` and names a host. A host in
    /// brackets is an IPv6 address, and no other bracket appears. A port after
    /// the host is empty or a number from 0 to 65535. At most one `#`
    /// appears, the one that starts the fragment. It holds only ASCII
    /// letters, digits, and the symbols `-._~:/?#[]@!$&'()*+,;=%`, which are
    /// the characters that RFC 3986 allows in a URI. Each `%` starts a
    /// percent-encoded byte such as `%20`. The parser rejects an `@`
    /// anywhere in the URL, not only the one that ends user information such
    /// as `user:pass@`, to keep a credential out of the registry and the logs.
    /// No error message repeats the text.
    ///
    /// The parser accepts the scheme in any case and writes it in lower case,
    /// its canonical form. It keeps the rest of the URL as written.
    #[derive(PartialEq, Eq, Hash)]
    WsusUrl, WsusUrlError, check_wsus_url, lower_case_scheme
}

/// The schemes of a [`WsusUrl`], each with the `://` that starts its
/// authority.
const WSUS_SCHEMES: [&str; 2] = ["http://", "https://"];

/// The symbols that a URI may hold: the unreserved symbols, the reserved
/// symbols, and the `%` of a percent-encoded byte (RFC 3986, sections 2.1 to
/// 2.3).
const URI_SYMBOLS: &str = "-._~:/?#[]@!$&'()*+,;=%";

fn check_wsus_url(text: &str) -> Result<(), WsusUrlError> {
    if text.is_empty() {
        return Err(WsusUrlError::Empty);
    }
    // A URI holds only letters, digits, and a few graphic symbols (RFC 3986,
    // section 2). Sections 2.2 and 2.3 list the symbols.
    if !text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || URI_SYMBOLS.contains(c))
    {
        return Err(WsusUrlError::InvalidChar);
    }
    // A `%` starts a percent-encoded byte, a `%` and two hex digits (RFC
    // 3986, section 2.1).
    let encoded = |after: &str| {
        after
            .get(..2)
            .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    if !text.split('%').skip(1).all(encoded) {
        return Err(WsusUrlError::InvalidChar);
    }
    // A scheme is case-insensitive, and a parser should accept upper case
    // (RFC 3986, section 3.1).
    let rest = WSUS_SCHEMES
        .iter()
        .find_map(|scheme| {
            let (prefix, rest) = text.split_at_checked(scheme.len())?;
            prefix.eq_ignore_ascii_case(scheme).then_some(rest)
        })
        .ok_or(WsusUrlError::Scheme)?;
    // An `@` in the authority ends the user information (RFC 3986, section
    // 3.2.1). The owner chose to reject an `@` anywhere, so that a password
    // whose unencoded `#`, `/`, or `?` ends the authority early still fails.
    if rest.contains('@') {
        return Err(WsusUrlError::UserInfo);
    }
    // The first `#` starts the fragment (RFC 3986, section 3.5). A fragment
    // holds path characters, `/`, and `?`, and a path character is never a
    // `#` (section 3.3).
    if rest
        .split_once('#')
        .is_some_and(|(_, fragment)| fragment.contains('#'))
    {
        return Err(WsusUrlError::Fragment);
    }
    // The authority ends at the first `/`, `?`, or `#` (RFC 3986, section
    // 3.2).
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // A host in brackets is an IP literal. A bracket appears nowhere else in
    // a URI (RFC 3986, section 3.2.2).
    if rest[authority.len()..].contains(['[', ']']) {
        return Err(WsusUrlError::IpLiteral);
    }
    let (host, after_host) = match authority.strip_prefix('[') {
        Some(literal) => {
            let (address, after) = literal.split_once(']').ok_or(WsusUrlError::IpLiteral)?;
            // An IP literal is an IPv6 address or a future format that starts
            // with a version flag, a `v` and a hex number. An application that
            // does not know the version flag should return an error (RFC 3986,
            // section 3.2.2), and deployarr knows none.
            if address.parse::<Ipv6Addr>().is_err() {
                return Err(WsusUrlError::IpLiteral);
            }
            (address, after)
        }
        None => authority.split_at(authority.find(':').unwrap_or(authority.len())),
    };
    // An `http` or `https` URI with an empty host is invalid (RFC 9110,
    // sections 4.2.1 and 4.2.2).
    if host.is_empty() {
        return Err(WsusUrlError::NoHost);
    }
    // A bracket also fails in a host that is not an IP literal and in the
    // port.
    if host.contains(['[', ']']) || after_host.contains(['[', ']']) {
        return Err(WsusUrlError::IpLiteral);
    }
    // A `:` and a port may follow the host. The port is decimal digits (RFC
    // 3986, section 3.2.3), and an empty port means the default port (RFC
    // 9110, section 4.2.1).
    if !after_host.is_empty() {
        let port = after_host
            .strip_prefix(':')
            .ok_or(WsusUrlError::IpLiteral)?;
        // An `http` or `https` port is a TCP port, which is 16 bits (RFC 9110,
        // section 4.2.1, and RFC 9293, section 3.1). `u16::from_str` also
        // takes a leading `+`, which a port never has.
        let digits = port.bytes().all(|b| b.is_ascii_digit());
        if !(port.is_empty() || (digits && port.parse::<u16>().is_ok())) {
            return Err(WsusUrlError::Port);
        }
    }
    Ok(())
}

/// Writes the scheme of a checked [`WsusUrl`] in lower case.
fn lower_case_scheme(mut text: String) -> String {
    if let Some(end) = text.find(':') {
        text[..end].make_ascii_lowercase();
    }
    text
}

/// Why a [`WsusUrl`] fails to parse. No variant holds the text, so an error
/// never repeats a credential that the text holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsusUrlError {
    Empty,
    /// A character that RFC 3986 allows in no URI, such as a space or a `\`,
    /// or a `%` without two hex digits after it.
    InvalidChar,
    Scheme,
    /// An `@` anywhere in the URL, which can end user information.
    UserInfo,
    NoHost,
    /// A `[` without its `]`, text between the brackets that is not an IPv6
    /// address, a bracket elsewhere in the URL, or text other than a `:` and a
    /// port after the `]`.
    IpLiteral,
    /// A port that is not a number from 0 to 65535.
    Port,
    /// A second `#`. The first `#` starts the fragment, and a fragment holds
    /// no `#`.
    Fragment,
}

impl fmt::Display for WsusUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "a WSUS URL must not be empty",
            Self::InvalidChar => {
                "a WSUS URL holds only ASCII letters, digits, and the symbols `-._~:/?#[]@!$&'()*+,;=%`, and each `%` starts a percent-encoded byte such as `%20`"
            }
            Self::Scheme => "a WSUS URL starts with `http://` or `https://`",
            Self::UserInfo => {
                "a WSUS URL must not hold an `@`, so that no user information, such as `user:pass@`, reaches the registry or a log"
            }
            Self::NoHost => {
                "a WSUS URL names a host after `//`, such as `https://wsus.example.com:8531`"
            }
            Self::IpLiteral => {
                "a WSUS URL writes an IPv6 address in brackets, such as `https://[fd00::5]:8531`, and has no other bracket"
            }
            Self::Port => {
                "a WSUS URL's port, after the host and a `:`, is a number from 0 to 65535, such as `8531`"
            }
            Self::Fragment => {
                "a WSUS URL holds at most one `#`, the one that starts its fragment"
            }
        })
    }
}

impl core::error::Error for WsusUrlError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSelection {
    One(AppId),
    FromList(VarName),
}

/// The defaults are Microsoft's Windows 11 minimums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReadinessChecks {
    pub memory: MemoryCheck,
    pub disk: DiskCheck,
    pub processor: ProcessorCheck,
    pub uefi: CheckMode,
    pub tpm_2_0: CheckMode,
    pub secure_boot: CheckMode,
}

impl Default for ReadinessChecks {
    fn default() -> Self {
        Self {
            memory: MemoryCheck::default(),
            disk: DiskCheck::default(),
            processor: ProcessorCheck::default(),
            uefi: CheckMode::Fail,
            tpm_2_0: CheckMode::Fail,
            // Windows 11 requires firmware that can run Secure Boot, not Secure Boot turned on.
            secure_boot: CheckMode::Warn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckMode {
    Fail,
    Warn,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryCheck {
    pub min_mib: u32,
    pub mode: CheckMode,
}

impl Default for MemoryCheck {
    fn default() -> Self {
        Self {
            min_mib: 4096,
            mode: CheckMode::Fail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiskCheck {
    /// Decimal gigabytes, 10^9 bytes, the unit disks are sold in. A disk sold
    /// as 64 GB holds less than 64 GiB.
    pub min_gb: u32,
    pub mode: CheckMode,
}

impl Default for DiskCheck {
    fn default() -> Self {
        Self {
            min_gb: 64,
            mode: CheckMode::Fail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProcessorCheck {
    pub min_cores: u32,
    pub mode: CheckMode,
}

impl Default for ProcessorCheck {
    fn default() -> Self {
        Self {
            min_cores: 2,
            mode: CheckMode::Fail,
        }
    }
}

/// The algorithm and key size that `EnableBitLocker` asks for.
///
/// `OsDefault` leaves the choice to Windows, which picks XTS-AES 128 on
/// Windows 10 1511 and later. The others are the `EncryptionMethod` values
/// 3, 4, 6, and 7 of `Win32_EncryptableVolume`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionMethod {
    #[default]
    OsDefault,
    Aes128,
    Aes256,
    XtsAes128,
    XtsAes256,
}

/// What `EnableBitLocker` does when the PC has no usable TPM. Any other TPM
/// failure fails the step, whatever this says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TpmFailure {
    /// The step fails.
    #[default]
    Fail,
    /// The step ends with a warning, and has changed nothing.
    Skip,
}

/// The number of restarts after which BitLocker turns a suspended volume's
/// protection back on: 1 to 15. The default is 1.
///
/// There is no 0. A count of 0 suspends protection until something turns it
/// back on, so a deployment with no later step that does would ship the PC
/// with its key in the clear. The JSON is a bare integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct SuspendRestarts(u8);

impl SuspendRestarts {
    /// The fewest restarts.
    pub const MIN: u8 = 1;
    /// The most restarts.
    pub const MAX: u8 = 15;

    /// Fails for a count outside 1 to 15.
    pub fn new(count: u8) -> Result<Self, SuspendRestartsError> {
        if (Self::MIN..=Self::MAX).contains(&count) {
            Ok(Self(count))
        } else {
            Err(SuspendRestartsError { count })
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for SuspendRestarts {
    fn default() -> Self {
        Self(Self::MIN)
    }
}

impl TryFrom<u8> for SuspendRestarts {
    type Error = SuspendRestartsError;

    fn try_from(count: u8) -> Result<Self, Self::Error> {
        Self::new(count)
    }
}

impl From<SuspendRestarts> for u8 {
    fn from(restarts: SuspendRestarts) -> Self {
        restarts.0
    }
}

/// A count of restarts outside 1 to 15.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendRestartsError {
    pub count: u8,
}

impl fmt::Display for SuspendRestartsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { count } = self;
        write!(
            f,
            "BitLocker stays suspended for {} to {} restarts, not {count}",
            SuspendRestarts::MIN,
            SuspendRestarts::MAX
        )
    }
}

impl core::error::Error for SuspendRestartsError {}

macro_rules! non_empty_id {
    ($(#[$meta:meta])* $name:ident) => {
        validated_string! {
            $(#[$meta])*
            #[derive(PartialEq, Eq, Hash)]
            $name, EmptyIdError, check_not_empty
        }
    };
}

non_empty_id!(DiskLayoutId);
non_empty_id!(OsImageRef);
non_empty_id!(UnattendTemplateId);
non_empty_id!(ContentRef);
non_empty_id!(AppId);
non_empty_id!(
    /// Names a secret that the server holds, such as a provisioning package.
    /// It never holds the secret's value. Only an action that the base defines
    /// to take a secret has a field of this type.
    SecretRef
);

/// The longest [`FileName`].
pub const FILE_NAME_MAX_LEN: usize = 255;

validated_string! {
    /// The name of a step file in the step's private folder, such as
    /// `collect-hash.ps1`: 1 to 255 ASCII letters, digits, `.`, `_`, or `-`.
    ///
    /// The name does not end with `.`, so it is never `.` or `..`. The part
    /// before its first `.` is not a name that Windows reserves for a device,
    /// such as `CON` or `NUL`, in any case.
    #[derive(PartialEq, Eq, Hash)]
    FileName, FileNameError, check_file_name
}

/// The names that Windows reserves for devices. A name with one of these
/// before its first `.`, such as `NUL.txt`, is the device too.
const RESERVED_FILE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn check_file_name(text: &str) -> Result<(), FileNameError> {
    if text.is_empty() {
        return Err(FileNameError::Empty);
    }
    if let Some(found) = text
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    {
        return Err(FileNameError::InvalidChar { found });
    }
    if text.len() > FILE_NAME_MAX_LEN {
        return Err(FileNameError::TooLong { len: text.len() });
    }
    if text.ends_with('.') {
        return Err(FileNameError::EndsWithDot);
    }
    let stem = text.split('.').next().unwrap_or(text);
    if RESERVED_FILE_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return Err(FileNameError::Reserved);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileNameError {
    Empty,
    InvalidChar { found: char },
    TooLong { len: usize },
    EndsWithDot,
    Reserved,
}

impl fmt::Display for FileNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a file name must not be empty"),
            Self::InvalidChar { found } => write!(
                f,
                "a file name holds only ASCII letters, digits, `.`, `_`, and `-`, not {found:?}"
            ),
            Self::TooLong { len } => write!(
                f,
                "a file name holds at most {FILE_NAME_MAX_LEN} characters, and this one holds {len}"
            ),
            Self::EndsWithDot => f.write_str("a file name must not end with `.`"),
            Self::Reserved => f.write_str(
                "Windows reserves this name for a device, so it cannot name a file, with or without an extension",
            ),
        }
    }
}

impl core::error::Error for FileNameError {}

fn check_not_empty(text: &str) -> Result<(), EmptyIdError> {
    if text.is_empty() {
        Err(EmptyIdError)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyIdError;

impl fmt::Display for EmptyIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an ID must not be empty")
    }
}

impl core::error::Error for EmptyIdError {}
