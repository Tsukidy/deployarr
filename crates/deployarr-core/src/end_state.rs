//! The state that a deployment leaves the PC in.

use serde::{Deserialize, Serialize};

/// How a deployment finishes. The `EndState` variable holds it, and these are
/// its only two values. Each is written as its name, such as `CustomerOobe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EndState {
    /// The unattend file skips OOBE, and the PC is ready to use.
    ReadyToUse,
    /// The agent runs `sysprep /oobe`, and the customer runs OOBE.
    CustomerOobe,
}
