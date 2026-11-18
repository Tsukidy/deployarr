//! The substitution pass. It builds the tree that the rule pass reads, with
//! each deferred step in place, and runs the checks that need the authored
//! step rather than the tree.

use alloc::collections::BTreeSet;
use alloc::string::ToString;
use alloc::vec::Vec;

use uuid::Uuid;

use super::{
    DeferredKind, Expansions, Found, NodePath, PlanCheck, Rule, SaveCheck, action_name,
    win_pe_full_os_condition,
};
use crate::action::{Action, BuiltinAction, SecretSource};
use crate::addon::AddOnStep;
use crate::catalog::Catalog;
use crate::form::{ConfigProblem, check_config};
use crate::sequence::{self, AuthoredSequence, NodeId, NodeMeta, RunIn};

/// A group of the tree.
pub(super) struct Group<'a> {
    pub meta: &'a NodeMeta,
    pub children: Vec<Node<'a>>,
}

pub(super) enum Node<'a> {
    Group(Group<'a>),
    Step(Step<'a>),
    Deferred(DeferredStep<'a>),
}

/// A built-in step: one of the authored sequence, or a child of an
/// expansion.
pub(super) struct Step<'a> {
    /// The step's ID in the program. A child of an expansion has an ID
    /// derived from the deferred step's. Its `meta.id` is the one that its
    /// add-on gave it, which the program does not use.
    pub id: NodeId,
    pub meta: &'a NodeMeta,
    pub run_in: RunIn,
    pub action: &'a BuiltinAction,
}

/// An add-on step or a join slot.
pub(super) struct DeferredStep<'a> {
    pub meta: &'a NodeMeta,
    pub run_in: RunIn,
    pub expansion: Expansion<'a>,
}

pub(super) enum Expansion<'a> {
    /// At save, the step has no expansion yet. It may hold any action that an
    /// add-on may return.
    Unexpanded,
    /// At planning, the step's expansion. It is empty for a step that the
    /// program drops.
    Expanded(Vec<Step<'a>>),
}

/// What the pass does with a deferred step at save or at planning.
pub(super) trait Mode<'a> {
    /// What the pass finds.
    type Check: From<Rule>;

    /// Checks a deferred step, and returns what the tree holds in its place.
    /// `kept` says whether the program keeps the step.
    fn deferred(
        &mut self,
        step: &'a sequence::Step<Action>,
        what: DeferredKind<'a>,
        kept: bool,
        finder: &mut Finder<Self::Check>,
    ) -> Expansion<'a>;
}

/// Builds the tree from the authored sequence, and returns it with what the
/// pass finds in document order.
pub(super) fn lower<'a, M: Mode<'a>>(
    sequence: &'a AuthoredSequence,
    mode: &mut M,
) -> (Group<'a>, Vec<Found<M::Check>>) {
    let mut lowerer = Lowerer {
        mode,
        finder: Finder {
            path: Vec::new(),
            found: Vec::new(),
        },
    };
    let root = lowerer.group(&sequence.root, true);
    (root, lowerer.finder.found)
}

/// Where the pass is in the authored sequence, and what it has found.
pub(super) struct Finder<C> {
    /// The child index at each level from the root down to the current node.
    path: Vec<usize>,
    found: Vec<Found<C>>,
}

impl<C> Finder<C> {
    /// Records `check` at the current node, whose ID is `node`.
    fn find(&mut self, node: NodeId, check: impl Into<C>) {
        self.found.push(Found {
            node,
            path: NodePath::from(self.path.clone()),
            check: check.into(),
        });
    }

    /// Records `check` at child `child` of the expansion of the current node,
    /// whose ID is `node`.
    fn find_in_child(&mut self, node: NodeId, child: usize, check: C) {
        self.found.push(Found {
            node,
            path: NodePath::from(self.path.clone()).with_child(child),
            check,
        });
    }
}

