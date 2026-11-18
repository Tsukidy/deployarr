//! The lookup chain's resolvers, the identity facts that a resolver can match
//! on, and the reasons the server may erase a PC's disk.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::addon::{AddOnRef, KindId};
use crate::non_empty::NonEmpty;
use crate::sequence::TaskSequenceId;

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
    /// A serial matches a known firmware placeholder, such as
    /// `To Be Filled By O.E.M.` or `Default string`, compared after trimming
    /// spaces and ignoring ASCII case, or holds only `0` digits. The private
    /// constant `SERIAL_PLACEHOLDERS` lists the known placeholders. A UUID or
    /// a MAC address holds only `0` digits or only `F` digits, with its
    /// separators ignored.
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
/// Its only constructor trims each value and drops blank values and known
/// placeholders, so no fact in it is one, and it holds at least one fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsableIdentity(NonEmpty<IdentityFact>);

impl UsableIdentity {
    /// The facts that are not placeholders, each with the spaces at both ends
    /// of its value trimmed, or `None` when none are left. A resolver then
    /// sees the same value that [`IdentityFact::is_placeholder`] checked.
    pub fn from_gathered(facts: &[IdentityFact]) -> Option<Self> {
        let usable: Vec<IdentityFact> = facts
            .iter()
            .filter(|fact| !fact.is_placeholder())
            .map(|fact| IdentityFact {
                kind: fact.kind,
                value: fact.value.trim().into(),
            })
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

/// What a technician typed at an add-on's prompt, by the prompt's field keys.
///
/// It names the prompt with its add-on, because a prompt ID is unique only
/// within one add-on. Its only constructor requires an answer that is not
/// blank, so it always holds a typed answer.
///
/// `Debug` prints the field keys and never the answers, because a prompt
/// field can be secret.
#[derive(Clone, PartialEq, Eq)]
pub struct PromptAnswers {
    prompt: AddOnRef,
    answers: BTreeMap<String, String>,
}

impl PromptAnswers {
    /// `None` when every answer is blank, because the technician then typed
    /// nothing.
    pub fn new(prompt: AddOnRef, answers: BTreeMap<String, String>) -> Option<Self> {
        if answers.values().all(|answer| answer.trim().is_empty()) {
            None
        } else {
            Some(Self { prompt, answers })
        }
    }

    /// The prompt that the technician answered: the add-on that declares it,
    /// and its ID within that add-on.
    pub fn prompt(&self) -> &AddOnRef {
        &self.prompt
    }

    pub fn answers(&self) -> &BTreeMap<String, String> {
        &self.answers
    }
}

impl fmt::Debug for PromptAnswers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { prompt, answers } = self;
        f.debug_struct("PromptAnswers")
            .field("prompt", prompt)
            .field("keys", &answers.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// What an add-on resolver declares that it reads.
///
/// It is written as `"identity"`, or as `{"prompt": "order-number"}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolverInput {
    /// The PC's usable identity facts.
    Identity,
    /// A technician's answers to the prompt with this ID that the resolver's
    /// own add-on declares.
    Prompt(KindId),
}

/// What the server sent to an add-on resolver. A match is checked against
/// this value, never against text that the add-on returns.
///
/// Its derived `Debug` prints no prompt answer, because
/// [`PromptAnswers`]'s `Debug` prints only the field keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupInput {
    Identity(UsableIdentity),
    Prompt(PromptAnswers),
}

/// Why the server may erase a PC's disk: a pre-staged record's match, an
/// add-on resolver's match, or a technician's pick from the manual list.
///
/// Each path has one constructor, which checks the match or the pick against
/// the facts, the input, or the list that the server used:
/// [`WipeAuthority::from_pre_staged_match`],
/// [`WipeAuthority::from_addon_match`], and
/// [`WipeAuthority::from_manual_pick`].
///
/// An add-on resolver returns a match and never authority.
/// [`WipeAuthority::from_addon_match`] builds the authority from the input
/// that the server sent. The variants' types have private fields, so no other
/// code can build them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WipeAuthority {
    /// A pre-staged record matched the PC's usable identity facts.
    PreStagedMatch(PreStagedMatch),
    /// An identity resolver matched on facts that the server sent.
    IdentityMatch(IdentityMatch),
    /// A technician typed an answer to a resolver's prompt.
    TechnicianEntry(TechnicianEntry),
    /// A technician picked a task sequence that the server offered on the
    /// manual list.
    ManualPick(ManualPick),
}

/// A pre-staged record's match. See [`WipeAuthority::PreStagedMatch`].
///
/// Only [`WipeAuthority::from_pre_staged_match`] builds one, from a
/// [`UsableIdentity`]:
///
/// ```
/// use deployarr_core::lookup::{IdentityFact, IdentityKind, UsableIdentity, WipeAuthority};
///
/// let serial = IdentityFact {
///     kind: IdentityKind::BaseboardSerial,
///     value: "MB-2231-0042".into(),
/// };
/// let identity = UsableIdentity::from_gathered(&[serial]).expect("the serial is usable");
/// let authority = WipeAuthority::from_pre_staged_match(&identity, IdentityKind::BaseboardSerial)?;
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
///
/// The compiler rejects one built from raw facts, which could hold a
/// placeholder:
///
/// ```compile_fail,E0451
/// use deployarr_core::lookup::{IdentityFact, IdentityKind, PreStagedMatch, WipeAuthority};
/// use deployarr_core::non_empty::NonEmpty;
///
/// let serial = IdentityFact {
///     kind: IdentityKind::BaseboardSerial,
///     value: "To Be Filled By O.E.M.".into(),
/// };
/// let authority = WipeAuthority::PreStagedMatch(PreStagedMatch {
///     facts: NonEmpty::new(serial),
/// });
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreStagedMatch {
    facts: NonEmpty<IdentityFact>,
}

