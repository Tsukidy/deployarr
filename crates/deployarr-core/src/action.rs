use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::num::NonZeroU32;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::non_empty::{NonEmpty, NonEmptySeed};
use crate::template::Template;
use crate::validated_string::validated_string;
use crate::variable::{SettableVarName, VarName};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    RunCommand {
        command: Template,
        working_dir: Option<Template>,
        #[serde(default)]
        exit: ExitCodes,
        #[serde(default)]
        limits: StepLimits,
    },
    #[serde(rename = "run_powershell")]
    RunPowerShell {
        script: ContentRef,
        args: Vec<Template>,
        #[serde(default)]
        exit: ExitCodes,
        #[serde(default)]
        limits: StepLimits,
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
    VerifyDrivers {
        policy: MissingDriverPolicy,
    },
    CheckReadiness {
        #[serde(default)]
        checks: ReadinessChecks,
    },
    ActivateWindows {
        online: bool,
    },
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
