//! Planning: the whole path from an authored task sequence to the program that
//! the agent runs, and the save check that runs the same rules earlier.
//!
//! [`validate`] checks a sequence when it is saved. [`plan`] builds the program
//! when a deployment is planned, after the add-on host has expanded each step
//! that [`deferred`] lists. Both run the same two passes, so each rule is
//! written once:
//!
//! 1. The substitution pass builds an internal tree from the authored
//!    sequence. At planning, each deferred step becomes its expansion. At
//!    save, each one becomes an unexpanded deferred step. The pass checks each
//!    add-on step, join slot, and action that only an expansion may hold. At
//!    save, it also checks each add-on step against its kind. At planning, it
//!    checks each expansion.
//! 2. The rule pass checks the rules in [`Rule`] over that tree, and builds
//!    the program.
//!
//! An unexpanded deferred step may hold any action whose
//! [`ActionKind`](crate::action::ActionKind) an add-on may return. The rule
//! pass tracks each fact about program order as no, maybe, or yes, and the
//! save check reports an order rule only when every possible expansion would
//! break it.

mod compile;
mod lower;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use crate::action::{Action, BuiltinAction, SecretSource, source_name};
use crate::addon::{AddOnRef, AddOnStep};
use crate::catalog::Catalog;
use crate::condition::Condition;
use crate::form::{UNREADABLE_CONFIG, ValueError, config_error, no_config_schema};
use crate::join::{JoinStage, stage_name};
use crate::program::Program;
use crate::sequence::{AuthoredSequence, Group, Node, NodeId, NodeMeta, RunIn, Step};

/// A step that the server must expand before it plans: an add-on step or a
/// join slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deferred<'a> {
    pub node: &'a NodeMeta,
    pub run_in: RunIn,
    pub what: DeferredKind<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredKind<'a> {
    /// A step from an add-on. Its add-on expands it.
    AddOn(&'a AddOnStep),
    /// A join slot. The join kind of the deployment's join profile names the
    /// step kind that expands it, or nothing.
    Join(JoinStage),
}

impl<'a> DeferredKind<'a> {
    fn of(action: &'a Action) -> Option<Self> {
        match action {
            Action::Builtin(_) => None,
            Action::AddOn(step) => Some(Self::AddOn(step)),
            Action::ApplyJoinProfile { stage } => Some(Self::Join(*stage)),
        }
    }
}

/// Lists, in document order, every add-on step and join slot that the program
/// keeps: each one that is enabled and inside no disabled group.
///
/// The program drops a disabled node and everything inside a disabled group,
/// so the server need not expand them, and a missing add-on does not stop a
/// deployment that would not run its step.
pub fn deferred(sequence: &AuthoredSequence) -> Vec<Deferred<'_>> {
    let mut found = Vec::new();
    collect_deferred(&sequence.root, &mut found);
    found
}

fn collect_deferred<'a>(group: &'a Group<Action>, found: &mut Vec<Deferred<'a>>) {
    if !group.meta.enabled {
        return;
    }
    for child in &group.children {
        match child {
            Node::Group(group) => collect_deferred(group, found),
            Node::Step(step) if step.meta.enabled => {
                if let Some(what) = DeferredKind::of(&step.action) {
                    found.push(Deferred {
                        node: &step.meta,
                        run_in: step.run_in,
                        what,
                    });
                }
            }
            Node::Step(_) => {}
        }
    }
}

/// The built-in steps that each deferred step expands to, by the step's node
/// ID. An empty list is an expansion to nothing, as a `Workgroup` or `Domain`
/// join profile gives a join slot.
///
/// A child is a `Step<Action>`, so that [`plan`] can reject an add-on step or a
/// join slot inside an expansion.
pub type Expansions = BTreeMap<NodeId, Vec<Step<Action>>>;

