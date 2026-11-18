//! The stored shapes of add-on steps, join slots, and the actions that add-on
//! steps expand to.

use std::collections::BTreeSet;

use deployarr_core::action::{Action, BuiltinAction, FileName, StepFile, StepFiles, StepLimits};
use deployarr_core::addon::{AddOnId, AddOnRef, KindId};
use deployarr_core::join::{JoinProfile, JoinStage};
use deployarr_core::sequence::{AuthoredSequence, LoweredSequence, Node, ParseError, Step};
use deployarr_core::validate::{Issue, Severity, validate};
use serde_json::{Value, json};

/// A config with its keys out of order, an integer that no `u64` or `f64`
/// holds, a float that an `f64` rounds, and a nested object.
const CONFIG: &str = r#"{"zeta":1,"alpha":12345678901234567890123,"ratio":0.1000000000000000055511151231257827,"nested":{"b":[3,2,1],"a":null}}"#;

/// A sequence with a step from an add-on that no catalog knows, a built-in
/// step, and a join slot, in the form that the serializer writes.
fn document_with_unknown_add_on() -> String {
    let meta = |n: u32, name: &str| {
        format!(
            r#"{{"id":"00000000-0000-4000-8000-{n:012x}","name":"{name}","enabled":true,"continue_on_error":false,"condition":null}}"#
        )
    };
    format!(
        concat!(
            r#"{{"schema_version":1,"id":"W11-AUTOPILOT","version":4,"name":"Autopilot","default_vars":[],"#,
            r#""root":{{"meta":{root},"children":["#,
            r#"{{"step":{{"meta":{register},"run_in":"full_os","action":{{"add_on":{{"addon":"acme.autopilot","kind":"register","config_version":1,"config":{CONFIG}}}}}}}}},"#,
            r#"{{"step":{{"meta":{set},"run_in":"either","action":{{"set_variable":{{"name":"GroupTag","value":"lab"}}}}}}}},"#,
            r#"{{"step":{{"meta":{join},"run_in":"full_os","action":{{"apply_join_profile":{{"stage":"full_os"}}}}}}}}"#,
            r#"]}}}}"#
        ),
        root = meta(1, "Root"),
        register = meta(2, "Register"),
        set = meta(3, "Set"),
        join = meta(4, "Join"),
        CONFIG = CONFIG,
    )
}

#[test]
fn an_unknown_add_on_step_round_trips_byte_for_byte() {
    let document = document_with_unknown_add_on();

    let sequence = AuthoredSequence::from_json(&document).expect("the document parses");
    let written = serde_json::to_string(&sequence).expect("a task sequence serializes");

    assert_eq!(written, document);
    let [Node::Step(register), Node::Step(_), Node::Step(join)] = sequence.root.children.as_slice()
    else {
        panic!("expected three steps");
    };
    let Action::AddOn(step) = &register.action else {
        panic!("expected an add-on step, found {:?}", register.action);
    };
    assert_eq!(step.config.get(), CONFIG);
    assert_eq!(step.addon.as_str(), "acme.autopilot");
    assert_eq!(step.kind.as_str(), "register");
    assert_eq!(step.config_version, 1);
    assert_eq!(
        join.action,
        Action::ApplyJoinProfile {
            stage: JoinStage::FullOs
        }
    );
}

#[test]
fn an_add_on_steps_config_keeps_its_spaces() {
    let config = "{ \"b\" : 2,\n  \"a\" : [ 1 , 2 ] }";
    let action = format!(
        r#"{{"add_on": {{"addon": "acme.join", "kind": "join", "config_version": 3, "config": {config}}}}}"#
    );

    let parsed: Action = serde_json::from_str(&action).expect("the action parses");
    let written = serde_json::to_string(&parsed).expect("an action serializes");

    assert_eq!(
        written,
        format!(
            r#"{{"add_on":{{"addon":"acme.join","kind":"join","config_version":3,"config":{config}}}}}"#
        )
    );
}

