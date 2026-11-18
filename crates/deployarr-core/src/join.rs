//! Join profiles, which say how a PC joins a workgroup, a domain, or a
//! directory that an add-on handles.

use alloc::boxed::Box;
use alloc::string::String;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::action::{EmptyIdError, check_not_empty};
use crate::addon::AddOnRef;
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
    /// [`crate::validate::validate`] and [`crate::lower::lower`] reject a join
    /// slot with any other `run_in`, `Either` included. A `Workgroup` or
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
