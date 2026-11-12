use serde::{Deserialize, Serialize};

/// Text that can hold `%Name%` variable references.
///
/// The model keeps the text as written. The agent substitutes variables when
/// it runs the step or evaluates the condition, so no text is rejected here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Template(String);

impl Template {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