#[test]
fn a_config_parsed_from_a_value_loses_its_key_order() {
    let action: Action = serde_json::from_value(json!({
        "add_on": {
            "addon": "acme.join",
            "kind": "join",
            "config_version": 1,
            "config": { "zeta": 1, "alpha": 2 },
        },
    }))
    .expect("the action parses");

    let Action::AddOn(step) = action else {
        panic!("expected an add-on step, found {action:?}");
    };
    assert_eq!(step.config.get(), r#"{"alpha":2,"zeta":1}"#);
}

#[test]
fn validate_warns_about_a_step_kind_that_no_catalog_knows() {
    let sequence =
        AuthoredSequence::from_json(&document_with_unknown_add_on()).expect("the document parses");

    let findings = validate(&sequence, &BTreeSet::new());

    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.severity(), Severity::Warning);
    assert_eq!(finding.path.to_string(), "root.children[0]");
    assert_eq!(
        finding.issue,
        Issue::UnknownStepKind {
            kind: AddOnRef {
                addon: "acme.autopilot".parse().expect("a valid add-on ID"),
                kind: "register".parse().expect("a valid kind ID"),
            },
        }
    );
}

#[test]
fn validate_finds_nothing_when_the_catalog_knows_the_step_kind() {
    let sequence =
        AuthoredSequence::from_json(&document_with_unknown_add_on()).expect("the document parses");
    let known = BTreeSet::from([AddOnRef {
        addon: "acme.autopilot".parse().expect("a valid add-on ID"),
        kind: "register".parse().expect("a valid kind ID"),
    }]);

    assert_eq!(validate(&sequence, &known), []);
}

#[test]
fn validate_rejects_a_win_pe_add_on_step_or_join_slot_with_a_full_os_condition() {
    let wmi = json!({
        "wmi": { "namespace": "root\\cimv2", "query": "SELECT * FROM Win32_Battery", "ignore_failure": true },
    });
    let installed_msi = json!({
        "not": { "installed_msi": { "product_code": "23170f69-40c1-2702-2408-000001000000" } },
    });
    let mut document = document();
    let register = &mut document["root"]["children"][0]["step"];
    register["run_in"] = json!("win_pe");
    register["meta"]["condition"] = wmi;
    let join = &mut document["root"]["children"][2]["step"];
    join["run_in"] = json!("win_pe");
    join["action"]["apply_join_profile"]["stage"] = json!("offline_image");
    join["meta"]["condition"] = installed_msi;
    let sequence = parse(&document).expect("the document parses");

    let findings = validate(&sequence, &BTreeSet::new());

    let found: Vec<(String, Severity, Issue)> = findings
        .iter()
        .map(|finding| {
            (
                finding.path.to_string(),
                finding.severity(),
                finding.issue.clone(),
            )
        })
        .collect();
    let unknown = Issue::UnknownStepKind {
        kind: AddOnRef {
            addon: "acme.autopilot".parse().expect("a valid add-on ID"),
            kind: "register".parse().expect("a valid kind ID"),
        },
    };
    assert_eq!(
        found,
        [
            (
                "root.children[0]".to_owned(),
                Severity::Error,
                Issue::FullOsCondition { condition: "wmi" },
            ),
            ("root.children[0]".to_owned(), Severity::Warning, unknown),
            (
                "root.children[2]".to_owned(),
                Severity::Error,
                Issue::FullOsCondition {
                    condition: "installed_msi",
                },
            ),
        ]
    );
}

fn document() -> Value {
    serde_json::from_str(&document_with_unknown_add_on()).expect("the document is JSON")
}

fn parse(document: &Value) -> Result<AuthoredSequence, ParseError> {
    AuthoredSequence::from_json(&document.to_string())
}

#[track_caller]
fn assert_fails(document: &Value, path: &str, message: &str) {
    match parse(document) {
        Ok(sequence) => panic!("expected an error at {path}, and it parsed: {sequence:?}"),
        Err(error) => {
            assert_eq!(error.path(), path, "{error}");
            assert!(error.to_string().contains(message), "{error}");
        }
    }
}

