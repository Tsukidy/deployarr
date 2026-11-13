use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The text that replaces each stretch of secret text. It is the same
/// whatever it hides, so it gives away neither a secret's length nor its
/// characters.
pub const MASK: &str = "********";

/// Replaces secret values in text with a mask.
///
/// [`Vars::redactor`](super::Vars::redactor) builds one from the store's
/// secret variables. Its `Debug` output shows how many secrets it holds, not
/// what they are.
#[derive(Clone)]
pub struct Redactor {
    /// Without empty strings or duplicates.
    secrets: Vec<String>,
}

impl Redactor {
    /// A redactor for these secret values. It skips an empty value, which
    /// would otherwise match at every position.
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
        Self { secrets }
    }

    /// Replaces every stretch of `text` that occurrences of secrets cover
    /// with one [`MASK`].
    ///
    /// Occurrences can overlap, both of one secret and of different secrets.
    /// Their union is masked, so no fragment of a longer secret survives next
    /// to a shorter one, and no character of `text` that a secret covers
    /// survives at all. A secret with no `*` in it never appears in the
    /// output. A secret that contains `*` can match the mask's own text, which
    /// shows nothing of it. Text with no secret in it comes back borrowed.
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
        covered.sort_unstable();

        let mut redacted = String::with_capacity(text.len());
        let mut copied_to = 0;
        let mut run = covered[0];
        for &(start, end) in &covered[1..] {
            if start <= run.1 {
                run.1 = run.1.max(end);
            } else {
                redacted.push_str(&text[copied_to..run.0]);
                redacted.push_str(MASK);
                copied_to = run.1;
                run = (start, end);
            }
        }
        redacted.push_str(&text[copied_to..run.0]);
        redacted.push_str(MASK);
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
