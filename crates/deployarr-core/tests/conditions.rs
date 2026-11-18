//! The condition evaluator, run against a machine that a table describes.
//!
//! A test whose name starts with `mdt_` covers a rule that deployarr keeps
//! from MDT. Its comment quotes the bullet of
//! `docs/research/mdt-8450-binaries.md#conditions` that states the rule, or
//! cites the function in MDT 8450's `TsCore.dll` that shows it. A test whose
//! name starts with `difference_` covers a place where deployarr differs from
//! MDT on purpose. The other tests cover rules that MDT has no counterpart for.
//!
//! The reference's bullets that are not evaluator rules have no test here:
//!
//! - When the engine evaluates a condition, and what a false one skips,
//!   belong to the engine.
//! - The expression type and operator names belong to the JSON parser.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use deployarr_core::condition::{Condition, FileVersion, MsiProduct};
use deployarr_core::evaluate::{
    ConditionError, Probe, ProbeError, ProbeKind, RegKey, RegValue, WmiError, evaluate,
};
use deployarr_core::non_empty::NonEmpty;
use deployarr_core::store::{Sensitivity, SetSource, Vars};
use deployarr_core::substitution::SubstituteError;
use deployarr_core::template::Template;
use deployarr_core::variable::VarName;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

// The machine.

const ACCESS_DENIED: i32 = 0x8007_0005_u32.cast_signed();
const INVALID_NAMESPACE: i32 = 0x8004_100E_u32.cast_signed();
const INVALID_QUERY: i32 = 0x8004_1017_u32.cast_signed();

const SEVEN_ZIP: Uuid = Uuid::from_u128(0x23170f69_40c1_2702_2408_000001000000);
const SEVEN_ZIP_UPGRADE: Uuid = Uuid::from_u128(0x8f4b1c2d_7a3e_4d5f_9b6a_1c2d3e4f5a6b);

/// A machine with fixed answers. Paths, keys, and value names ignore case, as
/// on Windows. It records every question it is asked.
#[derive(Default)]
struct Machine {
    env: BTreeMap<String, String>,
    keys: BTreeSet<String>,
    values: BTreeMap<(String, String), RegValue>,
    files: BTreeMap<String, File>,
    folders: BTreeMap<String, OffsetDateTime>,
    msi: Vec<MsiProduct>,
    wmi: BTreeMap<String, Result<bool, WmiError>>,
    /// Every question fails with this code.
    broken: Option<i32>,
    asked: RefCell<Vec<String>>,
}

struct File {
    version: Option<FileVersion>,
    modified: OffsetDateTime,
}

impl Machine {
    fn new() -> Self {
        Self::default()
    }

