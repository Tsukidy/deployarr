//! The variable store and `%Name%` substitution.

use std::cell::RefCell;

use deployarr_core::sequence::NodeId;
use deployarr_core::store::{ReadOnlyError, Sensitivity, SetSource, Source, Value, Vars};
use deployarr_core::substitution::SubstituteError;
use deployarr_core::template::Template;
use deployarr_core::variable::VarName;
use uuid::Uuid;

use Sensitivity::{Plain, Secret};

fn name(text: &str) -> VarName {
    text.parse().expect("a valid variable name")
}

fn step() -> SetSource {
    SetSource::Step(NodeId(Uuid::from_u128(0x5e7)))
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn substitute(template: &str, vars: &Vars) -> Result<String, SubstituteError> {
    Template::new(template).substitute(vars, no_env)
}

fn value_of(vars: &Vars, text: &str) -> Option<Value> {
    vars.get(&name(text)).map(|var| var.value().clone())
}

fn source(vars: &Vars, text: &str) -> Source {
    vars.get(&name(text)).expect("a set variable").source()
}

/// `Vars` with each name set by a rule to a plain scalar.
fn vars_of(pairs: &[(&str, &str)]) -> Vars {
    let mut vars = Vars::new();
    for &(text, value) in pairs {
        vars.set(name(text), value, Plain, SetSource::Rule)
            .expect("a name that does not start with `_`");
    }
    vars
}

// The store.

#[test]
fn names_ignore_case() {
    let mut vars = Vars::new();
    vars.set(name("OSDComputerName"), "PC-01", Plain, SetSource::Rule)
        .unwrap();

    assert_eq!(value_of(&vars, "osdcomputername"), Some("PC-01".into()));
    assert_eq!(value_of(&vars, "OSDCOMPUTERNAME"), Some("PC-01".into()));

    vars.set(name("OSDCOMPUTERNAME"), "PC-02", Plain, SetSource::Prompt)
        .unwrap();

    assert_eq!(vars.iter().count(), 1);
    let var = vars.get(&name("OsdComputerName")).unwrap();
    assert_eq!(var.value(), &Value::from("PC-02"));
    assert_eq!(var.name().as_str(), "OSDCOMPUTERNAME");
}

#[test]
fn iterates_in_name_order_without_regard_to_case() {
    let vars = vars_of(&[("b", "2"), ("A", "1"), ("c", "3")]);

    let names: Vec<&str> = vars.iter().map(|var| var.name().as_str()).collect();
    assert_eq!(names, ["A", "b", "c"]);
}

#[test]
fn holds_scalars_and_lists() {
    let mut vars = Vars::new();
    let apps = vec!["7-Zip".to_owned(), "Firefox".to_owned()];
    vars.set(name("Applications"), apps.clone(), Plain, SetSource::Lookup)
        .unwrap();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();

    assert_eq!(value_of(&vars, "Applications"), Some(Value::List(apps)));
    assert_eq!(
        value_of(&vars, "Make"),
        Some(Value::Scalar("Contoso".into()))
    );
}

#[test]
fn records_the_source_of_each_value() {
    let mut vars = Vars::new();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();
    vars.set(name("SkipWizard"), "YES", Plain, SetSource::Rule)
        .unwrap();
    vars.set(name("Role"), "Kiosk", Plain, SetSource::Lookup)
        .unwrap();
    vars.set(name("ComputerName"), "PC-01", Plain, SetSource::Prompt)
        .unwrap();
    vars.set(name("Stage"), "1", Plain, step()).unwrap();
    vars.apply_default(name("TimeZone"), "UTC").unwrap();
    vars.set_by_agent(name("_SMSTSLogPath"), r"X:\Deployarr\Logs", Plain);

    assert_eq!(source(&vars, "Make"), Source::Fact);
    assert_eq!(source(&vars, "SkipWizard"), Source::Rule);
    assert_eq!(source(&vars, "Role"), Source::Lookup);
    assert_eq!(source(&vars, "ComputerName"), Source::Prompt);
    assert_eq!(source(&vars, "Stage"), step().into());
    assert_eq!(source(&vars, "TimeZone"), Source::SequenceDefault);
    assert_eq!(source(&vars, "_SMSTSLogPath"), Source::Agent);

    vars.set(name("make"), "Fabrikam", Plain, step()).unwrap();
    assert_eq!(source(&vars, "Make"), step().into());
}

#[test]
fn only_the_agent_sets_names_that_start_with_an_underscore() {
    let mut vars = Vars::new();
    vars.set_by_agent(name("_SMSTSLogPath"), r"X:\Deployarr\Logs", Plain);

    for source in [
        SetSource::Fact,
        SetSource::Rule,
        SetSource::Lookup,
        SetSource::Prompt,
        step(),
    ] {
        let refused = vars.set(name("_smstslogpath"), r"C:\Temp", Plain, source);
        let error: ReadOnlyError = refused.unwrap_err();
        assert_eq!(error.name().as_str(), "_smstslogpath");
        assert!(error.to_string().contains("`_smstslogpath`"), "{error}");
    }
    let refused = vars.set(name("_NewName"), "1", Plain, step());
    assert!(refused.is_err());
    assert_eq!(vars.get(&name("_NewName")), None);

    let var = vars.get(&name("_SMSTSLogPath")).unwrap();
    assert_eq!(var.value(), &Value::from(r"X:\Deployarr\Logs"));
    assert_eq!(var.source(), Source::Agent);

    // Steps can still read them.
    assert_eq!(
        substitute(r"%_SMSTSLogPath%\setup.log", &vars),
        Ok(r"X:\Deployarr\Logs\setup.log".to_owned())
    );

    vars.set_by_agent(name("_SMSTSLogPath"), r"C:\Logs", Plain);
    assert_eq!(value_of(&vars, "_SMSTSLogPath"), Some(r"C:\Logs".into()));
}

#[test]
fn a_set_value_beats_a_sequence_default() {
    let mut vars = vars_of(&[("OSDComputerName", "PC-01")]);

    assert_eq!(
        vars.apply_default(name("osdcomputername"), "DEFAULT"),
        Ok(false)
    );

    let var = vars.get(&name("OSDComputerName")).unwrap();
    assert_eq!(var.value(), &Value::from("PC-01"));
    assert_eq!(var.source(), Source::Rule);
}

#[test]
fn a_default_fills_a_missing_or_empty_variable() {
    let mut vars = Vars::new();
    vars.set(name("Empty"), "", Plain, SetSource::Lookup)
        .unwrap();
    vars.set(name("NoApps"), Vec::<String>::new(), Plain, SetSource::Rule)
        .unwrap();

    assert_eq!(vars.apply_default(name("Missing"), "m"), Ok(true));
    assert_eq!(vars.apply_default(name("Empty"), "e"), Ok(true));
    assert_eq!(
        vars.apply_default(name("NoApps"), vec!["7-Zip".to_owned()]),
        Ok(true)
    );

    assert_eq!(value_of(&vars, "Missing"), Some("m".into()));
    assert_eq!(value_of(&vars, "Empty"), Some("e".into()));
    assert_eq!(
        value_of(&vars, "NoApps"),
        Some(Value::List(vec!["7-Zip".to_owned()]))
    );
    for text in ["Missing", "Empty", "NoApps"] {
        assert_eq!(source(&vars, text), Source::SequenceDefault);
    }

    // A second default does not replace the first.
    assert_eq!(vars.apply_default(name("Missing"), "again"), Ok(false));
    assert_eq!(value_of(&vars, "Missing"), Some("m".into()));
}

#[test]
fn a_default_cannot_name_an_agent_variable() {
    let mut vars = Vars::new();

    let refused = vars.apply_default(name("_SMSTSLogPath"), r"C:\Temp");

    assert_eq!(refused.unwrap_err().name().as_str(), "_SMSTSLogPath");
    assert_eq!(vars.get(&name("_SMSTSLogPath")), None);
}

#[test]
fn a_secret_stays_secret() {
    let mut vars = Vars::new();
    vars.set(name("AdminPassword"), "Hunter2!", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();

    assert!(vars.get(&name("AdminPassword")).unwrap().is_secret());
    assert!(!vars.get(&name("Make")).unwrap().is_secret());

    // A later plain write, or a default that fills an emptied secret, keeps
    // the flag.
    vars.set(name("adminpassword"), "Rotated#3", Plain, step())
        .unwrap();
    assert!(vars.get(&name("AdminPassword")).unwrap().is_secret());
    vars.set(name("AdminPassword"), "", Plain, step()).unwrap();
    vars.apply_default(name("AdminPassword"), "Default%4")
        .unwrap();
    let var = vars.get(&name("AdminPassword")).unwrap();
    assert!(var.is_secret());
    assert_eq!(var.value(), &Value::from("Default%4"));
}

#[test]
fn debug_output_leaves_out_secret_values() {
    let mut vars = Vars::new();
    vars.set(name("AdminPassword"), "Hunter2!", Secret, SetSource::Prompt)
        .unwrap();
    vars.set(
        name("Keys"),
        vec!["KEY-AAAA".to_owned(), "KEY-BBBB".to_owned()],
        Secret,
        SetSource::Lookup,
    )
    .unwrap();
    vars.set(name("Make"), "Contoso", Plain, SetSource::Fact)
        .unwrap();

    let printed = format!(
        "{vars:?} {:?} {:#?} {:?}",
        vars.get(&name("AdminPassword")).unwrap(),
        vars,
        vars.redactor()
    );

    for secret in ["Hunter2!", "KEY-AAAA", "KEY-BBBB"] {
        assert!(!printed.contains(secret), "{secret} in {printed}");
    }
    assert!(printed.contains("Contoso"));
    assert!(printed.contains("<secret>"));
}

// Substitution.

#[test]
fn substitutes_a_scalar_by_name_in_any_case() {
    let vars = vars_of(&[("OSDComputerName", "PC-01"), ("Domain", "corp")]);

    assert_eq!(
        substitute(r"netdom join %osdcomputername% /domain:%DOMAIN%", &vars),
        Ok(r"netdom join PC-01 /domain:corp".to_owned())
    );
    assert_eq!(substitute("no names", &vars), Ok("no names".to_owned()));
    assert_eq!(substitute("", &vars), Ok(String::new()));
}

#[test]
fn a_double_percent_is_a_literal_percent() {
    let vars = Vars::new();

    assert_eq!(substitute("%%", &vars), Ok("%".to_owned()));
    assert_eq!(substitute("100%%", &vars), Ok("100%".to_owned()));
    assert_eq!(substitute("%%%%", &vars), Ok("%%".to_owned()));
    assert_eq!(substitute("%%A%%", &vars), Ok("%A%".to_owned()));
}

#[test]
fn a_stored_reference_comes_out_as_stored() {
    let vars = vars_of(&[("A", "%%A%%")]);

    assert_eq!(substitute("%A%", &vars), Ok("%%A%%".to_owned()));
}

#[test]
fn inserted_text_is_never_scanned_again() {
    let vars = vars_of(&[
        ("SelfRef", "%SelfRef%"),
        ("Points", "%B%"),
        ("B", "b"),
        ("Undefined", "%Nope%"),
        ("Unterminated", "50%"),
        ("Pair", "%%"),
    ]);

    assert_eq!(substitute("%SelfRef%", &vars), Ok("%SelfRef%".to_owned()));
    assert_eq!(substitute("%Points%", &vars), Ok("%B%".to_owned()));
    assert_eq!(substitute("%Undefined%", &vars), Ok("%Nope%".to_owned()));
    assert_eq!(substitute("%Unterminated%", &vars), Ok("50%".to_owned()));
    assert_eq!(substitute("%Pair%%Pair%", &vars), Ok("%%%%".to_owned()));
}

#[test]
fn a_double_percent_next_to_a_name() {
    let vars = vars_of(&[("A", "a"), ("B", "b")]);

    assert_eq!(substitute("%%%A%%%", &vars), Ok("%a%".to_owned()));
    assert_eq!(substitute("%A%%%", &vars), Ok("a%".to_owned()));
    assert_eq!(substitute("%%%A%", &vars), Ok("%a".to_owned()));
    assert_eq!(substitute("%A%%B%", &vars), Ok("ab".to_owned()));
    assert_eq!(substitute("%A%%%%B%", &vars), Ok("a%b".to_owned()));
}

#[test]
fn reads_left_to_right_so_a_percent_pairs_with_the_next_one() {
    let vars = vars_of(&[("A", "a"), ("B", "b")]);

    // `%A%B%` pairs the first two `%`, so the third has no partner.
    assert_eq!(
        substitute("%A%B%", &vars),
        Err(SubstituteError::Unterminated { offset: 4 })
    );
    // `%%A%B%` reads `%%`, then `A`, then `%B%`.
    assert_eq!(substitute("%%A%B%", &vars), Ok("%Ab".to_owned()));
}

#[test]
fn an_unterminated_percent_is_an_error() {
    let vars = vars_of(&[("A", "a")]);

    let unterminated = |offset| Err(SubstituteError::Unterminated { offset });
    assert_eq!(substitute("100%", &vars), unterminated(3));
    assert_eq!(substitute("%A", &vars), unterminated(0));
    assert_eq!(substitute("%%%", &vars), unterminated(2));
    assert_eq!(substitute("%A% 50% off", &vars), unterminated(6));
    // Offsets count bytes, and `ö` and `ß` take two each.
    assert_eq!(substitute("Größe %", &vars), unterminated(8));

    let message = substitute("100%", &vars).unwrap_err().to_string();
    assert!(message.contains("byte 3"), "{message}");
}

#[test]
fn a_name_that_neither_defines_is_an_error() {
    let vars = vars_of(&[("A", "a")]);
    let undefined = |name: &str| {
        Err(SubstituteError::Undefined {
            name: name.to_owned(),
        })
    };

    assert_eq!(substitute("%A% %Nope%", &vars), undefined("Nope"));
    assert_eq!(substitute("% %", &vars), undefined(" "));
    assert_eq!(
        substitute("50% off and 20% more", &vars),
        undefined(" off and 20")
    );
    assert_eq!(
        substitute("%ProgramFiles(x86)%", &vars),
        undefined("ProgramFiles(x86)")
    );

    let message = substitute("%Nope%", &vars).unwrap_err().to_string();
    assert!(message.contains("`%Nope%`"), "{message}");
}

#[test]
fn the_caller_answers_for_the_process_environment() {
    let vars = vars_of(&[("Temp", "from-the-store"), ("Empty", "")]);
    let asked = RefCell::new(Vec::new());
    let env = |name: &str| {
        asked.borrow_mut().push(name.to_owned());
        match name {
            "TEMP" | "SystemRoot" => Some(format!("env:{name}")),
            "ProgramFiles(x86)" => Some(r"C:\Program Files (x86)".to_owned()),
            "Empty" | "Recursive" => Some("%Temp%".to_owned()),
            _ => None,
        }
    };
    let run = |template: &str| Template::new(template).substitute(&vars, env);

    // The store answers first, so `env` never hears of `Temp` or `Empty`.
    assert_eq!(run("%TEMP%"), Ok("from-the-store".to_owned()));
    assert_eq!(run("[%Empty%]"), Ok("[]".to_owned()));
    assert_eq!(asked.borrow().len(), 0);

    assert_eq!(
        run(r"%SystemRoot%\System32"),
        Ok(r"env:SystemRoot\System32".to_owned())
    );
    assert_eq!(
        run(r"%ProgramFiles(x86)%\App"),
        Ok(r"C:\Program Files (x86)\App".to_owned())
    );
    // An environment value is not scanned again either.
    assert_eq!(run("%Recursive%"), Ok("%Temp%".to_owned()));
    assert_eq!(
        run("%Nope%"),
        Err(SubstituteError::Undefined {
            name: "Nope".to_owned()
        })
    );

    assert_eq!(
        *asked.borrow(),
        ["SystemRoot", "ProgramFiles(x86)", "Recursive", "Nope"]
    );
}

#[test]
fn a_list_cannot_be_substituted() {
    let mut vars = Vars::new();
    vars.set(
        name("Applications"),
        vec!["7-Zip".to_owned()],
        Plain,
        SetSource::Rule,
    )
    .unwrap();
    // The environment does not answer for a name the store holds.
    let env = |_: &str| Some("from-env".to_owned());

    let result = Template::new("install %applications%").substitute(&vars, env);

    assert_eq!(
        result,
        Err(SubstituteError::List {
            name: name("Applications")
        })
    );
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("`%Applications%` names a list"),
        "{message}"
    );
}

#[test]
fn a_substituted_secret_is_redacted_from_the_command_line() {
    let mut vars = Vars::new();
    vars.set(name("JoinPassword"), "s3cr%t!", Secret, SetSource::Rule)
        .unwrap();
    vars.set(name("JoinAccount"), "svc-join", Plain, SetSource::Rule)
        .unwrap();
    let template = Template::new("netdom join /ud:%JoinAccount% /pd:%JoinPassword%");

    let command = template.substitute(&vars, no_env).unwrap();
    let logged = vars.redactor().redact(&command).into_owned();

    assert_eq!(command, "netdom join /ud:svc-join /pd:s3cr%t!");
    assert_eq!(logged, "netdom join /ud:svc-join /pd:████████");
    assert!(!logged.contains("s3cr%t!"));
}