/// Checks an authored sequence when it is saved, and returns what it finds in
/// document order. Pure. It runs no add-on code, so it works when an add-on is
/// disabled or gone.
///
/// It reports every rule in [`Rule`]. An add-on step or a join slot has no
/// expansion until a deployment is planned, so each one is an unexpanded
/// deferred step here. Its own ID and condition still take part in the rules.
/// A rule about program order is reported only when every expansion that the
/// add-ons may return would break it. A rule that needs the expansion, such as
/// the order of a `Restart` inside it, waits for [`plan`].
///
/// Every rule that `validate` reports also fails [`plan`], whatever the
/// expansions. A check against the catalog need not, because planning reads
/// no catalog.
///
/// At one node, the checks of the substitution pass come first, then the rule
/// pass's in its order. [`SaveCheck`] lists the checks. A step kind that the
/// catalog does not know is a warning, not an error, so the sequence still
/// saves. When the catalog holds the kind's descriptor, `validate` checks the
/// step's `run_in` against it, and the step's config against the schema at the
/// step's `config_version`. A config version with no schema is a warning,
/// because planning decides whether the add-on migrates the config. The kind's
/// [`crate::catalog::Availability`] changes nothing. Disabled nodes count,
/// because they are still part of the sequence that is saved.
pub fn validate(sequence: &AuthoredSequence, catalog: &Catalog) -> Vec<SaveIssue> {
    let (tree, mut found) = lower::lower(sequence, &mut lower::Saving { catalog });
    let (_, rules) = compile::compile(&tree);
    found.extend(rules.into_iter().map(Found::from_rule));
    // Both lists are in document order, and a node's path sorts in document
    // order. The sort is stable, so the findings at one node keep their order.
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
        .into_iter()
        .map(|Found { node, path, check }| SaveIssue { node, path, check })
        .collect()
}

/// Builds the program that the agent runs from an authored sequence and the
/// expansions of the steps that [`deferred`] lists. Pure.
///
/// Each deferred step becomes a group that keeps the step's [`NodeMeta`]: its
/// ID, name, `enabled`, `continue_on_error`, and condition. The step's
/// condition and `continue_on_error` therefore still apply. Each child keeps
/// its own meta, except its ID. The ID of child `i` is the version 5 UUID
/// whose namespace is the step's ID and whose name is `i` in decimal ASCII
/// digits, so the same input gives the same program. The program drops
/// disabled nodes, and works out the `on_error` of each step and each group,
/// so a failure needs no group stack at run time.
///
/// It fails with every check of the substitution pass that the input breaks,
/// then the expansions that no kept deferred step used. Only when it finds
/// neither does it run the rule pass, and then it fails with every rule that
/// the result breaks. [`PlanCheck`] lists the
/// checks. Both lists are in document order. An error inside an expansion
/// names the deferred step, its path, and the child's index.
pub fn plan(sequence: &AuthoredSequence, expansions: &Expansions) -> Result<Program, PlanErrors> {
    let mut deferral = lower::Planning::new(expansions);
    let (tree, found) = lower::lower(sequence, &mut deferral);
    let unused_expansions = deferral.unused();
    if !found.is_empty() || !unused_expansions.is_empty() {
        return Err(PlanErrors {
            issues: found.into_iter().map(PlanIssue::from).collect(),
            unused_expansions,
        });
    }
    let (instrs, rules) = compile::compile(&tree);
    if !rules.is_empty() {
        return Err(PlanErrors {
            issues: rules
                .into_iter()
                .map(|found| PlanIssue::from(Found::from_rule(found)))
                .collect(),
            unused_expansions: Vec::new(),
        });
    }
    Ok(Program::new(
        (sequence.id.clone(), sequence.version),
        instrs,
    ))
}

/// What a pass finds at a node. `check` is a [`Rule`], a [`SaveCheck`], or a
/// [`PlanCheck`].
struct Found<C> {
    node: NodeId,
    path: NodePath,
    check: C,
}

impl<C: From<Rule>> Found<C> {
    fn from_rule(found: Found<Rule>) -> Self {
        let Found { node, path, check } = found;
        Self {
            node,
            path,
            check: C::from(check),
        }
    }
}

impl From<Found<PlanCheck>> for PlanIssue {
    fn from(found: Found<PlanCheck>) -> Self {
        let Found { node, path, check } = found;
        Self { node, path, check }
    }
}

/// Where a node is in the authored sequence: the child index at each level
/// from the root down to the node. The root's path is empty. A step of an
/// expansion has the path of the add-on step or join slot that it replaces,
/// and the step's index in the expansion.
///
/// It is written like the path of a parse error, such as
/// `root.children[1].group.children[0]` for the first child of the root's
/// second child, and `root` for the root. A step of an expansion adds its
/// index, such as `root.children[1].expansion[0]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodePath {
    indices: Vec<usize>,
    child: Option<usize>,
}

impl NodePath {
    /// The child index at each level of the authored sequence.
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    /// For a step of an expansion, its index in the expansion.
    pub fn child(&self) -> Option<usize> {
        self.child
    }

