//! What add-ons contribute, as the web UI, `validate`, and the lookup chain
//! need it.
//!
//! The server builds the catalog from the manifests of the installed add-ons,
//! and from the descriptors that it keeps for every step kind and join kind
//! that was ever installed. A removed add-on's steps therefore still show as
//! forms.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::addon::{AddOnId, AddOnRef, KindId};
use crate::form::FormSchema;
use crate::lookup::ResolverInput;
use crate::prompt::PromptSpec;
use crate::sequence::RunIn;
use crate::validate::StepKinds;

/// Every add-on's contributions, by add-on ID.
///
/// It is written as a JSON object keyed by add-on ID, such as
/// `{"acme.autopilot": {"step_kinds": {"register": {...}}}}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Catalog {
    pub add_ons: BTreeMap<AddOnId, Contributions>,
}

impl Catalog {
    pub fn step_kind(&self, kind: &AddOnRef) -> Option<&StepKindInfo> {
        self.add_ons.get(&kind.addon)?.step_kinds.get(&kind.kind)
    }

    pub fn join_kind(&self, kind: &AddOnRef) -> Option<&JoinKindInfo> {
        self.add_ons.get(&kind.addon)?.join_kinds.get(&kind.kind)
    }

    pub fn resolver(&self, resolver: &AddOnRef) -> Option<&ResolverInfo> {
        self.add_ons
            .get(&resolver.addon)?
            .resolvers
            .get(&resolver.kind)
    }

    /// A prompt, named with the add-on that declares it, because a prompt ID
    /// is unique only within its add-on.
    pub fn prompt(&self, prompt: &AddOnRef) -> Option<&PromptSpec> {
        self.add_ons.get(&prompt.addon)?.prompts.get(&prompt.kind)
    }

    pub fn importer(&self, importer: &AddOnRef) -> Option<&ImporterInfo> {
        self.add_ons
            .get(&importer.addon)?
            .importers
            .get(&importer.kind)
    }
}

/// A step kind is declared when the catalog holds its descriptor, whether or
/// not its add-on is installed now.
impl StepKinds for Catalog {
    fn declares(&self, kind: &AddOnRef) -> bool {
        self.step_kind(kind).is_some()
    }
}

/// What one add-on contributes, each kind by its ID within the add-on.
///
/// Each prompt's key is its own ID, and a resolver with a prompt names a
/// prompt of the same add-on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ContributionsWire", into = "ContributionsWire")]
pub struct Contributions {
    pub step_kinds: BTreeMap<KindId, StepKindInfo>,
    pub join_kinds: BTreeMap<KindId, JoinKindInfo>,
    pub resolvers: BTreeMap<KindId, ResolverInfo>,
    pub prompts: BTreeMap<KindId, PromptSpec>,
    pub importers: BTreeMap<KindId, ImporterInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributionsWire {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    step_kinds: BTreeMap<KindId, StepKindInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    join_kinds: BTreeMap<KindId, JoinKindInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    resolvers: BTreeMap<KindId, ResolverInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    prompts: BTreeMap<KindId, PromptSpec>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    importers: BTreeMap<KindId, ImporterInfo>,
}

impl TryFrom<ContributionsWire> for Contributions {
    type Error = ContributionsError;

    fn try_from(wire: ContributionsWire) -> Result<Self, Self::Error> {
        let ContributionsWire {
            step_kinds,
            join_kinds,
            resolvers,
            prompts,
            importers,
        } = wire;
        if let Some((key, spec)) = prompts.iter().find(|(key, spec)| *key != spec.id()) {
            return Err(ContributionsError::PromptKey {
                key: key.clone(),
                id: spec.id().clone(),
            });
        }
        for (resolver, info) in &resolvers {
            if let ResolverInput::Prompt(prompt) = &info.input
                && !prompts.contains_key(prompt)
            {
                return Err(ContributionsError::UnknownPrompt {
                    resolver: resolver.clone(),
                    prompt: prompt.clone(),
                });
            }
        }
        Ok(Self {
            step_kinds,
            join_kinds,
            resolvers,
            prompts,
            importers,
        })
    }
}

impl From<Contributions> for ContributionsWire {
    fn from(contributions: Contributions) -> Self {
        let Contributions {
            step_kinds,
            join_kinds,
            resolvers,
            prompts,
            importers,
        } = contributions;
        Self {
            step_kinds,
            join_kinds,
            resolvers,
            prompts,
            importers,
        }
    }
}

/// Why an add-on's contributions do not fit together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContributionsError {
    /// A prompt is filed under a key that is not its ID.
    PromptKey { key: KindId, id: KindId },
    /// A resolver reads a prompt that the add-on does not declare.
    UnknownPrompt { resolver: KindId, prompt: KindId },
}

