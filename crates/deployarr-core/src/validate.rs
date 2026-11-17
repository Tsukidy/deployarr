//! The checks that the server runs on an authored task sequence when it is
//! saved. They run no add-on code, so they work when an add-on is disabled or
//! gone.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;

use crate::action::Action;
use crate::addon::AddOnRef;
use crate::compile::{NodePath, win_pe_full_os_condition};
use crate::sequence::{AuthoredSequence, Group, Node, NodeId, Step};

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

/// Checks a sequence's add-on steps and join slots, and returns what it finds
/// in document order. Pure. [`Issue`] lists the checks.
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
    validator.group(&sequence.root);
    validator.findings
}

struct Validator<'k, K: ?Sized> {
    kinds: &'k K,
    /// The child index at each level from the root down to the current node.
    path: Vec<usize>,
    findings: Vec<Finding>,
}

impl<K: StepKinds + ?Sized> Validator<'_, K> {
    fn group(&mut self, group: &Group<Action>) {
        for (index, child) in group.children.iter().enumerate() {
            self.path.push(index);
            match child {
                Node::Group(group) => self.group(group),
                Node::Step(step) => self.step(step),
            }
            self.path.pop();
        }
    }

    fn step(&mut self, step: &Step<Action>) {
        if let Action::Builtin(_) = step.action {
            return;
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
}

impl Issue {
    pub fn severity(&self) -> Severity {
        match self {
            Self::UnknownStepKind { .. } => Severity::Warning,
            Self::FullOsCondition { .. } => Severity::Error,
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
        }
    }
}