const REGISTER: &str = "/root/children/0/step/action";
const SET: &str = "/root/children/1/step/action";
const JOIN: &str = "/root/children/2/step/action";

#[test]
fn a_built_in_action_with_an_extra_field_fails() {
    let mut document = document();
    document.pointer_mut(SET).expect("the step")["set_variable"]["sneaky"] = json!(true);

    assert_fails(
        &document,
        "root.children[1].step.action.set_variable.sneaky",
        "unknown field `sneaky`",
    );
}

#[test]
fn an_add_on_step_with_an_extra_envelope_field_fails() {
    let mut document = document();
    document.pointer_mut(REGISTER).expect("the step")["add_on"]["sneaky"] = json!(true);

    assert_fails(
        &document,
        "root.children[0].step.action.add_on.sneaky",
        "unknown field `sneaky`",
    );
}

#[test]
fn a_join_slot_with_an_extra_field_fails() {
    let mut document = document();
    document.pointer_mut(JOIN).expect("the step")["apply_join_profile"]["sneaky"] = json!(true);

    assert_fails(
        &document,
        "root.children[2].step.action.apply_join_profile.sneaky",
        "unknown field `sneaky`",
    );
}

#[test]
fn an_unknown_action_type_fails_and_the_error_lists_every_key() {
    let mut document = document();
    *document.pointer_mut(SET).expect("the step") = json!({ "future_step": { "x": 1 } });

    assert_fails(
        &document,
        "root.children[1].step.action",
        "unknown variant `future_step`, expected one of `add_on`, `apply_join_profile`, `run_command`",
    );
    let error = parse(&document).expect_err("an unknown action must not parse");
    assert!(
        error.to_string().contains("`add_provisioning_package`"),
        "{error}"
    );
}

#[test]
fn an_add_on_step_without_config_version_fails() {
    let mut document = document();
    document.pointer_mut(REGISTER).expect("the step")["add_on"]
        .as_object_mut()
        .expect("an object")
        .remove("config_version");

    assert_fails(
        &document,
        "root.children[0].step.action.add_on",
        "missing field `config_version`",
    );
}

#[test]
fn an_action_with_two_keys_fails() {
    let mut document = document();
    document.pointer_mut(SET).expect("the step")["restart"] = json!({ "into": "win_pe" });

    let error = parse(&document).expect_err("an action with two keys must not parse");
    assert!(
        error.path().starts_with("root.children[1].step.action"),
        "{error}"
    );
}

#[test]
fn a_bad_field_fails_with_its_full_path() {
    let cases = [
        (
            "/root/children/0/step/action/add_on/config_version",
            json!(-1),
        ),
        (
            "/root/children/0/step/action/add_on/config_version",
            json!("1"),
        ),
        (
            "/root/children/0/step/action/add_on/addon",
            json!("Acme.Autopilot"),
        ),
        ("/root/children/0/step/action/add_on/kind", json!("")),
        (
            "/root/children/1/step/action/set_variable/name",
            json!("_GroupTag"),
        ),
        (
            "/root/children/2/step/action/apply_join_profile/stage",
            json!("winpe"),
        ),
    ];

    for (pointer, bad) in cases {
        let mut document = document();
        *document
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("the document has no {pointer}")) = bad.clone();

        let error = parse(&document).expect_err(&format!("{pointer} = {bad} must not parse"));

        let path = pointer
            .trim_start_matches('/')
            .split('/')
            .map(|token| match token.parse::<usize>() {
                Ok(index) => format!("[{index}]"),
                Err(_) => format!(".{token}"),
            })
            .collect::<String>()
            .trim_start_matches('.')
            .to_owned();
        assert_eq!(error.path(), path, "{pointer} = {bad}: {error}");
    }
}

#[test]
fn a_lowered_sequence_holds_no_add_on_step() {
    let error = LoweredSequence::from_json(&document_with_unknown_add_on())
        .expect_err("a lowered sequence must not hold an add-on step");

    assert_eq!(error.path(), "root.children[0].step.action", "{error}");
    assert!(
        error.to_string().contains("unknown variant `add_on`"),
        "{error}"
    );
}

