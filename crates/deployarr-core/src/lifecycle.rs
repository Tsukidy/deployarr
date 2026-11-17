//! The one pure planner that decides every change to an add-on: install,
//! upgrade, downgrade, reinstall, roll back, enable, disable, uninstall,
//! purge, and the cancel of a waiting change.
//!
//! [`plan`] takes what the server's database says, the change that the admin
//! asked for, and the facts that the host gathered. It returns the steps that
//! the executor runs and the effects that the review dialog shows, or every
//! reason it refuses the change. The dialog and the executor read the same
//! [`Plan`], so the dialog shows what will happen. The planner does no I/O.
//!
//! The executor, the journal on disk, and `reconcile` belong to the add-on
//! host. The executor writes each step's journal phase before the step's work
//! starts, and runs the code checks of [`Step::Prepare`] and [`Step::Check`].

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;
use core::ops::RangeInclusive;

use semver::Version;

use crate::addon::{ADD_ON_ID_PART_MAX_LEN, AddOnId, KindId, PartError, check_part};
use crate::non_empty::NonEmpty;
use crate::sequence::TaskSequenceId;

// ------------------------------------------------------------------ packages

/// A package's SHA-256, the name of its payload in the content store. It is
/// written as 64 lower-case hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageHash(pub [u8; 32]);

/// A minisign Ed25519 public key, whole. The server never picks a key by its
/// 8-byte key ID alone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey(pub [u8; 32]);

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    bytes.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
}

impl fmt::Display for PackageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for PackageHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PackageHash({self})")
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({self})")
    }
}

impl core::str::FromStr for PackageHash {
    type Err = PackageHashError;

    fn from_str(text: &str) -> Result<Self, PackageHashError> {
        let digits = text.as_bytes();
        if digits.len() != 64 {
            return Err(PackageHashError);
        }
        let mut hash = [0; 32];
        let (pairs, _) = digits.as_chunks::<2>();
        for (byte, [high, low]) in hash.iter_mut().zip(pairs) {
            let high = hex_digit(*high).ok_or(PackageHashError)?;
            let low = hex_digit(*low).ok_or(PackageHashError)?;
            *byte = (high << 4) | low;
        }
        Ok(Self(hash))
    }
}

fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// The text is not 64 lower-case hexadecimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageHashError;

impl fmt::Display for PackageHashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a package hash is 64 lower-case hexadecimal digits")
    }
}

impl core::error::Error for PackageHashError {}

/// Who signed a package, as the host found when it verified the signature.
/// The host refuses a bad signature before it plans anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signer {
    Unsigned,
    /// The signature is good, and this key made it. The trust bundle may not
    /// list the key.
    Key(PublicKey),
}

/// What the planner needs to know about one package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub hash: PackageHash,
    pub id: AddOnId,
    pub version: Version,
    /// The API level that the host computed from the component's imports.
    pub api_level: u32,
    /// The version of the store's data that the package works with.
    pub data_version: u32,
    pub signer: Signer,
}

// ------------------------------------------------------------------ registry

/// What the server's database says about every add-on that was ever installed
/// and not purged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    pub add_ons: BTreeMap<AddOnId, Installed>,
}

/// What the database says about one add-on: what the admin asked for, and
/// the open operation, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub active: Package,
    /// The package that a roll back switches to.
    pub previous: Option<Package>,
    pub enabled: bool,
    /// The admin uninstalled the add-on. Its settings, secrets, and store stay
    /// until a purge.
    pub uninstalled: bool,
    /// The publisher that signed the add-on, from the first signed package
    /// that was installed. An add-on with a publisher never takes an unsigned
    /// package or one from another publisher.
    pub publisher: Option<String>,
    /// The data version that the add-on's store holds now.
    pub store_data_version: u32,
    pub open: Option<OpenOperation>,
}

/// An operation that has not finished or been abandoned. The journal allows
/// one per add-on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOperation {
    pub kind: OpKind,
    pub phase: OpPhase,
}

impl OpenOperation {
    /// Whether the operation waits in [`OpPhase::Approved`], so that a cancel
    /// can abandon it and a forced change can take it over. It has done no
    /// work.
    pub fn is_waiting(&self) -> bool {
        self.phase == OpPhase::Approved
    }
}

