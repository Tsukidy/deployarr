//! The lookup chain's resolvers, the identity facts that a resolver can match
//! on, and the reasons the server may erase a PC's disk.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::addon::{AddOnRef, KindId};
use crate::non_empty::NonEmpty;

/// One entry of the lookup chain. The server tries the entries in order and
/// stops at the first one that returns a task sequence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolver {
    /// The machines table in deployarr's own database.
    PreStaged,
    /// A resolver that an add-on declares, such as SQL order lookup by
    /// identity. Its settings say which database, query, and identity fact
    /// it uses.
    AddOn(AddOnRef),
    /// The technician picks a task sequence.
    ManualList,
}

/// The identity facts that a resolver can match on. The WIT enum
/// `identity-kind` repeats this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    /// SMBIOS type 1.
    SystemSerial,
    /// SMBIOS type 2.
    BaseboardSerial,
    /// SMBIOS type 3.
    ChassisSerial,
    /// The SMBIOS system UUID.
    Uuid,
    /// A network adapter's MAC address.
    Mac,
}

/// The kind's name in JSON, such as `baseboard_serial`.
impl fmt::Display for IdentityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SystemSerial => "system_serial",
            Self::BaseboardSerial => "baseboard_serial",
            Self::ChassisSerial => "chassis_serial",
            Self::Uuid => "uuid",
            Self::Mac => "mac",
        })
    }
}

/// One identity fact that the agent gathered from the PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFact {
    pub kind: IdentityKind,
    pub value: String,
}

/// Serial numbers that firmware ships unset, compared after trimming spaces
/// and ignoring ASCII case. A serial of only `0` digits is one too.
const SERIAL_PLACEHOLDERS: &[&str] = &[
    "To Be Filled By O.E.M.",
    "Default string",
    "System Serial Number",
    "Base Board Serial Number",
    "Chassis Serial Number",
    "Not Specified",
    "Not Applicable",
    "None",
    "N/A",
    "Unknown",
    "0123456789",
];

impl IdentityFact {
    /// Whether the value is blank or a known placeholder, so that two PCs
    /// could share it.
    ///
    /// A serial matches [`SERIAL_PLACEHOLDERS`] or holds only `0` digits. A
    /// UUID or a MAC address holds only `0` digits or only `F` digits, with
    /// its separators ignored.
    pub fn is_placeholder(&self) -> bool {
        let value = self.value.trim();
        if value.is_empty() {
            return true;
        }
        match self.kind {
            IdentityKind::SystemSerial
            | IdentityKind::BaseboardSerial
            | IdentityKind::ChassisSerial => {
                SERIAL_PLACEHOLDERS
                    .iter()
                    .any(|placeholder| value.eq_ignore_ascii_case(placeholder))
                    || value.bytes().all(|byte| byte == b'0')
            }
            IdentityKind::Uuid | IdentityKind::Mac => {
                let mut digits = value
                    .chars()
                    .filter(|c| !matches!(c, '-' | ':' | '.' | '{' | '}'));
                match digits.next() {
                    None => true,
                    Some(first @ ('0' | 'f' | 'F')) => {
                        digits.all(|digit| digit.eq_ignore_ascii_case(&first))
                    }
                    Some(_) => false,
                }
            }
        }
    }
}

/// The identity facts that the server may send to an identity resolver.
///
/// Its only constructor drops blank values and known placeholders, so no fact
/// in it is one, and it holds at least one fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsableIdentity(NonEmpty<IdentityFact>);

impl UsableIdentity {
    /// The facts that are not placeholders, or `None` when none are left.
    /// See [`IdentityFact::is_placeholder`].
    pub fn from_gathered(facts: &[IdentityFact]) -> Option<Self> {
        let usable: Vec<IdentityFact> = facts
            .iter()
            .filter(|fact| !fact.is_placeholder())
            .cloned()
            .collect();
        NonEmpty::try_from(usable).ok().map(Self)
    }

    pub fn facts(&self) -> &[IdentityFact] {
        &self.0
    }

    pub fn of_kind(&self, kind: IdentityKind) -> impl Iterator<Item = &IdentityFact> {
        self.0.iter().filter(move |fact| fact.kind == kind)
    }
}

/// What a technician typed at an add-on resolver's prompt, by the prompt's
/// field keys.
///
/// Its only constructor requires an answer that is not blank, so it always
/// holds a typed answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAnswers {
    prompt: KindId,
    answers: BTreeMap<String, String>,
}

impl PromptAnswers {
    /// `None` when every answer is blank, because the technician then typed
    /// nothing.
    pub fn new(prompt: KindId, answers: BTreeMap<String, String>) -> Option<Self> {
        if answers.values().all(|answer| answer.trim().is_empty()) {
            None
        } else {
            Some(Self { prompt, answers })
        }
    }