    fn env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.to_lowercase(), value.into());
        self
    }

    fn key(mut self, key: &str) -> Self {
        self.keys.insert(key.to_lowercase());
        self
    }

    fn value(mut self, key: &str, name: &str, value: RegValue) -> Self {
        self = self.key(key);
        self.values
            .insert((key.to_lowercase(), name.to_lowercase()), value);
        self
    }

    fn file(mut self, path: &str, version: Option<&str>, modified: &str) -> Self {
        let file = File {
            version: version.map(|text| text.parse().expect("a file version")),
            modified: time(modified),
        };
        self.files.insert(path.to_lowercase(), file);
        self
    }

    fn folder(mut self, path: &str, modified: &str) -> Self {
        self.folders.insert(path.to_lowercase(), time(modified));
        self
    }

    fn msi(mut self, product: MsiProduct) -> Self {
        self.msi.push(product);
        self
    }

    fn wmi(mut self, query: &str, answer: Result<bool, WmiError>) -> Self {
        self.wmi.insert(query.into(), answer);
        self
    }

    fn broken(code: i32) -> Self {
        Self {
            broken: Some(code),
            ..Self::default()
        }
    }

    fn ask(&self, question: String) -> Result<(), ProbeError> {
        self.asked.borrow_mut().push(question);
        match self.broken {
            Some(code) => Err(ProbeError::new(code)),
            None => Ok(()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

fn key_text(key: &RegKey) -> String {
    format!("{}\\{}", key.hive().name(), key.path()).to_lowercase()
}

impl Probe for Machine {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(&name.to_lowercase()).cloned()
    }

    fn registry_key_exists(&self, key: &RegKey) -> Result<bool, ProbeError> {
        self.ask(format!("key {}", key_text(key)))?;
        Ok(self.keys.contains(&key_text(key)))
    }

    fn registry_value(&self, key: &RegKey, name: &str) -> Result<Option<RegValue>, ProbeError> {
        assert!(
            !name.is_empty(),
            "the evaluator never asks for an empty name"
        );
        self.ask(format!("value {} {name}", key_text(key)))?;
        Ok(self
            .values
            .get(&(key_text(key), name.to_lowercase()))
            .cloned())
    }

    fn file_exists(&self, path: &str) -> Result<bool, ProbeError> {
        self.ask(format!("file {path}"))?;
        Ok(self.files.contains_key(&path.to_lowercase()))
    }

    fn folder_exists(&self, path: &str) -> Result<bool, ProbeError> {
        self.ask(format!("folder {path}"))?;
        Ok(self.folders.contains_key(&path.to_lowercase()))
    }

    fn file_version(&self, path: &str) -> Result<Option<FileVersion>, ProbeError> {
        self.ask(format!("version {path}"))?;
        let file = self
            .files
            .get(&path.to_lowercase())
            .expect("an existing file");
        Ok(file.version)
    }

    fn modified(&self, path: &str) -> Result<OffsetDateTime, ProbeError> {
        self.ask(format!("modified {path}"))?;
        let path = path.to_lowercase();
        let found = self.files.get(&path).map(|file| file.modified);
        Ok(found
            .or_else(|| self.folders.get(&path).copied())
            .expect("an existing file or folder"))
    }

    fn installed_msi(&self, product: &MsiProduct) -> Result<bool, ProbeError> {
        self.ask(format!("msi {product:?}"))?;
        Ok(self.msi.contains(product))
    }

    fn wmi_has_rows(&self, namespace: &str, query: &str) -> Result<bool, WmiError> {
        self.ask(format!("wmi {namespace} {query}"))
            .map_err(WmiError::Namespace)?;
        if namespace != r"root\cimv2" {
            return Err(WmiError::Namespace(ProbeError::new(INVALID_NAMESPACE)));
        }
        *self.wmi.get(query).expect("a known query")
    }
}

// Helpers.

fn time(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).expect("an RFC 3339 time")
}

fn name(text: &str) -> VarName {
    text.parse().expect("a valid variable name")
}

fn condition(json: Value) -> Condition {
    serde_json::from_value(json.clone()).unwrap_or_else(|error| panic!("{json}: {error}"))
}

/// `Vars` with each name set by a rule to a plain scalar.
fn vars_of(pairs: &[(&str, &str)]) -> Vars {
    let mut vars = Vars::new();
    for &(text, value) in pairs {
        vars.set(name(text), value, Sensitivity::Plain, SetSource::Rule)
            .expect("a name that does not start with `_`");
    }
    vars
}

/// `Vars` with each name set by a rule to a list.
fn lists_of(pairs: &[(&str, &[&str])]) -> Vars {
    let mut vars = Vars::new();
    for &(text, items) in pairs {
        let items: Vec<String> = items.iter().map(|&item| item.to_owned()).collect();
        vars.set(name(text), items, Sensitivity::Plain, SetSource::Rule)
            .expect("a name that does not start with `_`");
    }
    vars
}

fn run(json: Value, vars: &Vars, machine: &Machine) -> Result<bool, ConditionError> {
    evaluate(&condition(json), vars, machine)
}

fn check(json: Value, vars: &Vars, machine: &Machine) -> bool {
    let text = json.to_string();
    run(json, vars, machine).unwrap_or_else(|error| panic!("{text}: {error}"))
}

fn var(name: &str, test: Value) -> Value {
    json!({ "var": { "name": name, "test": test } })
}

fn registry(key: &str, value: Option<&str>, test: Value) -> Value {
    json!({ "registry": { "key": key, "value": value, "test": test } })
}

fn file(path: &str, version: Option<(&str, &str)>, modified: Option<(&str, &str)>) -> Value {
    let comparison =
        |pair: Option<(&str, &str)>| pair.map(|(op, value)| json!({ "op": op, "value": value }));
    json!({ "file": { "path": path, "version": comparison(version), "modified": comparison(modified) } })
}

fn folder(path: &str, modified: Option<(&str, &str)>) -> Value {
    let modified = modified.map(|(op, value)| json!({ "op": op, "value": value }));
    json!({ "folder": { "path": path, "modified": modified } })
}

fn wmi(namespace: &str, query: &str, ignore_failure: bool) -> Value {
    json!({ "wmi": { "namespace": namespace, "query": query, "ignore_failure": ignore_failure } })
}

/// A condition that is true with [`yes`].
fn holds() -> Value {
    var("Yes", json!({ "equals": "1" }))
}

/// A condition that is false with [`yes`].
fn fails() -> Value {
    var("Yes", json!({ "equals": "0" }))
}

/// A condition that fails to evaluate, because `%Missing%` names nothing.
fn errs() -> Value {
    var("Yes", json!({ "equals": "%Missing%" }))
}

fn yes() -> Vars {
    vars_of(&[("Yes", "1")])
}

const HKLM_CONTOSO: &str = r"HKEY_LOCAL_MACHINE\SOFTWARE\Contoso";

// All, Any, and Not.

/// "The `<condition>` element acts as an AND of its children. The operator
/// nodes are `and`, `or`, and `not`" (`TsCore!0x18019c384`).
#[test]
fn mdt_all_any_and_not_follow_and_or_and_not() {
    let cases = [
        (json!({ "all": [holds(), holds()] }), true),
        (json!({ "all": [holds(), fails()] }), false),
        (json!({ "all": [fails(), fails()] }), false),
        (json!({ "any": [fails(), holds()] }), true),
        (json!({ "any": [fails(), fails()] }), false),
        (json!({ "any": [holds(), holds()] }), true),
        (json!({ "not": holds() }), false),
        (json!({ "not": fails() }), true),
        (json!({ "all": [holds()] }), true),
        (json!({ "any": [fails()] }), false),
        (
            json!({ "not": { "all": [holds(), { "any": [fails(), holds()] }] } }),
            false,
        ),
    ];
    for (json, expected) in cases {
        let text = json.to_string();
        assert_eq!(check(json, &yes(), &Machine::new()), expected, "{text}");
    }
}

/// MDT's `and`, `or`, and `not` read their children in order. Each returns at
/// the first child that settles its answer, or at the first child that fails
/// to evaluate (`TsCore!0x1801a0dc0`, `TsCore!0x1801a55e8`,
/// `TsCore!0x1801a53e4`).
#[test]
fn mdt_all_any_and_not_stop_at_the_first_child_that_settles_them() {
    let settled = [
        (json!({ "all": [fails(), errs()] }), false),
        (json!({ "any": [holds(), errs()] }), true),
        (json!({ "not": { "any": [holds(), errs()] } }), false),
    ];
    for (json, expected) in settled {
        let text = json.to_string();
        assert_eq!(check(json, &yes(), &Machine::new()), expected, "{text}");
    }

    let failed = [
        json!({ "all": [holds(), errs(), fails()] }),
        json!({ "all": [errs(), fails()] }),
        json!({ "any": [fails(), errs(), holds()] }),
        json!({ "any": [errs(), holds()] }),
        json!({ "not": errs() }),
    ];
    for json in failed {
        let text = json.to_string();
        let error = run(json, &yes(), &Machine::new()).expect_err(&text);
        assert!(
            matches!(error, ConditionError::Substitute { .. }),
            "{text}: {error:?}"
        );
    }
}

/// The probe is asked nothing for a child that is never reached.
#[test]
fn a_child_that_is_never_reached_asks_the_probe_nothing() {
    let machine = Machine::new();
    let json = json!({ "any": [holds(), file(r"C:\Never", None, None)] });

    assert!(check(json, &yes(), &machine));
    assert_eq!(machine.asked(), Vec::<String>::new());
}

/// "A NOT with several children means 'none of them is true'"
/// (`TsCore!0x1801a53e4`). deployarr's `Not` takes one child, so an importer
/// writes MDT's `not` with several children as `Not(Any(children))`.
#[test]
fn difference_not_takes_one_child_and_not_any_means_none_is_true() {
    let cases = [
        (vec![fails(), fails()], true),
        (vec![fails(), holds()], false),
        (vec![holds(), holds()], false),
    ];
    for (children, expected) in cases {
        let json = json!({ "not": { "any": children } });
        let text = json.to_string();
        assert_eq!(check(json, &yes(), &Machine::new()), expected, "{text}");
    }

    let two_children = json!({ "not": [holds(), fails()] });
    assert!(serde_json::from_value::<Condition>(two_children).is_err());
}

/// "An empty AND is true, and so is an empty OR" (`TsCore!0x1801a0dc0`,
/// `TsCore!0x1801a55e8`). deployarr cannot hold either, so the evaluator never
/// meets one.
#[test]
fn difference_an_empty_all_or_any_cannot_be_built() {
    assert!(NonEmpty::<Condition>::try_from(Vec::new()).is_err());
    for json in [json!({ "all": [] }), json!({ "any": [] })] {
        let text = json.to_string();
        assert!(serde_json::from_value::<Condition>(json).is_err(), "{text}");
    }
}

// Var.

/// "A Variable condition compares as integers when both sides are digits with
/// an optional leading `-`" (`TsCore!0x1801a96c0`).
#[test]
fn mdt_var_compares_as_integers_when_both_sides_are_integers() {
    let cases = [
        ("10", "greater", "9", true),
        ("10", "less", "9", false),
        ("-5", "less", "3", true),
        ("-5", "greater", "-12", true),
        ("007", "equals", "7", true),
        ("-0", "equals", "0", true),
        ("26100", "greater_equal", "26100", true),
        ("26100", "less_equal", "22631", false),
        ("2", "not_equals", "02", false),
        (
            "123456789012345678901234567890",
            "less",
            "123456789012345678901234567891",
            true,
        ),
    ];
    for (value, op, operand, expected) in cases {
        let vars = vars_of(&[("Build", value)]);
        let json = var("Build", json!({ op: operand }));
        assert_eq!(
            check(json, &vars, &Machine::new()),
            expected,
            "{value} {op} {operand}"
        );
    }
}

/// "Otherwise it compares strings without regard to case"
/// (`TsCore!0x1801a96c0`). One side that is not an integer makes both text.
#[test]
fn mdt_var_compares_other_text_without_regard_to_case() {
    let cases = [
        ("TRUE", "equals", "true", true),
        ("Contoso", "not_equals", "CONTOSO", false),
        ("apple", "less", "Banana", true),
        ("Zebra", "greater", "apple", true),
        ("ÄRGER", "equals", "ärger", true),
        // As text, "10" sorts before "9".
        ("10", "less", "9x", true),
        ("1.10", "less", "1.9", true),
        (" 5", "equals", "5", false),
        ("+5", "equals", "5", false),
        ("10.0.0.1", "equals", "10.0.0.1", true),
    ];
    for (value, op, operand, expected) in cases {
        let vars = vars_of(&[("Text", value)]);
        let json = var("Text", json!({ op: operand }));
        assert_eq!(
            check(json, &vars, &Machine::new()),
            expected,
            "{value:?} {op} {operand:?}"
        );
    }
}

/// "Like uses `PathMatchSpecW` wildcards" (`TsCore!0x1801a96c0`). `*`
/// matches any run of characters and `?` matches one, without regard to case.
#[test]
fn mdt_var_like_matches_star_and_question_mark() {
    let cases = [
        ("Latitude 7440", "Latitude*", true),
        ("Latitude 7440", "latitude 74?0", true),
        ("Latitude 7440", "*74*", true),
        ("Latitude 7440", "Latitude", false),
        ("Latitude 7440", "?atitude*", true),
        ("Latitude 7440", "Latitude 74?", false),
        ("OptiPlex", "*", true),
        ("", "*", true),
        ("", "?", false),
        ("a.b", "a?b", true),
        ("abc", "a[b]c", false),
        // Integers are text to `like`.
        ("100", "1*", true),
    ];
    for (value, pattern, expected) in cases {
        let vars = vars_of(&[("Model", value)]);
        let json = var("Model", json!({ "like": pattern }));
        assert_eq!(
            check(json, &vars, &Machine::new()),
            expected,
            "{value:?} like {pattern:?}"
        );
    }
}

/// "Exists means 'not empty', so an unset variable and an empty one look the
/// same" (`TsCore!0x1801a96c0`).
#[test]
fn mdt_var_exists_means_set_and_not_empty() {
    let vars = vars_of(&[("Empty", ""), ("Full", "x"), ("Space", " ")]);

    let cases = [
        ("Unset", false),
        ("Empty", false),
        ("Full", true),
        ("FULL", true),
        ("Space", true),
    ];
    for (variable, set) in cases {
        let exists = check(var(variable, json!("exists")), &vars, &Machine::new());
        let not_exists = check(var(variable, json!("not_exists")), &vars, &Machine::new());
        assert_eq!((exists, not_exists), (set, !set), "{variable}");
    }
}

/// An unset variable reads as empty text, so it compares like an empty one
/// (`TsCore!0x1801a96c0`).
#[test]
fn mdt_var_an_unset_variable_compares_as_empty_text() {
    let vars = vars_of(&[("Empty", "")]);
    for variable in ["Unset", "Empty"] {
        let cases = [
            ("equals", "x", false),
            ("not_equals", "x", true),
            ("less", "a", true),
            ("like", "*", true),
            ("like", "?*", false),
        ];
        for (op, operand, expected) in cases {
            let json = var(variable, json!({ op: operand }));
            assert_eq!(
                check(json, &vars, &Machine::new()),
                expected,
                "{variable} {op} {operand}"
            );
        }
    }
}

/// "Comparing against an empty Value is an error" (`TsCore!0x1801a96c0`).
/// MDT checks after it expands the value, and only for the six comparisons.
/// deployarr's parser rejects an empty value, so only a template that
/// substitutes to empty text reaches this rule.
#[test]
fn mdt_var_comparing_against_empty_text_is_an_error() {
    let vars = vars_of(&[("Empty", ""), ("Name", "")]);
    for op in [
        "equals",
        "not_equals",
        "less",
        "less_equal",
        "greater",
        "greater_equal",
    ] {
        let error = run(
            var("Name", json!({ op: "%Empty%" })),
            &vars,
            &Machine::new(),
        )
        .expect_err(op);
        assert_eq!(
            error,
            ConditionError::EmptyOperand {
                template: Template::new("%Empty%")
            },
            "{op}"
        );
    }

    let like = var("Name", json!({ "like": "%Empty%" }));
    assert!(check(like, &vars, &Machine::new()));
}

/// MDT counts empty text as the integer 0 when the other side is an integer
/// (`TsCore!0x1801de96c`). deployarr needs a digit, so empty text compares as
/// text.
#[test]
fn difference_empty_text_is_not_an_integer() {
    let vars = vars_of(&[("Empty", "")]);

    // MDT finds 0 greater than -1. As text, "" sorts before "-1".
    assert!(!check(
        var("Empty", json!({ "greater": "-1" })),
        &vars,
        &Machine::new()
    ));
    assert!(check(
        var("Unset", json!({ "less": "-1" })),
        &vars,
        &Machine::new()
    ));
    assert!(!check(
        var("Unset", json!({ "equals": "0" })),
        &vars,
        &Machine::new()
    ));
}

/// MDT keeps a list's items in numbered variables, `Name001`, `Name002`, and
/// so on, and blanks the bare `Name` (`ZTIUtility.vbs:1206-1234` in MDT 8456).
/// Its engine therefore never compares a whole list. deployarr keeps a list as
/// one value, so a comparison or `Like` test on it is an error, not false, as
/// a list in a `Template` is (owner decision on #177). A test on list items
/// needs its own operator.
#[test]
fn difference_a_comparison_on_a_list_is_an_error() {
    let vars = lists_of(&[
        ("NoApps", &[]),
        ("Apps", &["7-Zip"]),
        ("Nics", &["10.0.0.5", "10.0.0.6"]),
    ]);
    let tests = [
        json!({ "equals": "7-Zip" }),
        json!({ "not_equals": "7-Zip" }),
        json!({ "less": "z" }),
        json!({ "less_equal": "z" }),
        json!({ "greater": "a" }),
        json!({ "greater_equal": "a" }),
        json!({ "like": "*" }),
    ];
    for variable in ["NoApps", "Apps", "Nics"] {
        for test in &tests {
            let json = var(variable, test.clone());
            let text = json.to_string();
            let error = run(json, &vars, &Machine::new()).expect_err(&text);
            assert_eq!(
                error,
                ConditionError::List {
                    name: name(variable)
                },
                "{text}"
            );
        }
    }

    // The error is not false, so `not` cannot turn it into true.
    let negated = json!({ "not": var("Apps", json!({ "equals": "Office" })) });
    assert_eq!(
        run(negated, &vars, &Machine::new()),
        Err(ConditionError::List { name: name("Apps") })
    );
}

/// `Exists` and `NotExists` work on a list. A list with at least one item
/// exists, even when that item is empty text, as `Value::is_empty` says
/// (owner decision on #177).
#[test]
fn exists_on_a_list_means_it_has_an_item() {
    let vars = lists_of(&[
        ("NoApps", &[]),
        ("Apps", &["7-Zip"]),
        ("Nics", &["10.0.0.5", "10.0.0.6"]),
        ("Blank", &[""]),
    ]);
    let cases = [
        ("NoApps", false),
        ("Apps", true),
        ("APPS", true),
        ("Nics", true),
        ("Blank", true),
    ];
    for (variable, set) in cases {
        let exists = check(var(variable, json!("exists")), &vars, &Machine::new());
        let not_exists = check(var(variable, json!("not_exists")), &vars, &Machine::new());
        assert_eq!((exists, not_exists), (set, !set), "{variable}");
    }
}

/// "MakeModel, Location, and Computer conditions use substring tests against
/// the gathered facts. A gateway condition of `10.0.0.1` therefore also
/// matches `10.0.0.10`" (`TsCore!0x1801a4c28`, `TsCore!0x1801a4784`,
/// `TsCore!0x1801a0fc4`). deployarr writes them as `var` conditions on the
/// facts, which compare exactly.
#[test]
fn difference_gathered_facts_compare_exactly_not_by_substring() {
    let vars = vars_of(&[
        ("DefaultGateway", "10.0.0.10"),
        ("Make", "Contoso Ltd."),
        ("Model", "Latitude 7440"),
    ]);
    let cases = [
        var("DefaultGateway", json!({ "equals": "10.0.0.1" })),
        var("Make", json!({ "equals": "Contoso" })),
        var("Model", json!({ "equals": "7440" })),
    ];
    for json in cases {
        let text = json.to_string();
        assert!(!check(json, &vars, &Machine::new()), "{text}");
    }
    let exact = var("DefaultGateway", json!({ "equals": "10.0.0.10" }));
    assert!(check(exact, &vars, &Machine::new()));
}

// Substitution.

/// "Every parameter ... is `%VAR%`-expanded when the condition runs"
/// (`TsCore!0x1801a96c0`). Each `Template` field is substituted when the
/// condition is evaluated, so a later value changes the answer.
#[test]
fn mdt_template_fields_are_substituted_when_the_condition_runs() {
    let machine = Machine::new()
        .file(r"W:\Windows\explorer.exe", None, "2026-01-01T00:00:00Z")
        .folder(r"W:\Deployarr", "2026-01-01T00:00:00Z")
        .value(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Contoso\W11",
            "Stage",
            RegValue::Sz("Done".into()),
        )
        .wmi("SELECT * FROM Win32_Battery", Ok(true))
        .env("SystemDrive", "W:");
    let conditions = [
        var("Stage", json!({ "equals": "%Wanted%" })),
        file(r"%Drive%\Windows\explorer.exe", None, None),
        folder(r"%Drive%\Deployarr", None),
        registry(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Contoso\%Image%",
            Some("Stage"),
            json!({ "equals": "Done" }),
        ),
        wmi(r"root\cimv2", "SELECT * FROM %Class%", false),
    ];
    let before = vars_of(&[
        ("Stage", "Done"),
        ("Wanted", "Pending"),
        ("Drive", "X:"),
        ("Image", "W10"),
        ("Class", "Win32_Fan"),
    ]);
    let after = vars_of(&[
        ("Stage", "Done"),
        ("Wanted", "Done"),
        ("Drive", "W:"),
        ("Image", "W11"),
        ("Class", "Win32_Battery"),
    ]);
    let machine = machine.wmi("SELECT * FROM Win32_Fan", Ok(false));

    for json in conditions {
        let text = json.to_string();
        assert!(!check(json.clone(), &before, &machine), "before: {text}");
        assert!(check(json, &after, &machine), "after: {text}");
    }

    // A name that no variable holds comes from the process environment.
    let from_env = file(r"%SystemDrive%\Windows\explorer.exe", None, None);
    assert!(check(from_env, &Vars::new(), &machine));
}

/// MDT leaves an undefined `%Name%` in the text and carries on
/// (`TsCore!0x18012cc00`). deployarr's substitution fails, so the condition
/// fails to evaluate, as `docs/architecture.md#variables` says.
#[test]
fn difference_an_undefined_name_in_a_template_is_an_error() {
    let machine = Machine::new().file(r"C:\%Missing%\x.txt", None, "2026-01-01T00:00:00Z");
    let error = run(
        file(r"C:\%Missing%\x.txt", None, None),
        &Vars::new(),
        &machine,
    )
    .unwrap_err();

    assert_eq!(
        error,
        ConditionError::Substitute {
            template: Template::new(r"C:\%Missing%\x.txt"),
            error: SubstituteError::Undefined {
                name: "Missing".into()
            },
        }
    );
    assert_eq!(machine.asked(), Vec::<String>::new());
}

/// MDT's expander scans inserted text again (`TsCore!0x18012cc00`). deployarr
/// substitutes once, so a value that holds `%Name%` compares as written.
#[test]
fn difference_substituted_text_is_not_scanned_again() {
    let vars = vars_of(&[("Wanted", "%Other%"), ("Other", "x"), ("Value", "%Other%")]);

    assert!(check(
        var("Value", json!({ "equals": "%Wanted%" })),
        &vars,
        &Machine::new()
    ));
    assert!(!check(
        var("Value", json!({ "equals": "x" })),
        &vars,
        &Machine::new()
    ));
}

/// "Every parameter, including the variable name and the WMI query, is
/// `%VAR%`-expanded" (`TsCore!0x1801a96c0`). deployarr substitutes only its
/// `Template` fields. A variable name, a registry value name, registry data,
/// and a WMI namespace are taken as written.
#[test]
fn difference_only_template_fields_are_substituted() {
    assert!("%Name%".parse::<VarName>().is_err());

    let vars = vars_of(&[("Value", "Stage"), ("Data", "Done"), ("Ns", r"root\cimv2")]);
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "%Value%", RegValue::Sz("%Data%".into()))
        .wmi("SELECT * FROM Win32_Battery", Ok(true));

    let by_name = registry(HKLM_CONTOSO, Some("%Value%"), json!({ "equals": "%Data%" }));
    assert!(check(by_name, &vars, &machine));

    let namespace = wmi("%Ns%", "SELECT * FROM Win32_Battery", false);
    let error = run(namespace, &vars, &machine).unwrap_err();
    assert_eq!(
        error,
        ConditionError::Probe {
            kind: ProbeKind::WmiNamespace,
            error: ProbeError::new(INVALID_NAMESPACE),
        }
    );
    assert!(
        machine
            .asked()
            .contains(&"wmi %Ns% SELECT * FROM Win32_Battery".to_owned())
    );
}

