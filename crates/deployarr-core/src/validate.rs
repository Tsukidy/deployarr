//! The checks that the server runs on an authored task sequence or a join
//! profile when it is saved. They run no add-on code, so they work when an
//! add-on is disabled or gone.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;

use serde_json::Value;
use serde_json::value::RawValue;

use crate::action::{Action, BuiltinAction};
use crate::addon::{AddOnRef, AddOnStep};
use crate::catalog::{Catalog, ConfigSchemas, StepKindInfo};
use crate::compile::{
    CompileError, NodePath, Rule, action_name, compile, run_in_name, win_pe_full_os_condition,
};
use crate::form::ValueError;
use crate::join::{JoinProfile, JoinStage, stage_name};
use crate::sequence::{AuthoredSequence, Group, LoweredSequence, Node, NodeId, RunIn, Step};

/// The step kinds that add-ons declare, as [`validate`] needs them.
pub trait StepKinds {
    /// The descriptor of `kind`, from an installed add-on or one that the
    /// server keeps for a removed one. [`validate`] checks an add-on step's
    /// `run_in` and config against it.
    fn descriptor(&self, kind: &AddOnRef) -> Option<&StepKindInfo>;

    /// Whether an installed add-on, or a descriptor that the server keeps for
    /// a removed one, declares `kind` as a step kind.
    fn declares(&self, kind: &AddOnRef) -> bool {
        self.descriptor(kind).is_some()
    }
}

/// A set of step kinds holds their names and no descriptors, so [`validate`]
/// checks only that each add-on step's kind is in the set.
impl StepKinds for BTreeSet<AddOnRef> {
    fn descriptor(&self, _kind: &AddOnRef) -> Option<&StepKindInfo> {
        None
    }

    fn declares(&self, kind: &AddOnRef) -> bool {
        self.contains(kind)
    }
}

/// Checks an authored sequence, and returns what it finds in document order.
/// Pure. [`Issue`] lists the checks.
///
/// It reports every rule that [`compile`] checks. An add-on step or a join
/// slot has no expansion until a deployment is planned, so `validate` puts an
/// empty group that keeps the step's [`crate::sequence::NodeMeta`] in its
/// place and compiles the result. The step's ID therefore takes part in the
/// duplicate ID rule. A rule that needs the expansion, such as the order of a
/// `Restart` inside it, runs when a deployment is planned and `compile` gets
/// the lowered sequence. An expansion might hold the first `Restart` into the
/// installed OS, so `validate` leaves out an error of
/// [`Rule::FullOsConditionBeforeInstalledOs`] at a node after the first add-on
/// step or join slot that the program keeps. Planning checks that node. The
/// step's own empty group comes before its expansion, so `validate` still
/// checks it.
///
/// Each finding names the node's ID and its path in the authored sequence. At
/// one node, the findings about the step itself come first, then the
/// compiler's in its order. A node gets at most one finding for a
/// full-OS-only condition.
///
/// A step kind that no add-on declares is a warning, not an error, so the
/// sequence still saves. When `kinds` holds the kind's descriptor, `validate`
/// checks the step's `run_in` against it, and the step's config against the
/// schema at the step's `config_version`. A config version with no schema is
/// a warning, because planning decides whether the add-on migrates the
/// config. The kind's [`crate::catalog::Availability`] changes nothing.
/// Disabled nodes count, because they are still part of the sequence that is
/// saved.
pub fn validate<K: StepKinds + ?Sized>(sequence: &AuthoredSequence, kinds: &K) -> Vec<Finding> {
    let mut validator = Validator {
        kinds,
        path: Vec::new(),
        findings: Vec::new(),
        first_deferred: None,
    };
    let stand_in = LoweredSequence {
        id: sequence.id.clone(),
        version: sequence.version,
        name: sequence.name.clone(),
        default_vars: sequence.default_vars.clone(),
        root: validator.group(&sequence.root, true),
    };
    let Validator {
        mut findings,
        first_deferred,
        ..
    } = validator;
    if let Err(errors) = compile(&stand_in) {
        let compiled = compiler_findings(&findings, first_deferred.as_ref(), errors.as_slice());
        findings.extend(compiled);
    }
    // Both lists are in document order, and a node's path sorts in document
    // order. The sort is stable, so the findings at one node keep their order.
    findings.sort_by(|a, b| a.path.cmp(&b.path));
    findings
}

