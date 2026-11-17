//! Compiles a lowered task sequence into the flat program that the agent
//! runs. The server also compiles a sequence when it is saved, to reject one
//! that cannot run. A sequence with add-on steps or join slots must be
//! lowered first. Issue #188 tracks the save-time checks for such a sequence.
//!
//! [`compile`] takes a [`LoweredSequence`], whose steps hold only built-in
//! actions, so a program that names an add-on does not type-check.

use alloc::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use alloc::vec::Vec;
use core::fmt;

use crate::action::BuiltinAction;
use crate::condition::Condition;
use crate::non_empty::NonEmpty;
use crate::sequence::{
    Group, LoweredSequence, Node, NodeId, NodeMeta, RunIn, Step, TaskSequenceId,
};

/// A task sequence compiled into a flat list of instructions.
///
/// Only [`compile`] builds one. Every jump in it lands after the instruction
/// that holds it and no later than [`Program::end`], so a run only moves
/// forward and cannot loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    source: (TaskSequenceId, u32),
    instrs: Vec<Instr>,
}

impl Program {
    /// The ID and version of the task sequence that the program comes from.
    pub fn source(&self) -> &(TaskSequenceId, u32) {
        &self.source
    }

    pub fn instrs(&self) -> &[Instr] {
        &self.instrs
    }

    /// The program counter just past the last instruction. A run that reaches
    /// it is done.
    pub fn end(&self) -> Pc {
        Pc(self.instrs.len())
    }
}

/// A program counter: the index of an instruction in [`Program::instrs`].
/// [`Program::end`] is one past the last instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pc(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instr {
    /// The start of a group. If the condition is false, the run jumps to
    /// `skip_to`, past the group's last child.
    Enter {
        node: NodeId,
        condition: Option<Condition>,
        skip_to: Pc,
    },
    /// A step. If the condition is false, the run skips the step.
    Exec {
        node: NodeId,
        condition: Option<Condition>,
        run_in: RunIn,
        action: BuiltinAction,
        on_error: OnError,
    },
}

/// Where the run goes when a step fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnError {
    /// The next instruction. The step has `continue_on_error`.
    Continue,
    /// The end of the nearest enclosing group with `continue_on_error`.
    JumpTo(Pc),
    /// Nowhere. The deployment fails, because no enclosing group has
    /// `continue_on_error`.
    FailDeployment,
}

/// Compiles a lowered task sequence into the program that the agent runs.
///
/// The compiler drops disabled nodes and works out each step's `on_error`, so
/// a failure needs no group stack at run time. A group, the root included,
/// compiles to an [`Instr::Enter`] followed by its children.
///
/// It fails with every rule that the sequence breaks, in document order.
/// [`Rule`] lists the rules. A disabled node is still checked against every
/// rule but the ones about program order, because it is still part of the
/// sequence that is saved.
pub fn compile(sequence: &LoweredSequence) -> Result<Program, CompileErrors> {
    let mut compiler = Compiler::default();
    compiler.group(
        &sequence.root,
        Place::Kept {
            on_failure: OnError::FailDeployment,
        },
    );
    let Compiler { instrs, errors, .. } = compiler;
    match NonEmpty::try_from(errors) {
        Ok(errors) => Err(CompileErrors(errors)),
        Err(_) => Ok(Program {
            source: (sequence.id.clone(), sequence.version),
            instrs,
        }),
    }
}

#[derive(Default)]
struct Compiler {
    instrs: Vec<Instr>,
    errors: Vec<CompileError>,
    /// The first node with each ID.
    ids: BTreeMap<NodeId, NodePath>,
    /// The child index at each level from the root down to the current node.
    path: Vec<usize>,
    /// Whether the program holds a `PartitionDisk` step so far.
    partitioned: bool,
    /// Whether the program holds an `ApplyOsImage` step so far.
    imaged: bool,
}

