//! The form language: how a field's table parses, and how a document checks
//! against a schema.
//!
//! The hostile inputs come from item 10 of
//! `docs/research/addon-design-attacks-wasmtime-50.0.1.md`, where a derived
//! tagged enum accepted `allow_write = true` on a `sql_text` field.

use deployarr_core::form::{Field, FieldKind, FormSchema, Pattern, ValueError, ValueProblem};
use deployarr_core::sequence::TaskSequenceIdError;
use serde_json::{Value, json};

/// One field kind: the table keys it owns, a value it accepts, a value it
/// refuses with the problem it reports, and a key of another kind.
struct Kind {
    name: &'static str,
    own: &'static str,
    valid: Value,
    invalid: Value,
    problem: ValueProblem,
    /// The pointer of the problem below the field's own value, such as
    /// `/0/allow_write` for a list item.
    below: &'static str,
    foreign: &'static str,
}

fn kinds() -> Vec<Kind> {
    vec![
        Kind {
            name: "text",
            own: "pattern = \"[A-Z]{2}-[0-9]+\"\nmax_len = 8",
            valid: json!("AB-12"),
            invalid: json!("ab-12"),
            problem: ValueProblem::NoMatch {
                pattern: "[A-Z]{2}-[0-9]+".into(),
            },
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "integer",
            own: "min = 1\nmax = 10",
            valid: json!(10),
            invalid: json!(11),
            problem: ValueProblem::AboveMax { max: 10 },
            below: "",
            foreign: "options = [\"a\"]",
        },
        Kind {
            name: "bool",
            own: "",
            valid: json!(false),
            invalid: json!("true"),
            problem: ValueProblem::WrongType {
                expected: "true or false",
            },
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "choice",
            own: "options = [\"pro\", \"home\"]",
            valid: json!("home"),
            invalid: json!("enterprise"),
            problem: ValueProblem::NotAnOption,
            below: "",
            foreign: "max_len = 3",
        },
        Kind {
            name: "template",
            own: "",
            valid: json!("%OrderNo%-100%%"),
            invalid: json!("ORD-%OrderNo"),
            problem: ValueProblem::Unterminated { offset: 4 },
            below: "",
            foreign: "pattern = \"x\"",
        },
        Kind {
            name: "sql_text",
            own: "",
            valid: json!("SELECT ts FROM orders WHERE order_no = :order_no"),
            invalid: json!(["SELECT 1"]),
            problem: ValueProblem::WrongType { expected: "text" },
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "secret",
            own: "",
            valid: json!({"set": true}),
            invalid: json!("hunter2"),
            problem: ValueProblem::NotASecretMark,
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "secret_file",
            own: "max_bytes = 1024",
            valid: json!({"set": false}),
            invalid: json!({"set": true, "bytes": "UEsDBA=="}),
            problem: ValueProblem::NotASecretMark,
            below: "",
            foreign: "pattern = \"x\"",
        },
        Kind {
            name: "connection_ref",
            own: "connection = \"orders\"",
            valid: json!("orders-main"),
            invalid: json!(7),
            problem: ValueProblem::WrongType { expected: "text" },
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "task_sequence_ref",
            own: "",
            valid: json!("W11-PRO"),
            invalid: json!("w11 pro"),
            problem: ValueProblem::TaskSequenceId(TaskSequenceIdError::InvalidChar { found: 'w' }),
            below: "",
            foreign: "max_bytes = 5",
        },
        Kind {
            name: "list",
            own: "item = [{ key = \"statement\", label = \"Statement\", kind = \"sql_text\" }]",
            valid: json!([{"statement": "SELECT 1"}, {}]),
            invalid: json!([{"statement": "SELECT 1", "allow_write": true}]),
            problem: ValueProblem::UnknownKey,
            below: "/0/allow_write",
            foreign: "max_bytes = 5",
        },
    ]
}

/// A field table with the key `k` and the kind's own keys.
fn table(kind: &Kind) -> String {
    format!(
        "key = \"k\"\nlabel = \"K\"\nkind = \"{}\"\n{}\n",
        kind.name, kind.own
    )
}

fn field_toml(text: &str) -> Result<Field, String> {
    toml::from_str::<Field>(text).map_err(|error| error.message().to_string())
}

fn field_json(text: &str) -> Result<Field, String> {
    serde_json::from_str::<Field>(text).map_err(|error| error.to_string())
}

fn schema(field: Field) -> FormSchema {
    FormSchema::new(vec![field]).expect("one field has a unique key")
}