// Files and folders.

/// "A missing file, folder, registry key, or registry value makes the
/// condition false" (`TsCore!0x1801a2494`, `TsCore!0x1801a6bec`). That holds
/// for every comparison, `not_equals` included. Only `not_exists` is true.
#[test]
fn mdt_a_missing_file_folder_key_or_value_is_false() {
    let machine = Machine::new()
        .key(HKLM_CONTOSO)
        .folder(r"C:\Windows\explorer.exe", "2026-01-01T00:00:00Z")
        .file(r"C:\Deployarr", None, "2026-01-01T00:00:00Z");
    let at = "2026-01-01T00:00:00Z";
    let missing_value = |test| registry(HKLM_CONTOSO, Some("Missing"), test);
    let missing_key = |test| registry(r"HKEY_LOCAL_MACHINE\SOFTWARE\Missing", Some("Stage"), test);

    let false_cases = [
        file(r"C:\Missing.txt", None, None),
        file(r"C:\Missing.txt", Some(("not_equals", "1.0.0.0")), None),
        file(r"C:\Missing.txt", None, Some(("not_equals", at))),
        // A folder is not a file, and a file is not a folder.
        file(r"C:\Windows\explorer.exe", None, None),
        folder(r"C:\Deployarr", None),
        folder(r"C:\Missing", Some(("less", at))),
        missing_value(json!("exists")),
        missing_value(json!({ "equals": "1" })),
        missing_value(json!({ "not_equals": "1" })),
        missing_value(json!({ "not_equals": 1 })),
        missing_value(json!({ "like": "*" })),
        missing_key(json!({ "not_equals": 1 })),
        missing_key(json!("exists")),
        registry(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Missing",
            None,
            json!("exists"),
        ),
    ];
    for json in false_cases {
        let text = json.to_string();
        assert!(!check(json, &Vars::new(), &machine), "{text}");
    }

    let true_cases = [
        missing_value(json!("not_exists")),
        missing_key(json!("not_exists")),
        registry(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Missing",
            None,
            json!("not_exists"),
        ),
    ];
    for json in true_cases {
        let text = json.to_string();
        assert!(check(json, &Vars::new(), &machine), "{text}");
    }
}

