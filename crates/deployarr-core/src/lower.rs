//! Puts the built-in steps that add-ons return in place of the steps that the
//! server must expand, so that the sequence can compile.
//!
//! Planning a deployment runs in two parts before [`crate::compile::compile`].
//! The add-on host asks each add-on to expand the steps that [`deferred`]
//! lists. Then [`lower`] puts each expansion in place of its step.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;

use uuid::Uuid;

use crate::action::{Action, BuiltinAction};
use crate::addon::AddOnStep;
use crate::compile::{action_name, run_in_name, win_pe_full_os_condition};
use crate::join::{JoinStage, stage_name};
use crate::non_empty::NonEmpty;
use crate::sequence::{
    AuthoredSequence, Group, LoweredSequence, Node, NodeId, NodeMeta, RunIn, Step,
};

/// A step that the server must expand before it compiles.
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
/// The compiler drops a disabled node and everything inside a disabled group,
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
/// A child is a `Step<Action>`, so that [`lower`] can reject an add-on step or
/// a join slot inside an expansion.
pub type Expansions = BTreeMap<NodeId, Vec<Step<Action>>>;

/// Replaces each step that [`deferred`] lists with a group that holds its
/// expansion. Pure.
///
/// The group keeps the step's [`NodeMeta`]: its ID, name, `enabled`,
/// `continue_on_error`, and condition. The step's condition and
/// `continue_on_error` therefore still apply. Each child keeps its own meta,
/// except its ID. The ID of child `i` is the version 5 UUID whose namespace is
/// the step's ID and whose name is `i` in decimal ASCII digits, so the same
/// input gives the same node IDs. An add-on step or join slot that the
/// program drops becomes an empty group with the step's meta.
///
/// It fails with every rule that the input breaks. [`LowerRule`] lists them.
pub fn lower(
    sequence: &AuthoredSequence,
    expansions: &Expansions,
) -> Result<LoweredSequence, LowerErrors> {
    let mut lowerer = Lowerer {
        expansions,
        used: BTreeSet::new(),
        errors: Vec::new(),
    };
    let root = lowerer.group(&sequence.root, true);
    let Lowerer {
        used, mut errors, ..
    } = lowerer;
    errors.extend(
        expansions
            .keys()
            .filter(|node| !used.contains(node))
            .map(|&node| LowerError {
                node,
                rule: LowerRule::UnexpectedExpansion,
            }),
    );
    match NonEmpty::try_from(errors) {
        Ok(errors) => Err(LowerErrors(errors)),
        Err(_) => Ok(LoweredSequence {
            id: sequence.id.clone(),
            version: sequence.version,
            name: sequence.name.clone(),
            default_vars: sequence.default_vars.clone(),
            root,
        }),
    }
}

/// The ID of child `index` of the expansion of the step with ID `step`.
fn child_id(step: NodeId, index: usize) -> NodeId {
    NodeId(Uuid::new_v5(&step.0, index.to_string().as_bytes()))
}

struct Lowerer<'e> {
    expansions: &'e Expansions,
    /// The steps whose expansions were used.
    used: BTreeSet<NodeId>,
    errors: Vec<LowerError>,
}

impl Lowerer<'_> {
    /// `kept` is whether the program keeps the group's parent.
    fn group(&mut self, group: &Group<Action>, kept: bool) -> Group<BuiltinAction> {
        let kept = kept && group.meta.enabled;
        Group {
            meta: group.meta.clone(),
            children: group
                .children
                .iter()
                .map(|child| self.node(child, kept))
                .collect(),
        }
    }

    fn node(&mut self, node: &Node<Action>, kept: bool) -> Node<BuiltinAction> {
        match node {
            Node::Group(group) => Node::Group(self.group(group, kept)),
            Node::Step(step) => self.step(step, kept),
        }
    }

    fn step(&mut self, step: &Step<Action>, kept: bool) -> Node<BuiltinAction> {
        let Action::Builtin(action) = &step.action else {
            // Disabled join slots count.
            if let Action::ApplyJoinProfile { stage } = step.action
                && step.run_in != stage.run_in()
            {
                self.fail(
                    step.meta.id,
                    LowerRule::JoinStageRunIn {
                        stage,
                        run_in: step.run_in,
                    },
                );
            }
            // The compiler checks this rule on steps, and this step becomes a
            // group. Disabled steps count, as they do in the compiler.
            if let Some(condition) = win_pe_full_os_condition(step.run_in, &step.meta) {
                self.fail(step.meta.id, LowerRule::FullOsCondition { condition });
            }
            let children = if kept && step.meta.enabled {
                self.expansion(step)
            } else {
                Vec::new()
            };
            return Node::Group(Group {
                meta: step.meta.clone(),
                children,
            });
        };
        if action.only_from_addon() {
            self.fail(
                step.meta.id,
                LowerRule::AddOnOnlyAction {
                    action: action_name(action),
                },
            );
        }
        Node::Step(Step {
            meta: step.meta.clone(),
            run_in: step.run_in,
            action: action.clone(),
        })
    }

    fn expansion(&mut self, step: &Step<Action>) -> Vec<Node<BuiltinAction>> {
        let node = step.meta.id;
        let Some(expansion) = self.expansions.get(&node) else {
            self.fail(node, LowerRule::MissingExpansion);
            return Vec::new();
        };
        self.used.insert(node);
        let mut children = Vec::with_capacity(expansion.len());
        for (child, expanded) in expansion.iter().enumerate() {
            if expanded.run_in != step.run_in {
                self.fail(
                    node,
                    LowerRule::RunIn {
                        child,
                        run_in: expanded.run_in,
                        expected: step.run_in,
                    },
                );
            }
            match &expanded.action {
                Action::Builtin(action) if action.allowed_from_addon() => {
                    children.push(Node::Step(Step {
                        meta: NodeMeta {
                            id: child_id(node, child),
                            ..expanded.meta.clone()
                        },
                        run_in: expanded.run_in,
                        action: action.clone(),
                    }));
                }
                Action::Builtin(action) => self.fail(
                    node,
                    LowerRule::NotAllowed {
                        child,
                        action: action_name(action),
                    },
                ),
                Action::AddOn(_) | Action::ApplyJoinProfile { .. } => {
                    self.fail(node, LowerRule::Nested { child });
                }
            }
        }
        children
    }

    fn fail(&mut self, node: NodeId, rule: LowerRule) {
        self.errors.push(LowerError { node, rule });
    }
}