#[test]
fn a_sequence_without_add_ons_reads_the_same_as_either_type() {
    let sample = include_str!("data/windows-11-pro.json");

    let authored = AuthoredSequence::from_json(sample).expect("the sample parses");
    let lowered = LoweredSequence::from_json(sample).expect("the sample parses");

    assert_eq!(
        serde_json::to_string(&authored).expect("a task sequence serializes"),
        serde_json::to_string(&lowered).expect("a task sequence serializes")
    );
}

fn expansion(text: &str) -> Result<Vec<Step<Action>>, serde_json::Error> {
    serde_json::from_str(text)
}

fn expansion_with_files(files: Value) -> String {
    json!([{
        "meta": {
            "id": "00000000-0000-4000-8000-000000000009",
            "name": "Collect the hash",
            "enabled": true,
            "continue_on_error": false,
            "condition": null,
        },
        "run_in": "full_os",
        "action": { "run_command": { "command": "cmd /c type %_StepDir%\\enroll.ppkg", "working_dir": null, "files": files } },
    }])
    .to_string()
}

#[test]
fn an_expansion_whose_step_file_names_a_secret_fails() {
    let cases = [
        json!([{ "name": "enroll.ppkg", "secret_file": "enrollment_package" }]),
        json!([{ "name": "enroll.ppkg", "content": "assets/enroll.ppkg", "secret": "enrollment_package" }]),
        json!([{ "name": "enroll.ppkg", "content": { "secret_file": "enrollment_package" } }]),
    ];

    for files in cases {
        let error = expansion(&expansion_with_files(files.clone()))
            .expect_err(&format!("{files} must not parse"));
        assert!(error.is_data(), "{files}: {error}");
    }
}

#[test]
fn a_step_file_holds_a_name_and_content() {
    let steps = expansion(&expansion_with_files(json!([
        { "name": "collect-hash.ps1", "content": "assets/collect-hash.ps1" },
    ])))
    .expect("the expansion parses");

    let Action::Builtin(BuiltinAction::RunCommand { files, .. }) = &steps[0].action else {
        panic!("expected a RunCommand, found {:?}", steps[0].action);
    };
    assert_eq!(files.as_slice().len(), 1);
    assert_eq!(files.as_slice()[0].name.as_str(), "collect-hash.ps1");
    assert_eq!(
        files.as_slice()[0].content.as_str(),
        "assets/collect-hash.ps1"
    );
}

#[test]
fn two_step_files_with_one_name_in_any_case_fail_at_the_second() {
    let mut document = document();
    *document.pointer_mut(SET).expect("the step") = json!({
        "run_command": {
            "command": "powershell -File %_StepDir%\\a.ps1",
            "working_dir": null,
            "files": [
                { "name": "a.ps1", "content": "assets/a.ps1" },
                { "name": "b.ps1", "content": "assets/b.ps1" },
                { "name": "A.PS1", "content": "assets/other.ps1" },
            ],
        },
    });

    assert_fails(
        &document,
        "root.children[1].step.action.run_command.files[2].name",
        "files[0] already has this name, as `a.ps1`",
    );
}

#[test]
fn step_files_built_in_code_cannot_share_a_name() {
    let file = |name: &str| StepFile {
        name: name.parse().expect("a valid file name"),
        content: "assets/a.ps1".parse().expect("a valid content reference"),
    };

    let error = StepFiles::new(vec![file("a.ps1"), file("A.ps1")])
        .expect_err("two files with one name must fail");

    assert_eq!((error.first, error.second), (0, 1));
    assert_eq!(error.name.as_str(), "a.ps1");
    StepFiles::new(vec![file("a.ps1"), file("b.ps1")]).expect("two names are fine");
}