    /// The path of child `index` of the expansion of the step at this path.
    pub fn with_child(self, index: usize) -> Self {
        Self {
            child: Some(index),
            ..self
        }
    }
}

impl From<Vec<usize>> for NodePath {
    fn from(indices: Vec<usize>) -> Self {
        Self {
            indices,
            child: None,
        }
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("root")?;
        for (depth, index) in self.indices.iter().enumerate() {
            if depth > 0 {
                f.write_str(".group")?;
            }
            write!(f, ".children[{index}]")?;
        }
        if let Some(child) = self.child {
            write!(f, ".expansion[{child}]")?;
        }
        Ok(())
    }
}

/// The rules that both [`validate`] and [`plan`] check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// The step's `run_in` is not one that its action allows. See
    /// [`BuiltinAction::allowed_run_in`]. Disabled steps count.
    RunIn {
        action: &'static str,
        run_in: RunIn,
        allowed: &'static [RunIn],
    },
    /// The step has `run_in` `WinPe`, and its condition holds an
    /// `installed_msi` or `wmi` condition, which only the full OS can answer.
    /// `condition` names the first one. It covers built-in steps, add-on
    /// steps, and join slots. An add-on step or a join slot becomes a group
    /// that keeps its condition, and the rule on a group's condition checks it
    /// only before the first `Restart` into the installed OS. Disabled steps
    /// count.
    FullOsCondition { condition: &'static str },
    /// The node is a group, the root included, a step with `run_in` `Either`,
    /// or an add-on step or join slot whose `run_in` is not `WinPe`. Its
    /// condition holds an `installed_msi` or `wmi` condition, and `condition`
    /// names the first one. The node comes at or before the first `Restart`
    /// with `into` `InstalledOs` in program order. Every run starts in WinPE,
    /// and only that restart leaves it, so the run reaches the node in WinPE,
    /// where the condition fails to evaluate on every run. The run evaluates
    /// the restart's own condition before it restarts, so the same holds for
    /// the restart. Conditions do not count, so a `Restart` with a condition,
    /// or in a group with one, still counts. The program drops disabled nodes,
    /// so they do not count.
    ///
    /// An add-on may return a `Restart`, so [`validate`] does not report the
    /// rule at a node after the first unexpanded deferred step that the
    /// program keeps. The step's own condition comes before its expansion, so
    /// the rule still checks it.
    FullOsConditionBeforeInstalledOs { condition: &'static str },
    /// The step is a `Restart` with `run_in` `WinPe`, and no `PartitionDisk`
    /// step comes before it in program order. Until that step, the state is
    /// on `X:`, and a restart loses it. The program drops disabled nodes, so
    /// they do not count.
    RestartBeforePartition,
    /// The step is an `ApplyDrivers` or `ApplyUnattend`, named by `action`,
    /// and no `ApplyOsImage` step comes before it in program order. Both
    /// actions act on the applied image. Conditions do not count, so an
    /// `ApplyOsImage` step in a group with a condition still comes before.
    /// The program drops disabled nodes, so they do not count.
    BeforeOsImage { action: &'static str },
    /// The step is a `Restart` with `into` `WinPe`, and an `EnableBitLocker`
    /// step comes before it in program order. After the partition step, the
    /// state is on the Windows volume. Once BitLocker protects that volume,
    /// the agent in WinPE cannot read the state there. The architecture labels
    /// that last point inferred. The program drops disabled nodes, so they do
    /// not count.
    RestartIntoWinPeAfterBitLocker,
    /// An earlier node, at `first`, has the same ID. Disabled nodes count.
    DuplicateNodeId { first: NodePath },
    /// The step is a join slot, and its `run_in` is not the one that its
    /// stage needs. See [`JoinStage::run_in`]. Disabled join slots count.
    JoinStageRunIn { stage: JoinStage, run_in: RunIn },
    /// The step is a built-in step of the task sequence, outside any
    /// expansion, and its action is one that only an expansion may hold. See
    /// [`BuiltinAction::only_from_addon`]. Disabled steps count.
    AddOnOnlyAction { action: &'static str },
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunIn {
                action,
                run_in,
                allowed,
            } => {
                write!(f, "`{action}` runs only with `run_in` ")?;
                for (index, value) in allowed.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" or ")?;
                    }
                    write!(f, "`{}`", run_in_name(*value))?;
                }
                write!(f, ", and this step has `{}`", run_in_name(*run_in))
            }
            Self::FullOsCondition { condition } => write!(
                f,
                "a `{condition}` condition works only in the full OS, so a step with `run_in` `win_pe` cannot have one"
            ),
            Self::FullOsConditionBeforeInstalledOs { condition } => write!(
                f,
                "a `{condition}` condition works only in the full OS. The run reaches this node in WinPE, because no `restart` into `installed_os` comes before it"
            ),
            Self::RestartBeforePartition => f.write_str(
                "a `restart` with `run_in` `win_pe` must come after the first `partition_disk` step. Until then the state is on `X:`, and the restart loses it",
            ),
            Self::BeforeOsImage { action } => write!(
                f,
                "`{action}` acts on the applied image, so it must come after the first `apply_os_image` step"
            ),
            Self::RestartIntoWinPeAfterBitLocker => f.write_str(
                "a `restart` with `into` `win_pe` must not come after an `enable_bitlocker` step. The state is on the Windows volume, and the agent in WinPE cannot read it once BitLocker protects that volume",
            ),
            Self::DuplicateNodeId { first } => {
                write!(f, "the node at `{first}` has the same ID")
            }
            Self::JoinStageRunIn { stage, run_in } => write!(
                f,
                "a join slot with `stage` `{}` runs only with `run_in` `{}`, and this step has `{}`",
                stage_name(*stage),
                run_in_name(stage.run_in()),
                run_in_name(*run_in)
            ),
            Self::AddOnOnlyAction { action } => write!(
                f,
                "only an add-on step's expansion may hold `{action}`, so a task sequence cannot hold it as a step of its own"
            ),
        }
    }
}