/// What an operation changes, as the journal records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// Install, upgrade, downgrade, or reinstall this package.
    Apply {
        package: PackageHash,
    },
    /// Switch to this package, the previous one.
    RollBack {
        to: PackageHash,
    },
    Enable,
    Disable,
    Uninstall,
    Purge,
}

impl fmt::Display for OpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Apply { package } => write!(f, "the change to package {package}"),
            Self::RollBack { to } => write!(f, "the roll back to package {to}"),
            Self::Enable => f.write_str("the enable"),
            Self::Disable => f.write_str("the disable"),
            Self::Uninstall => f.write_str("the uninstall"),
            Self::Purge => f.write_str("the purge"),
        }
    }
}

/// The phase of an open operation. The executor writes each phase before the
/// work that it names starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPhase {
    /// The admin confirmed. The operation waits here while deployments use
    /// the add-on.
    Approved,
    Preparing,
    Migrating,
    Checking,
    Committing,
}

// --------------------------------------------------------------------- facts

/// What the host gathered for the change: from the server, the trust bundle,
/// and the records of what uses the add-on.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The API levels that this server serves.
    pub serves: &'a RangeInclusive<u32>,
    pub trust: &'a Trust,
    /// The server setting "Allow unsigned add-ons".
    pub allow_unsigned: bool,
    /// What uses the add-on that the change names.
    pub usage: &'a Usage,
}

/// The trust bundle and the admin's local keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trust {
    pub publishers: Vec<Publisher>,
    /// No package that one of these keys signed installs, unless its hash is
    /// allowed.
    pub revoked_keys: BTreeSet<PublicKey>,
    /// No package with one of these hashes installs.
    pub revoked_packages: BTreeSet<PackageHash>,
    /// Known good packages that a revoked key signed. They install as if their
    /// key were not revoked.
    pub allowed_packages: BTreeSet<PackageHash>,
}

impl Trust {
    /// The publisher that holds this key.
    pub fn publisher_of(&self, key: &PublicKey) -> Option<&Publisher> {
        self.publishers
            .iter()
            .find(|publisher| publisher.key == *key)
    }
}

/// A key that the trust bundle lists, or that the admin added on the Trust
/// page, with the add-on IDs that it may sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publisher {
    pub name: String,
    pub key: PublicKey,
    pub scopes: NonEmpty<IdScope>,
    /// The admin added the key. The trust bundle's revoked keys still apply.
    pub local: bool,
}

impl Publisher {
    pub fn covers(&self, id: &AddOnId) -> bool {
        self.scopes.iter().any(|scope| scope.covers(id))
    }
}

/// The add-on IDs that one key may sign: every ID of one publisher part, such
/// as `deployarr.*`, or one ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdScope {
    /// Every ID whose publisher part is this text.
    Publisher(String),
    One(AddOnId),
}

impl IdScope {
    pub fn covers(&self, id: &AddOnId) -> bool {
        match self {
            Self::Publisher(publisher) => id.publisher() == publisher,
            Self::One(one) => one == id,
        }
    }
}

impl core::str::FromStr for IdScope {
    type Err = IdScopeError;

    fn from_str(text: &str) -> Result<Self, IdScopeError> {
        match text.strip_suffix(".*") {
            Some(publisher) => {
                check_part(publisher, ADD_ON_ID_PART_MAX_LEN).map_err(IdScopeError::Publisher)?;
                Ok(Self::Publisher(publisher.into()))
            }
            None => text
                .parse()
                .map(Self::One)
                .map_err(|_| IdScopeError::NotAnId),
        }
    }
}

impl fmt::Display for IdScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Publisher(publisher) => write!(f, "{publisher}.*"),
            Self::One(id) => write!(f, "{id}"),
        }
    }
}

/// Why the text is not an [`IdScope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdScopeError {
    /// The part before `.*` is not a valid publisher part.
    Publisher(PartError),
    /// The text has no `.*` at its end, and it is not an add-on ID.
    NotAnId,
}

impl fmt::Display for IdScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Publisher(problem) => write!(f, "a scope's publisher {problem}"),
            Self::NotAnId => {
                f.write_str("a scope is `<publisher>.*`, such as `deployarr.*`, or one add-on ID")
            }
        }
    }
}

impl core::error::Error for IdScopeError {}

