//! Redaction of secret values.

use std::borrow::Cow;

use deployarr_core::store::{Redactor, Sensitivity, SetSource, Vars};
use deployarr_core::template::Template;
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
    vars.set(name("AdminPassword"), "Hunter2!", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(
        name("ProductKeys"),
        vec!["AAAAA-11111".to_owned(), "BBBBB-22222".to_owned()],
        Secret,
        SetSource::Lookup,
    )
    .unwrap();
    vars.set_by_agent(name("_JoinToken"), "tok-9f8e", Secret);
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();
    let text = "Contoso: user=admin pw=Hunter2! keys=AAAAA-11111,BBBBB-22222 \
                token=tok-9f8e again Hunter2!Hunter2! AAAAA-11111";

    let redacted = vars.redactor().redact(text);

    for secret in ["Hunter2!", "AAAAA-11111", "BBBBB-22222", "tok-9f8e"] {
        assert!(!redacted.contains(secret), "{secret} in {redacted}");
    }
    assert_eq!(
        redacted,
        "Contoso: user=admin pw=████████ keys=████████,████████ \
         token=████████ again ████████ ████████"
    );
}

#[test]
fn every_occurrence_is_masked() {
    assert_eq!(
        redact(&["pw"], "pw, pw and pw"),
        "████████, ████████ and ████████"
    );
}

#[test]
fn overlapping_secrets_leave_no_fragment() {
    // A shorter secret that starts first would leave `def` behind if it were
    // masked on its own.
    assert_eq!(redact(&["abc", "bcdef"], "xabcdefy"), "x████████y");
    // A secret inside a longer one.
    assert_eq!(
        redact(&["pass", "password1"], "password1 pass passport"),
        "████████ ████████ ████████port"
    );
    // Occurrences of one secret that overlap each other.
    assert_eq!(redact(&["aa"], "aaa"), "████████");
    assert_eq!(redact(&["abab"], "xababab"), "x████████");
    // Secrets that touch hide where one ends.
    assert_eq!(redact(&["pw1", "pw2"], "pw1pw2"), "████████");
}

