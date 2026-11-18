//! The checks that the server runs on an authored task sequence when it is
//! saved. They run no add-on code, so they work when an add-on is disabled or
//! gone.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;

use crate::action::{Action, BuiltinAction};
use crate::addon::AddOnRef;
use crate::compile::{
    CompileError, NodePath, Rule, action_name, compile, run_in_name, win_pe_full_os_condition,
};
use crate::join::{JoinStage, stage_name};
use crate::sequence::{AuthoredSequence, Group, LoweredSequence, Node, NodeId, RunIn, Step};

/// The step kinds that add-ons declare, as [`validate`] needs them.
pub trait StepKinds {
    /// Whether an installed add-on, or a descriptor that the server keeps for
    /// a removed one, declares `kind` as a step kind.
    fn declares(&self, kind: &AddOnRef) -> bool;
}

impl StepKinds for BTreeSet<AddOnRef> {
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
/// the lowered sequence.
///
/// Each finding names the node's ID and its path in the authored sequence. At
/// one node, the findings about the step itself come first, then the
/// compiler's in its order. A node gets at most one finding for a
/// full-OS-only condition.
///
/// A step kind that no add-on declares is a warning, not an error, so the
/// sequence still saves. Disabled nodes count, because they are still part
/// of the sequence that is saved.
pub fn validate<K: StepKinds + ?Sized>(sequence: &AuthoredSequence, kinds: &K) -> Vec<Finding> {
    let mut validator = Validator {
        kinds,
        path: Vec::new(),
        findings: Vec::new(),
    };
    let stand_in = LoweredSequence {
        id: sequence.id.clone(),
        version: sequence.version,
        name: sequence.name.clone(),
        default_vars: sequence.default_vars.clone(),
        root: validator.group(&sequence.root),
    };
    let mut findings = validator.findings;
    if let Err(errors) = compile(&stand_in) {
        let compiled = compiler_findings(&findings, errors.as_slice());
        findings.extend(compiled);
    }
    // Both lists are in document order, and a node's path sorts in document
    // order. The sort is stable, so the findings at one node keep their order.
    findings.sort_by(|a, b| a.path.cmp(&b.path));
    findings
}

/// The compiler's errors as findings. It leaves out an error about a
/// full-OS-only condition at a node that `own` already reports one for.
fn compiler_findings(own: &[Finding], errors: &[CompileError]) -> Vec<Finding> {
    let reported: BTreeSet<&NodePath> = own
        .iter()
        .filter(|finding| matches!(finding.issue, Issue::FullOsCondition { .. }))
        .map(|finding| &finding.path)
        .collect();
    errors
        .iter()
        .filter(|error| error.rule.full_os_condition().is_none() || !reported.contains(&error.path))
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
}

impl<K: StepKinds + ?Sized> Validator<'_, K> {
    /// Checks the group's children, and returns the group with an empty group
    /// in place of each add-on step and join slot.
    fn group(&mut self, group: &Group<Action>) -> Group<BuiltinAction> {
        let mut children = Vec::with_capacity(group.children.len());
        for (index, child) in group.children.iter().enumerate() {
            self.path.push(index);
            children.push(match child {
                Node::Group(group) => Node::Group(self.group(group)),
                Node::Step(step) => self.step(step),
            });
            self.path.pop();
        }
        Group {
            meta: group.meta.clone(),
            children,
        }
    }

    fn step(&mut self, step: &Step<Action>) -> Node<BuiltinAction> {
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
            let kind = add_on.step_kind();
            if !self.kinds.declares(&kind) {
                self.find(step, Issue::UnknownStepKind { kind });
            }
        }
        // `lower` makes the same group for a step that the program drops.
        Node::Group(Group {
            meta: step.meta.clone(),
            children: Vec::new(),
        })
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
            Self::UnknownStepKind { .. } => Severity::Warning,
            Self::FullOsCondition { .. }
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
        // No compiler rule checks the condition of the empty group that stands
        // in for an add-on step yet. A rule on group conditions would, so the
        // test hands such an error to `compiler_findings` directly.
        let wmi = Issue::FullOsCondition { condition: "wmi" };
        let own = [finding(2, vec![0], wmi)];
        let errors = [
            compile_error(2, vec![0], Rule::FullOsCondition { condition: "wmi" }),
            compile_error(
                2,
                vec![0],
                Rule::DuplicateNodeId {
                    first: NodePath::default(),
                },
            ),
            compile_error(3, vec![1], Rule::FullOsCondition { condition: "wmi" }),
        ];

        let found = compiler_findings(&own, &errors);

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
