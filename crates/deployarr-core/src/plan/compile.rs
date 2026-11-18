//! The rule pass. It checks the rules in [`Rule`] over the tree that the
//! substitution pass builds, and builds the program's instructions.

use alloc::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use alloc::vec::Vec;

use super::lower::{DeferredStep, Expansion, Group, Node, Step};
use super::{Found, NodePath, Rule, action_name, full_os_only, win_pe_full_os_condition};
use crate::action::{ActionKind, BuiltinAction, RestartTarget};
use crate::program::{Instr, OnError, Pc};
use crate::sequence::{NodeId, NodeMeta, RunIn};

/// Checks the rules over the tree, and builds the instructions of its
/// program. It returns every rule that the tree breaks, in document order.
///
/// A disabled node is still checked against every rule but the ones about
/// program order, because it is still part of the sequence that is saved.
pub(super) fn compile(root: &Group<'_>) -> (Vec<Instr>, Vec<Found<Rule>>) {
    let mut compiler = Compiler {
        instrs: Vec::new(),
        found: Vec::new(),
        ids: BTreeMap::new(),
        path: Vec::new(),
        facts: Facts::default(),
    };
    compiler.group(
        root,
        Place::Kept {
            on_failure: OnError::FailDeployment,
        },
    );
    (compiler.instrs, compiler.found)
}

struct Compiler {
    instrs: Vec<Instr>,
    found: Vec<Found<Rule>>,
    /// Where the first node with each ID is.
    ids: BTreeMap<NodeId, NodePath>,
    /// The child index at each level from the root down to the current node.
    path: Vec<usize>,
    facts: Facts,
}

/// Whether the program keeps a node.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// The node is disabled, or inside a disabled group.
    Dropped,
    /// The program keeps the node. A step here that fails, and has no
    /// `continue_on_error`, takes `on_failure`. So does a group here without
    /// `continue_on_error` whose condition fails to evaluate.
    Kept { on_failure: OnError },
}

/// Where a rule is reported: a node of the authored sequence and its path.
/// For a step of an expansion, the node is the deferred step, and the path
/// holds the child's index.
struct At {
    node: NodeId,
    path: NodePath,
}

/// Whether a fact about the program so far holds. An unexpanded deferred step
/// makes a fact maybe, when an action that it might hold would make it true.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Fact {
    #[default]
    No,
    Maybe,
    Yes,
}

/// What the program holds so far, as the rules about program order need it.
#[derive(Debug, Default)]
struct Facts {
    /// A `PartitionDisk` step.
    partitioned: Fact,
    /// An `ApplyOsImage` step.
    imaged: Fact,
    /// An `EnableBitLocker` step.
    bitlocker_enabled: Fact,
    /// A `Restart` into the installed OS.
    restarted_into_os: Fact,
}

impl Facts {
    /// The fact that an action of `kind` can make true. A `Restart` makes its
    /// fact true only with `into` `InstalledOs`.
    ///
    /// The match is exhaustive, so a new action does not compile until
    /// someone decides whether it makes a fact true.
    fn made_true_by(&mut self, kind: ActionKind) -> Option<&mut Fact> {
        match kind {
            ActionKind::PartitionDisk => Some(&mut self.partitioned),
            ActionKind::ApplyOsImage => Some(&mut self.imaged),
            ActionKind::EnableBitLocker => Some(&mut self.bitlocker_enabled),
            ActionKind::Restart => Some(&mut self.restarted_into_os),
            ActionKind::RunCommand
            | ActionKind::RunPowerShell
            | ActionKind::SetVariable
            | ActionKind::ApplyDrivers
            | ActionKind::ApplyUnattend
            | ActionKind::InstallApplications
            | ActionKind::InstallUpdates
            | ActionKind::VerifyDrivers
            | ActionKind::SuspendBitLocker
            | ActionKind::CheckReadiness
            | ActionKind::ActivateWindows
            | ActionKind::ServerCall
            | ActionKind::AddProvisioningPackage => None,
        }
    }

    /// Records a step that the program keeps.
    fn step(&mut self, action: &BuiltinAction) {
        let makes_true = match action {
            BuiltinAction::Restart { into } => *into == RestartTarget::InstalledOs,
            _ => true,
        };
        if makes_true && let Some(fact) = self.made_true_by(action.kind()) {
            *fact = Fact::Yes;
        }
    }

