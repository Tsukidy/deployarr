use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::template::Template;
use crate::validated_string::validated_string;
use crate::variable::VarName;

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
        name: VarName,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExitCodes {
    pub success: Vec<i32>,
    /// Codes that mean success, then a reboot.
    pub reboot: Vec<i32>,
}

impl Default for ExitCodes {
    fn default() -> Self {
        Self {
            success: vec![0],
            reboot: vec![3010, 1641],
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