struct Lowerer<'a, 'm, M: Mode<'a>> {
    mode: &'m mut M,
    finder: Finder<M::Check>,
}

impl<'a, M: Mode<'a>> Lowerer<'a, '_, M> {
    /// `kept` says whether the program keeps the group's parent, or `true`
    /// for the root.
    fn group(&mut self, group: &'a sequence::Group<Action>, kept: bool) -> Group<'a> {
        let kept = kept && group.meta.enabled;
        let mut children = Vec::with_capacity(group.children.len());
        for (index, child) in group.children.iter().enumerate() {
            self.finder.path.push(index);
            children.push(match child {
                sequence::Node::Group(group) => Node::Group(self.group(group, kept)),
                sequence::Node::Step(step) => self.step(step, kept),
            });
            self.finder.path.pop();
        }
        Group {
            meta: &group.meta,
            children,
        }
    }

    /// `kept` says whether the program keeps the step's parent.
    fn step(&mut self, step: &'a sequence::Step<Action>, kept: bool) -> Node<'a> {
        let what = match &step.action {
            Action::Builtin(action) => {
                if action.only_from_addon() {
                    self.finder.find(
                        step.meta.id,
                        Rule::AddOnOnlyAction {
                            action: action_name(action),
                        },
                    );
                }
                return Node::Step(Step {
                    id: step.meta.id,
                    meta: &step.meta,
                    run_in: step.run_in,
                    action,
                });
            }
            Action::AddOn(add_on) => DeferredKind::AddOn(add_on),
            Action::ApplyJoinProfile { stage } => DeferredKind::Join(*stage),
        };
        if let DeferredKind::Join(stage) = what
            && step.run_in != stage.run_in()
        {
            self.finder.find(
                step.meta.id,
                Rule::JoinStageRunIn {
                    stage,
                    run_in: step.run_in,
                },
            );
        }
        if let Some(condition) = win_pe_full_os_condition(step.run_in, &step.meta) {
            self.finder
                .find(step.meta.id, Rule::FullOsCondition { condition });
        }
        let kept = kept && step.meta.enabled;
        let expansion = self.mode.deferred(step, what, kept, &mut self.finder);
        Node::Deferred(DeferredStep {
            meta: &step.meta,
            run_in: step.run_in,
            expansion,
        })
    }
}

/// The pass at save. Each deferred step stays unexpanded, and each add-on step
/// is checked against its kind in the catalog.
pub(super) struct Saving<'c> {
    pub catalog: &'c Catalog,
}

impl<'a> Mode<'a> for Saving<'_> {
    type Check = SaveCheck;

    fn deferred(
        &mut self,
        step: &'a sequence::Step<Action>,
        what: DeferredKind<'a>,
        _kept: bool,
        finder: &mut Finder<SaveCheck>,
    ) -> Expansion<'a> {
        if let DeferredKind::AddOn(add_on) = what {
            self.add_on_step(step, add_on, finder);
        }
        Expansion::Unexpanded
    }
}

impl Saving<'_> {
    /// Checks an add-on step against its kind's descriptor. Disabled steps
    /// count.
    fn add_on_step(
        &self,
        step: &sequence::Step<Action>,
        add_on: &AddOnStep,
        finder: &mut Finder<SaveCheck>,
    ) {
        let node = step.meta.id;
        let kind = add_on.step_kind();
        let Some(info) = self.catalog.step_kind(&kind) else {
            finder.find(node, SaveCheck::UnknownStepKind { kind });
            return;
        };
        if step.run_in != info.run_in {
            finder.find(
                node,
                SaveCheck::StepKindRunIn {
                    kind: kind.clone(),
                    declared: info.run_in,
                    run_in: step.run_in,
                },
            );
        }
        for problem in check_config(&info.config, add_on.config_version, &add_on.config) {
            let check = match problem {
                ConfigProblem::NoSchema => SaveCheck::NoConfigSchema {
                    kind: kind.clone(),
                    config_version: add_on.config_version,
                },
                ConfigProblem::Unreadable => SaveCheck::UnreadableConfig,
                ConfigProblem::Value(error) => SaveCheck::Config(error),
            };
            finder.find(node, check);
        }
    }
}