    /// Records an unexpanded deferred step that the program keeps. Its
    /// expansion may hold any action whose kind an add-on may return, so each
    /// fact that such an action can make true is now maybe, unless it is
    /// already yes.
    fn unexpanded(&mut self) {
        for kind in ActionKind::ALL {
            if kind.allowed_from_addon()
                && let Some(fact) = self.made_true_by(*kind)
                && *fact == Fact::No
            {
                *fact = Fact::Maybe;
            }
        }
    }
}

impl Compiler {
    /// The path of the current node.
    fn path(&self) -> NodePath {
        NodePath::from(self.path.clone())
    }

    fn node(&mut self, node: &Node<'_>, place: Place) {
        match node {
            Node::Group(group) => self.group(group, place),
            Node::Step(step) => {
                let at = At {
                    node: step.id,
                    path: self.path(),
                };
                self.step(step, &at, place);
            }
            Node::Deferred(deferred) => self.deferred(deferred, place),
        }
    }

    fn group(&mut self, group: &Group<'_>, place: Place) {
        let meta = group.meta;
        let at = At {
            node: meta.id,
            path: self.path(),
        };
        self.check_id(meta.id, &at);
        let (children, end) = match place {
            Place::Kept { on_failure } if meta.enabled => {
                self.check_condition_before_os(meta, &at);
                let end = Pc(self.instrs.len() + kept_len(group));
                let on_failure = self.enter(meta, end, on_failure);
                (Place::Kept { on_failure }, Some(end))
            }
            _ => (Place::Dropped, None),
        };
        for (index, child) in group.children.iter().enumerate() {
            self.path.push(index);
            self.node(child, children);
            self.path.pop();
        }
        if let Some(end) = end {
            debug_assert_eq!(
                Pc(self.instrs.len()),
                end,
                "kept_len counts what group emits"
            );
        }
    }

    /// Emits the `Enter` of a kept group whose instructions end at `end`, and
    /// returns the `on_failure` of the nodes directly inside it.
    fn enter(&mut self, meta: &NodeMeta, end: Pc, on_failure: OnError) -> OnError {
        let on_failure = if meta.continue_on_error {
            OnError::JumpTo(end)
        } else {
            on_failure
        };
        // A condition error fails the group the way a step without
        // `continue_on_error` directly inside it fails, so `Enter` takes that
        // step's target.
        self.instrs.push(Instr::Enter {
            node: meta.id,
            condition: meta.condition.clone(),
            skip_to: end,
            on_error: on_failure,
        });
        on_failure
    }

    /// An add-on step or a join slot. Expanded, it is a group that holds its
    /// expansion.
    fn deferred(&mut self, deferred: &DeferredStep<'_>, place: Place) {
        let meta = deferred.meta;
        let at = At {
            node: meta.id,
            path: self.path(),
        };
        self.check_id(meta.id, &at);
        let Place::Kept { on_failure } = place else {
            return;
        };
        if !meta.enabled {
            return;
        }
        // A `WinPe` step with a full-OS-only condition breaks
        // `Rule::FullOsCondition`, which the substitution pass reports, so
        // the step gets one issue for its condition.
        if deferred.run_in != RunIn::WinPe {
            self.check_condition_before_os(meta, &at);
        }
        match &deferred.expansion {
            Expansion::Unexpanded => self.facts.unexpanded(),
            Expansion::Expanded(children) => {
                let end = Pc(self.instrs.len() + expanded_len(children));
                let on_failure = self.enter(meta, end, on_failure);
                for (index, child) in children.iter().enumerate() {
                    let at = At {
                        node: meta.id,
                        path: at.path.clone().with_child(index),
                    };
                    self.step(child, &at, Place::Kept { on_failure });
                }
                debug_assert_eq!(
                    Pc(self.instrs.len()),
                    end,
                    "expanded_len counts what deferred emits"
                );
            }
        }
    }