/// Whether a save issue stops the sequence from saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The sequence saves, and the editor shows the issue.
    Warning,
    /// The sequence does not save.
    Error,
}

/// Something that [`validate`] found at a node of the authored sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveIssue {
    pub node: NodeId,
    pub path: NodePath,
    pub check: SaveCheck,
}

impl SaveIssue {
    pub fn severity(&self) -> Severity {
        self.check.severity()
    }
}

impl fmt::Display for SaveIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { node, path, check } = self;
        let severity = match check.severity() {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{path}, node {}: {severity}: {check}", node.0)
    }
}

/// What [`validate`] checks: the rules, and the checks of an add-on step
/// against its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveCheck {
    /// The sequence breaks a rule that planning checks too.
    Rule(Rule),
    /// No installed add-on, and no descriptor that the server keeps for a
    /// removed one, declares the step's kind. The step loads, saves, and
    /// exports. Planning a deployment that runs it fails.
    UnknownStepKind { kind: AddOnRef },
    /// An add-on step's `run_in` is not the one that its kind declares in
    /// [`crate::catalog::StepKindInfo::run_in`].
    StepKindRunIn {
        kind: AddOnRef,
        declared: RunIn,
        run_in: RunIn,
    },
    /// The kind's descriptor holds no schema for the add-on step's
    /// `config_version`, so its config could not be checked. The sequence
    /// saves. Planning decides whether the add-on migrates the config.
    NoConfigSchema { kind: AddOnRef, config_version: u32 },
    /// The add-on step's config does not fit the schema at its
    /// `config_version`. [`ValueError::path`] is a JSON pointer into the
    /// config.
    Config(ValueError),
    /// The add-on step's config text holds a value that a
    /// [`serde_json::Value`] cannot hold, such as a number out of range, so
    /// the schema check cannot read it.
    UnreadableConfig,
}

impl SaveCheck {
    pub fn severity(&self) -> Severity {
        match self {
            Self::UnknownStepKind { .. } | Self::NoConfigSchema { .. } => Severity::Warning,
            Self::Rule(_)
            | Self::StepKindRunIn { .. }
            | Self::Config(_)
            | Self::UnreadableConfig => Severity::Error,
        }
    }
}

impl From<Rule> for SaveCheck {
    fn from(rule: Rule) -> Self {
        Self::Rule(rule)
    }
}

impl fmt::Display for SaveCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(rule) => rule.fmt(f),
            Self::UnknownStepKind { kind } => write!(
                f,
                "no add-on that this server knows declares the step kind `{}` of `{}`",
                kind.kind, kind.addon
            ),
            Self::StepKindRunIn {
                kind,
                declared,
                run_in,
            } => write!(
                f,
                "the step kind `{}` of `{}` runs only with `run_in` `{}`, and this step has `{}`",
                kind.kind,
                kind.addon,
                run_in_name(*declared),
                run_in_name(*run_in)
            ),
            Self::NoConfigSchema {
                kind,
                config_version,
            } => no_config_schema(f, "step", kind, *config_version),
            Self::Config(error) => config_error(f, error),
            Self::UnreadableConfig => f.write_str(UNREADABLE_CONFIG),
        }
    }
}