/// What uses one add-on now. An uninstall needs all of it empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    /// Current task sequence versions that hold a step of the add-on.
    pub task_sequences: Vec<SequenceVersion>,
    /// The add-on's resolvers that the lookup chain holds.
    pub chain_entries: Vec<KindId>,
    /// The names of the join profiles of the add-on's join kinds.
    pub join_profiles: Vec<String>,
    /// How many running deployments use the add-on.
    pub running_deployments: u32,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        let Self {
            task_sequences,
            chain_entries,
            join_profiles,
            running_deployments,
        } = self;
        task_sequences.is_empty()
            && chain_entries.is_empty()
            && join_profiles.is_empty()
            && *running_deployments == 0
    }
}

/// One version of a task sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceVersion {
    pub id: TaskSequenceId,
    pub version: u32,
}

// -------------------------------------------------------------------- change

/// What the admin asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Install, upgrade, downgrade, or reinstall the package, whichever
    /// applies.
    Apply {
        package: Package,
        approval: Approval,
    },
    /// Abandon the add-on's operation while it waits for deployments.
    Cancel {
        addon: AddOnId,
    },
    Enable {
        addon: AddOnId,
    },
    /// Stop calls to the add-on. It runs no add-on code and deletes nothing.
    Disable {
        addon: AddOnId,
        when_busy: WhenBusy,
    },
    /// Switch to the previous package.
    RollBack {
        addon: AddOnId,
        when_busy: WhenBusy,
    },
    /// Keep the settings, secrets, and store, and stop the add-on for good.
    Uninstall {
        addon: AddOnId,
    },
    /// Delete an uninstalled add-on's settings, secrets, store, and packages.
    Purge {
        addon: AddOnId,
    },
}

impl Change {
    /// The add-on that the change names.
    pub fn addon(&self) -> &AddOnId {
        match self {
            Self::Apply { package, .. } => &package.id,
            Self::Cancel { addon }
            | Self::Enable { addon }
            | Self::Disable { addon, .. }
            | Self::RollBack { addon, .. }
            | Self::Uninstall { addon }
            | Self::Purge { addon } => addon,
        }
    }

    fn when_busy(&self) -> Option<WhenBusy> {
        match self {
            Self::Apply { approval, .. } => Some(approval.when_busy),
            Self::Disable { when_busy, .. } | Self::RollBack { when_busy, .. } => Some(*when_busy),
            Self::Cancel { .. }
            | Self::Enable { .. }
            | Self::Uninstall { .. }
            | Self::Purge { .. } => None,
        }
    }

    /// The journal's record of the change, when the add-on is in `installed`.
    fn op_kind(&self, installed: Option<&Installed>) -> Option<OpKind> {
        Some(match self {
            Self::Apply { package, .. } => OpKind::Apply {
                package: package.hash,
            },
            Self::RollBack { .. } => OpKind::RollBack {
                to: installed?.previous.as_ref()?.hash,
            },
            Self::Enable { .. } => OpKind::Enable,
            Self::Disable { .. } => OpKind::Disable,
            Self::Uninstall { .. } => OpKind::Uninstall,
            Self::Purge { .. } => OpKind::Purge,
            Self::Cancel { .. } => return None,
        })
    }
}

/// What the admin confirmed in the review dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approval {
    /// The admin confirmed a package older than the active one.
    pub allow_downgrade: bool,
    /// The SHA-256 that the admin confirmed for a package that is unsigned,
    /// or signed by a key that the trust bundle does not list.
    pub unsigned_hash: Option<PackageHash>,
    pub when_busy: WhenBusy,
}

/// What a change does while running deployments use the add-on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WhenBusy {
    /// The operation waits in [`OpPhase::Approved`] until they finish.
    #[default]
    Wait,
    /// The change is refused.
    Refuse,
    /// The change goes ahead. It also takes over a waiting operation for the
    /// same change.
    Force,
}

// ---------------------------------------------------------------------- plan

/// What a change does: the steps that the executor runs, in order, and the
/// effects that the review dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub addon: AddOnId,
    /// What the journal records. A plan that only abandons a waiting
    /// operation has none.
    pub kind: Option<OpKind>,
    pub steps: Vec<Step>,
    pub effects: Vec<Effect>,
}