/// The pass at planning. Each kept deferred step becomes its expansion, which
/// is checked.
pub(super) struct Planning<'a> {
    expansions: &'a Expansions,
    /// The steps whose expansions were used.
    used: BTreeSet<NodeId>,
}

impl<'a> Planning<'a> {
    pub fn new(expansions: &'a Expansions) -> Self {
        Self {
            expansions,
            used: BTreeSet::new(),
        }
    }

    /// The IDs of the expansions that no kept deferred step used, in ID
    /// order.
    pub fn unused(&self) -> Vec<NodeId> {
        self.expansions
            .keys()
            .filter(|node| !self.used.contains(node))
            .copied()
            .collect()
    }
}

impl<'a> Mode<'a> for Planning<'a> {
    type Check = PlanCheck;

    fn deferred(
        &mut self,
        step: &'a sequence::Step<Action>,
        what: DeferredKind<'a>,
        kept: bool,
        finder: &mut Finder<PlanCheck>,
    ) -> Expansion<'a> {
        if !kept {
            return Expansion::Expanded(Vec::new());
        }
        let node = step.meta.id;
        let Some(expansion) = self.expansions.get(&node) else {
            finder.find(node, PlanCheck::MissingExpansion);
            return Expansion::Expanded(Vec::new());
        };
        self.used.insert(node);
        let mut children = Vec::with_capacity(expansion.len());
        for (child, expanded) in expansion.iter().enumerate() {
            if expanded.run_in != step.run_in {
                finder.find_in_child(
                    node,
                    child,
                    PlanCheck::ChildRunIn {
                        run_in: expanded.run_in,
                        expected: step.run_in,
                    },
                );
            }
            match &expanded.action {
                Action::Builtin(action) if action.allowed_from_addon() => {
                    if let BuiltinAction::AddProvisioningPackage { package, .. } = action
                        && !secret_source_fits(what, package.source)
                    {
                        finder.find_in_child(
                            node,
                            child,
                            PlanCheck::SecretSource {
                                source: package.source,
                            },
                        );
                    }
                    children.push(Step {
                        id: child_id(node, child),
                        meta: &expanded.meta,
                        run_in: expanded.run_in,
                        action,
                    });
                }
                Action::Builtin(action) => finder.find_in_child(
                    node,
                    child,
                    PlanCheck::NotAllowed {
                        action: action_name(action),
                    },
                ),
                Action::AddOn(_) | Action::ApplyJoinProfile { .. } => {
                    finder.find_in_child(node, child, PlanCheck::Nested);
                }
            }
        }
        Expansion::Expanded(children)
    }
}

/// The ID of child `index` of the expansion of the step with ID `step`.
fn child_id(step: NodeId, index: usize) -> NodeId {
    NodeId(Uuid::new_v5(&step.0, index.to_string().as_bytes()))
}

/// Whether the expansion of a step of kind `what` may name a secret in
/// `source`.
///
/// An add-on step has its config. A join slot has the deployment's join
/// profile, whose join kind names the step kind that expands the slot. Both
/// have the settings of the add-on that expands them. The match is
/// exhaustive, so a new source does not compile until someone decides where
/// it fits.
fn secret_source_fits(what: DeferredKind<'_>, source: SecretSource) -> bool {
    match (what, source) {
        (DeferredKind::AddOn(_), SecretSource::StepConfig | SecretSource::AddOnSettings)
        | (DeferredKind::Join(_), SecretSource::JoinProfile | SecretSource::AddOnSettings) => true,
        (DeferredKind::AddOn(_), SecretSource::JoinProfile)
        | (DeferredKind::Join(_), SecretSource::StepConfig) => false,
    }
}