impl fmt::Display for ContributionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PromptKey { key, id } => {
                write!(f, "the prompt under `{key}` has the ID `{id}`")
            }
            Self::UnknownPrompt { resolver, prompt } => write!(
                f,
                "the resolver `{resolver}` reads the prompt `{prompt}`, which the add-on does not declare"
            ),
        }
    }
}

impl core::error::Error for ContributionsError {}

/// Whether the server can run what an add-on contributes now. The web UI shows
/// it, and planning a deployment that needs anything but `Enabled` fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Enabled,
    Disabled,
    /// Preparing the add-on failed. The status page gives the reason.
    Failed,
    /// The add-on needs an API level that this server does not serve.
    Incompatible,
    /// The trust bundle revokes the add-on's key or package.
    Revoked,
    /// No installed version of the add-on declares this kind. The server
    /// keeps its descriptor.
    NotInstalled,
}

/// One schema for each config version that the add-on ever shipped, and the
/// version that the installed add-on writes.
///
/// The server keeps every schema after an upgrade or an uninstall, so a step
/// saved with an old config version still shows as a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConfigSchemasWire", into = "ConfigSchemasWire")]
pub struct ConfigSchemas {
    current: u32,
    versions: BTreeMap<u32, FormSchema>,
}

impl ConfigSchemas {
    /// The schemas, or an error when none is for the current version.
    pub fn new(current: u32, versions: BTreeMap<u32, FormSchema>) -> Result<Self, NoCurrentSchema> {
        if versions.contains_key(&current) {
            Ok(Self { current, versions })
        } else {
            Err(NoCurrentSchema { current })
        }
    }

    /// The config version that the installed add-on writes.
    pub fn current(&self) -> u32 {
        self.current
    }

    /// The schema of a config version.
    pub fn at(&self, version: u32) -> Option<&FormSchema> {
        self.versions.get(&version)
    }

    pub fn versions(&self) -> &BTreeMap<u32, FormSchema> {
        &self.versions
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigSchemasWire {
    current: u32,
    versions: BTreeMap<u32, FormSchema>,
}

impl TryFrom<ConfigSchemasWire> for ConfigSchemas {
    type Error = NoCurrentSchema;

    fn try_from(wire: ConfigSchemasWire) -> Result<Self, Self::Error> {
        Self::new(wire.current, wire.versions)
    }
}

impl From<ConfigSchemas> for ConfigSchemasWire {
    fn from(schemas: ConfigSchemas) -> Self {
        let ConfigSchemas { current, versions } = schemas;
        Self { current, versions }
    }
}

/// The schemas hold none for the current config version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoCurrentSchema {
    pub current: u32,
}

impl fmt::Display for NoCurrentSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "there is no schema for the current config version {}",
            self.current
        )
    }
}

impl core::error::Error for NoCurrentSchema {}

/// A step kind that an add-on declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepKindInfo {
    pub label: String,
    /// Where the step runs. A step of this kind must have this `run_in`.
    pub run_in: RunIn,
    pub availability: Availability,
    pub config: ConfigSchemas,
}

/// A join kind that an add-on declares. A join profile of this kind holds a
/// config that the schemas describe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinKindInfo {
    pub label: String,
    pub availability: Availability,
    pub config: ConfigSchemas,
}

/// A lookup resolver that an add-on declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolverInfo {
    pub label: String,
    pub availability: Availability,
    pub input: ResolverInput,
}

/// An importer that an add-on declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImporterInfo {
    pub label: String,
    pub availability: Availability,
    /// The file name extensions that the importer reads, such as `xml`.
    pub extensions: Vec<String>,
}