/// A rule that the input to [`lower`] breaks. `node` is the ID of the step
/// that breaks it: an add-on step or a join slot, or, for
/// [`LowerRule::AddOnOnlyAction`], a built-in step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError {
    pub node: NodeId,
    pub rule: LowerRule,
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { node, rule } = self;
        write!(f, "node {}: {rule}", node.0)
    }
}

impl core::error::Error for LowerError {}

/// The rules that [`lower`] checks. `child` is an index into the step's
/// expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerRule {
    /// The program keeps the step, and the expansions hold none for it.
    MissingExpansion,
    /// The expansions hold one for a node that is not a step that
    /// [`deferred`] lists.
    UnexpectedExpansion,
    /// The child's action is not one of the six that an add-on may return.
    /// See [`BuiltinAction::allowed_from_addon`].
    NotAllowed { child: usize, action: &'static str },
    /// The child's `run_in` differs from the step's.
    RunIn {
        child: usize,
        run_in: RunIn,
        expected: RunIn,
    },
    /// The child is an add-on step or a join slot.
    Nested { child: usize },
    /// The step has `run_in` `WinPe`, and its condition holds an
    /// `installed_msi` or `wmi` condition, which only the full OS can answer.
    /// `condition` names the first one. The compiler checks this rule on
    /// steps, and the group that replaces this step keeps its condition.
    /// Disabled steps count.
    FullOsCondition { condition: &'static str },
    /// The step is a join slot, and its `run_in` is not the one that its
    /// stage needs. See [`JoinStage::run_in`]. Disabled join slots count.
    JoinStageRunIn { stage: JoinStage, run_in: RunIn },
    /// The step is a built-in step of the task sequence, outside any
    /// expansion, and its action is one that only an expansion may hold. See
    /// [`BuiltinAction::only_from_addon`]. Disabled steps count.
    AddOnOnlyAction { action: &'static str },
}

impl fmt::Display for LowerRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExpansion => f.write_str(
                "the server must expand this step before it compiles, and no expansion was given",
            ),
            Self::UnexpectedExpansion => f.write_str(
                "an expansion was given for a node that is not an enabled add-on step or join slot",
            ),
            Self::NotAllowed { child, action } => write!(
                f,
                "child {child} is `{action}`, and an add-on may return only `run_command`, `run_powershell`, `set_variable`, `restart`, `server_call`, and `add_provisioning_package`"
            ),
            Self::RunIn {
                child,
                run_in,
                expected,
            } => write!(
                f,
                "child {child} has `run_in` `{}`, and the step it replaces has `{}`",
                run_in_name(*run_in),
                run_in_name(*expected)
            ),
            Self::Nested { child } => write!(
                f,
                "child {child} is an add-on step or a join slot, and an expansion holds only built-in actions"
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
        }
    }
}

/// Every rule that the input to [`lower`] breaks: the ones inside the tree in
/// document order, then the unexpected expansions in node ID order. It holds
/// at least one error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerErrors(NonEmpty<LowerError>);

impl LowerErrors {
    pub fn as_slice(&self) -> &[LowerError] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a LowerErrors {
    type Item = &'a LowerError;
    type IntoIter = core::slice::Iter<'a, LowerError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// One error on each line.
impl fmt::Display for LowerErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            error.fmt(f)?;
        }
        Ok(())
    }
}

impl core::error::Error for LowerErrors {}