/// One step of a plan. Each runs in the journal phase that its comment names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Write the operation's journal row in phase `Approved`. The unique
    /// index on open operations refuses a second one.
    Approve,
    /// Take over the waiting operation's row for the same change.
    TakeOver,
    /// Stay in phase `Approved` until no running deployment uses the add-on.
    WaitForDeployments,
    /// Phase `Preparing`. Compile the package's component in memory, register
    /// its assets, and store its descriptors.
    Prepare(PackageHash),
    /// Phase `Migrating`. Snapshot the store file, then migrate its data up,
    /// one version at a time.
    MigrateData { from: u32, to: u32 },
    /// Phase `Migrating`. Restore the store snapshot that was taken before
    /// the store moved past this data version.
    RestoreSnapshot { to: u32 },
    /// Phase `Checking`. Instantiate the component once, link every import,
    /// compare `describe` with the manifest, and migrate the saved settings.
    /// A changed, added, or moved `sql_text` value fails the check.
    Check(PackageHash),
    /// Phase `Committing`. One transaction writes the change and finishes the
    /// operation.
    Commit(Commit),
    /// Abandon the waiting operation. It did no work, so nothing is undone.
    Abandon,
}

/// What the last transaction of an operation writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Commit {
    /// The active and previous packages, the publisher, and the enabled flag,
    /// with the grants and the migrated settings that the check produced. It
    /// also clears the uninstalled flag, so an uninstalled add-on comes back.
    Packages {
        active: PackageHash,
        previous: Option<PackageHash>,
        publisher: Option<String>,
        enabled: bool,
    },
    Enabled(bool),
    /// Mark the add-on uninstalled, and keep its settings, secrets, and
    /// store.
    Uninstalled,
    /// Delete the add-on's settings, secrets, store, and packages. The
    /// descriptors of its step kinds and join kinds stay.
    Purged,
}

/// What the review dialog shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Installs {
        version: Version,
    },
    Upgrades {
        from: Version,
        to: Version,
    },
    Downgrades {
        from: Version,
        to: Version,
    },
    Reinstalls {
        version: Version,
    },
    RollsBack {
        from: Version,
        to: Version,
    },
    /// The add-on was uninstalled. It comes back enabled, with the settings,
    /// secrets, and store that it kept.
    Restores,
    Enables,
    Disables,
    /// The settings, secrets, and store stay until a purge.
    Uninstalls,
    /// The settings, secrets, store, and packages are deleted. Task sequences
    /// that hold the add-on's steps still load and show.
    Purges,
    /// The package is unsigned, or its key is not in the trust bundle. The
    /// add-on shows an Unsigned badge for good.
    Unsigned,
    MigratesData {
        from: u32,
        to: u32,
    },
    RestoresSnapshot {
        from: u32,
        to: u32,
    },
    /// The change waits until this many running deployments finish.
    WaitsForDeployments {
        count: u32,
    },
    /// The change goes ahead while this many running deployments use the
    /// add-on.
    Forced {
        running_deployments: u32,
    },
    /// The change takes over the operation that waits for deployments.
    TakesOver,
    /// The waiting operation is abandoned.
    Abandons {
        waiting: OpKind,
    },
}

// ------------------------------------------------------------------- refusal

/// Every reason that [`plan`] refused a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusals(NonEmpty<Refusal>);

impl Refusals {
    pub fn as_slice(&self) -> &[Refusal] {
        &self.0
    }
}

impl fmt::Display for Refusals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, refusal) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{refusal}")?;
        }
        Ok(())
    }
}

impl core::error::Error for Refusals {}