#[test]
fn omitted_files_are_empty() {
    let action: BuiltinAction = serde_json::from_value(json!({
        "run_powershell": { "script": "scripts/a.ps1", "args": [] },
    }))
    .expect("the action parses");

    let BuiltinAction::RunPowerShell { files, .. } = action else {
        panic!("expected RunPowerShell, found {action:?}");
    };
    assert_eq!(files.as_slice(), []);
}

#[test]
fn a_step_file_name_is_one_safe_windows_file_name() {
    for good in ["collect-hash.ps1", "a", ".config", "NUL_backup.txt", "CON1"] {
        good.parse::<FileName>()
            .unwrap_or_else(|error| panic!("{good}: {error}"));
    }
    let long = "a".repeat(256);
    for bad in [
        "",
        ".",
        "..",
        "a.",
        "a/b",
        "a\\b",
        "..\\a",
        "c:x",
        "a b",
        "NUL",
        "nul.txt",
        "Com1.ps1",
        "LPT9",
        "naïve.txt",
        &long,
    ] {
        assert!(bad.parse::<FileName>().is_err(), "{bad:?} must not parse");
    }
}

#[test]
fn the_new_built_in_actions_have_their_json_keys() {
    let server_call: BuiltinAction = serde_json::from_value(json!({
        "server_call": { "send": ["HardwareHash", "_SMSTSMachineName"] },
    }))
    .expect("the action parses");
    let package: BuiltinAction = serde_json::from_value(json!({
        "add_provisioning_package": { "package": "enrollment_package" },
    }))
    .expect("the action parses");

    assert_eq!(
        serde_json::to_value(&server_call).expect("an action serializes"),
        json!({
            "server_call": {
                "send": ["HardwareHash", "_SMSTSMachineName"],
                "limits": { "max_runtime_secs": null, "rerun_if_interrupted": false },
            },
        })
    );
    let BuiltinAction::AddProvisioningPackage { package, limits } = package else {
        panic!("expected AddProvisioningPackage, found {package:?}");
    };
    assert_eq!(package.as_str(), "enrollment_package");
    assert_eq!(limits, StepLimits::default());
    assert!(
        serde_json::from_value::<BuiltinAction>(json!({
            "add_provisioning_package": { "package": "" },
        }))
        .is_err()
    );
}

#[test]
fn an_add_on_may_return_six_built_in_actions() {
    let cases = [
        (
            json!({ "run_command": { "command": "x", "working_dir": null } }),
            true,
        ),
        (
            json!({ "run_powershell": { "script": "a.ps1", "args": [] } }),
            true,
        ),
        (
            json!({ "set_variable": { "name": "A", "value": "b" } }),
            true,
        ),
        (json!({ "restart": { "into": "installed_os" } }), true),
        (json!({ "server_call": { "send": [] } }), true),
        (
            json!({ "add_provisioning_package": { "package": "p" } }),
            true,
        ),
        (json!({ "partition_disk": { "layout": "uefi-gpt" } }), false),
        (json!({ "apply_os_image": { "image": "w11" } }), false),
        (json!({ "apply_drivers": { "selection": "auto" } }), false),
        (json!({ "apply_unattend": { "template": "t" } }), false),
        (
            json!({ "install_applications": { "apps": { "one": "a" } } }),
            false,
        ),
        (json!({ "install_updates": {} }), false),
        (json!({ "verify_drivers": { "policy": "warn" } }), false),
        (json!({ "check_readiness": {} }), false),
        (json!({ "activate_windows": { "online": true } }), false),
    ];

    for (action, allowed) in cases {
        let parsed: BuiltinAction =
            serde_json::from_value(action.clone()).expect("the action parses");
        assert_eq!(parsed.allowed_from_addon(), allowed, "{action}");
    }
}