    /// The prompt's ID, which its add-on declares.
    pub fn prompt(&self) -> &KindId {
        &self.prompt
    }

    pub fn answers(&self) -> &BTreeMap<String, String> {
        &self.answers
    }
}

/// What an add-on resolver declares that it reads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolverInput {
    /// The PC's usable identity facts.
    Identity,
    /// A technician's answers to the add-on's prompt with this ID.
    Prompt(KindId),
}

/// What the server sent to an add-on resolver. A match is checked against
/// this value, never against text that the add-on returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupInput {
    Identity(UsableIdentity),
    Prompt(PromptAnswers),
}

/// Why the server may erase a PC's disk.
///
/// An add-on resolver returns a match and never authority.
/// [`WipeAuthority::from_addon_match`] builds the authority from the input
/// that the server sent. The variants' types have private fields, so no other
/// code can build them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WipeAuthority {
    /// An identity resolver matched on facts that the server sent.
    IdentityMatch(IdentityMatch),
    /// A technician typed an answer to a resolver's prompt.
    TechnicianEntry(TechnicianEntry),
}

/// An identity resolver's match. See [`WipeAuthority::IdentityMatch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMatch {
    by: AddOnRef,
    facts: NonEmpty<IdentityFact>,
}

impl IdentityMatch {
    /// The resolver that matched.
    pub fn by(&self) -> &AddOnRef {
        &self.by
    }

    /// The facts of the matched kind that the server sent. The trace names
    /// these, never text from the add-on.
    pub fn facts(&self) -> &[IdentityFact] {
        &self.facts
    }
}

/// A match on a technician's typed answer. See
/// [`WipeAuthority::TechnicianEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicianEntry {
    by: AddOnRef,
    prompt: KindId,
}

impl TechnicianEntry {
    /// The resolver that matched.
    pub fn by(&self) -> &AddOnRef {
        &self.by
    }

    /// The prompt that the technician answered.
    pub fn prompt(&self) -> &KindId {
        &self.prompt
    }
}

impl WipeAuthority {
    /// The only way an add-on resolver's match becomes authority. Pure.
    ///
    /// - A resolver that declares [`ResolverInput::Identity`] must name the
    ///   kind it matched on in `matched_on`, and `sent` must hold a fact of
    ///   that kind.
    /// - A resolver that declares [`ResolverInput::Prompt`] must leave
    ///   `matched_on` as `None`, and `sent` must hold the answers to that
    ///   prompt.
    ///
    /// Any other combination is an error, and the chain moves on.
    pub fn from_addon_match(
        by: &AddOnRef,
        declared: &ResolverInput,
        sent: &LookupInput,
        matched_on: Option<IdentityKind>,
    ) -> Result<Self, AuthorityError> {
        match (declared, sent) {
            (ResolverInput::Identity, LookupInput::Identity(identity)) => {
                let kind = matched_on.ok_or(AuthorityError::NoKindNamed)?;
                let facts: Vec<IdentityFact> = identity.of_kind(kind).cloned().collect();
                let facts =
                    NonEmpty::try_from(facts).map_err(|_| AuthorityError::KindNotSent(kind))?;
                Ok(Self::IdentityMatch(IdentityMatch {
                    by: by.clone(),
                    facts,
                }))
            }
            (ResolverInput::Prompt(prompt), LookupInput::Prompt(answers))
                if answers.prompt() == prompt =>
            {
                if let Some(kind) = matched_on {
                    return Err(AuthorityError::KindNamedForPrompt(kind));
                }
                Ok(Self::TechnicianEntry(TechnicianEntry {
                    by: by.clone(),
                    prompt: prompt.clone(),
                }))
            }
            _ => Err(AuthorityError::InputDoesNotMatchDeclaration),
        }
    }
}

/// Why an add-on resolver's match gives no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    /// An identity resolver's match names no kind.
    NoKindNamed,
    /// The match names a kind of fact that the server did not send.
    KindNotSent(IdentityKind),
    /// A prompt resolver's match names a kind of fact.
    KindNamedForPrompt(IdentityKind),
    /// The server sent input of another kind than the resolver declares, or
    /// the answers to another prompt.
    InputDoesNotMatchDeclaration,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKindNamed => {
                f.write_str("the identity resolver's match does not name the fact it matched on")
            }
            Self::KindNotSent(kind) => write!(
                f,
                "the match names `{kind}`, and the server sent no fact of that kind"
            ),
            Self::KindNamedForPrompt(kind) => write!(
                f,
                "the prompt resolver's match names `{kind}`, and only a typed answer gives it authority"
            ),
            Self::InputDoesNotMatchDeclaration => {
                f.write_str("the server sent input that the resolver does not declare")
            }
        }
    }
}

impl core::error::Error for AuthorityError {}