/// "File versions are compared as four 16-bit fields" (`TsCore!0x1801a2494`,
/// `TsCore!0x1801df958`). Each field compares as a number, not as text.
#[test]
fn mdt_file_versions_compare_as_four_numbers() {
    let machine = Machine::new().file(
        r"C:\Windows\explorer.exe",
        Some("10.0.26100.2454"),
        "2026-01-01T00:00:00Z",
    );
    let cases = [
        ("equals", "10.0.26100.2454", true),
        ("not_equals", "10.0.26100.2454", false),
        ("greater", "10.0.9200.0", true),
        ("greater_equal", "10.0.26100.2455", false),
        ("less", "10.1.0.0", true),
        ("less_equal", "10.0.26100.2454", true),
        ("greater", "9.65535.65535.65535", true),
        ("less", "10.0.26100.10000", true),
    ];
    for (op, version, expected) in cases {
        let json = file(r"C:\Windows\explorer.exe", Some((op, version)), None);
        assert_eq!(
            check(json, &Vars::new(), &machine),
            expected,
            "{op} {version}"
        );
    }
}

/// "Dates are compared with the last-write time to the second"
/// (`TsCore!0x1801a2494`, `TsCore!0x1801df900`). A fraction of a second on
/// either side does not count. Both sides are instants, so a time written
/// with another offset compares by the moment it names.
#[test]
fn mdt_modified_times_compare_to_the_second() {
    let machine = Machine::new()
        .file(r"C:\x.txt", None, "2026-03-01T12:00:00.900Z")
        .folder(r"C:\Logs", "2026-03-01T12:00:00.900Z");
    let cases = [
        ("equals", "2026-03-01T12:00:00Z", true),
        ("equals", "2026-03-01T12:00:00.1Z", true),
        ("greater", "2026-03-01T12:00:00.5Z", false),
        ("less", "2026-03-01T12:00:01Z", true),
        ("greater_equal", "2026-03-01T12:00:01Z", false),
        ("greater", "2026-03-01T11:59:59Z", true),
        ("not_equals", "2026-03-01T12:00:00Z", false),
        ("less_equal", "2026-03-01T12:00:00Z", true),
        ("equals", "2026-03-01T13:00:00+01:00", true),
        ("equals", "2026-03-01T12:00:00+01:00", false),
    ];
    for (op, at, expected) in cases {
        let as_file = file(r"C:\x.txt", None, Some((op, at)));
        assert_eq!(
            check(as_file, &Vars::new(), &machine),
            expected,
            "file {op} {at}"
        );
        let as_folder = folder(r"C:\Logs", Some((op, at)));
        assert_eq!(
            check(as_folder, &Vars::new(), &machine),
            expected,
            "folder {op} {at}"
        );
    }
}