#[test]
fn only_server_call_and_add_provisioning_package_come_only_from_an_add_on() {
    let cases = [
        (json!({ "server_call": { "send": [] } }), true),
        (
            json!({ "add_provisioning_package": { "package": "p" } }),
            true,
        ),
        (
            json!({ "run_command": { "command": "x", "working_dir": null } }),
            false,
        ),
        (
            json!({ "run_powershell": { "script": "a.ps1", "args": [] } }),
            false,
        ),
        (
            json!({ "set_variable": { "name": "A", "value": "b" } }),
            false,
        ),
        (json!({ "restart": { "into": "installed_os" } }), false),
        (json!({ "partition_disk": { "layout": "uefi-gpt" } }), false),
        (json!({ "apply_os_image": { "image": "w11" } }), false),
        (json!({ "apply_drivers": { "selection": "auto" } }), false),
        (json!({ "apply_unattend": { "template": "t" } }), false),
        (
            json!({ "install_applications": { "apps": { "one": "a" } } }),
            false,
        ),
        (json!({ "install_updates": {} }), false),
        (json!({ "verify_drivers": { "policy": "warn" } }), false),
        (json!({ "check_readiness": {} }), false),
        (json!({ "activate_windows": { "online": true } }), false),
    ];

    for (action, only) in cases {
        let parsed: BuiltinAction =
            serde_json::from_value(action.clone()).expect("the action parses");
        assert_eq!(parsed.only_from_addon(), only, "{action}");
    }
}

#[test]
fn add_on_ids_are_publisher_dot_name_in_lower_case() {
    for good in [
        "deployarr.sql-order-lookup",
        "acme.x",
        "a1.b-2",
        &format!("{}.{}", "a".repeat(32), "b".repeat(32)),
    ] {
        let id: AddOnId = good
            .parse()
            .unwrap_or_else(|error| panic!("{good}: {error}"));
        assert_eq!(id.as_str(), good);
    }
    let id: AddOnId = "deployarr.webhooks".parse().expect("a valid ID");
    assert_eq!((id.publisher(), id.name()), ("deployarr", "webhooks"));

    for bad in [
        "",
        "deployarr",
        "Deployarr.webhooks",
        "deployarr.Webhooks",
        "deployarr.web.hooks",
        ".webhooks",
        "deployarr.",
        "1acme.x",
        "acme.-x",
        "acme.x_y",
        "acme.x y",
        &format!("{}.b", "a".repeat(33)),
        &format!("a.{}", "b".repeat(33)),
    ] {
        assert!(bad.parse::<AddOnId>().is_err(), "{bad:?} must not parse");
    }
}

#[test]
fn kind_ids_are_lower_case_and_at_most_63_characters() {
    for good in ["register", "by-identity", "a", "a1", &"k".repeat(63)] {
        good.parse::<KindId>()
            .unwrap_or_else(|error| panic!("{good}: {error}"));
    }
    for bad in ["", "Register", "1st", "-a", "a.b", "a_b", &"k".repeat(64)] {
        assert!(bad.parse::<KindId>().is_err(), "{bad:?} must not parse");
    }
}

#[test]
fn an_add_on_ref_refuses_unknown_fields() {
    let good = json!({ "addon": "deployarr.sql-order-lookup", "kind": "by-identity" });
    let parsed: AddOnRef = serde_json::from_value(good.clone()).expect("the reference parses");
    assert_eq!(serde_json::to_value(&parsed).expect("it serializes"), good);

    let mut bad = good;
    bad["version"] = json!(1);
    assert!(serde_json::from_value::<AddOnRef>(bad).is_err());
}

#[test]
fn an_add_on_join_profile_keeps_its_config_bytes() {
    let text = format!(
        r#"{{"add_on":{{"kind":{{"addon":"deployarr.entra-join","kind":"bulk-enrollment"}},"config_version":2,"config":{CONFIG}}}}}"#
    );

    let profile: JoinProfile = serde_json::from_str(&text).expect("the profile parses");

    assert_eq!(
        serde_json::to_string(&profile).expect("a profile serializes"),
        text
    );
    let domain = json!({
        "domain": {
            "domain": "corp.example",
            "ou": null,
            "account": "join",
            "password": "domain-join-password",
            "site": "x",
        },
    });
    assert!(serde_json::from_value::<JoinProfile>(domain).is_err());
}
