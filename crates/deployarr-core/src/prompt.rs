//! What the agent shows a technician in WinPE when a lookup resolver needs
//! typed input, such as an order number.
//!
//! An add-on declares a prompt as a `[[prompt]]` table in its manifest. The
//! agent renders it, and no add-on code runs on the PC. A barcode scanner that
//! types like a keyboard works with it too.
//!
//! ```toml
//! [[prompt]]
//! id = "order-number"
//! title = "Order number"
//!
//! [[prompt.field]]
//! key = "order_no"
//! label = "Scan or type the order number"
//! pattern = "[A-Za-z0-9-]{1,40}"
//! max_len = 40
//! ```

use alloc::string::String;
use core::fmt;
use core::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::addon::KindId;
use crate::form::{FieldKey, Pattern};
use crate::non_empty::NonEmpty;

/// One screen of typed input. Its ID is unique within the add-on that
/// declares it. No two of its fields have the same key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PromptWire", into = "PromptWire")]
pub struct PromptSpec {
    id: KindId,
    title: String,
    fields: NonEmpty<PromptField>,
}

impl PromptSpec {
    /// The prompt, or the first key that two of its fields share.
    pub fn new(
        id: KindId,
        title: String,
        fields: NonEmpty<PromptField>,
    ) -> Result<Self, DuplicatePromptKey> {
        for (index, field) in fields.iter().enumerate() {
            if fields[..index].iter().any(|seen| seen.key == field.key) {
                return Err(DuplicatePromptKey(field.key.clone()));
            }
        }
        Ok(Self { id, title, fields })
    }

    pub fn id(&self) -> &KindId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn fields(&self) -> &[PromptField] {
        &self.fields
    }
}

/// A prompt as it is written. Its fields are under `field`, so a manifest
/// holds one `[[prompt.field]]` table per field.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptWire {
    id: KindId,
    title: String,
    field: NonEmpty<PromptField>,
}

impl TryFrom<PromptWire> for PromptSpec {
    type Error = DuplicatePromptKey;

    fn try_from(wire: PromptWire) -> Result<Self, Self::Error> {
        let PromptWire { id, title, field } = wire;
        Self::new(id, title, field)
    }
}

impl From<PromptSpec> for PromptWire {
    fn from(spec: PromptSpec) -> Self {
        let PromptSpec { id, title, fields } = spec;
        Self {
            id,
            title,
            field: fields,
        }
    }
}

/// One line of typed input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptField {
    pub key: FieldKey,
    pub label: String,
    /// A pattern that the whole answer must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<Pattern>,
    /// The most characters that the answer may hold.
    pub max_len: NonZeroU32,
    /// The agent masks the answer on screen, and no log holds it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Two fields of one prompt have this key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePromptKey(pub FieldKey);

impl fmt::Display for DuplicatePromptKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "two fields of the prompt have the key `{}`", self.0)
    }
}

impl core::error::Error for DuplicatePromptKey {}