/// MDT checks the version first and then the time, and both must hold
/// (`TsCore!0x1801a2494`). A file or folder with no comparison only has to
/// exist.
#[test]
fn mdt_a_file_must_pass_both_its_version_and_its_time() {
    let machine = Machine::new()
        .file(r"C:\x.exe", Some("2.0.0.0"), "2026-01-01T00:00:00Z")
        .folder(r"C:\Logs", "2026-01-01T00:00:00Z");
    let newer = ("greater", "2025-01-01T00:00:00Z");
    let older = ("less", "2025-01-01T00:00:00Z");
    let cases = [
        (
            file(r"C:\x.exe", Some(("equals", "2.0.0.0")), Some(newer)),
            true,
        ),
        (
            file(r"C:\x.exe", Some(("equals", "2.0.0.0")), Some(older)),
            false,
        ),
        (
            file(r"C:\x.exe", Some(("equals", "1.0.0.0")), Some(newer)),
            false,
        ),
        (file(r"c:\X.EXE", None, None), true),
        (folder(r"C:\Logs", None), true),
    ];
    for (json, expected) in cases {
        let text = json.to_string();
        assert_eq!(check(json, &Vars::new(), &machine), expected, "{text}");
    }

    // A failed version test does not read the time.
    let machine_asked = Machine::new().file(r"C:\x.exe", Some("2.0.0.0"), "2026-01-01T00:00:00Z");
    check(
        file(r"C:\x.exe", Some(("equals", "1.0.0.0")), Some(newer)),
        &Vars::new(),
        &machine_asked,
    );
    assert_eq!(
        machine_asked.asked(),
        [r"file C:\x.exe", r"version C:\x.exe"]
    );
}

/// MDT fails the condition when it cannot read a file's version, because
/// `TsCore!0x1801de644` returns the error from `CcmGetFileVersion` (inferred:
/// `CcmGetFileVersion` is in `CcmCore.dll`, which this test does not cite).
#[test]
fn mdt_a_version_test_on_a_file_without_a_version_is_an_error() {
    let machine = Machine::new().file(r"C:\readme.txt", None, "2026-01-01T00:00:00Z");
    let json = file(r"C:\readme.txt", Some(("greater_equal", "1.0.0.0")), None);

    assert_eq!(
        run(json, &Vars::new(), &machine),
        Err(ConditionError::NoFileVersion {
            path: Template::new(r"C:\readme.txt")
        })
    );
}

// Registry.