#[test]
fn the_table_covers_every_field_kind() {
    let names: Vec<&str> = kinds().iter().map(|kind| kind.name).collect();
    assert_eq!(
        names,
        [
            "text",
            "integer",
            "bool",
            "choice",
            "template",
            "sql_text",
            "secret",
            "secret_file",
            "connection_ref",
            "task_sequence_ref",
            "list"
        ]
    );
    for kind in kinds() {
        let field = field_toml(&table(&kind)).expect("a valid field parses");
        assert_eq!(field.kind.name(), kind.name);
        assert_eq!(listed(&field.kind), kind.name);
    }
}

/// An exhaustive match, so a new field kind does not compile until the table
/// above has a row for it.
fn listed(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Text { .. } => "text",
        FieldKind::Integer { .. } => "integer",
        FieldKind::Bool => "bool",
        FieldKind::Choice { .. } => "choice",
        FieldKind::Template => "template",
        FieldKind::SqlText => "sql_text",
        FieldKind::Secret => "secret",
        FieldKind::SecretFile { .. } => "secret_file",
        FieldKind::ConnectionRef { .. } => "connection_ref",
        FieldKind::TaskSequenceRef => "task_sequence_ref",
        FieldKind::List { .. } => "list",
    }
}

#[test]
fn every_kind_accepts_a_valid_value() {
    for kind in kinds() {
        let field = field_toml(&table(&kind))
            .unwrap_or_else(|error| panic!("{}: a valid field parses: {error}", kind.name));
        let document = json!({ "k": kind.valid });
        assert_eq!(
            schema(field).check(&document),
            Ok(()),
            "{}: {document}",
            kind.name
        );
    }
}

#[test]
fn every_kind_refuses_an_invalid_value() {
    for kind in kinds() {
        let field = field_toml(&table(&kind)).expect("a valid field parses");
        let document = json!({ "k": kind.invalid });
        let errors = schema(field)
            .check(&document)
            .expect_err(&format!("{}: {document} is refused", kind.name));
        assert_eq!(
            errors.as_slice(),
            [ValueError {
                path: format!("/k{}", kind.below),
                problem: kind.problem.clone(),
            }],
            "{}",
            kind.name
        );
    }
}

#[test]
fn every_kind_refuses_an_unknown_key() {
    for kind in kinds() {
        let error = field_toml(&format!("{}allow_write = true\n", table(&kind)))
            .expect_err(&format!("{}: an unknown key fails the parse", kind.name));
        assert!(
            error.contains("unknown field `allow_write`"),
            "{}: {error}",
            kind.name
        );
    }
}

#[test]
fn every_kind_refuses_a_key_of_another_kind() {
    for kind in kinds() {
        let error = field_toml(&format!("{}{}\n", table(&kind), kind.foreign)).expect_err(
            &format!("{}: a key of another kind fails the parse", kind.name),
        );
        let foreign = kind.foreign.split(' ').next().unwrap();
        assert!(
            error.contains(&format!("`{foreign}` belongs to another kind")),
            "{}: {error}",
            kind.name
        );
    }
}

#[test]
fn an_unknown_kind_fails_the_parse() {
    let error = field_toml("key = \"c\"\nlabel = \"C\"\nkind = \"color\"\n")
        .expect_err("an unknown kind fails the parse");
    assert!(error.contains("unknown variant `color`"), "{error}");
}

/// The JSON inputs of item 10, which the first design accepted.
#[test]
fn json_fields_refuse_the_keys_of_item_10() {
    for (input, key) in [
        (
            r#"{"key":"q","label":"Statement","kind":"sql_text","allow_write":true}"#,
            "allow_write",
        ),
        (
            r#"{"key":"pw","label":"Password","kind":"secret","reveal_to_addon":true}"#,
            "reveal_to_addon",
        ),
        (
            r#"{"key":"l","label":"L","kind":"list","item":[{"key":"q","label":"Q","kind":"sql_text","allow_write":true}]}"#,
            "allow_write",
        ),
    ] {
        let error = field_json(input).expect_err(input);
        assert!(error.contains(&format!("unknown field `{key}`")), "{error}");
    }
}

#[test]
fn a_kind_without_its_needed_key_fails_the_parse() {
    for (name, needs) in [
        ("choice", "options"),
        ("secret_file", "max_bytes"),
        ("connection_ref", "connection"),
        ("list", "item"),
    ] {
        let error = field_toml(&format!("key = \"k\"\nlabel = \"K\"\nkind = \"{name}\"\n"))
            .expect_err(name);
        assert!(
            error.contains(&format!("the kind needs `{needs}`")),
            "{error}"
        );
    }
}