/// The compiler's errors as findings. It leaves out an error about a
/// full-OS-only condition at a node that `own` already reports one for. It
/// also leaves out an error of [`Rule::FullOsConditionBeforeInstalledOs`] at a
/// node after `first_deferred`, the first add-on step or join slot that the
/// program keeps, because that step's expansion might hold the `Restart`.
fn compiler_findings(
    own: &[Finding],
    first_deferred: Option<&NodePath>,
    errors: &[CompileError],
) -> Vec<Finding> {
    let reported: BTreeSet<&NodePath> = own
        .iter()
        .filter(|finding| matches!(finding.issue, Issue::FullOsCondition { .. }))
        .map(|finding| &finding.path)
        .collect();
    // A path sorts in document order, so a node after the step has a greater
    // path. The step's own group has the same path, and a group around it has
    // a smaller one.
    let after_deferred = |path: &NodePath| first_deferred.is_some_and(|first| path > first);
    errors
        .iter()
        .filter(|error| !error.rule.is_full_os_condition() || !reported.contains(&error.path))
        .filter(|error| {
            !matches!(error.rule, Rule::FullOsConditionBeforeInstalledOs { .. })
                || !after_deferred(&error.path)
        })
        .map(|error| Finding {
            node: error.node,
            path: error.path.clone(),
            issue: Issue::Compile(error.rule.clone()),
        })
        .collect()
}

struct Validator<'k, K: ?Sized> {
    kinds: &'k K,
    /// The child index at each level from the root down to the current node.
    path: Vec<usize>,
    findings: Vec<Finding>,
    /// The path of the first add-on step or join slot that the program keeps.
    first_deferred: Option<NodePath>,
}

impl<K: StepKinds + ?Sized> Validator<'_, K> {
    /// Checks the group's children, and returns the group with an empty group
    /// in place of each add-on step and join slot. `kept` says whether the
    /// program keeps the group's parent, or `true` for the root.
    fn group(&mut self, group: &Group<Action>, kept: bool) -> Group<BuiltinAction> {
        let kept = kept && group.meta.enabled;
        let mut children = Vec::with_capacity(group.children.len());
        for (index, child) in group.children.iter().enumerate() {
            self.path.push(index);
            children.push(match child {
                Node::Group(group) => Node::Group(self.group(group, kept)),
                Node::Step(step) => self.step(step, kept),
            });
            self.path.pop();
        }
        Group {
            meta: group.meta.clone(),
            children,
        }
    }

    /// `kept` says whether the program keeps the step's parent.
    fn step(&mut self, step: &Step<Action>, kept: bool) -> Node<BuiltinAction> {
        match &step.action {
            Action::Builtin(action) => {
                if action.only_from_addon() {
                    self.find(
                        step,
                        Issue::AddOnOnlyAction {
                            action: action_name(action),
                        },
                    );
                }
                return Node::Step(Step {
                    meta: step.meta.clone(),
                    run_in: step.run_in,
                    action: action.clone(),
                });
            }
            Action::ApplyJoinProfile { stage } => {
                if step.run_in != stage.run_in() {
                    self.find(
                        step,
                        Issue::JoinStageRunIn {
                            stage: *stage,
                            run_in: step.run_in,
                        },
                    );
                }
            }
            Action::AddOn(_) => {}
        }
        if let Some(condition) = win_pe_full_os_condition(step.run_in, &step.meta) {
            self.find(step, Issue::FullOsCondition { condition });
        }
        if let Action::AddOn(add_on) = &step.action {
            let kinds = self.kinds;
            let kind = add_on.step_kind();
            match kinds.descriptor(&kind) {
                Some(info) => self.add_on_step(step, add_on, kind, info),
                None if !kinds.declares(&kind) => {
                    self.find(step, Issue::UnknownStepKind { kind });
                }
                None => {}
            }
        }
        if kept && step.meta.enabled && self.first_deferred.is_none() {
            self.first_deferred = Some(NodePath::from(self.path.clone()));
        }
        // `lower` makes the same group for a step that the program drops.
        Node::Group(Group {
            meta: step.meta.clone(),
            children: Vec::new(),
        })
    }

    /// Checks an add-on step against its kind's descriptor.
    fn add_on_step(
        &mut self,
        step: &Step<Action>,
        add_on: &AddOnStep,
        kind: AddOnRef,
        info: &StepKindInfo,
    ) {
        if step.run_in != info.run_in {
            self.find(
                step,
                Issue::StepKindRunIn {
                    kind: kind.clone(),
                    declared: info.run_in,
                    run_in: step.run_in,
                },
            );
        }
        for problem in check_config(&info.config, add_on.config_version, &add_on.config) {
            let issue = match problem {
                ConfigProblem::NoSchema => Issue::NoConfigSchema {
                    kind: kind.clone(),
                    config_version: add_on.config_version,
                },
                ConfigProblem::Unreadable => Issue::UnreadableConfig,
                ConfigProblem::Value(error) => Issue::Config(error),
            };
            self.find(step, issue);
        }
    }

    fn find(&mut self, step: &Step<Action>, issue: Issue) {
        self.findings.push(Finding {
            node: step.meta.id,
            path: NodePath::from(self.path.clone()),
            issue,
        });
    }
}