/// "Registry conditions accept only full hive names" (`TsCore!0x1801a6bec`).
/// The four that MDT accepts are `HKEY_LOCAL_MACHINE`, `HKEY_CURRENT_USER`,
/// `HKEY_CURRENT_CONFIG`, and `HKEY_USERS`, in any case and followed by `\`
/// (`TsCore!0x1801ab5c0`). Another key fails to evaluate.
#[test]
fn mdt_registry_keys_start_with_a_full_hive_name() {
    let machine = Machine::new()
        .key(r"HKEY_LOCAL_MACHINE\SOFTWARE\Contoso")
        .key(r"HKEY_CURRENT_USER\Software\Contoso")
        .key(r"HKEY_CURRENT_CONFIG\System")
        .key(r"HKEY_USERS\.DEFAULT");
    for key in [
        r"HKEY_LOCAL_MACHINE\SOFTWARE\Contoso",
        r"hkey_local_machine\software\contoso",
        r"HKEY_CURRENT_USER\Software\Contoso",
        r"HKEY_CURRENT_CONFIG\System",
        r"HKEY_USERS\.DEFAULT",
    ] {
        assert!(
            check(registry(key, None, json!("exists")), &Vars::new(), &machine),
            "{key}"
        );
    }

    for key in [
        r"HKLM\SOFTWARE\Contoso",
        r"HKEY_CLASSES_ROOT\.txt",
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\Contoso",
        r"HKEY_LOCAL_MACHINE_X\SOFTWARE",
    ] {
        let error = run(registry(key, None, json!("exists")), &Vars::new(), &machine).unwrap_err();
        assert_eq!(
            error,
            ConditionError::RegistryHive {
                key: Template::new(key)
            },
            "{key}"
        );
    }
}

/// A key with no value name tests whether the key exists
/// (`TsCore!0x1801a6bec`). A `value` that is left out or `null` names none.
#[test]
fn mdt_registry_exists_without_a_value_tests_the_key() {
    let machine = Machine::new().key(HKLM_CONTOSO);
    let left_out = |test: Value| json!({ "registry": { "key": HKLM_CONTOSO, "test": test } });
    let null = |test: Value| registry(HKLM_CONTOSO, None, test);
    for json in [left_out(json!("exists")), null(json!("exists"))] {
        let text = json.to_string();
        assert!(check(json, &Vars::new(), &machine), "{text}");
    }
    for json in [left_out(json!("not_exists")), null(json!("not_exists"))] {
        let text = json.to_string();
        assert!(!check(json, &Vars::new(), &machine), "{text}");
    }
}

/// The message of the error that a condition fails to parse with.
fn parse_error(json: Value) -> String {
    let text = json.to_string();
    match serde_json::from_value::<Condition>(json) {
        Ok(condition) => panic!("{text} parsed as {condition:?}"),
        Err(error) => error.to_string(),
    }
}

/// A comparison must name a value, because a key has no data. MDT fails a
/// comparison without a value name when it runs (`TsCore!0x1801a6bec`).
/// deployarr's parser rejects it, and `like` without one too, so the
/// evaluator never meets one.
#[test]
fn mdt_registry_a_comparison_without_a_value_name_fails_to_parse() {
    for test in [json!({ "equals": 1 }), json!({ "like": "*" })] {
        let left_out = json!({ "registry": { "key": HKLM_CONTOSO, "test": test } });
        let null = registry(HKLM_CONTOSO, None, test);
        for json in [left_out, null] {
            let text = json.to_string();
            let error = parse_error(json);
            assert!(error.contains("needs a value name"), "{text}: {error}");
        }
    }
}

/// MDT reads an empty value name as no value name, so `exists` tests the key
/// and a comparison fails (`TsCore!0x1801a6bec`). deployarr's parser rejects
/// an empty value name for every test, so a test of the key leaves out
/// `value` or sets it to `null` (owner decision on #183).
#[test]
fn difference_registry_an_empty_value_name_fails_to_parse() {
    for test in [
        json!("exists"),
        json!("not_exists"),
        json!({ "equals": 1 }),
        json!({ "like": "*" }),
    ] {
        let json = registry(HKLM_CONTOSO, Some(""), test);
        let text = json.to_string();
        let error = parse_error(json);
        assert!(
            error.contains("value name must not be empty"),
            "{text}: {error}"
        );
    }
}

/// With a value name, `exists` tests the value. MDT finds a value of any type
/// when the condition names no type, and deployarr's names none
/// (`TsCore!0x1801a6bec`).
#[test]
fn mdt_registry_exists_with_a_value_tests_the_value() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Blob", RegValue::Other(3))
        .value(HKLM_CONTOSO, "Stage", RegValue::Sz(String::new()));
    for value in ["Blob", "Stage", "stage"] {
        assert!(
            check(
                registry(HKLM_CONTOSO, Some(value), json!("exists")),
                &Vars::new(),
                &machine
            ),
            "{value}"
        );
    }
}

/// MDT compares a `REG_DWORD` value as an unsigned number
/// (`TsCore!0x1801a6bec`). By the `var` rules, a number value and integer data
/// compare as integers too.
#[test]
fn mdt_registry_compares_dword_values_as_numbers() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Build", RegValue::Dword(26100))
        .value(HKLM_CONTOSO, "Max", RegValue::Dword(u32::MAX));
    let cases = [
        ("Build", "equals", 26100, true),
        ("Build", "greater_equal", 26100, true),
        ("Build", "greater", 9600, true),
        ("Build", "less", 100000, true),
        ("Build", "not_equals", 26100, false),
        ("Max", "equals", u64::from(u32::MAX), true),
        ("Max", "less", u64::from(u32::MAX) + 1, true),
    ];
    for (value, op, data, expected) in cases {
        let json = registry(HKLM_CONTOSO, Some(value), json!({ op: data }));
        assert_eq!(
            check(json, &Vars::new(), &machine),
            expected,
            "{value} {op} {data}"
        );
    }
}

/// MDT compares a `REG_SZ` value as text without regard to case, and a
/// `REG_EXPAND_SZ` value after it expands it (`TsCore!0x1801a6bec`). The `var`
/// rules do the same for text that is not an integer. Here the probe has
/// expanded `%SystemRoot%\System32` already.
#[test]
fn mdt_registry_compares_string_values_without_regard_to_case() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Edition", RegValue::Sz("Professional".into()))
        .value(
            HKLM_CONTOSO,
            "Path",
            RegValue::ExpandSz(r"C:\Windows\System32".into()),
        );
    let cases = [
        ("Edition", json!({ "equals": "PROFESSIONAL" }), true),
        ("Edition", json!({ "not_equals": "Enterprise" }), true),
        ("Edition", json!({ "less": "Server" }), true),
        ("Edition", json!({ "greater": "enterprise" }), true),
        ("Path", json!({ "equals": r"c:\windows\system32" }), true),
        ("Path", json!({ "equals": r"%SystemRoot%\System32" }), false),
    ];
    for (value, test, expected) in cases {
        let text = test.to_string();
        let json = registry(HKLM_CONTOSO, Some(value), test);
        assert_eq!(
            check(json, &Vars::new(), &machine),
            expected,
            "{value} {text}"
        );
    }
}