#[test]
fn an_empty_secret_masks_nothing() {
    let redactor = Redactor::new([""]);
    assert!(matches!(redactor.redact("abc"), Cow::Borrowed("abc")));
    assert!(matches!(redactor.redact(""), Cow::Borrowed("")));

    let mut vars = Vars::new();
    vars.set(name("Blank"), "", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(
        name("Keys"),
        vec![String::new(), "k3y".to_owned()],
        Secret,
        SetSource::Rule,
    )
    .unwrap();
    assert_eq!(vars.redactor().redact("a k3y b"), "a ████████ b");
}

#[test]
fn text_without_secrets_comes_back_borrowed() {
    let mut vars = Vars::new();
    vars.set(name("AdminPassword"), "Hunter2!", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();

    let redacted = vars.redactor().redact("Make is Contoso");

    assert!(matches!(redacted, Cow::Borrowed("Make is Contoso")));
}

#[test]
fn plain_values_are_not_masked() {
    let mut vars = Vars::new();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();
    vars.set(
        name("Apps"),
        vec!["7-Zip".to_owned()],
        Plain,
        SetSource::Rule,
    )
    .unwrap();

    assert_eq!(vars.redactor().redact("Contoso 7-Zip"), "Contoso 7-Zip");
}

#[test]
fn a_secret_set_again_masks_its_old_and_new_values() {
    let mut vars = Vars::new();
    vars.set(name("Pw"), "old", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(name("pw"), "new", Plain, SetSource::Rule).unwrap();
    vars.set(
        name("Keys"),
        vec!["k1".to_owned(), "k2".to_owned()],
        Secret,
        SetSource::Lookup,
    )
    .unwrap();
    vars.set(name("Keys"), vec!["k3".to_owned()], Secret, SetSource::Rule)
        .unwrap();

    assert_eq!(
        vars.redactor().redact("old new k1 k2 k3 kept"),
        "████████ ████████ ████████ ████████ ████████ kept"
    );
}

#[test]
fn an_emptied_secret_still_masks_its_old_value() {
    let mut vars = Vars::new();
    vars.set(name("Pw"), "old", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(name("Pw"), "", Secret, SetSource::Rule).unwrap();

    assert_eq!(vars.redactor().redact("pw was old"), "pw was ████████");
}

#[test]
fn a_copy_of_an_old_secret_is_masked_after_the_secret_changes() {
    let mut vars = Vars::new();
    vars.set(name("Pw"), "old", Secret, SetSource::Prompt)
        .unwrap();
    let copy = Template::new("%Pw%").substitute(&vars, |_| None).unwrap();
    vars.set(name("Copy"), copy, Plain, SetSource::Rule)
        .unwrap();
    vars.set(name("Pw"), "new", Secret, SetSource::Prompt)
        .unwrap();

    let line = Template::new("copy=%Copy% pw=%Pw%")
        .substitute(&vars, |_| None)
        .unwrap();

    assert_eq!(line, "copy=old pw=new");
    assert_eq!(vars.redactor().redact(&line), "copy=████████ pw=████████");
}

#[test]
fn debug_output_leaves_out_old_secret_values() {
    let mut vars = Vars::new();
    vars.set(name("Pw"), "Hunter2!", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(name("Pw"), "Rotated#3", Secret, SetSource::Prompt)
        .unwrap();

    let printed = format!("{vars:?} {vars:#?}");

    assert!(!printed.contains("Hunter2!"), "{printed}");
    assert!(!printed.contains("Rotated#3"), "{printed}");
    assert!(printed.contains("count: 1"), "{printed}");
}

#[test]
fn secrets_with_wide_characters() {
    assert_eq!(redact(&["pä"], "xpäy"), "x████████y");
    assert_eq!(redact(&["ää"], "äää ä"), "████████ ä");
    assert_eq!(redact(&["日本"], "日本語"), "████████語");
}

#[test]
fn a_secret_made_of_asterisks_never_appears() {
    let redactor = Redactor::new(["**"]);

    for text in ["**", "***", "a**b", "*a**b*", "**********"] {
        let redacted = redactor.redact(text);
        assert!(!redacted.contains("**"), "{redacted:?}, from {text:?}");
    }
    assert_eq!(redactor.redact("a**b"), "a████████b");
    assert_eq!(redactor.redact("*a**b*"), "*a████████b*");
}

#[test]
fn the_mask_uses_a_character_that_no_secret_contains() {
    let mask_of = |secrets: &[&str]| Redactor::new(secrets.iter().copied()).mask().to_owned();

    assert_eq!(mask_of(&[]), "████████");
    assert_eq!(mask_of(&["Hunter2!", "a*b"]), "████████");
    assert_eq!(mask_of(&["x█"]), "********");
    assert_eq!(mask_of(&["█", "*"]), "########");
    assert_eq!(mask_of(&["█*#"]), "~~~~~~~~");
    assert_eq!(mask_of(&["█*#~"]), "\u{E000}".repeat(8));
    assert_eq!(mask_of(&["█*#~\u{E000}"]), "\u{E001}".repeat(8));

    // A secret that is the usual mask is masked with another one.
    assert_eq!(redact(&["████████"], "a████████b"), "a********b");
    assert_eq!(redact(&["█", "*"], "1█2*3"), "1########2########3");
}

#[test]
fn secrets_that_hold_every_character_leave_no_text() {
    let every_character: String = (0..=u32::from(char::MAX))
        .filter_map(char::from_u32)
        .collect();
    let redactor = Redactor::new([every_character, "pw".to_owned()]);

    assert_eq!(redactor.mask(), "");
    assert!(matches!(
        redactor.redact("no secret"),
        Cow::Borrowed("no secret")
    ));
    assert_eq!(redactor.redact("my pw is here"), "");
}

#[test]
fn debug_output_shows_no_secret() {
    let redactor = Redactor::new(["Hunter2!", "tok-9f8e"]);

    let printed = format!("{redactor:?}");

    assert_eq!(printed, "Redactor { secrets: 2 }");
}

/// Replaces each maximal run of bytes that an occurrence of a secret covers
/// with `mask`, found by trying every start. A slow model of
/// `Redactor::redact`.
fn model(secrets: &[&str], mask: &str, text: &str) -> String {
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
            out.push_str(mask);
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
    // Each set of secrets, with the character its mask repeats.
    let secret_sets: &[(&[&str], char)] = &[
        (&["a"], '█'),
        (&["ab"], '█'),
        (&["ab", "ba"], '█'),
        (&["aba", "b"], '█'),
        (&["aa", "aaa"], '█'),
        (&["abaab", "ba", "bb"], '█'),
        (&["a*", "*b"], '█'),
        (&["*"], '█'),
        (&["**"], '█'),
        (&["", "ab"], '█'),
        (&["█"], '*'),
        (&["██", "a"], '*'),
        (&["a█", "█b"], '*'),
        (&["*█"], '#'),
        (&["█", "*", "ab"], '#'),
    ];
    let all = texts(&['a', 'b', '*', '█']);
    assert_eq!(all.len(), 5461);

    for &(secrets, mask_char) in secret_sets {
        let redactor = Redactor::new(secrets.iter().copied());
        let mask = mask_char.to_string().repeat(8);
        assert_eq!(redactor.mask(), mask, "{secrets:?}");
        for text in &all {
            let redacted = redactor.redact(text);
            assert_eq!(
                redacted,
                model(secrets, &mask, text),
                "{secrets:?} on {text:?}"
            );
            for secret in secrets.iter().filter(|s| !s.is_empty()) {
                assert!(
                    !redacted.contains(secret),
                    "{secret:?} shows in {redacted:?}, from {text:?}"
                );
            }
        }
    }
}