#[test]
fn a_kind_key_with_an_invalid_value_fails_the_parse() {
    for (name, own, message) in [
        ("text", "pattern = \"(\"", "not a valid regular expression"),
        ("text", "max_len = 0", "nonzero"),
        (
            "integer",
            "min = 5\nmax = 4",
            "`min` 5 is greater than `max` 4",
        ),
        ("choice", "options = []", "at least one option"),
        ("choice", "options = [\"a\", \"a\"]", "holds \"a\" twice"),
        ("choice", "options = [\"\"]", "an empty option"),
        ("secret_file", "max_bytes = 0", "nonzero"),
        ("connection_ref", "connection = \"Orders\"", "lower-case"),
        ("list", "item = []", "at least one field"),
        (
            "list",
            "item = [{ key = \"a\", label = \"A\", kind = \"bool\" }, { key = \"a\", label = \"B\", kind = \"bool\" }]",
            "two fields of `item` have the key `a`",
        ),
    ] {
        let text = format!("key = \"k\"\nlabel = \"K\"\nkind = \"{name}\"\n{own}\n");
        let error = field_toml(&text).expect_err(&text);
        assert!(error.contains(message), "{text}: {error}");
    }
}

#[test]
fn a_field_key_holds_lower_case_letters_digits_and_underscores() {
    for key in ["Order", "order-no", "2nd", "", "a/b"] {
        let text = format!("key = {key:?}\nlabel = \"K\"\nkind = \"bool\"\n");
        assert!(field_toml(&text).is_err(), "{key:?} is refused");
    }
    assert!(field_toml("key = \"order_no_2\"\nlabel = \"K\"\nkind = \"bool\"\n").is_ok());
}

/// A schema file in TOML holds one `[[field]]` table per field.
#[test]
fn a_schema_file_parses_and_round_trips() {
    let text = r#"
[[field]]
key = "connection"
label = "Order database"
kind = "connection_ref"
connection = "orders"
required = true

[[field]]
key = "rules"
label = "Lookup rules"
help = "Tried in order"
kind = "list"
item = [
    { key = "statement", label = "Statement", kind = "sql_text", required = true },
    { key = "timeout", label = "Timeout in seconds", kind = "integer", min = 1, max = 60 },
]
"#;
    let schema: FormSchema = toml::from_str(text).expect("the schema parses");
    assert_eq!(schema.fields().len(), 2);
    let FieldKind::List { item } = &schema.field("rules").expect("rules").kind else {
        panic!("expected a list");
    };
    assert_eq!(item.fields().len(), 2);

    let json = serde_json::to_string(&schema).expect("a schema serializes");
    assert_eq!(
        serde_json::from_str::<FormSchema>(&json).expect("the JSON parses"),
        schema
    );
    let toml_text = toml::to_string(&schema).expect("a schema serializes to TOML");
    assert_eq!(
        toml::from_str::<FormSchema>(&toml_text).expect("the TOML parses"),
        schema
    );
}

#[test]
fn a_form_with_no_fields_is_an_empty_table() {
    let schema: FormSchema = serde_json::from_str("{}").expect("an empty form parses");
    assert!(schema.fields().is_empty());
    assert_eq!(serde_json::to_string(&schema).unwrap(), "{}");
    assert_eq!(schema.check(&json!({})), Ok(()));
}

#[test]
fn a_schema_refuses_two_fields_with_one_key() {
    let error = serde_json::from_str::<FormSchema>(
        r#"{"field":[{"key":"a","label":"A","kind":"bool"},{"key":"a","label":"B","kind":"secret"}]}"#,
    )
    .expect_err("two fields with one key");
    assert!(
        error.to_string().contains("two fields have the key `a`"),
        "{error}"
    );
}

fn field(text: &str) -> Field {
    field_toml(text).expect("the field parses")
}

fn problems(schema: &FormSchema, document: Value) -> Vec<(String, ValueProblem)> {
    match schema.check(&document) {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .as_slice()
            .iter()
            .map(|error| (error.path.clone(), error.problem.clone()))
            .collect(),
    }
}

#[test]
fn a_required_field_must_hold_a_value() {
    let schema = FormSchema::new(vec![
        field("key = \"name\"\nlabel = \"N\"\nkind = \"text\"\nrequired = true"),
        field("key = \"password\"\nlabel = \"P\"\nkind = \"secret\"\nrequired = true"),
        field("key = \"rules\"\nlabel = \"R\"\nkind = \"list\"\nrequired = true\nitem = [{ key = \"a\", label = \"A\", kind = \"bool\" }]"),
        field("key = \"note\"\nlabel = \"O\"\nkind = \"text\""),
    ])
    .unwrap();

    assert_eq!(
        problems(&schema, json!({})),
        [
            ("/name".into(), ValueProblem::Missing),
            ("/password".into(), ValueProblem::Missing),
            ("/rules".into(), ValueProblem::Missing),
        ]
    );
    assert_eq!(
        problems(
            &schema,
            json!({"name": "", "password": {"set": false}, "rules": [], "note": ""})
        ),
        [
            ("/name".into(), ValueProblem::Empty),
            ("/password".into(), ValueProblem::Missing),
            ("/rules".into(), ValueProblem::Empty),
        ]
    );
    assert_eq!(
        problems(
            &schema,
            json!({"name": "x", "password": {"set": true}, "rules": [{"a": true}]})
        ),
        []
    );
}