/// MDT fails a comparison against empty registry data when it runs
/// (`TsCore!0x1801a6bec`). MDT checks only before the six comparisons.
/// deployarr's parser rejects empty data for `like` too (owner decision on
/// #183), so the evaluator never meets it.
#[test]
fn mdt_registry_comparing_against_empty_data_fails_to_parse() {
    for op in [
        "equals",
        "not_equals",
        "less",
        "less_equal",
        "greater",
        "greater_equal",
        "like",
    ] {
        let error = parse_error(registry(HKLM_CONTOSO, Some("Stage"), json!({ op: "" })));
        assert!(error.contains("must not be empty"), "{op}: {error}");
    }
}

/// "Registry conditions accept only ... `REG_DWORD`, `REG_SZ`, and
/// `REG_EXPAND_SZ` values" (`TsCore!0x1801a6bec`). deployarr adds `REG_QWORD`.
/// A comparison with a value of another type, such as `REG_BINARY` or
/// `REG_MULTI_SZ`, fails to evaluate.
#[test]
fn mdt_registry_comparing_another_value_type_is_an_error() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Blob", RegValue::Other(3))
        .value(HKLM_CONTOSO, "Lines", RegValue::Other(7));
    for (value, reg_type) in [("Blob", 3), ("Lines", 7)] {
        for test in [
            json!({ "equals": 1 }),
            json!({ "equals": "1" }),
            json!({ "like": "*" }),
        ] {
            let error = run(
                registry(HKLM_CONTOSO, Some(value), test),
                &Vars::new(),
                &machine,
            )
            .unwrap_err();
            assert_eq!(
                error,
                ConditionError::RegistryType {
                    key: Template::new(HKLM_CONTOSO),
                    value: value.into(),
                    reg_type,
                },
                "{value}"
            );
        }
    }
}

/// `RegData` is "compared by the `Var` rules" (`docs/architecture.md`, Core
/// data shapes). Both sides are text: a `REG_DWORD` or `REG_QWORD` value and
/// integer data are their decimal numbers. They compare as integers when both
/// are integers, and otherwise as text without regard to case. `like` matches
/// the value's text, whatever its type.
#[test]
fn registry_compares_data_and_value_by_the_var_rules() {
    let machine = Machine::new()
        .value(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuildNumber",
            RegValue::Sz("26100".into()),
        )
        .value(HKLM_CONTOSO, "Old", RegValue::Sz("9600".into()))
        .value(HKLM_CONTOSO, "Padded", RegValue::ExpandSz("026100".into()))
        .value(HKLM_CONTOSO, "Edition", RegValue::Sz("Professional".into()))
        .value(HKLM_CONTOSO, "Count", RegValue::Dword(5))
        .value(HKLM_CONTOSO, "Big", RegValue::Qword(26100));
    let build = |test| {
        registry(
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            Some("CurrentBuildNumber"),
            test,
        )
    };
    let contoso = |value, test| registry(HKLM_CONTOSO, Some(value), test);
    let cases = [
        // `REG_SZ` "26100" against `Integer(26100)`.
        (build(json!({ "greater_equal": 26100 })), true),
        (build(json!({ "equals": 26100 })), true),
        (build(json!({ "less": 26100 })), false),
        (build(json!({ "greater": 22631 })), true),
        // `REG_SZ` "9600" against `String("26100")` compares as integers.
        (contoso("Old", json!({ "less": "26100" })), true),
        (contoso("Old", json!({ "greater": "26100" })), false),
        (contoso("Old", json!({ "less": 26100 })), true),
        (contoso("Padded", json!({ "equals": 26100 })), true),
        // A number value against text data.
        (contoso("Count", json!({ "equals": "5" })), true),
        (contoso("Count", json!({ "equals": "005" })), true),
        (contoso("Count", json!({ "less": "10" })), true),
        (contoso("Big", json!({ "equals": "26100" })), true),
        // One side that is not an integer makes both text.
        (contoso("Count", json!({ "equals": "five" })), false),
        (contoso("Count", json!({ "less": "five" })), true),
        (contoso("Edition", json!({ "greater": 5 })), true),
        (contoso("Edition", json!({ "equals": 5 })), false),
        // `like` matches the value's text.
        (contoso("Count", json!({ "like": "5" })), true),
        (contoso("Count", json!({ "like": 5 })), true),
        (contoso("Big", json!({ "like": "261*" })), true),
        (contoso("Big", json!({ "like": "2610?" })), true),
        (contoso("Big", json!({ "like": "9*" })), false),
        (build(json!({ "like": "26???" })), true),
    ];
    for (json, expected) in cases {
        let text = json.to_string();
        assert_eq!(check(json, &Vars::new(), &machine), expected, "{text}");
    }
}

/// MDT picks the comparison by the value's real type (`TsCore!0x1801a6bec`). It
/// compares a `REG_SZ` or `REG_EXPAND_SZ` value as text without regard to
/// case, never as integers, so it finds "9600" greater than "26100" and
/// "026100" unequal to "26100". deployarr applies the `var` rules to both
/// sides, so two integers compare as integers whatever the value's type.
#[test]
fn difference_registry_compares_by_the_var_rules_not_by_the_value_type() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Old", RegValue::Sz("9600".into()))
        .value(HKLM_CONTOSO, "Padded", RegValue::Sz("026100".into()));
    let cases = [
        (
            registry(HKLM_CONTOSO, Some("Old"), json!({ "greater": "26100" })),
            false,
        ),
        (
            registry(HKLM_CONTOSO, Some("Old"), json!({ "less": "26100" })),
            true,
        ),
        (
            registry(HKLM_CONTOSO, Some("Padded"), json!({ "equals": "26100" })),
            true,
        ),
    ];
    for (json, expected) in cases {
        let text = json.to_string();
        assert_eq!(check(json, &Vars::new(), &machine), expected, "{text}");
    }
}

/// MDT's registry condition fails to compare a `REG_QWORD` value
/// (`TsCore!0x1801a6bec`). deployarr compares them, as the owner decided in
/// #62.
#[test]
fn difference_registry_reads_qword_values() {
    let machine = Machine::new().value(HKLM_CONTOSO, "Big", RegValue::Qword(u64::MAX));
    let cases = [
        (json!("exists"), true),
        (json!({ "equals": u64::MAX }), true),
        (json!({ "greater": u64::from(u32::MAX) }), true),
        (json!({ "less": u64::MAX }), false),
    ];
    for (test, expected) in cases {
        let text = test.to_string();
        let json = registry(HKLM_CONTOSO, Some("Big"), test);
        assert_eq!(check(json, &Vars::new(), &machine), expected, "{text}");
    }
}

/// MDT's registry condition has no branch for `Like`, so it is true for every
/// value that exists, whatever the pattern (`TsCore!0x1801a6bec`,
/// `TsCore!0x1801ab054`). deployarr matches the pattern against the value's
/// text, as a `var` condition does.
#[test]
fn difference_registry_like_matches_wildcards() {
    let machine = Machine::new()
        .value(HKLM_CONTOSO, "Edition", RegValue::Sz("Professional".into()))
        .value(HKLM_CONTOSO, "Build", RegValue::Dword(26100));
    let cases = [
        ("Edition", "Pro*", true),
        ("Edition", "pro?essional", true),
        ("Edition", "Enterprise*", false),
        ("Edition", "*", true),
        ("Build", "26*", true),
        ("Build", "19*", false),
    ];
    for (value, pattern, expected) in cases {
        let json = registry(HKLM_CONTOSO, Some(value), json!({ "like": pattern }));
        assert_eq!(
            check(json, &Vars::new(), &machine),
            expected,
            "{value} {pattern}"
        );
    }
}