    fn step(&mut self, step: &Step<'_>, at: &At, place: Place) {
        let meta = step.meta;
        self.check_id(step.id, at);
        self.check_step(step, at);
        let Place::Kept { on_failure } = place else {
            return;
        };
        if !meta.enabled {
            return;
        }
        self.check_order(step, at);
        self.instrs.push(Instr::Exec {
            node: step.id,
            condition: meta.condition.clone(),
            run_in: step.run_in,
            action: step.action.clone(),
            on_error: if meta.continue_on_error {
                OnError::Continue
            } else {
                on_failure
            },
        });
    }

    /// `id` is the node's ID in the program.
    fn check_id(&mut self, id: NodeId, at: &At) {
        match self.ids.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(at.path.clone());
            }
            Entry::Occupied(entry) => {
                let first = entry.get().clone();
                self.fail(at, Rule::DuplicateNodeId { first });
            }
        }
    }

    /// The rules about the step by itself, which cover every built-in step.
    fn check_step(&mut self, step: &Step<'_>, at: &At) {
        let allowed = step.action.allowed_run_in();
        if !allowed.contains(&step.run_in) {
            self.fail(
                at,
                Rule::RunIn {
                    action: action_name(step.action),
                    run_in: step.run_in,
                    allowed,
                },
            );
        }
        if let Some(condition) = win_pe_full_os_condition(step.run_in, step.meta) {
            self.fail(at, Rule::FullOsCondition { condition });
        }
    }

    /// The rules about program order, which cover only the steps that the
    /// program keeps. Each one is reported only when the facts say that every
    /// expansion of an unexpanded deferred step would break it.
    fn check_order(&mut self, step: &Step<'_>, at: &At) {
        // The run evaluates the condition of the first `Restart` into the
        // installed OS in WinPE, so this check comes before the step counts.
        if step.run_in == RunIn::Either {
            self.check_condition_before_os(step.meta, at);
        }
        match step.action {
            BuiltinAction::Restart { into } => {
                if step.run_in == RunIn::WinPe && self.facts.partitioned == Fact::No {
                    self.fail(at, Rule::RestartBeforePartition);
                }
                if *into == RestartTarget::WinPe && self.facts.bitlocker_enabled == Fact::Yes {
                    self.fail(at, Rule::RestartIntoWinPeAfterBitLocker);
                }
            }
            BuiltinAction::ApplyDrivers { .. } | BuiltinAction::ApplyUnattend { .. }
                if self.facts.imaged == Fact::No =>
            {
                self.fail(
                    at,
                    Rule::BeforeOsImage {
                        action: action_name(step.action),
                    },
                );
            }
            _ => {}
        }
        self.facts.step(step.action);
    }

    /// The rule about a full-OS-only condition at or before the first
    /// `Restart` into the installed OS. It covers the groups, the `Either`
    /// steps, and the deferred steps that the program keeps.
    fn check_condition_before_os(&mut self, meta: &NodeMeta, at: &At) {
        if self.facts.restarted_into_os != Fact::No {
            return;
        }
        if let Some(condition) = meta.condition.as_ref().and_then(full_os_only) {
            self.fail(at, Rule::FullOsConditionBeforeInstalledOs { condition });
        }
    }

    fn fail(&mut self, at: &At, rule: Rule) {
        self.found.push(Found {
            node: at.node,
            path: at.path.clone(),
            check: rule,
        });
    }
}

/// How many instructions a kept group compiles to: its `Enter`, and one for
/// each kept node inside it.
fn kept_len(group: &Group<'_>) -> usize {
    let children: usize = group
        .children
        .iter()
        .map(|child| match child {
            Node::Group(group) if group.meta.enabled => kept_len(group),
            Node::Step(step) if step.meta.enabled => 1,
            Node::Deferred(deferred) if deferred.meta.enabled => match &deferred.expansion {
                Expansion::Unexpanded => 0,
                Expansion::Expanded(children) => expanded_len(children),
            },
            Node::Group(_) | Node::Step(_) | Node::Deferred(_) => 0,
        })
        .sum();
    1 + children
}

/// How many instructions a kept deferred step with this expansion compiles
/// to: its `Enter`, and one for each enabled step of the expansion.
fn expanded_len(children: &[Step<'_>]) -> usize {
    1 + children.iter().filter(|child| child.meta.enabled).count()
}
