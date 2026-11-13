use alloc::borrow::Cow;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The characters a mask tries first, in this order. A mask uses the first one
/// that no secret contains.
const MASK_CANDIDATES: [char; 4] = ['█', '*', '#', '~'];

/// How many times a mask repeats its character.
const MASK_LEN: usize = 8;

/// Replaces secret values in text with a mask.
///
/// [`Vars::redactor`](super::Vars::redactor) builds one from the store's
/// secret variables. Its `Debug` output shows how many secrets it holds, not
/// what they are.
#[derive(Clone)]
pub struct Redactor {
    /// Without empty strings or duplicates.
    secrets: Vec<String>,
    /// [`MASK_LEN`] copies of a character that no secret contains, or empty
    /// when the secrets hold every character.
    mask: String,
}

impl Redactor {
    /// A redactor for these secret values. It skips an empty value, which
    /// would otherwise match at every position.
    ///
    /// It also picks the mask. The mask repeats the first character of
    /// `█`, `*`, `#`, and `~` that no secret contains. When the secrets hold
    /// all four, it walks every Unicode character from U+E000 up, which
    /// starts with the private use area, and then the ones below U+E000.
    ///
    /// The choice reveals one fact. A mask of `*` means that some secret
    /// contains `█`, and each later candidate means the same of every
    /// candidate before it. It does not say which secret holds the character,
    /// where, or how often. `█` is a block drawing character that a typed
    /// password seldom holds, so most redactors mask with `████████` and
    /// reveal nothing. A fixed mask would be worse, because a secret made of
    /// its character would show in the output.
    pub fn new<I, S>(secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut secrets: Vec<String> = secrets
            .into_iter()
            .map(Into::into)
            .filter(|secret| !secret.is_empty())
            .collect();
        secrets.sort_unstable();
        secrets.dedup();
        let mask = mask_char(&secrets)
            .map(|c| core::iter::repeat_n(c, MASK_LEN).collect())
            .unwrap_or_default();
        Self { secrets, mask }
    }

    /// The text that replaces each run of secret text. It is the same
    /// whatever it hides, so it gives away no secret's length.
    ///
    /// It is empty only when the secrets together hold every Unicode
    /// character, more than 4 MB of text. No mask is safe then, so
    /// [`Redactor::redact`] returns empty text for any text that holds a
    /// secret.
    pub fn mask(&self) -> &str {
        &self.mask
    }

    /// Replaces every stretch of `text` that occurrences of secrets cover
    /// with one [mask](Redactor::mask). Text with no secret in it comes back
    /// borrowed.
    ///
    /// Occurrences can overlap, both of one secret and of different secrets.
    /// Their union is masked, so no fragment of a longer secret survives next
    /// to a shorter one, and no character of `text` that a secret covers
    /// survives at all.
    ///
    /// No secret appears in the output. The output alternates stretches of
    /// `text` that no occurrence covers with masks. Take any occurrence of a
    /// secret in the output. A match in UTF-8 starts and ends on character
    /// boundaries, so if the occurrence overlaps a mask, it contains the
    /// mask's character. No secret contains that character. Otherwise the
    /// occurrence lies inside one uncovered stretch, so it is an occurrence
    /// in `text` too. The scan covers every occurrence in `text`, so no
    /// uncovered stretch holds one. Neither case can happen.
    pub fn redact<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut covered: Vec<(usize, usize)> = Vec::new();
        for secret in &self.secrets {
            // Step one character past each match, so the scan also finds
            // occurrences that overlap it, such as both `aa` in `aaa`.
            let step = secret.chars().next().map_or(1, char::len_utf8);
            let mut from = 0;
            while let Some(found) = text[from..].find(secret.as_str()) {
                let start = from + found;
                covered.push((start, start + secret.len()));
                from = start + step;
            }
        }
        if covered.is_empty() {
            return Cow::Borrowed(text);
        }
        if self.mask.is_empty() {
            // Every character is in some secret, so any mask could complete
            // one. Keep nothing.
            return Cow::Owned(String::new());
        }
        covered.sort_unstable();

        let mut redacted = String::with_capacity(text.len());
        let mut copied_to = 0;
        let mut run = covered[0];
        for &(start, end) in &covered[1..] {
            if start <= run.1 {
                run.1 = run.1.max(end);
            } else {
                redacted.push_str(&text[copied_to..run.0]);
                redacted.push_str(&self.mask);
                copied_to = run.1;
                run = (start, end);
            }
        }
        redacted.push_str(&text[copied_to..run.0]);
        redacted.push_str(&self.mask);
        redacted.push_str(&text[run.1..]);
        Cow::Owned(redacted)
    }
}

impl fmt::Debug for Redactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Redactor")
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

/// The first candidate character that no secret contains, then the first
/// such character from U+E000 up, then below it. `None` only when the
/// secrets hold every character.
fn mask_char(secrets: &[String]) -> Option<char> {
    let used: BTreeSet<char> = secrets.iter().flat_map(|secret| secret.chars()).collect();
    MASK_CANDIDATES
        .into_iter()
        .chain((0xE000..=u32::from(char::MAX)).filter_map(char::from_u32))
        .chain((0..0xE000).filter_map(char::from_u32))
        .find(|c| !used.contains(c))
}