// Installed MSI products and WMI.

/// "A Software condition checks an MSI ProductCode or UpgradeCode ... It
/// compares no versions" (`TsCore!0x1801a8800`).
#[test]
fn mdt_installed_msi_checks_a_product_code_or_an_upgrade_code() {
    let machine = Machine::new()
        .msi(MsiProduct::ProductCode(SEVEN_ZIP))
        .msi(MsiProduct::UpgradeCode(SEVEN_ZIP_UPGRADE));
    let cases = [
        (json!({ "product_code": SEVEN_ZIP }), true),
        (json!({ "upgrade_code": SEVEN_ZIP_UPGRADE }), true),
        (json!({ "product_code": SEVEN_ZIP_UPGRADE }), false),
        (json!({ "upgrade_code": SEVEN_ZIP }), false),
    ];
    for (product, expected) in cases {
        let text = product.to_string();
        let json = json!({ "installed_msi": product });
        assert_eq!(check(json, &Vars::new(), &machine), expected, "{text}");
    }
}

/// "A WMI condition is true when the query returns at least one row. A failed
/// query is an error unless the expression has `ignoreQueryFailure="true"`"
/// (`TsCore!0x1801aa380`).
#[test]
fn mdt_wmi_is_true_when_the_query_returns_a_row() {
    let machine = Machine::new()
        .wmi("SELECT * FROM Win32_Battery", Ok(true))
        .wmi("SELECT * FROM Win32_Fan", Ok(false))
        .wmi(
            "SELEC oops",
            Err(WmiError::Query(ProbeError::new(INVALID_QUERY))),
        );
    let cases = [
        ("SELECT * FROM Win32_Battery", false, Ok(true)),
        ("SELECT * FROM Win32_Battery", true, Ok(true)),
        ("SELECT * FROM Win32_Fan", false, Ok(false)),
        (
            "SELEC oops",
            false,
            Err(ConditionError::Probe {
                kind: ProbeKind::WmiQuery,
                error: ProbeError::new(INVALID_QUERY),
            }),
        ),
        ("SELEC oops", true, Ok(false)),
    ];
    for (query, ignore_failure, expected) in cases {
        let json = wmi(r"root\cimv2", query, ignore_failure);
        assert_eq!(
            run(json, &Vars::new(), &machine),
            expected,
            "{query} {ignore_failure}"
        );
    }
}

/// MDT fails the condition when it cannot open the namespace, even with
/// `ignoreQueryFailure="true"`. That flag covers only the query and its rows
/// (`TsCore!0x1801aa380`).
#[test]
fn mdt_wmi_a_namespace_that_cannot_open_is_an_error_even_when_ignored() {
    let machine = Machine::new();
    for ignore_failure in [false, true] {
        let json = wmi(
            r"root\nothing",
            "SELECT * FROM Win32_Battery",
            ignore_failure,
        );
        assert_eq!(
            run(json, &Vars::new(), &machine),
            Err(ConditionError::Probe {
                kind: ProbeKind::WmiNamespace,
                error: ProbeError::new(INVALID_NAMESPACE),
            }),
            "{ignore_failure}"
        );
    }
}

// Errors.

/// "A condition that fails to evaluate is not false" (`TsCore!0x1801a96c0`
/// and the other evaluators). A probe that cannot answer makes an error, and
/// `Not` keeps it an error.
#[test]
fn mdt_a_probe_that_cannot_answer_is_an_error_not_false() {
    let machine = Machine::broken(ACCESS_DENIED);
    let cases = [
        (file(r"C:\x.txt", None, None), ProbeKind::File),
        (folder(r"C:\Logs", None), ProbeKind::Folder),
        (
            registry(HKLM_CONTOSO, None, json!("exists")),
            ProbeKind::Registry,
        ),
        (
            registry(HKLM_CONTOSO, Some("Stage"), json!("not_exists")),
            ProbeKind::Registry,
        ),
        (
            registry(HKLM_CONTOSO, Some("Stage"), json!({ "equals": 1 })),
            ProbeKind::Registry,
        ),
        (
            json!({ "installed_msi": { "product_code": SEVEN_ZIP } }),
            ProbeKind::InstalledMsi,
        ),
        (
            wmi(r"root\cimv2", "SELECT * FROM Win32_Battery", true),
            ProbeKind::WmiNamespace,
        ),
    ];
    for (json, kind) in cases {
        for json in [json.clone(), json!({ "not": json })] {
            let text = json.to_string();
            assert_eq!(
                run(json, &Vars::new(), &machine),
                Err(ConditionError::Probe {
                    kind,
                    error: ProbeError::new(ACCESS_DENIED),
                }),
                "{text}"
            );
        }
    }
}

/// MDT's `File::Exists` and `Directory::Exists` return only true or false
/// (`TsCore!0x1801a2494`, `TsCore!0x1801a3970`), so MDT cannot tell a missing
/// file from one it cannot read (inferred: both are in `CcmCore.dll`). A
/// deployarr probe returns an error for a path it cannot read.
#[test]
fn difference_a_file_or_folder_that_cannot_be_read_is_an_error() {
    let machine = Machine::broken(ACCESS_DENIED);
    for (json, kind) in [
        (file(r"C:\Secret\x.txt", None, None), ProbeKind::File),
        (folder(r"C:\Secret", None), ProbeKind::Folder),
    ] {
        let error = run(json, &Vars::new(), &machine).unwrap_err();
        assert_eq!(
            error,
            ConditionError::Probe {
                kind,
                error: ProbeError::new(ACCESS_DENIED)
            }
        );
    }
}

/// No error holds substituted text, so a secret in a variable cannot reach a
/// log line through an error message. Each error names the template as
/// written.
#[test]
fn errors_name_the_template_not_the_substituted_text() {
    const SECRET: &str = "hunter2-secret";
    let mut vars = Vars::new();
    vars.set(
        name("Password"),
        SECRET,
        Sensitivity::Secret,
        SetSource::Prompt,
    )
    .unwrap();
    let machine = Machine::broken(ACCESS_DENIED);
    let cases = [
        file(r"C:\%Password%\x.txt", None, None),
        registry(r"HKLM\%Password%", None, json!("exists")),
        registry(
            r"HKEY_LOCAL_MACHINE\%Password%",
            Some("Stage"),
            json!({ "equals": 1 }),
        ),
        wmi(
            r"root\cimv2",
            "SELECT * FROM X WHERE Name = '%Password%'",
            false,
        ),
        var("Password", json!({ "equals": "%Password%%Missing%" })),
    ];
    for json in cases {
        let text = json.to_string();
        let error = run(json, &vars, &machine).expect_err(&text);
        let message = format!("{error} {error:?}");
        assert!(!message.contains(SECRET), "{text}: {message}");
        if !matches!(error, ConditionError::Probe { .. }) {
            assert!(message.contains("%Password%"), "{text}: {message}");
        }
    }
}