/// Whether a finding stops the sequence from saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The sequence saves, and the editor shows the finding.
    Warning,
    /// The sequence does not save.
    Error,
}

/// Something that [`validate`] found at a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub node: NodeId,
    pub path: NodePath,
    pub issue: Issue,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.issue.severity()
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { node, path, issue } = self;
        let severity = match issue.severity() {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{path}, node {}: {severity}: {issue}", node.0)
    }
}

/// What [`validate`] finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// No installed add-on, and no descriptor that the server keeps for a
    /// removed one, declares the step's kind. The step loads, saves, and
    /// exports. Planning a deployment that runs it fails.
    UnknownStepKind { kind: AddOnRef },
    /// An add-on step's `run_in` is not the one that its kind declares in
    /// [`StepKindInfo::run_in`].
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
    /// An add-on step or a join slot has `run_in` `WinPe`, and its condition
    /// holds an `installed_msi` or `wmi` condition, which only the full OS can
    /// answer. `condition` names the first one. The compiler rejects a
    /// built-in step like it.
    FullOsCondition { condition: &'static str },
    /// A join slot's `run_in` is not the one that its stage needs. See
    /// [`JoinStage::run_in`].
    JoinStageRunIn { stage: JoinStage, run_in: RunIn },
    /// A built-in step outside any expansion holds an action that only an
    /// add-on step's expansion may hold. See
    /// [`BuiltinAction::only_from_addon`].
    AddOnOnlyAction { action: &'static str },
    /// The sequence breaks a rule that the compiler checks. [`Rule`] lists
    /// them.
    Compile(Rule),
}

impl Issue {
    pub fn severity(&self) -> Severity {
        match self {
            Self::UnknownStepKind { .. } | Self::NoConfigSchema { .. } => Severity::Warning,
            Self::StepKindRunIn { .. }
            | Self::Config(_)
            | Self::UnreadableConfig
            | Self::FullOsCondition { .. }
            | Self::JoinStageRunIn { .. }
            | Self::AddOnOnlyAction { .. }
            | Self::Compile(_) => Severity::Error,
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::FullOsCondition { condition } => write!(
                f,
                "a `{condition}` condition works only in the full OS, so a step with `run_in` `win_pe` cannot have one"
            ),
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
            Self::Compile(rule) => rule.fmt(f),
        }
    }
}

/// Checks a join profile against the join kinds in the catalog, and returns
/// what it finds. Pure. The server runs it when a profile is saved.
///
/// A `Workgroup` or `Domain` profile gets no finding. For an add-on's join
/// kind, it checks the profile's config against the kind's schema at the
/// profile's `config_version`, as [`validate`] does for an add-on step. The
/// kind's [`crate::catalog::Availability`] changes nothing.
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

const UNREADABLE_CONFIG: &str =
    "the config holds a value that the schema check cannot read, such as a number out of range";

/// `of` is `step` or `join`. The message does not say that planning fails,
/// because the add-on may migrate the config when a deployment is planned.
fn no_config_schema(
    f: &mut fmt::Formatter<'_>,
    of: &str,
    kind: &AddOnRef,
    config_version: u32,
) -> fmt::Result {
    write!(
        f,
        "the config could not be checked, because this server holds no schema for config version {config_version} of the {of} kind `{}` of `{}`",
        kind.kind, kind.addon
    )
}

/// The error names no part of the value, so a message cannot leak a secret.
fn config_error(f: &mut fmt::Formatter<'_>, error: &ValueError) -> fmt::Result {
    write!(f, "the config does not fit its schema: {error}")
}

/// What the check of an add-on's config against its kind's schemas finds.
enum ConfigProblem {
    NoSchema,
    Unreadable,
    Value(ValueError),
}

/// Checks a config against the schema at `version`. It parses the config's
/// text into a [`Value`] for the check, and leaves the text unchanged.
fn check_config(schemas: &ConfigSchemas, version: u32, config: &RawValue) -> Vec<ConfigProblem> {
    let Some(schema) = schemas.at(version) else {
        return Vec::from([ConfigProblem::NoSchema]);
    };
    // A number out of range is valid JSON text, which a `Value` cannot hold.
    let Ok(document) = serde_json::from_str::<Value>(config.get()) else {
        return Vec::from([ConfigProblem::Unreadable]);
    };
    match schema.check(&document) {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .as_slice()
            .iter()
            .cloned()
            .map(ConfigProblem::Value)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(n: u128) -> NodeId {
        NodeId(uuid::Uuid::from_u128(n))
    }

    fn finding(n: u128, path: Vec<usize>, issue: Issue) -> Finding {
        Finding {
            node: node(n),
            path: NodePath::from(path),
            issue,
        }
    }

    fn compile_error(n: u128, path: Vec<usize>, rule: Rule) -> CompileError {
        CompileError {
            node: node(n),
            path: NodePath::from(path),
            rule,
        }
    }

    #[test]
    fn a_compiler_error_about_a_full_os_condition_at_a_reported_node_is_left_out() {
        // The rule on conditions before the first `Restart` into the installed
        // OS checks the empty group that stands in for an add-on step. The test
        // hands its error to `compiler_findings` directly.
        let wmi = Issue::FullOsCondition { condition: "wmi" };
        let own = [finding(2, vec![0], wmi)];
        let errors = [
            compile_error(
                2,
                vec![0],
                Rule::FullOsConditionBeforeInstalledOs { condition: "wmi" },
            ),
            compile_error(
                2,
                vec![0],
                Rule::DuplicateNodeId {
                    first: NodePath::default(),
                },
            ),
            compile_error(3, vec![1], Rule::FullOsCondition { condition: "wmi" }),
        ];

        let found = compiler_findings(&own, None, &errors);

        assert_eq!(
            found,
            [
                finding(
                    2,
                    vec![0],
                    Issue::Compile(Rule::DuplicateNodeId {
                        first: NodePath::default(),
                    }),
                ),
                finding(
                    3,
                    vec![1],
                    Issue::Compile(Rule::FullOsCondition { condition: "wmi" }),
                ),
            ]
        );
    }
}
