//! A prompt as a manifest's `[[prompt]]` table declares it, and as the agent
//! gets it.

use deployarr_core::prompt::PromptSpec;
use serde::Deserialize;

/// The part of a manifest that holds prompts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    prompt: Vec<PromptSpec>,
}

const MANIFEST: &str = r#"
[[prompt]]
id = "order-number"
title = "Order number"

[[prompt.field]]
key = "order_no"
label = "Scan or type the order number"
pattern = "[A-Za-z0-9-]{1,40}"
max_len = 40

[[prompt.field]]
key = "pin"
label = "Technician PIN"
max_len = 8
secret = true
"#;

fn manifest(text: &str) -> Result<Manifest, String> {
    toml::from_str::<Manifest>(text).map_err(|error| error.message().to_string())
}

#[test]
fn a_manifest_prompt_parses() {
    let manifest = manifest(MANIFEST).expect("the prompt parses");
    let [prompt] = manifest.prompt.as_slice() else {
        panic!("expected one prompt");
    };
    assert_eq!(prompt.id().as_str(), "order-number");
    assert_eq!(prompt.title(), "Order number");
    let [order_no, pin] = prompt.fields() else {
        panic!("expected two fields");
    };
    assert_eq!(order_no.key.as_str(), "order_no");
    assert!(
        order_no
            .pattern
            .as_ref()
            .expect("a pattern")
            .is_match("ORD-123")
    );
    assert_eq!(order_no.max_len.get(), 40);
    assert!(!order_no.secret);
    assert!(pin.secret);
    assert_eq!(pin.pattern, None);
}

#[test]
fn a_prompt_round_trips_through_json() {
    let manifest = manifest(MANIFEST).unwrap();
    let prompt = &manifest.prompt[0];
    let json = serde_json::to_string(prompt).expect("a prompt serializes");
    assert_eq!(
        json,
        concat!(
            r#"{"id":"order-number","title":"Order number","field":["#,
            r#"{"key":"order_no","label":"Scan or type the order number","pattern":"[A-Za-z0-9-]{1,40}","max_len":40},"#,
            r#"{"key":"pin","label":"Technician PIN","max_len":8,"secret":true}]}"#
        )
    );
    assert_eq!(
        &serde_json::from_str::<PromptSpec>(&json).expect("the JSON parses"),
        prompt
    );
}

#[test]
fn a_prompt_refuses_what_it_does_not_declare() {
    for (change, message) in [
        (
            MANIFEST.replace("max_len = 8\n", "max_len = 8\nkind = \"text\"\n"),
            "unknown field `kind`",
        ),
        (
            MANIFEST.replace(
                "title = \"Order number\"",
                "title = \"Order number\"\nauto_submit = true",
            ),
            "unknown field `auto_submit`",
        ),
        (
            MANIFEST.replace("key = \"pin\"", "key = \"order_no\""),
            "two fields of the prompt have the key `order_no`",
        ),
        (MANIFEST.replace("max_len = 8", "max_len = 0"), "nonzero"),
        (
            MANIFEST.replace("{1,40}", "{1,40"),
            "not a valid regular expression",
        ),
        (
            MANIFEST.replace("id = \"order-number\"", "id = \"Order Number\""),
            "lower-case",
        ),
        (
            MANIFEST.replace("max_len = 40\n", ""),
            "missing field `max_len`",
        ),
    ] {
        let error = manifest(&change).expect_err(message);
        assert!(error.contains(message), "{message}: {error}");
    }
}

#[test]
fn a_prompt_has_at_least_one_field() {
    let error = serde_json::from_str::<PromptSpec>(r#"{"id":"p","title":"P","field":[]}"#)
        .expect_err("a prompt with no fields");
    assert!(error.to_string().contains("at least one item"), "{error}");
}