/// Why [`plan`] refused a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Another operation on the add-on is open. A waiting one can be
    /// cancelled, or taken over by the same change with Force.
    Busy {
        open: OpenOperation,
    },
    /// No add-on with this ID was ever installed, or it was purged.
    UnknownAddOn,
    /// The add-on is uninstalled.
    Uninstalled,
    /// A purge needs an uninstalled add-on.
    NotUninstalled,
    AlreadyEnabled,
    AlreadyDisabled,
    /// No operation waits.
    NothingToCancel,
    /// The operation has started, so only the executor or `reconcile` can end
    /// it.
    Started {
        open: OpenOperation,
    },
    /// No previous package was kept.
    NoPrevious,
    /// The package needs an API level that this server does not serve.
    IncompatibleLevel {
        needs: u32,
        serves: RangeInclusive<u32>,
    },
    /// The trust bundle revokes the package's hash.
    RevokedPackage,
    /// The trust bundle revokes the key that signed the package.
    RevokedKey {
        key: PublicKey,
    },
    /// The key's scopes do not cover the add-on's ID.
    KeyOutOfScope {
        publisher: String,
        id: AddOnId,
    },
    /// The package is unsigned, or its key is not in the trust bundle. It
    /// installs only when the server setting allows unsigned add-ons and the
    /// admin confirms its hash.
    Unsigned {
        setting_allows: bool,
    },
    /// A publisher signed the add-on, and this package is unsigned or from
    /// another publisher. An upgrade and a roll back both refuse it.
    PublisherChanged {
        was: String,
        now: Option<String>,
    },
    /// The package is older than the active one, and the admin did not
    /// confirm the downgrade.
    Downgrade {
        from: Version,
        to: Version,
    },
    /// The store holds a newer data version than the package works with.
    /// Migrations only go up. A roll back restores the store's snapshot.
    DataTooNew {
        store: u32,
        package: u32,
    },
    /// Running deployments use the add-on, and the admin chose to refuse.
    RunningDeployments {
        count: u32,
    },
    /// Something uses the add-on, so it cannot be uninstalled.
    InUse(Usage),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy { open } => {
                write!(f, "{} is open on this add-on", open.kind)?;
                if open.is_waiting() {
                    f.write_str(" and waits for deployments. Cancel it, or force the same change")?;
                }
                Ok(())
            }
            Self::UnknownAddOn => f.write_str("no add-on with this ID is installed"),
            Self::Uninstalled => f.write_str("the add-on is uninstalled"),
            Self::NotUninstalled => f.write_str("only an uninstalled add-on can be purged"),
            Self::AlreadyEnabled => f.write_str("the add-on is already enabled"),
            Self::AlreadyDisabled => f.write_str("the add-on is already disabled"),
            Self::NothingToCancel => f.write_str("no operation waits on this add-on"),
            Self::Started { open } => {
                write!(f, "{} has started and cannot be cancelled", open.kind)
            }
            Self::NoPrevious => f.write_str("there is no previous version to roll back to"),
            Self::IncompatibleLevel { needs, serves } => write!(
                f,
                "the package needs API level {needs}, and this server serves levels {} to {}",
                serves.start(),
                serves.end()
            ),
            Self::RevokedPackage => f.write_str("the trust bundle revokes this package"),
            Self::RevokedKey { key } => {
                write!(
                    f,
                    "the trust bundle revokes the key {key} that signed the package"
                )
            }
            Self::KeyOutOfScope { publisher, id } => write!(
                f,
                "the key of publisher {publisher:?} may not sign the add-on `{id}`"
            ),
            Self::Unsigned { setting_allows } => {
                f.write_str("the package is unsigned, or its key is not in the trust bundle")?;
                if *setting_allows {
                    f.write_str(". Confirm its SHA-256 to install it")
                } else {
                    f.write_str(", and the server setting \"Allow unsigned add-ons\" is off")
                }
            }
            Self::PublisherChanged { was, now } => match now {
                Some(now) => write!(
                    f,
                    "publisher {was:?} signed the add-on, and publisher {now:?} signed this package"
                ),
                None => write!(
                    f,
                    "publisher {was:?} signed the add-on, and this package is unsigned or its key is unknown"
                ),
            },
            Self::Downgrade { from, to } => write!(
                f,
                "version {to} is older than the active version {from}. Confirm the downgrade"
            ),
            Self::DataTooNew { store, package } => write!(
                f,
                "the store holds data version {store}, and the package works with version {package}. Roll back instead"
            ),
            Self::RunningDeployments { count } => {
                write!(f, "{count} running deployments use the add-on")
            }
            Self::InUse(usage) => {
                f.write_str("the add-on is in use:")?;
                for sequence in &usage.task_sequences {
                    write!(
                        f,
                        " task sequence {} version {};",
                        sequence.id, sequence.version
                    )?;
                }
                for resolver in &usage.chain_entries {
                    write!(f, " lookup chain entry `{resolver}`;")?;
                }
                for profile in &usage.join_profiles {
                    write!(f, " join profile {profile:?};")?;
                }
                if usage.running_deployments > 0 {
                    write!(f, " {} running deployments;", usage.running_deployments)?;
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------- the planner

/// Decides a change. Pure.
///
/// While an operation is open on the add-on, it plans only two changes. A
/// cancel abandons an operation that waits for deployments. The same change
/// with [`WhenBusy::Force`] takes the waiting operation over. It refuses any
/// other change with [`Refusal::Busy`], which names the open operation.
///
/// Otherwise it checks the change and returns every reason it refuses, or the
/// plan.
pub fn plan(registry: &Registry, change: &Change, facts: &Facts<'_>) -> Result<Plan, Refusals> {
    let installed = registry.add_ons.get(change.addon());
    let take_over = match installed.and_then(|installed| installed.open.as_ref()) {
        None => false,
        Some(open) => match change {
            Change::Cancel { addon } if open.is_waiting() => {
                return Ok(Plan {
                    addon: addon.clone(),
                    kind: None,
                    steps: vec![Step::Abandon],
                    effects: vec![Effect::Abandons { waiting: open.kind }],
                });
            }
            Change::Cancel { .. } => return refuse(Refusal::Started { open: open.clone() }),
            _ if open.is_waiting()
                && change.when_busy() == Some(WhenBusy::Force)
                && change.op_kind(installed) == Some(open.kind) =>
            {
                true
            }
            _ => return refuse(Refusal::Busy { open: open.clone() }),
        },
    };
    let mut planner = Planner {
        facts,
        take_over,
        refusals: Vec::new(),
        steps: vec![if take_over {
            Step::TakeOver
        } else {
            Step::Approve
        }],
        effects: if take_over {
            vec![Effect::TakesOver]
        } else {
            Vec::new()
        },
    };
    let kind = match (change, installed) {
        (Change::Apply { package, approval }, _) => planner.apply(installed, package, approval),
        (Change::Cancel { .. }, Some(_)) => return refuse(Refusal::NothingToCancel),
        (_, None) => return refuse(Refusal::UnknownAddOn),
        (Change::Enable { .. }, Some(installed)) => planner.enable(installed),
        (Change::Disable { when_busy, .. }, Some(installed)) => {
            planner.disable(installed, *when_busy)
        }
        (Change::RollBack { when_busy, .. }, Some(installed)) => {
            planner.roll_back(installed, *when_busy)
        }
        (Change::Uninstall { .. }, Some(installed)) => planner.uninstall(installed),
        (Change::Purge { .. }, Some(installed)) => planner.purge(installed),
    };
    match NonEmpty::try_from(planner.refusals) {
        Ok(refusals) => Err(Refusals(refusals)),
        Err(_) => Ok(Plan {
            addon: change.addon().clone(),
            kind,
            steps: planner.steps,
            effects: planner.effects,
        }),
    }
}

fn refuse(refusal: Refusal) -> Result<Plan, Refusals> {
    Err(Refusals(NonEmpty::new(refusal)))
}

struct Planner<'f> {
    facts: &'f Facts<'f>,
    take_over: bool,
    refusals: Vec<Refusal>,
    steps: Vec<Step>,
    effects: Vec<Effect>,
}

impl Planner<'_> {
    fn refuse(&mut self, refusal: Refusal) {
        self.refusals.push(refusal);
    }

    fn apply(
        &mut self,
        installed: Option<&Installed>,
        package: &Package,
        approval: &Approval,
    ) -> Option<OpKind> {
        self.check_level(package);
        let verdict = self.check_new_package(package, approval);
        let publisher = self.check_publisher(installed, &verdict);
        // An uninstalled add-on comes back enabled, on the store it kept.
        let (store_data_version, enabled, previous) = match installed {
            None => {
                self.effects.push(Effect::Installs {
                    version: package.version.clone(),
                });
                (0, true, None)
            }
            Some(installed) => {
                self.direction(installed, package, approval);
                let previous = if package.hash == installed.active.hash {
                    installed.previous.as_ref().map(|previous| previous.hash)
                } else {
                    Some(installed.active.hash)
                };
                if installed.uninstalled {
                    self.effects.push(Effect::Restores);
                    (installed.store_data_version, true, previous)
                } else {
                    self.when_busy(approval.when_busy);
                    (installed.store_data_version, installed.enabled, previous)
                }
            }
        };
        self.steps.push(Step::Prepare(package.hash));
        match package.data_version.cmp(&store_data_version) {
            Ordering::Less => self.refuse(Refusal::DataTooNew {
                store: store_data_version,
                package: package.data_version,
            }),
            Ordering::Equal => {}
            Ordering::Greater => self.migrate(store_data_version, package.data_version),
        }
        self.steps.push(Step::Check(package.hash));
        self.steps.push(Step::Commit(Commit::Packages {
            active: package.hash,
            previous,
            publisher,
            enabled,
        }));
        Some(OpKind::Apply {
            package: package.hash,
        })
    }

    /// Records the effect of applying `package` over the active package, and
    /// refuses a downgrade that the admin did not confirm.
    fn direction(&mut self, installed: &Installed, package: &Package, approval: &Approval) {
        let from = installed.active.version.clone();
        let to = package.version.clone();
        let effect = match to.cmp(&from) {
            Ordering::Greater => Effect::Upgrades { from, to },
            Ordering::Equal => Effect::Reinstalls { version: to },
            Ordering::Less if approval.allow_downgrade => Effect::Downgrades { from, to },
            Ordering::Less => {
                self.refuse(Refusal::Downgrade { from, to });
                return;
            }
        };
        self.effects.push(effect);
    }

    fn enable(&mut self, installed: &Installed) -> Option<OpKind> {
        if installed.uninstalled {
            self.refuse(Refusal::Uninstalled);
        } else if installed.enabled {
            self.refuse(Refusal::AlreadyEnabled);
        }
        // The host checks the trust bundle again at every enable.
        self.check_level(&installed.active);
        self.check_trust(&installed.active);
        self.effects.push(Effect::Enables);
        self.steps.push(Step::Commit(Commit::Enabled(true)));
        Some(OpKind::Enable)
    }

    fn disable(&mut self, installed: &Installed, when_busy: WhenBusy) -> Option<OpKind> {
        if installed.uninstalled {
            self.refuse(Refusal::Uninstalled);
        } else if !installed.enabled {
            self.refuse(Refusal::AlreadyDisabled);
        }
        self.effects.push(Effect::Disables);
        self.when_busy(when_busy);
        self.steps.push(Step::Commit(Commit::Enabled(false)));
        Some(OpKind::Disable)
    }

    fn roll_back(&mut self, installed: &Installed, when_busy: WhenBusy) -> Option<OpKind> {
        if installed.uninstalled {
            self.refuse(Refusal::Uninstalled);
        }
        let Some(previous) = &installed.previous else {
            self.refuse(Refusal::NoPrevious);
            return None;
        };
        self.check_level(previous);
        // The admin accepted the previous package when it was installed. The
        // trust bundle may have changed since, and a later package may have
        // given the add-on a publisher.
        let verdict = self.check_trust(previous);
        let publisher = self.check_publisher(Some(installed), &verdict);
        if verdict == Verdict::Untrusted {
            self.effects.push(Effect::Unsigned);
        }
        self.effects.push(Effect::RollsBack {
            from: installed.active.version.clone(),
            to: previous.version.clone(),
        });
        self.when_busy(when_busy);
        self.steps.push(Step::Prepare(previous.hash));
        let store = installed.store_data_version;
        match previous.data_version.cmp(&store) {
            Ordering::Less => {
                self.effects.push(Effect::RestoresSnapshot {
                    from: store,
                    to: previous.data_version,
                });
                self.steps.push(Step::RestoreSnapshot {
                    to: previous.data_version,
                });
            }
            Ordering::Equal => {}
            Ordering::Greater => self.migrate(store, previous.data_version),
        }
        self.steps.push(Step::Check(previous.hash));
        self.steps.push(Step::Commit(Commit::Packages {
            active: previous.hash,
            previous: Some(installed.active.hash),
            publisher,
            enabled: installed.enabled,
        }));
        Some(OpKind::RollBack { to: previous.hash })
    }

    fn uninstall(&mut self, installed: &Installed) -> Option<OpKind> {
        if installed.uninstalled {
            self.refuse(Refusal::Uninstalled);
        }
        if !self.facts.usage.is_empty() {
            self.refuse(Refusal::InUse(self.facts.usage.clone()));
        }
        self.effects.push(Effect::Uninstalls);
        self.steps.push(Step::Commit(Commit::Uninstalled));
        Some(OpKind::Uninstall)
    }

    fn purge(&mut self, installed: &Installed) -> Option<OpKind> {
        if !installed.uninstalled {
            self.refuse(Refusal::NotUninstalled);
        }
        self.effects.push(Effect::Purges);
        self.steps.push(Step::Commit(Commit::Purged));
        Some(OpKind::Purge)
    }

    fn migrate(&mut self, from: u32, to: u32) {
        self.effects.push(Effect::MigratesData { from, to });
        self.steps.push(Step::MigrateData { from, to });
    }

    /// Applies the admin's choice when running deployments use the add-on.
    fn when_busy(&mut self, when_busy: WhenBusy) {
        let count = self.facts.usage.running_deployments;
        if count == 0 {
            return;
        }
        // A take over is a forced change by definition.
        let when_busy = if self.take_over {
            WhenBusy::Force
        } else {
            when_busy
        };
        match when_busy {
            WhenBusy::Wait => {
                self.effects.push(Effect::WaitsForDeployments { count });
                self.steps.push(Step::WaitForDeployments);
            }
            WhenBusy::Refuse => self.refuse(Refusal::RunningDeployments { count }),
            WhenBusy::Force => self.effects.push(Effect::Forced {
                running_deployments: count,
            }),
        }
    }

    fn check_level(&mut self, package: &Package) {
        if !self.facts.serves.contains(&package.api_level) {
            self.refuse(Refusal::IncompatibleLevel {
                needs: package.api_level,
                serves: self.facts.serves.clone(),
            });
        }
    }

    /// The trust checks for a package that is not installed yet. A package
    /// that is unsigned, or signed by a key that the trust bundle does not
    /// list, installs only when the server setting allows it and the admin
    /// confirmed its hash.
    fn check_new_package(&mut self, package: &Package, approval: &Approval) -> Verdict {
        let verdict = self.check_trust(package);
        if verdict == Verdict::Untrusted {
            let setting_allows = self.facts.allow_unsigned;
            if setting_allows && approval.unsigned_hash == Some(package.hash) {
                self.effects.push(Effect::Unsigned);
            } else {
                self.refuse(Refusal::Unsigned { setting_allows });
            }
        }
        verdict
    }

    /// Refuses a package that is unsigned, or from another publisher, for an
    /// add-on that a publisher signed. Returns the publisher that the commit
    /// records: the add-on's, or the package's when the add-on has none.
    fn check_publisher(
        &mut self,
        installed: Option<&Installed>,
        verdict: &Verdict,
    ) -> Option<String> {
        let now = match verdict {
            Verdict::Publisher(name) => Some(name),
            Verdict::Untrusted | Verdict::Refused => None,
        };
        let Some(was) = installed.and_then(|installed| installed.publisher.as_ref()) else {
            return now.cloned();
        };
        // A trust refusal already says why a refused package does not install.
        if *verdict != Verdict::Refused && now != Some(was) {
            self.refuse(Refusal::PublisherChanged {
                was: was.clone(),
                now: now.cloned(),
            });
        }
        Some(was.clone())
    }

    /// Refuses a revoked hash, a revoked key, and a key whose scopes do not
    /// cover the ID. An enable and a roll back run these checks again on a
    /// package that the admin already accepted.
    fn check_trust(&mut self, package: &Package) -> Verdict {
        let trust = self.facts.trust;
        let mut refused = false;
        if trust.revoked_packages.contains(&package.hash) {
            self.refuse(Refusal::RevokedPackage);
            refused = true;
        }
        let publisher = match package.signer {
            Signer::Unsigned => None,
            Signer::Key(key) => {
                if trust.revoked_keys.contains(&key)
                    && !trust.allowed_packages.contains(&package.hash)
                {
                    self.refuse(Refusal::RevokedKey { key });
                    refused = true;
                }
                trust.publisher_of(&key)
            }
        };
        if let Some(publisher) = publisher
            && !publisher.covers(&package.id)
        {
            self.refuse(Refusal::KeyOutOfScope {
                publisher: publisher.name.clone(),
                id: package.id.clone(),
            });
            refused = true;
        }
        match publisher {
            _ if refused => Verdict::Refused,
            Some(publisher) => Verdict::Publisher(publisher.name.clone()),
            None => Verdict::Untrusted,
        }
    }
}

/// What the trust checks found about a package.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// A key of this publisher signed it, and may sign its ID.
    Publisher(String),
    /// It is unsigned, or the trust bundle does not list its key.
    Untrusted,
    /// A trust check refused it.
    Refused,
}
