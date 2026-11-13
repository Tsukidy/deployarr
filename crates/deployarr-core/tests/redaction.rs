//! Redaction of secret values.

use std::borrow::Cow;

use deployarr_core::store::{MASK, Redactor, Sensitivity, Source, Vars};
use deployarr_core::variable::VarName;

use Sensitivity::{Plain, Secret};

fn name(text: &str) -> VarName {
    text.parse().expect("a valid variable name")
}

fn redact(secrets: &[&str], text: &str) -> String {
    Redactor::new(secrets.iter().copied())
        .redact(text)
        .into_owned()
}

#[test]
fn a_secret_never_appears_in_redacted_output() {
    let mut vars = Vars::new();
    vars.set(name("AdminPassword"), "Hunter2!", Secret, Source::Prompt)
        .unwrap();
    vars.set(
        name("ProductKeys"),
        vec!["AAAAA-11111".to_owned(), "BBBBB-22222".to_owned()],
        Secret,
        Source::DataSource,
    )
    .unwrap();
    vars.set_by_agent(name("_JoinToken"), "tok-9f8e", Secret);
    vars.set(name("Make"), "Contoso", Plain, Source::Fact)
        .unwrap();
    let text = "Contoso: user=admin pw=Hunter2! keys=AAAAA-11111,BBBBB-22222 \
                token=tok-9f8e again Hunter2!Hunter2! AAAAA-11111";

    let redacted = vars.redactor().redact(text);

    for secret in ["Hunter2!", "AAAAA-11111", "BBBBB-22222", "tok-9f8e"] {
        assert!(!redacted.contains(secret), "{secret} in {redacted}");
    }
    assert_eq!(
        redacted,
        "Contoso: user=admin pw=******** keys=********,******** \
         token=******** again ******** ********"
    );
}

#[test]
fn every_occurrence_is_masked() {
    assert_eq!(
        redact(&["pw"], "pw, pw and pw"),
        "********, ******** and ********"
    );
}

#[test]
fn overlapping_secrets_leave_no_fragment() {
    // A shorter secret that starts first would leave `def` behind if it were
    // masked on its own.
    assert_eq!(redact(&["abc", "bcdef"], "xabcdefy"), "x********y");
    // A secret inside a longer one.
    assert_eq!(
        redact(&["pass", "password1"], "password1 pass passport"),
        "******** ******** ********port"
    );
    // Occurrences of one secret that overlap each other.
    assert_eq!(redact(&["aa"], "aaa"), "********");
    assert_eq!(redact(&["abab"], "xababab"), "x********");
    // Secrets that touch hide where one ends.
    assert_eq!(redact(&["pw1", "pw2"], "pw1pw2"), "********");
}

#[test]
fn an_empty_secret_masks_nothing() {
    let redactor = Redactor::new([""]);
    assert!(matches!(redactor.redact("abc"), Cow::Borrowed("abc")));
    assert!(matches!(redactor.redact(""), Cow::Borrowed("")));

    let mut vars = Vars::new();
    vars.set(name("Blank"), "", Secret, Source::Prompt).unwrap();
    vars.set(
        name("Keys"),
        vec![String::new(), "k3y".to_owned()],
        Secret,
        Source::Rule,
    )
    .unwrap();
    assert_eq!(vars.redactor().redact("a k3y b"), "a ******** b");
}

#[test]
fn text_without_secrets_comes_back_borrowed() {
    let mut vars = Vars::new();
    vars.set(name("AdminPassword"), "Hunter2!", Secret, Source::Prompt)
        .unwrap();
    vars.set(name("Make"), "Contoso", Plain, Source::Fact)
        .unwrap();

    let redacted = vars.redactor().redact("Make is Contoso");

    assert!(matches!(redacted, Cow::Borrowed("Make is Contoso")));
}

#[test]
fn plain_values_are_not_masked() {
    let mut vars = Vars::new();
    vars.set(name("Make"), "Contoso", Plain, Source::Fact)
        .unwrap();
    vars.set(name("Apps"), vec!["7-Zip".to_owned()], Plain, Source::Rule)
        .unwrap();

    assert_eq!(vars.redactor().redact("Contoso 7-Zip"), "Contoso 7-Zip");
}

#[test]
fn secrets_with_wide_characters() {
    assert_eq!(redact(&["pä"], "xpäy"), "x********y");
    assert_eq!(redact(&["ää"], "äää ä"), "******** ä");
    assert_eq!(redact(&["日本"], "日本語"), "********語");
}

#[test]
fn a_secret_that_contains_the_mask_character() {
    // No character the secret covers survives. What remains is the mask.
    assert_eq!(redact(&["a*b"], "a*b and *"), "******** and *");
    assert_eq!(redact(&[MASK], MASK), MASK);
}

#[test]
fn debug_output_shows_no_secret() {
    let redactor = Redactor::new(["Hunter2!", "tok-9f8e"]);

    let printed = format!("{redactor:?}");

    assert_eq!(printed, "Redactor { secrets: 2 }");
}

/// Masks each maximal run of bytes that an occurrence of a secret covers,
/// found by trying every start. A slow model of `Redactor::redact`.
fn model(secrets: &[&str], text: &str) -> String {
    let mut covered = vec![false; text.len()];
    for start in 0..text.len() {
        for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
            if text.is_char_boundary(start) && text[start..].starts_with(secret) {
                covered[start..start + secret.len()].fill(true);
            }
        }
    }
    let mut out = String::new();
    let mut index = 0;
    while index < text.len() {
        if covered[index] {
            while index < text.len() && covered[index] {
                index += 1;
            }
            out.push_str(MASK);
        } else {
            let c = text[index..].chars().next().unwrap();
            out.push(c);
            index += c.len_utf8();
        }
    }
    out
}

/// Every text of up to six characters from `alphabet`.
fn texts(alphabet: &[char]) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut last = vec![String::new()];
    for _ in 0..6 {
        last = last
            .iter()
            .flat_map(|text| {
                alphabet.iter().map(move |c| {
                    let mut longer = text.clone();
                    longer.push(*c);
                    longer
                })
            })
            .collect();
        all.extend(last.iter().cloned());
    }
    all
}

#[test]
fn every_short_text_matches_the_model_and_shows_no_secret() {
    let secret_sets: &[&[&str]] = &[
        &["a"],
        &["ab"],
        &["ab", "ba"],
        &["aba", "b"],
        &["aa", "aaa"],
        &["abaab", "ba", "bb"],
        &["a*", "*b"],
        &["*"],
        &["", "ab"],
    ];
    let all = texts(&['a', 'b', '*']);
    assert_eq!(all.len(), 1093);

    for secrets in secret_sets {
        let redactor = Redactor::new(secrets.iter().copied());
        for text in &all {
            let redacted = redactor.redact(text);
            assert_eq!(redacted, model(secrets, text), "{secrets:?} on {text:?}");
            for secret in secrets.iter().filter(|s| !s.is_empty() && !s.contains('*')) {
                assert!(
                    !redacted.contains(secret),
                    "{secret:?} shows in {redacted:?}, from {text:?}"
                );
            }
        }
    }
}