/// A check that [`plan`] fails, at a node of the authored sequence. Inside an
/// expansion, `node` is the add-on step or join slot, and `path` holds the
/// child's index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanIssue {
    pub node: NodeId,
    pub path: NodePath,
    pub check: PlanCheck,
}

impl fmt::Display for PlanIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { node, path, check } = self;
        write!(f, "{path}, node {}: {check}", node.0)
    }
}

/// What [`plan`] checks: the rules, and the checks of each expansion. An
/// expansion check names a child of the expansion in the issue's path, except
/// [`PlanCheck::MissingExpansion`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanCheck {
    /// The sequence breaks a rule that the save check checks too.
    Rule(Rule),
    /// The program keeps the step, and the expansions hold none for it.
    MissingExpansion,
    /// The child's action is not one that an add-on may return. See
    /// [`crate::action::ActionKind::allowed_from_addon`].
    NotAllowed { action: &'static str },
    /// The child's `run_in` differs from the step's.
    ChildRunIn { run_in: RunIn, expected: RunIn },
    /// The child is an add-on step or a join slot.
    Nested,
    /// The child is an `AddProvisioningPackage` whose package names a secret
    /// in a source that the step does not have. An add-on step's expansion
    /// may name `step_config` or `addon_settings`. The deployment's join
    /// profile need not belong to the step's add-on, so `join_profile` fails
    /// there. A join slot's expansion may name `join_profile` or
    /// `addon_settings`. A join slot has no step config, so `step_config`
    /// fails there.
    SecretSource { source: SecretSource },
}

impl From<Rule> for PlanCheck {
    fn from(rule: Rule) -> Self {
        Self::Rule(rule)
    }
}

impl fmt::Display for PlanCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(rule) => rule.fmt(f),
            Self::MissingExpansion => f.write_str(
                "the server must expand this step before it compiles, and no expansion was given",
            ),
            Self::NotAllowed { action } => write!(
                f,
                "the child is `{action}`, and an add-on may return only `run_command`, `run_powershell`, `set_variable`, `restart`, `server_call`, and `add_provisioning_package`"
            ),
            Self::ChildRunIn { run_in, expected } => write!(
                f,
                "the child has `run_in` `{}`, and the step it replaces has `{}`",
                run_in_name(*run_in),
                run_in_name(*expected)
            ),
            Self::Nested => f.write_str(
                "the child is an add-on step or a join slot, and an expansion holds only built-in actions",
            ),
            Self::SecretSource { source } => write!(
                f,
                "the child names a secret in `{}`. An add-on step's expansion may name only `step_config` and `addon_settings`, and a join slot's expansion only `join_profile` and `addon_settings`",
                source_name(*source)
            ),
        }
    }
}

/// Why [`plan`] built no program. It holds at least one issue or one unused
/// expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanErrors {
    issues: Vec<PlanIssue>,
    unused_expansions: Vec<NodeId>,
}

impl PlanErrors {
    /// Every check that the input fails, in document order.
    pub fn issues(&self) -> &[PlanIssue] {
        &self.issues
    }

    /// The IDs of the expansions that no kept add-on step or join slot used,
    /// in ID order. The ID may name no node of the sequence, so an unused
    /// expansion has no path. It is a fault of the server that built the
    /// expansions, not of the sequence.
    pub fn unused_expansions(&self) -> &[NodeId] {
        &self.unused_expansions
    }
}

/// One error on each line: the issues, then the unused expansions.
impl fmt::Display for PlanErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut line = |f: &mut fmt::Formatter<'_>| {
            if first {
                first = false;
                Ok(())
            } else {
                f.write_str("\n")
            }
        };
        for issue in &self.issues {
            line(f)?;
            issue.fmt(f)?;
        }
        for node in &self.unused_expansions {
            line(f)?;
            write!(
                f,
                "node {}: an expansion was given for a node that is not an enabled add-on step or join slot",
                node.0
            )?;
        }
        Ok(())
    }
}

impl core::error::Error for PlanErrors {}