#[test]
fn a_document_holds_only_the_schemas_keys() {
    let schema = schema(field("key = \"a\"\nlabel = \"A\"\nkind = \"bool\""));
    assert_eq!(
        problems(&schema, json!({"a": true, "z": 1, "allow_write": true})),
        [
            ("/allow_write".into(), ValueProblem::UnknownKey),
            ("/z".into(), ValueProblem::UnknownKey),
        ]
    );
    assert_eq!(
        problems(&schema, json!([true])),
        [(String::new(), ValueProblem::NotAnObject)]
    );
    assert_eq!(
        problems(
            &FormSchema::new(vec![field(
                "key = \"l\"\nlabel = \"L\"\nkind = \"list\"\nitem = [{ key = \"a\", label = \"A\", kind = \"bool\" }]"
            )])
            .unwrap(),
            json!({"l": [{"a": true}, "x"]})
        ),
        [("/l/1".into(), ValueProblem::NotAnObject)]
    );
}

#[test]
fn text_holds_at_most_max_len_characters() {
    let schema = schema(field(
        "key = \"t\"\nlabel = \"T\"\nkind = \"text\"\nmax_len = 3",
    ));
    assert_eq!(problems(&schema, json!({"t": "äöü"})), []);
    assert_eq!(
        problems(&schema, json!({"t": "abcd"})),
        [("/t".into(), ValueProblem::TooLong { len: 4, max_len: 3 })]
    );
}

#[test]
fn integers_are_whole_numbers_in_range() {
    let schema = schema(field(
        "key = \"n\"\nlabel = \"N\"\nkind = \"integer\"\nmin = -5\nmax = 5",
    ));
    assert_eq!(problems(&schema, json!({"n": -5})), []);
    assert_eq!(
        problems(&schema, json!({"n": -6})),
        [("/n".into(), ValueProblem::BelowMin { min: -5 })]
    );
    for value in [json!(1.5), json!("1")] {
        assert!(matches!(
            problems(&schema, json!({ "n": value })).as_slice(),
            [(_, ValueProblem::WrongType { .. })]
        ));
    }
    let big: Value = serde_json::from_str(r#"{"n": 12345678901234567890123}"#).unwrap();
    assert!(matches!(
        problems(&schema, big).as_slice(),
        [(_, ValueProblem::WrongType { .. })]
    ));
}

#[test]
fn a_pattern_matches_the_whole_value() {
    let pattern = Pattern::new("[0-9]+").unwrap();
    assert!(pattern.is_match("123"));
    assert!(!pattern.is_match("12a"));
    assert!(!pattern.is_match("a12"));
    assert!(Pattern::new("^[0-9]+$").unwrap().is_match("123"));
    // Leftmost-first search would stop at `a` and miss the whole match.
    assert!(Pattern::new("a|ab").unwrap().is_match("ab"));
    // A flag in the pattern does not move the anchors.
    assert!(Pattern::new("(?i)ab").unwrap().is_match("AB"));
    assert!(!Pattern::new("(?i)ab").unwrap().is_match("ABC"));
    assert!(Pattern::new(r"\d{2}").unwrap().is_match("42"));
}

#[test]
fn a_field_round_trips_through_json_with_only_its_own_keys() {
    let parsed = field("key = \"t\"\nlabel = \"T\"\nkind = \"text\"\npattern = \"[a-z]+\"");
    let json = serde_json::to_string(&parsed).unwrap();
    assert_eq!(
        json,
        r#"{"key":"t","label":"T","kind":"text","pattern":"[a-z]+"}"#
    );
    assert_eq!(field_json(&json).unwrap(), parsed);
}

#[test]
fn field_errors_name_the_field_and_its_kind() {
    let error = field_toml("key = \"q\"\nlabel = \"Q\"\nkind = \"sql_text\"\nmax_bytes = 5")
        .expect_err("a key of another kind");
    assert_eq!(
        error,
        "field `q` of kind `sql_text`: `max_bytes` belongs to another kind"
    );
}

#[test]
fn a_value_error_never_holds_the_value() {
    let schema = schema(field("key = \"pw\"\nlabel = \"P\"\nkind = \"secret\""));
    let errors = schema
        .check(&json!({"pw": "hunter2"}))
        .expect_err("a secret's text is refused");
    let message = errors.to_string();
    assert!(!message.contains("hunter2"), "{message}");
    assert!(!format!("{errors:?}").contains("hunter2"));
}
