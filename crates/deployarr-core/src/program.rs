//! The flat program that the agent runs. Only [`crate::plan::plan`] builds
//! one, from an authored task sequence and the expansions of its add-on steps
//! and join slots.

use alloc::vec::Vec;

use crate::action::BuiltinAction;
use crate::condition::Condition;
use crate::sequence::{NodeId, RunIn, TaskSequenceId};

/// A task sequence compiled into a flat list of instructions.
///
/// Only [`crate::plan::plan`] builds one. Every jump in it lands after the
/// instruction that holds it and no later than [`Program::end`], so a run only
/// moves forward and cannot loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    source: (TaskSequenceId, u32),
    instrs: Vec<Instr>,
}

impl Program {
    pub(crate) fn new(source: (TaskSequenceId, u32), instrs: Vec<Instr>) -> Self {
        Self { source, instrs }
    }

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
    /// `skip_to`, past the group's last child. If the condition fails to
    /// evaluate, the group fails, and the run follows `on_error`. It is the
    /// `on_error` of a step without `continue_on_error` directly inside the
    /// group, and never [`OnError::Continue`].
    ///
    /// An add-on step or a join slot becomes a group that keeps the step's
    /// node ID and holds its expansion.
    Enter {
        node: NodeId,
        condition: Option<Condition>,
        skip_to: Pc,
        on_error: OnError,
    },
    /// A step. If the condition is false, the run skips the step.
    ///
    /// A step of an expansion has a node ID derived from the add-on step or
    /// join slot that it replaces. See [`crate::plan::plan`].
    Exec {
        node: NodeId,
        condition: Option<Condition>,
        run_in: RunIn,
        action: BuiltinAction,
        on_error: OnError,
    },
}

/// Where the run goes when a step fails, or when a group's condition fails to
/// evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnError {
    /// The next instruction. The step has `continue_on_error`. Only a step
    /// takes it, never an [`Instr::Enter`].
    Continue,
    /// The end of the nearest enclosing group with `continue_on_error`. For an
    /// [`Instr::Enter`], that group can be the group itself.
    JumpTo(Pc),
    /// Nowhere. The deployment fails, because no enclosing group has
    /// `continue_on_error`. For an [`Instr::Enter`], the group itself has none
    /// either.
    FailDeployment,
}