impl PreStagedMatch {
    /// The PC's usable facts of the kind that the record matched on.
    pub fn facts(&self) -> &[IdentityFact] {
        &self.facts
    }
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

/// A technician's pick from the manual list. See
/// [`WipeAuthority::ManualPick`].
///
/// Only [`WipeAuthority::from_manual_pick`] builds one, from the list that
/// the server offered:
///
/// ```
/// use deployarr_core::lookup::WipeAuthority;
/// use deployarr_core::sequence::TaskSequenceId;
///
/// let offered: [TaskSequenceId; 2] = ["W11-PRO-STD".parse()?, "W11-HOME-STD".parse()?];
/// let authority = WipeAuthority::from_manual_pick(&offered, &offered[0])?;
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
///
/// The compiler rejects one built from a pick alone:
///
/// ```compile_fail,E0451
/// use deployarr_core::lookup::{ManualPick, WipeAuthority};
/// use deployarr_core::sequence::TaskSequenceId;
///
/// let picked: TaskSequenceId = "W11-PRO-STD".parse()?;
/// let authority = WipeAuthority::ManualPick(ManualPick {
///     task_sequence: picked,
/// });
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualPick {
    task_sequence: TaskSequenceId,
}

impl ManualPick {
    /// The task sequence that the technician picked.
    pub fn task_sequence(&self) -> &TaskSequenceId {
        &self.task_sequence
    }
}

impl WipeAuthority {
    /// The only way a pre-staged record's match becomes authority. Pure.
    ///
    /// `identity` is the PC's usable identity that the server matched the
    /// record against, and `matched_on` is the kind of fact that the record
    /// matched on. The authority holds every fact of that kind in `identity`.
    /// It is an error when `identity` holds no fact of that kind.
    ///
    /// It takes no gathered facts, so a placeholder never gives authority.
    pub fn from_pre_staged_match(
        identity: &UsableIdentity,
        matched_on: IdentityKind,
    ) -> Result<Self, AuthorityError> {
        let facts: Vec<IdentityFact> = identity.of_kind(matched_on).cloned().collect();
        let facts =
            NonEmpty::try_from(facts).map_err(|_| AuthorityError::KindNotUsable(matched_on))?;
        Ok(Self::PreStagedMatch(PreStagedMatch { facts }))
    }

    /// The only way an add-on resolver's match becomes authority. Pure.
    ///
    /// - A resolver that declares [`ResolverInput::Identity`] must name the
    ///   kind it matched on in `matched_on`, and `sent` must hold a fact of
    ///   that kind.
    /// - A resolver that declares [`ResolverInput::Prompt`] must leave
    ///   `matched_on` as `None`, and `sent` must hold the answers to that
    ///   prompt of the resolver's own add-on.
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
            // A prompt ID is unique only within its add-on, so the answers
            // must be to the resolver's own add-on's prompt.
            (ResolverInput::Prompt(prompt), LookupInput::Prompt(answers))
                if answers.prompt().addon == by.addon && answers.prompt().kind == *prompt =>
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

    /// The only way a technician's pick from the manual list becomes
    /// authority. Pure.
    ///
    /// `offered` is the list of task sequences that the server offered, and
    /// `picked` is the one that the technician picked. It is an error when
    /// `offered` does not hold `picked`, which it never does when it is empty.
    pub fn from_manual_pick(
        offered: &[TaskSequenceId],
        picked: &TaskSequenceId,
    ) -> Result<Self, AuthorityError> {
        if offered.contains(picked) {
            Ok(Self::ManualPick(ManualPick {
                task_sequence: picked.clone(),
            }))
        } else {
            Err(AuthorityError::PickNotOffered(picked.clone()))
        }
    }
}

/// Why a resolver's match or a technician's pick gives no authority.
///
/// [`WipeAuthority::from_addon_match`] returns [`Self::NoKindNamed`],
/// [`Self::KindNotSent`], [`Self::KindNamedForPrompt`], and
/// [`Self::InputDoesNotMatchDeclaration`].
/// [`WipeAuthority::from_pre_staged_match`] returns [`Self::KindNotUsable`],
/// and [`WipeAuthority::from_manual_pick`] returns [`Self::PickNotOffered`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityError {
    /// An identity resolver's match names no kind.
    NoKindNamed,
    /// The match names a kind of fact that the server did not send.
    KindNotSent(IdentityKind),
    /// A prompt resolver's match names a kind of fact.
    KindNamedForPrompt(IdentityKind),
    /// The server sent input of another kind than the resolver declares, or
    /// the answers to another prompt, which may be a prompt with the same ID
    /// from another add-on.
    InputDoesNotMatchDeclaration,
    /// A pre-staged record matched on a kind of fact that the PC's usable
    /// identity does not hold.
    KindNotUsable(IdentityKind),
    /// The technician picked a task sequence that the server did not offer on
    /// the manual list.
    PickNotOffered(TaskSequenceId),
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
            Self::KindNotUsable(kind) => write!(
                f,
                "the pre-staged record matched on `{kind}`, and the PC has no usable fact of that kind"
            ),
            Self::PickNotOffered(picked) => write!(
                f,
                "the technician picked `{picked}`, and the manual list that the server offered does not hold it"
            ),
        }
    }
}

impl core::error::Error for AuthorityError {}
