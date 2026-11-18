//! Join profiles, which say how a PC joins a workgroup, a domain, or a
//! directory that an add-on handles.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::action::{EmptyIdError, check_not_empty};
use crate::addon::AddOnRef;
use crate::catalog::Catalog;
use crate::form::{
    ConfigProblem, UNREADABLE_CONFIG, ValueError, check_config, config_error, no_config_schema,
};
use crate::plan::Severity;
use crate::sequence::RunIn;
use crate::validated_string::validated_string;

/// The stage of a deployment that an `ApplyJoinProfile` join slot marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinStage {
    /// The applied image, in WinPE.
    OfflineImage,
    /// The installed Windows.
    FullOs,
}

impl JoinStage {
    /// The `run_in` that a join slot at this stage must have. The offline
    /// image needs `WinPe`, and the installed Windows needs `FullOs`.
    /// [`crate::plan::validate`] and [`crate::plan::plan`] reject a join slot
    /// with any other `run_in`, `Either` included. A `Workgroup` or
    /// `Domain` profile expands the slot to nothing, so no later check would
    /// catch it.
    pub fn run_in(self) -> RunIn {
        match self {
            Self::OfflineImage => RunIn::WinPe,
            Self::FullOs => RunIn::FullOs,
        }
    }
}

/// The stage as the JSON format writes it.
pub(crate) fn stage_name(stage: JoinStage) -> &'static str {
    match stage {
        JoinStage::OfflineImage => "offline_image",
        JoinStage::FullOs => "full_os",
    }
}

validated_string! {
    /// Names the join account's password in a `Domain` profile, a secret that
    /// the server stores with the profile. It never holds the password. It is
    /// a non-empty string, and no add-on's form holds the secret, so it is not
    /// a [`crate::action::SecretRef`].
    #[derive(PartialEq, Eq, Hash)]
    JoinPasswordRef, EmptyIdError, check_not_empty
}

/// How a PC joins a workgroup or a directory. An admin defines profiles on
/// the server, and the `JoinProfile` variable names the one a deployment uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum JoinProfile {
    /// The unattend file names the workgroup.
    Workgroup { name: String },
    /// The unattend file joins the domain in the specialize pass.
    Domain {
        domain: String,
        /// The full LDAP path of the OU.
        ou: Option<String>,
        account: String,
        password: JoinPasswordRef,
    },
    /// A join kind that an add-on declares, such as Entra ID join. Its config
    /// keeps the JSON text that was saved, as an add-on step's config does.
    AddOn {
        kind: AddOnRef,
        config_version: u32,
        config: Box<RawValue>,
    },
}

/// Two profiles are equal when their fields are, and an add-on profile's
/// config texts are equal byte for byte.
impl PartialEq for JoinProfile {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Workgroup { name }, Self::Workgroup { name: other_name }) => name == other_name,
            (
                Self::Domain {
                    domain,
                    ou,
                    account,
                    password,
                },
                Self::Domain {
                    domain: other_domain,
                    ou: other_ou,
                    account: other_account,
                    password: other_password,
                },
            ) => {
                domain == other_domain
                    && ou == other_ou
                    && account == other_account
                    && password == other_password
            }
            (
                Self::AddOn {
                    kind,
                    config_version,
                    config,
                },
                Self::AddOn {
                    kind: other_kind,
                    config_version: other_version,
                    config: other_config,
                },
            ) => {
                kind == other_kind
                    && config_version == other_version
                    && config.get() == other_config.get()
            }
            (Self::Workgroup { .. } | Self::Domain { .. } | Self::AddOn { .. }, _) => false,
        }
    }
}

impl Eq for JoinProfile {}

/// Checks a join profile against the join kinds in the catalog, and returns
/// what it finds. Pure. The server runs it when a profile is saved.
///
/// A `Workgroup` or `Domain` profile gets no finding. For an add-on's join
/// kind, it checks the profile's config against the kind's schema at the
/// profile's `config_version`, as [`crate::plan::validate`] does for an add-on
/// step. The kind's [`crate::catalog::Availability`] changes nothing.
pub fn validate_join_profile(profile: &JoinProfile, catalog: &Catalog) -> Vec<JoinProfileIssue> {
    match profile {
        JoinProfile::Workgroup { .. } | JoinProfile::Domain { .. } => Vec::new(),
        JoinProfile::AddOn {
            kind,
            config_version,
            config,
        } => {
            let Some(info) = catalog.join_kind(kind) else {
                return Vec::from([JoinProfileIssue::UnknownJoinKind { kind: kind.clone() }]);
            };
            check_config(&info.config, *config_version, config)
                .into_iter()
                .map(|problem| match problem {
                    ConfigProblem::NoSchema => JoinProfileIssue::NoConfigSchema {
                        kind: kind.clone(),
                        config_version: *config_version,
                    },
                    ConfigProblem::Unreadable => JoinProfileIssue::UnreadableConfig,
                    ConfigProblem::Value(error) => JoinProfileIssue::Config(error),
                })
                .collect()
        }
    }
}

/// What [`validate_join_profile`] finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinProfileIssue {
    /// No installed add-on, and no descriptor that the server keeps for a
    /// removed one, declares the profile's join kind. The profile saves.
    UnknownJoinKind { kind: AddOnRef },
    /// The join kind's descriptor holds no schema for the profile's
    /// `config_version`, so its config could not be checked. The profile
    /// saves.
    NoConfigSchema { kind: AddOnRef, config_version: u32 },
    /// The profile's config does not fit the schema at its `config_version`.
    /// [`ValueError::path`] is a JSON pointer into the config.
    Config(ValueError),
    /// The profile's config text holds a value that a [`serde_json::Value`]
    /// cannot hold, such as a number out of range, so the schema check cannot
    /// read it.
    UnreadableConfig,
}

impl JoinProfileIssue {
    pub fn severity(&self) -> Severity {
        match self {
            Self::UnknownJoinKind { .. } | Self::NoConfigSchema { .. } => Severity::Warning,
            Self::Config(_) | Self::UnreadableConfig => Severity::Error,
        }
    }
}

impl fmt::Display for JoinProfileIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownJoinKind { kind } => write!(
                f,
                "no add-on that this server knows declares the join kind `{}` of `{}`",
                kind.kind, kind.addon
            ),
            Self::NoConfigSchema {
                kind,
                config_version,
            } => no_config_schema(f, "join", kind, *config_version),
            Self::Config(error) => config_error(f, error),
            Self::UnreadableConfig => f.write_str(UNREADABLE_CONFIG),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_names_are_the_json_values() {
        for stage in [JoinStage::OfflineImage, JoinStage::FullOs] {
            let json = serde_json::to_value(stage).expect("a stage serializes");
            assert_eq!(json.as_str(), Some(stage_name(stage)));
        }
    }
}