/// For a step with `run_in` `WinPe`, the JSON name of the first condition in
/// its condition, in document order, that only the full OS can answer. See
/// [`Rule::FullOsCondition`].
fn win_pe_full_os_condition(run_in: RunIn, meta: &NodeMeta) -> Option<&'static str> {
    if run_in == RunIn::WinPe {
        meta.condition.as_ref().and_then(full_os_only)
    } else {
        None
    }
}

/// The JSON name of the first condition in `condition`, in document order,
/// that only the full OS can answer.
fn full_os_only(condition: &Condition) -> Option<&'static str> {
    match condition {
        Condition::All(children) | Condition::Any(children) => {
            children.iter().find_map(full_os_only)
        }
        Condition::Not(child) => full_os_only(child),
        Condition::InstalledMsi(_) => Some("installed_msi"),
        Condition::Wmi { .. } => Some("wmi"),
        Condition::Var { .. }
        | Condition::Registry { .. }
        | Condition::File { .. }
        | Condition::Folder { .. } => None,
    }
}

/// The action's key in the JSON format.
fn action_name(action: &BuiltinAction) -> &'static str {
    match action {
        BuiltinAction::RunCommand { .. } => "run_command",
        BuiltinAction::RunPowerShell { .. } => "run_powershell",
        BuiltinAction::SetVariable { .. } => "set_variable",
        BuiltinAction::Restart { .. } => "restart",
        BuiltinAction::PartitionDisk { .. } => "partition_disk",
        BuiltinAction::ApplyOsImage { .. } => "apply_os_image",
        BuiltinAction::ApplyDrivers { .. } => "apply_drivers",
        BuiltinAction::ApplyUnattend { .. } => "apply_unattend",
        BuiltinAction::InstallApplications { .. } => "install_applications",
        BuiltinAction::InstallUpdates { .. } => "install_updates",
        BuiltinAction::VerifyDrivers { .. } => "verify_drivers",
        BuiltinAction::EnableBitLocker { .. } => "enable_bitlocker",
        BuiltinAction::SuspendBitLocker { .. } => "suspend_bitlocker",
        BuiltinAction::CheckReadiness { .. } => "check_readiness",
        BuiltinAction::ActivateWindows { .. } => "activate_windows",
        BuiltinAction::ServerCall { .. } => "server_call",
        BuiltinAction::AddProvisioningPackage { .. } => "add_provisioning_package",
    }
}

/// The `run_in` value as the JSON format writes it.
fn run_in_name(run_in: RunIn) -> &'static str {
    match run_in {
        RunIn::WinPe => "win_pe",
        RunIn::FullOs => "full_os",
        RunIn::Either => "either",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::LoweredSequence;

    fn steps<'a>(group: &'a Group<BuiltinAction>, out: &mut Vec<&'a Step<BuiltinAction>>) {
        for child in &group.children {
            match child {
                Node::Group(group) => steps(group, out),
                Node::Step(step) => out.push(step),
            }
        }
    }

    #[test]
    fn action_names_are_the_json_keys() {
        let sequence =
            LoweredSequence::from_json(include_str!("../tests/data/windows-11-pro.json"))
                .expect("the sample parses");
        let mut all = Vec::new();
        steps(&sequence.root, &mut all);
        let mut actions: Vec<BuiltinAction> =
            all.into_iter().map(|step| step.action.clone()).collect();
        // The sample uses every action but the two that add-on steps expand to.
        actions.push(BuiltinAction::ServerCall {
            send: alloc::vec!["SerialNumber".parse().expect("a valid name")],
            limits: crate::action::StepLimits::default(),
        });
        actions.push(BuiltinAction::AddProvisioningPackage {
            package: crate::action::SecretRef {
                source: crate::action::SecretSource::StepConfig,
                field: "enrollment".parse().expect("a valid field key"),
            },
            limits: crate::action::StepLimits::default(),
        });
        let mut seen = alloc::collections::BTreeSet::new();
        for action in &actions {
            let json = serde_json::to_value(action).expect("an action serializes");
            let key = json
                .as_object()
                .and_then(|object| object.keys().next())
                .expect("an action is an object with one key");
            assert_eq!(action_name(action), key);
            seen.insert(action_name(action));
        }
        // Every action was checked.
        assert_eq!(seen.len(), 17);
    }

    #[test]
    fn run_in_names_are_the_json_values() {
        for run_in in [RunIn::WinPe, RunIn::FullOs, RunIn::Either] {
            let json = serde_json::to_value(run_in).expect("a run_in serializes");
            assert_eq!(json.as_str(), Some(run_in_name(run_in)));
        }
    }
}