/// Whether the program keeps a node.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// The node is disabled, or inside a disabled group.
    Dropped,
    /// The program keeps the node. A step here that fails, and has no
    /// `continue_on_error`, takes `on_failure`.
    Kept { on_failure: OnError },
}

impl Compiler {
    fn node(&mut self, node: &Node<BuiltinAction>, place: Place) {
        match node {
            Node::Group(group) => self.group(group, place),
            Node::Step(step) => self.step(step, place),
        }
    }

    fn group(&mut self, group: &Group<BuiltinAction>, place: Place) {
        let meta = &group.meta;
        self.check_id(meta);
        let (children, end) = match place {
            Place::Kept { on_failure } if meta.enabled => {
                let end = Pc(self.instrs.len() + kept_len(group));
                self.instrs.push(Instr::Enter {
                    node: meta.id,
                    condition: meta.condition.clone(),
                    skip_to: end,
                });
                let on_failure = if meta.continue_on_error {
                    OnError::JumpTo(end)
                } else {
                    on_failure
                };
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

    fn step(&mut self, step: &Step<BuiltinAction>, place: Place) {
        let meta = &step.meta;
        self.check_id(meta);
        self.check_step(step);
        let Place::Kept { on_failure } = place else {
            return;
        };
        if !meta.enabled {
            return;
        }
        self.check_order(step);
        self.instrs.push(Instr::Exec {
            node: meta.id,
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

    fn check_id(&mut self, meta: &NodeMeta) {
        let path = NodePath(self.path.clone());
        match self.ids.entry(meta.id) {
            Entry::Vacant(entry) => {
                entry.insert(path);
            }
            Entry::Occupied(entry) => {
                let first = entry.get().clone();
                self.errors.push(CompileError {
                    node: meta.id,
                    path,
                    rule: Rule::DuplicateNodeId { first },
                });
            }
        }
    }

    /// The rules about the step by itself, which cover every step.
    fn check_step(&mut self, step: &Step<BuiltinAction>) {
        let allowed = step.action.allowed_run_in();
        if !allowed.contains(&step.run_in) {
            self.fail(
                step.meta.id,
                Rule::RunIn {
                    action: action_name(&step.action),
                    run_in: step.run_in,
                    allowed,
                },
            );
        }
        if let Some(condition) = win_pe_full_os_condition(step.run_in, &step.meta) {
            self.fail(step.meta.id, Rule::FullOsCondition { condition });
        }
    }

    /// The rules about program order, which cover only the steps that the
    /// program keeps.
    fn check_order(&mut self, step: &Step<BuiltinAction>) {
        match step.action {
            BuiltinAction::PartitionDisk { .. } => self.partitioned = true,
            BuiltinAction::ApplyOsImage { .. } => self.imaged = true,
            BuiltinAction::Restart { .. } if step.run_in == RunIn::WinPe && !self.partitioned => {
                self.fail(step.meta.id, Rule::RestartBeforePartition);
            }
            BuiltinAction::ApplyDrivers { .. } | BuiltinAction::ApplyUnattend { .. }
                if !self.imaged =>
            {
                self.fail(
                    step.meta.id,
                    Rule::BeforeOsImage {
                        action: action_name(&step.action),
                    },
                );
            }
            _ => {}
        }
    }

    fn fail(&mut self, node: NodeId, rule: Rule) {
        self.errors.push(CompileError {
            node,
            path: NodePath(self.path.clone()),
            rule,
        });
    }
}

/// How many instructions a kept group compiles to: its `Enter`, and one for
/// each kept node inside it.
fn kept_len(group: &Group<BuiltinAction>) -> usize {
    let children: usize = group
        .children
        .iter()
        .map(|child| match child {
            Node::Group(group) if group.meta.enabled => kept_len(group),
            Node::Step(step) if step.meta.enabled => 1,
            Node::Group(_) | Node::Step(_) => 0,
        })
        .sum();
    1 + children
}

/// For a step with `run_in` `WinPe`, the JSON name of the first condition in
/// its condition, in document order, that only the full OS can answer.
///
/// The compiler rejects a built-in step that has one. [`crate::lower::lower`]
/// and [`crate::validate::validate`] reject an add-on step or a join slot that
/// has one, because the group that replaces such a step is not checked.
pub(crate) fn win_pe_full_os_condition(run_in: RunIn, meta: &NodeMeta) -> Option<&'static str> {
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
pub(crate) fn action_name(action: &BuiltinAction) -> &'static str {
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
        BuiltinAction::VerifyDrivers { .. } => "verify_drivers",
        BuiltinAction::CheckReadiness { .. } => "check_readiness",
        BuiltinAction::ActivateWindows { .. } => "activate_windows",
        BuiltinAction::ServerCall { .. } => "server_call",
        BuiltinAction::AddProvisioningPackage { .. } => "add_provisioning_package",
    }
}

/// The `run_in` value as the JSON format writes it.
pub(crate) fn run_in_name(run_in: RunIn) -> &'static str {
    match run_in {
        RunIn::WinPe => "win_pe",
        RunIn::FullOs => "full_os",
        RunIn::Either => "either",
    }
}

/// Where a node is in the tree: the child index at each level from the root
/// down to the node. The root's path is empty.
///
/// It is written like the path of a parse error, such as
/// `root.children[1].group.children[0]` for the first child of the root's
/// second child, and `root` for the root.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodePath(Vec<usize>);

impl NodePath {
    pub fn indices(&self) -> &[usize] {
        &self.0
    }
}

impl From<Vec<usize>> for NodePath {
    fn from(indices: Vec<usize>) -> Self {
        Self(indices)
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("root")?;
        for (depth, index) in self.0.iter().enumerate() {
            if depth > 0 {
                f.write_str(".group")?;
            }
            write!(f, ".children[{index}]")?;
        }
        Ok(())
    }
}

/// A rule that a task sequence breaks. It names the node by its ID and by its
/// place in the tree, because a duplicate ID alone cannot locate the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub node: NodeId,
    pub path: NodePath,
    pub rule: Rule,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { node, path, rule } = self;
        write!(f, "{path}, node {}: {rule}", node.0)
    }
}

impl core::error::Error for CompileError {}

/// The rules that the compiler checks when a sequence is saved.
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
    /// `condition` names the first one. Disabled steps count.
    FullOsCondition { condition: &'static str },
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
    /// An earlier node in the tree, at `first`, has the same ID. Disabled
    /// nodes count.
    DuplicateNodeId { first: NodePath },
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
            Self::RestartBeforePartition => f.write_str(
                "a `restart` with `run_in` `win_pe` must come after the first `partition_disk` step. Until then the state is on `X:`, and the restart loses it",
            ),
            Self::BeforeOsImage { action } => write!(
                f,
                "`{action}` acts on the applied image, so it must come after the first `apply_os_image` step"
            ),
            Self::DuplicateNodeId { first } => {
                write!(f, "the node at `{first}` has the same ID")
            }
        }
    }
}

/// Every rule that a task sequence breaks, in document order. It holds at
/// least one error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileErrors(NonEmpty<CompileError>);

impl CompileErrors {
    pub fn as_slice(&self) -> &[CompileError] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a CompileErrors {
    type Item = &'a CompileError;
    type IntoIter = core::slice::Iter<'a, CompileError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// One error on each line.
impl fmt::Display for CompileErrors {
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

impl core::error::Error for CompileErrors {}

#[cfg(test)]
mod tests {
    use super::*;

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
            package: "enrollment".parse().expect("a valid secret reference"),
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
        assert_eq!(seen.len(), 14);
    }

    #[test]
    fn run_in_names_are_the_json_values() {
        for run_in in [RunIn::WinPe, RunIn::FullOs, RunIn::Either] {
            let json = serde_json::to_value(run_in).expect("a run_in serializes");
            assert_eq!(json.as_str(), Some(run_in_name(run_in)));
        }
    }
}
