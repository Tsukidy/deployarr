use std::collections::BTreeSet;
use std::num::{NonZeroU8, NonZeroU32};

use deployarr_core::action::{
    Action, AppSelection, BuiltinAction, CheckMode, DiskCheck, DiskSelector, ExitCodes,
    MemoryCheck, MissingUpdatePolicy, ProcessorCheck, ReadinessChecks, StepLimits, UpdateExclusion,
    UpdateSource, UpdateTimeouts, WsusUrl, WsusUrlError,
};
use deployarr_core::condition::{Comparison, Condition, FileVersion, RegData, Test};
use deployarr_core::sequence::{
    AuthoredSequence, DefaultVars, DefaultVarsError, Node, NodeMeta, ParseError, TaskSequenceId,
};
use deployarr_core::template::Template;
use deployarr_core::variable::{SettableVarName, VarAssignment, VarName};
use serde_json::{Value, json};
use uuid::Uuid;

const SAMPLE: &str = include_str!("data/windows-11-pro.json");

const INSTALL: &str = "/root/children/0/group/children";
const FINISH: &str = "/root/children/1/group/children";
const FINISH_CONDITION: &str = "/root/children/1/group/meta/condition";

fn sample() -> Value {
    serde_json::from_str(SAMPLE).expect("the sample is JSON")
}

fn parse(document: &Value) -> Result<AuthoredSequence, ParseError> {
    AuthoredSequence::from_json(&document.to_string())
}

#[test]
fn sample_round_trips_unchanged() {
    let sequence = AuthoredSequence::from_json(SAMPLE).expect("the sample parses");

    let written = serde_json::to_string(&sequence).expect("a task sequence serializes");

    let written_value: Value = serde_json::from_str(&written).expect("the output is JSON");
    assert_eq!(written_value, sample());
    assert_eq!(AuthoredSequence::from_json(&written), Ok(sequence));
}

fn action_name(action: &Action) -> &'static str {
    let action = match action {
        Action::Builtin(action) => action,
        Action::AddOn(_) => return "AddOn",
        Action::ApplyJoinProfile { .. } => return "ApplyJoinProfile",
    };
    match action {
        BuiltinAction::RunCommand { .. } => "RunCommand",
        BuiltinAction::RunPowerShell { .. } => "RunPowerShell",
        BuiltinAction::SetVariable { .. } => "SetVariable",
        BuiltinAction::Restart { .. } => "Restart",
        BuiltinAction::PartitionDisk { .. } => "PartitionDisk",
        BuiltinAction::ApplyOsImage { .. } => "ApplyOsImage",
        BuiltinAction::ApplyDrivers { .. } => "ApplyDrivers",
        BuiltinAction::ApplyUnattend { .. } => "ApplyUnattend",
        BuiltinAction::InstallApplications { .. } => "InstallApplications",
        BuiltinAction::InstallUpdates { .. } => "InstallUpdates",
        BuiltinAction::VerifyDrivers { .. } => "VerifyDrivers",
        BuiltinAction::CheckReadiness { .. } => "CheckReadiness",
        BuiltinAction::ActivateWindows { .. } => "ActivateWindows",
        BuiltinAction::ServerCall { .. } => "ServerCall",
        BuiltinAction::AddProvisioningPackage { .. } => "AddProvisioningPackage",
    }
}

fn collect_condition_names(condition: &Condition, names: &mut BTreeSet<&'static str>) {
    let name = match condition {
        Condition::All(children) | Condition::Any(children) => {
            for child in children {
                collect_condition_names(child, names);
            }
            if matches!(condition, Condition::All(_)) {
                "All"
            } else {
                "Any"
            }
        }
        Condition::Not(child) => {
            collect_condition_names(child, names);
            "Not"
        }
        Condition::Var { .. } => "Var",
        Condition::Registry { .. } => "Registry",
        Condition::File { .. } => "File",
        Condition::Folder { .. } => "Folder",
        Condition::InstalledMsi(_) => "InstalledMsi",
        Condition::Wmi { .. } => "Wmi",
    };
    names.insert(name);
}

#[derive(Default)]
struct VariantsSeen {
    actions: BTreeSet<&'static str>,
    conditions: BTreeSet<&'static str>,
}

impl VariantsSeen {
    fn visit_meta(&mut self, meta: &NodeMeta) {
        if let Some(condition) = &meta.condition {
            collect_condition_names(condition, &mut self.conditions);
        }
    }

    fn visit(&mut self, node: &Node<Action>) {
        match node {
            Node::Group(group) => {
                self.visit_meta(&group.meta);
                for child in &group.children {
                    self.visit(child);
                }
            }
            Node::Step(step) => {
                self.visit_meta(&step.meta);
                self.actions.insert(action_name(&step.action));
            }
        }
    }
}

// The sample is a sequence without add-ons. It leaves out the add-on step, the
// join slot, and the two built-in actions that add-on steps expand to,
// `ServerCall` and `AddProvisioningPackage`. `tests/addon.rs` covers those.
#[test]
fn sample_uses_every_condition_and_every_action_outside_add_ons() {
    let sequence = AuthoredSequence::from_json(SAMPLE).expect("the sample parses");
    let mut seen = VariantsSeen::default();
    for child in &sequence.root.children {
        seen.visit(child);
    }

    assert_eq!(
        seen.actions,
        BTreeSet::from([
            "ActivateWindows",
            "ApplyDrivers",
            "ApplyOsImage",
            "ApplyUnattend",
            "CheckReadiness",
            "InstallApplications",
            "InstallUpdates",
            "PartitionDisk",
            "Restart",
            "RunCommand",
            "RunPowerShell",
            "SetVariable",
            "VerifyDrivers",
        ])
    );
    assert_eq!(
        seen.conditions,
        BTreeSet::from([
            "All",
            "Any",
            "File",
            "Folder",
            "InstalledMsi",
            "Not",
            "Registry",
            "Var",
            "Wmi",
        ])
    );
}

fn only_step_action(document: Value) -> BuiltinAction {
    let sequence = parse(&document).expect("the document parses");
    match sequence.root.children.as_slice() {
        [Node::Step(step)] => match &step.action {
            Action::Builtin(action) => action.clone(),
            other => panic!("expected a built-in action, found {other:?}"),
        },
        other => panic!("expected one step, found {other:?}"),
    }
}

fn sequence_with_action(action: Value) -> Value {
    json!({
        "schema_version": 1,
        "id": "MINIMAL",
        "version": 1,
        "name": "Minimal",
        "default_vars": [],
        "root": {
            "meta": {
                "id": "00000000-0000-4000-8000-000000000001",
                "name": "Root",
                "enabled": true,
                "continue_on_error": false,
            },
            "children": [{
                "step": {
                    "meta": {
                        "id": "00000000-0000-4000-8000-000000000002",
                        "name": "Step",
                        "enabled": true,
                        "continue_on_error": false,
                    },
                    "run_in": "either",
                    "action": action,
                },
            }],
        },
    })
}

#[test]
fn omitted_exit_codes_and_limits_take_the_documented_defaults() {
    let action = only_step_action(sequence_with_action(json!({
        "run_command": { "command": "ipconfig /all" },
    })));

    let BuiltinAction::RunCommand {
        working_dir,
        exit,
        limits,
        ..
    } = action
    else {
        panic!("expected RunCommand, found {action:?}");
    };
    assert_eq!(working_dir, None);
    assert_eq!(exit, ExitCodes::default());
    assert_eq!(exit.success(), [0]);
    assert_eq!(exit.reboot(), [3010, 1641]);
    assert_eq!(
        limits,
        StepLimits {
            max_runtime_secs: None,
            rerun_if_interrupted: false,
        }
    );
}

#[test]
fn omitted_readiness_checks_are_the_windows_11_minimums() {
    let action = only_step_action(sequence_with_action(json!({ "check_readiness": {} })));

    assert_eq!(
        action,
        BuiltinAction::CheckReadiness {
            checks: ReadinessChecks {
                memory: MemoryCheck {
                    min_mib: 4096,
                    mode: CheckMode::Fail,
                },
                disk: DiskCheck {
                    min_gb: 64,
                    mode: CheckMode::Fail,
                },
                processor: ProcessorCheck {
                    min_cores: 2,
                    mode: CheckMode::Fail,
                },
                uefi: CheckMode::Fail,
                tpm_2_0: CheckMode::Fail,
                secure_boot: CheckMode::Warn,
            },
        }
    );
}

#[test]
fn a_readiness_check_that_sets_one_field_keeps_the_other_defaults() {
    let action = only_step_action(sequence_with_action(json!({
        "check_readiness": { "checks": { "tpm_2_0": "off", "disk": { "mode": "warn" } } },
    })));

    assert_eq!(
        action,
        BuiltinAction::CheckReadiness {
            checks: ReadinessChecks {
                disk: DiskCheck {
                    min_gb: 64,
                    mode: CheckMode::Warn,
                },
                tpm_2_0: CheckMode::Off,
                ..ReadinessChecks::default()
            },
        }
    );
}

#[test]
fn an_omitted_disk_is_the_first_internal_disk() {
    let action = only_step_action(sequence_with_action(json!({
        "partition_disk": { "layout": "uefi-gpt" },
    })));

    let BuiltinAction::PartitionDisk { disk, .. } = action else {
        panic!("expected PartitionDisk, found {action:?}");
    };
    assert_eq!(disk, DiskSelector::FirstInternal);
}

#[test]
fn a_runtime_limit_is_whole_seconds() {
    let action = only_step_action(sequence_with_action(json!({
        "run_command": { "command": "setup.exe", "limits": { "max_runtime_secs": 90 } },
    })));

    let BuiltinAction::RunCommand { limits, .. } = action else {
        panic!("expected RunCommand, found {action:?}");
    };
    assert_eq!(limits.max_runtime_secs, NonZeroU32::new(90));
}

#[test]
fn variable_names_ignore_case_and_keep_their_spelling() {
    let written: VarName = "OSDComputerName".parse().expect("a valid name");
    let lower: VarName = "osdcomputername".parse().expect("a valid name");

    assert_eq!(written, lower);
    assert_eq!(written.as_str(), "OSDComputerName");
    assert_ne!(written, "OSDComputerNam".parse().expect("a valid name"));
}

#[test]
fn a_variable_name_holds_at_most_256_characters() {
    assert!("A".repeat(256).parse::<VarName>().is_ok());
    assert!("A".repeat(257).parse::<VarName>().is_err());
}

#[test]
fn task_sequence_ids_are_upper_case() {
    assert!("W11-PRO_STD2".parse::<TaskSequenceId>().is_ok());
    assert!("W11-Pro-STD".parse::<TaskSequenceId>().is_err());
}

#[test]
fn file_versions_order_by_field_not_by_text() {
    let older: FileVersion = "10.0.9200.0".parse().expect("a valid version");
    let newer: FileVersion = "10.0.26100.1".parse().expect("a valid version");

    assert!(older < newer);
    assert_eq!(newer.to_string(), "10.0.26100.1");
}

#[test]
fn omitted_comparisons_in_a_file_condition_are_none() {
    let mut document = sequence_with_action(json!({ "activate_windows": { "online": false } }));
    document["root"]["meta"]["condition"] = json!({ "file": { "path": "C:\\marker.txt" } });

    let sequence = parse(&document).expect("the document parses");

    let Some(Condition::File {
        version, modified, ..
    }) = sequence.root.meta.condition
    else {
        panic!("expected a File condition");
    };
    assert_eq!(version, None::<Comparison<FileVersion>>);
    assert_eq!(modified, None);
}

fn field_path(pointer: &str) -> String {
    let mut path = String::new();
    for token in pointer.split('/').skip(1) {
        if token.parse::<usize>().is_ok() {
            path.push_str(&format!("[{token}]"));
        } else {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(token);
        }
    }
    path
}

#[test]
fn a_bad_value_fails_and_the_error_names_its_field() {
    let install = |rest: &str| format!("{INSTALL}{rest}");
    let finish = |rest: &str| format!("{FINISH}{rest}");
    let cases: Vec<(String, Value)> = vec![
        ("/schema_version".into(), json!(2)),
        ("/schema_version".into(), json!("1")),
        ("/id".into(), json!("w11-pro-std")),
        ("/id".into(), json!("W11 PRO")),
        ("/id".into(), json!("")),
        ("/version".into(), json!(-1)),
        ("/name".into(), json!(5)),
        ("/default_vars/0/name".into(), json!("OSD ComputerName")),
        ("/default_vars/0/name".into(), json!("")),
        ("/default_vars/0/name".into(), json!("A".repeat(257))),
        ("/default_vars/1/value".into(), json!(null)),
        ("/root/meta/id".into(), json!("not-a-uuid")),
        ("/root/meta/enabled".into(), json!("yes")),
        (install("/0/step/run_in"), json!("winpe")),
        (
            install("/0/step/action/check_readiness/checks/memory/min_mib"),
            json!(-1),
        ),
        (
            install("/0/step/action/check_readiness/checks/disk/mode"),
            json!("maybe"),
        ),
        (
            install("/0/step/action/check_readiness/checks/tpm_2_0"),
            json!("on"),
        ),
        (
            install("/1/step/action/set_variable/name"),
            json!("%AssetTag%"),
        ),
        (install("/1/step/meta/condition/var/test"), json!("equal")),
        (install("/2/step/action/partition_disk/layout"), json!("")),
        (install("/2/step/action/partition_disk/disk"), json!("usb")),
        (install("/3/step/action/apply_os_image/image"), json!("")),
        (
            install("/4/step/action/apply_drivers/selection"),
            json!("all"),
        ),
        (install("/5/step/action/apply_unattend/template"), json!("")),
        (
            install("/6/step/action/run_command/exit/success/1"),
            json!(4_294_967_296_i64),
        ),
        (
            install("/6/step/action/run_command/limits/max_runtime_secs"),
            json!(0),
        ),
        (
            install("/6/step/meta/condition/file/version/value"),
            json!("10.0.26100"),
        ),
        (
            install("/6/step/meta/condition/file/version/value"),
            json!("10.0.70000.0"),
        ),
        (
            install("/6/step/meta/condition/file/version/value"),
            json!("10.0.26100.0.1"),
        ),
        (
            install("/6/step/meta/condition/file/modified/value"),
            json!("yesterday"),
        ),
        (install("/7/step/action/restart/into"), json!("bios")),
        (
            finish("/0/step/action/verify_drivers/policy"),
            json!("retry"),
        ),
        (
            finish("/1/step/action/install_applications/apps/from_list"),
            json!("my apps"),
        ),
        (
            finish("/2/step/action/install_applications/apps/one"),
            json!(""),
        ),
        (
            finish("/2/step/meta/condition/not/installed_msi/product_code"),
            json!("7-Zip"),
        ),
        (finish("/3/step/action/run_powershell/script"), json!("")),
        (finish("/3/step/action/run_powershell/args/1"), json!(5)),
        (
            finish("/4/step/action/activate_windows/online"),
            json!("true"),
        ),
        (
            format!("{FINISH_CONDITION}/all/0/any/0/registry/test/greater_equal"),
            json!(true),
        ),
        (
            format!("{FINISH_CONDITION}/all/2/folder/modified/op"),
            json!("before"),
        ),
    ];

    for (pointer, bad) in cases {
        let mut document = sample();
        *document
            .pointer_mut(&pointer)
            .unwrap_or_else(|| panic!("the sample has no {pointer}")) = bad.clone();

        let error = parse(&document).expect_err(&format!("{pointer} = {bad} must not parse"));

        assert_eq!(
            error.path(),
            field_path(&pointer),
            "{pointer} = {bad}: {error}"
        );
    }
}

fn object_at<'a>(document: &'a mut Value, pointer: &str) -> &'a mut serde_json::Map<String, Value> {
    document
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("the sample has no object at {pointer}"))
}

#[test]
fn a_missing_field_fails_and_the_error_names_it() {
    let cases = [
        ("", "schema_version"),
        ("", "default_vars"),
        ("/root", "children"),
        ("/root/meta", "name"),
        ("/root/meta", "continue_on_error"),
        ("/root/children/0/group/children/0/step", "run_in"),
        (
            "/root/children/0/group/children/1/step/action/set_variable",
            "value",
        ),
        (
            "/root/children/1/group/children/3/step/action/run_powershell",
            "args",
        ),
        (
            "/root/children/1/group/children/3/step/meta/condition/wmi",
            "ignore_failure",
        ),
    ];

    for (pointer, field) in cases {
        let mut document = sample();
        object_at(&mut document, pointer)
            .remove(field)
            .unwrap_or_else(|| panic!("the sample has no {pointer}/{field}"));

        let error =
            parse(&document).expect_err(&format!("{pointer} without {field} must not parse"));

        assert!(
            error.to_string().contains(&format!("`{field}`")),
            "{pointer} without {field}: {error}"
        );
    }
}

#[test]
fn an_unknown_field_fails_and_the_error_names_it() {
    let cases = [
        ("", "schemaVersion"),
        ("/root", "run_in"),
        ("/root/meta", "continue_on_eror"),
        ("/root/children/0/group/children/0/step", "limits"),
        (
            "/root/children/0/group/children/6/step/action/run_command",
            "timeout",
        ),
        (
            "/root/children/0/group/children/6/step/action/run_command/exit",
            "failure",
        ),
        (
            "/root/children/0/group/children/0/step/action/check_readiness/checks",
            "tpm",
        ),
        (
            "/root/children/0/group/children/1/step/meta/condition/var",
            "operator",
        ),
        (
            "/root/children/0/group/children/6/step/meta/condition/file/version",
            "than",
        ),
    ];

    for (pointer, field) in cases {
        let mut document = sample();
        object_at(&mut document, pointer).insert(field.to_owned(), json!(1));

        let error = parse(&document).expect_err(&format!("{pointer} with {field} must not parse"));

        assert!(
            error.to_string().contains(&format!("`{field}`")),
            "{pointer} with {field}: {error}"
        );
    }
}

#[test]
fn an_unknown_action_fails_and_the_error_names_the_action_field() {
    let pointer = format!("{INSTALL}/3/step/action");
    let mut document = sample();
    *document
        .pointer_mut(&pointer)
        .expect("the sample has the step") = json!({ "format_disk": { "disk": 0 } });

    let error = parse(&document).expect_err("an unknown action must not parse");

    assert!(error.path().starts_with(&field_path(&pointer)), "{error}");
    assert!(error.to_string().contains("`format_disk`"), "{error}");
}

#[test]
fn text_that_is_not_one_json_document_fails() {
    assert!(AuthoredSequence::from_json("").is_err());
    assert!(AuthoredSequence::from_json("{\"schema_version\": 1,").is_err());
    assert!(AuthoredSequence::from_json(&format!("{SAMPLE} {SAMPLE}")).is_err());
}

// States that the JSON once allowed but that cannot run. Each one fails to
// parse, and the error path locates the field. The owner's answer on #62 lists
// them.

const STEP: &str = "root.children[0].step";

fn sequence_with_condition(condition: Value) -> Value {
    let mut document = sequence_with_action(json!({ "activate_windows": { "online": false } }));
    document["root"]["children"][0]["step"]["meta"]["condition"] = condition;
    document
}

#[track_caller]
fn assert_fails_at(document: &Value, path: &str) -> ParseError {
    match parse(document) {
        Ok(sequence) => panic!("expected an error at {path}, and it parsed: {sequence:?}"),
        Err(error) => {
            assert_eq!(error.path(), path, "{error}");
            error
        }
    }
}

const OPS_WITH_OPERAND: [&str; 7] = [
    "equals",
    "not_equals",
    "less",
    "less_equal",
    "greater",
    "greater_equal",
    "like",
];

fn registry(test: Value) -> Value {
    json!({
        "registry": {
            "key": "HKEY_LOCAL_MACHINE\\SOFTWARE\\Deployarr",
            "value": "Marker",
            "test": test,
        },
    })
}

#[test]
fn a_registry_exists_test_with_data_fails() {
    for op in ["exists", "not_exists"] {
        for data in [json!(1), json!("1")] {
            let document = sequence_with_condition(registry(json!({ op: data })));

            assert_fails_at(
                &document,
                &format!("{STEP}.meta.condition.registry.test.{op}"),
            );
        }
    }
}

#[test]
fn a_registry_equals_test_without_data_fails() {
    for op in OPS_WITH_OPERAND {
        let bare = sequence_with_condition(registry(json!(op)));
        assert_fails_at(&bare, &format!("{STEP}.meta.condition.registry.test.{op}"));

        let null = sequence_with_condition(registry(json!({ op: null })));
        assert_fails_at(&null, &format!("{STEP}.meta.condition.registry.test.{op}"));
    }
}

fn var(name: &str, test: Value) -> Value {
    json!({ "var": { "name": name, "test": test } })
}

#[test]
fn a_var_exists_test_with_a_value_fails() {
    for op in ["exists", "not_exists"] {
        let document = sequence_with_condition(var("AssetTag", json!({ op: "x" })));

        assert_fails_at(&document, &format!("{STEP}.meta.condition.var.test.{op}"));
    }
}

#[test]
fn a_null_operand_on_exists_is_written_back_as_the_bare_operator() {
    for op in ["exists", "not_exists"] {
        for (kind, condition) in [
            ("var", var("AssetTag", json!({ op: null }))),
            ("registry", registry(json!({ op: null }))),
        ] {
            let sequence = parse(&sequence_with_condition(condition))
                .unwrap_or_else(|error| panic!("{kind} {op}: {error}"));

            let written = serde_json::to_value(&sequence).expect("a task sequence serializes");

            assert_eq!(
                written["root"]["children"][0]["step"]["meta"]["condition"][kind]["test"],
                json!(op),
                "{kind} {op}"
            );
        }
    }
}

#[test]
fn a_var_equals_test_with_an_empty_value_fails() {
    for op in OPS_WITH_OPERAND {
        let empty = sequence_with_condition(var("AssetTag", json!({ op: "" })));
        let error = assert_fails_at(&empty, &format!("{STEP}.meta.condition.var.test.{op}"));
        assert!(error.to_string().contains("not_exists"), "{error}");

        let bare = sequence_with_condition(var("AssetTag", json!(op)));
        assert_fails_at(&bare, &format!("{STEP}.meta.condition.var.test.{op}"));
    }
}

#[test]
fn file_version_and_timestamp_comparisons_admit_only_equality_and_order() {
    let conditions = [
        ("file", "version", "10.0.26100.0"),
        ("file", "modified", "2024-10-01T00:00:00Z"),
        ("folder", "modified", "2024-10-01T00:00:00Z"),
    ];
    for (kind, field, value) in conditions {
        for op in ["like", "exists", "not_exists"] {
            let document = sequence_with_condition(json!({
                kind: { "path": "C:\\Windows", field: { "op": op, "value": value } },
            }));

            assert_fails_at(
                &document,
                &format!("{STEP}.meta.condition.{kind}.{field}.op"),
            );
        }
        for op in [
            "equals",
            "not_equals",
            "less",
            "less_equal",
            "greater",
            "greater_equal",
        ] {
            let document = sequence_with_condition(json!({
                kind: { "path": "C:\\Windows", field: { "op": op, "value": value } },
            }));

            parse(&document).unwrap_or_else(|error| panic!("{kind}.{field} {op}: {error}"));
        }
    }
}

#[test]
fn an_empty_all_or_any_fails() {
    let marker = json!({ "folder": { "path": "C:\\Deployarr" } });
    let cases = [
        (json!({ "all": [] }), "all"),
        (json!({ "any": [] }), "any"),
        (json!({ "not": { "all": [] } }), "not.all"),
        (json!({ "all": [marker, { "any": [] }] }), "all[1].any"),
    ];

    for (condition, path) in cases {
        let document = sequence_with_condition(condition);

        assert_fails_at(&document, &format!("{STEP}.meta.condition.{path}"));
    }
}

#[test]
fn negative_registry_data_fails() {
    for data in [json!(-1), json!(i64::MIN)] {
        let document = sequence_with_condition(registry(json!({ "equals": data })));

        assert_fails_at(
            &document,
            &format!("{STEP}.meta.condition.registry.test.equals"),
        );
    }
}

#[test]
fn registry_data_holds_every_qword() {
    let document = sequence_with_condition(registry(json!({
        "equals": 18_446_744_073_709_551_615_u64,
    })));

    let sequence = parse(&document).expect("the document parses");

    let [Node::Step(step)] = sequence.root.children.as_slice() else {
        panic!("expected one step");
    };
    let Some(Condition::Registry { test, .. }) = &step.meta.condition else {
        panic!("expected a Registry condition");
    };
    assert_eq!(*test, Test::Equals(RegData::Integer(u64::MAX)));
}

#[test]
fn a_sequence_cannot_set_a_variable_that_starts_with_an_underscore() {
    let step = sequence_with_action(json!({ "set_variable": { "name": "_Foo", "value": "x" } }));
    let error = assert_fails_at(&step, &format!("{STEP}.action.set_variable.name"));
    assert!(
        error.to_string().contains("belongs to the agent"),
        "{error}"
    );

    let mut default = sequence_with_action(json!({ "activate_windows": { "online": false } }));
    default["default_vars"] = json!([{ "name": "_Foo", "value": "x" }]);
    assert_fails_at(&default, "default_vars[0].name");

    assert!("_Foo".parse::<SettableVarName>().is_err());
    assert!("Foo_".parse::<SettableVarName>().is_ok());
}

#[test]
fn conditions_and_application_lists_can_read_a_variable_that_starts_with_an_underscore() {
    let mut document = sequence_with_action(json!({
        "install_applications": { "apps": { "from_list": "_Foo" } },
    }));
    document["root"]["children"][0]["step"]["meta"]["condition"] =
        var("_Foo", json!({ "equals": "x" }));

    let sequence = parse(&document).expect("the document parses");

    let [Node::Step(step)] = sequence.root.children.as_slice() else {
        panic!("expected one step");
    };
    let Action::Builtin(BuiltinAction::InstallApplications {
        apps: AppSelection::FromList(list),
    }) = &step.action
    else {
        panic!(
            "expected InstallApplications from a list, found {:?}",
            step.action
        );
    };
    assert_eq!(list.as_str(), "_Foo");
    let Some(Condition::Var { name, .. }) = &step.meta.condition else {
        panic!("expected a Var condition");
    };
    assert_eq!(name.as_str(), "_Foo");
}

#[test]
fn two_defaults_for_one_variable_fail_at_the_second() {
    let cases = [
        (
            vec!["OSDComputerName", "osdcomputername"],
            "default_vars[1].name",
            "default_vars[0] already names this variable, as `OSDComputerName`",
        ),
        (
            vec!["A", "A"],
            "default_vars[1].name",
            "default_vars[0] already names this variable, as `A`",
        ),
        (
            vec!["A", "B", "b"],
            "default_vars[2].name",
            "default_vars[1] already names this variable, as `B`",
        ),
    ];

    for (names, path, message) in cases {
        let mut document = sequence_with_action(json!({ "activate_windows": { "online": false } }));
        document["default_vars"] = names
            .iter()
            .map(|name| json!({ "name": name, "value": "x" }))
            .collect();

        let error = assert_fails_at(&document, path);

        assert!(error.to_string().contains(message), "{error}");
    }
}

fn default_var(name: &str) -> VarAssignment {
    VarAssignment {
        name: name.parse().expect("a settable name"),
        value: Template::new("x"),
    }
}

#[test]
fn default_vars_built_in_code_cannot_name_one_variable_twice() {
    let error = DefaultVars::new(vec![
        default_var("OSDComputerName"),
        default_var("osdcomputername"),
    ])
    .expect_err("two defaults for one variable");

    assert_eq!(
        error,
        DefaultVarsError {
            first: 0,
            second: 1,
            name: "OSDComputerName".parse().expect("a settable name"),
        }
    );
    assert!(
        error
            .to_string()
            .contains("default_vars[0] already names this variable, as `OSDComputerName`"),
        "{error}"
    );

    let defaults = DefaultVars::new(vec![
        default_var("OSDComputerName"),
        default_var("TimeZone"),
    ])
    .expect("distinct names");
    let mut names = Vec::new();
    for default in &defaults {
        names.push(default.name.as_str());
    }
    assert_eq!(names, ["OSDComputerName", "TimeZone"]);
    assert!(DefaultVars::default().as_slice().is_empty());
}

fn run_command_with_exit(exit: Value) -> Value {
    sequence_with_action(json!({ "run_command": { "command": "setup.exe", "exit": exit } }))
}

const EXIT: &str = "root.children[0].step.action.run_command.exit";

#[test]
fn an_empty_success_list_fails() {
    for exit in [
        json!({ "success": [] }),
        json!({ "success": [], "reboot": [] }),
    ] {
        assert_fails_at(&run_command_with_exit(exit), &format!("{EXIT}.success"));
    }

    let powershell = sequence_with_action(json!({
        "run_powershell": { "script": "scripts/a.ps1", "args": [], "exit": { "success": [] } },
    }));
    assert_fails_at(
        &powershell,
        "root.children[0].step.action.run_powershell.exit.success",
    );
}

#[test]
fn a_code_in_both_success_and_reboot_fails() {
    // `Value` sorts object keys, so the cases are text, to keep the field order.
    let cases = [
        (
            r#"{"success": [0, 3010], "reboot": [3010]}"#,
            format!("{EXIT}.reboot[0]"),
            "",
        ),
        (
            r#"{"reboot": [1641, 3010], "success": [0, 1641]}"#,
            format!("{EXIT}.success[1]"),
            "",
        ),
        // The omitted list takes its default, which holds the code.
        (
            r#"{"success": [0, 3010]}"#,
            EXIT.to_owned(),
            ". An omitted `reboot` is [3010, 1641]",
        ),
        (
            r#"{"reboot": [0]}"#,
            EXIT.to_owned(),
            ". An omitted `success` is [0]",
        ),
    ];

    for (exit, path, note) in cases {
        let text = run_command_with_exit(json!("EXIT"))
            .to_string()
            .replace(r#""EXIT""#, exit);

        let error = AuthoredSequence::from_json(&text).expect_err(exit);

        assert_eq!(error.path(), path, "{exit}: {error}");
        assert!(error.to_string().contains("in both"), "{exit}: {error}");
        assert_eq!(
            error.to_string().contains("An omitted"),
            !note.is_empty(),
            "{exit}: {error}"
        );
        assert!(error.to_string().contains(note), "{exit}: {error}");
    }
}

#[test]
fn an_exit_code_at_or_above_0x80000000_is_written_as_its_signed_value() {
    let action = only_step_action(run_command_with_exit(json!({
        "success": [0, -2_147_024_891],
        "reboot": [],
    })));

    let BuiltinAction::RunCommand { exit, .. } = action else {
        panic!("expected RunCommand, found {action:?}");
    };
    assert_eq!(exit.success(), [0, -2_147_024_891]);
    assert_eq!(
        exit.success()[1],
        i32::from_ne_bytes(0x8007_0005_u32.to_ne_bytes())
    );

    let unsigned = run_command_with_exit(json!({ "success": [2_147_942_405_u32] }));
    assert_fails_at(&unsigned, &format!("{EXIT}.success[0]"));
}

#[test]
fn an_omitted_list_in_exit_takes_its_default() {
    let action = only_step_action(run_command_with_exit(json!({ "success": [0, 1] })));

    let BuiltinAction::RunCommand { exit, .. } = action else {
        panic!("expected RunCommand, found {action:?}");
    };
    assert_eq!(exit.success(), [0, 1]);
    assert_eq!(exit.reboot(), [3010, 1641]);
}

// The settings of `install_updates`. Every field has a default, and the
// serializer writes every field.

const INSTALL_UPDATES: &str = "root.children[0].step.action.install_updates";

fn install_updates(settings: Value) -> Value {
    sequence_with_action(json!({ "install_updates": settings }))
}

fn secs(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("not zero")
}

fn passes(n: u8) -> NonZeroU8 {
    NonZeroU8::new(n).expect("not zero")
}

fn written_settings(sequence: &AuthoredSequence) -> Value {
    let written = serde_json::to_value(sequence).expect("a task sequence serializes");
    written["root"]["children"][0]["step"]["action"]["install_updates"].clone()
}

#[test]
fn omitted_update_settings_take_the_documented_defaults() {
    let document = install_updates(json!({}));

    let action = only_step_action(document.clone());

    assert_eq!(
        action,
        BuiltinAction::InstallUpdates {
            source: UpdateSource::WindowsUpdate,
            missing: MissingUpdatePolicy::Warn,
            max_passes: passes(7),
            exclude: Vec::new(),
            timeouts: UpdateTimeouts {
                search_secs: secs(3600),
                download_secs: secs(7200),
                install_secs: secs(7200),
            },
        }
    );
    let sequence = parse(&document).expect("the document parses");
    assert_eq!(
        written_settings(&sequence),
        json!({
            "source": "windows_update",
            "missing": "warn",
            "max_passes": 7,
            "exclude": [],
            "timeouts": { "search_secs": 3600, "download_secs": 7200, "install_secs": 7200 },
        })
    );
}

#[test]
fn update_timeouts_that_set_one_limit_keep_the_other_defaults() {
    let action = only_step_action(install_updates(json!({
        "timeouts": { "download_secs": 600 },
    })));

    let BuiltinAction::InstallUpdates { timeouts, .. } = action else {
        panic!("expected InstallUpdates, found {action:?}");
    };
    assert_eq!(
        timeouts,
        UpdateTimeouts {
            download_secs: secs(600),
            ..UpdateTimeouts::default()
        }
    );
}

#[test]
fn update_settings_with_a_wsus_server_and_both_kinds_of_exclusion_round_trip() {
    let settings = json!({
        "source": { "wsus": "https://wsus.example.com:8531" },
        "missing": "fail",
        "max_passes": 3,
        "exclude": [
            { "kb": 5034441 },
            { "update_id": "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21" },
        ],
        "timeouts": { "search_secs": 600, "download_secs": 1800, "install_secs": 3600 },
    });
    let document = install_updates(settings.clone());

    let sequence = parse(&document).expect("the document parses");

    let [Node::Step(step)] = sequence.root.children.as_slice() else {
        panic!("expected one step");
    };
    assert_eq!(
        step.action,
        Action::Builtin(BuiltinAction::InstallUpdates {
            source: UpdateSource::Wsus(
                "https://wsus.example.com:8531"
                    .parse()
                    .expect("a valid WSUS URL")
            ),
            missing: MissingUpdatePolicy::Fail,
            max_passes: passes(3),
            exclude: vec![
                UpdateExclusion::Kb(secs(5_034_441)),
                UpdateExclusion::UpdateId(Uuid::from_u128(0xe2b6a5f1_4c3d_4b2a_9f8e_7d6c5b4a3f21)),
            ],
            timeouts: UpdateTimeouts {
                search_secs: secs(600),
                download_secs: secs(1800),
                install_secs: secs(3600),
            },
        })
    );
    assert_eq!(written_settings(&sequence), settings);
    let written = serde_json::to_string(&sequence).expect("a task sequence serializes");
    assert_eq!(AuthoredSequence::from_json(&written), Ok(sequence));
}

#[test]
fn an_update_id_is_written_in_lower_case_without_braces() {
    for id in [
        "{E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21}",
        "E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21",
        "{e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21}",
    ] {
        let document = install_updates(json!({ "exclude": [{ "update_id": id }] }));

        let sequence = parse(&document).unwrap_or_else(|error| panic!("{id}: {error}"));

        assert_eq!(
            written_settings(&sequence)["exclude"],
            json!([{ "update_id": "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21" }]),
            "{id}"
        );
    }
}

#[test]
fn a_bad_update_setting_fails_and_the_error_names_its_field() {
    let cases = [
        (json!({ "max_passes": 0 }), ".max_passes"),
        (
            json!({ "timeouts": { "search_secs": 0 } }),
            ".timeouts.search_secs",
        ),
        (
            json!({ "timeouts": { "download_secs": 0 } }),
            ".timeouts.download_secs",
        ),
        (
            json!({ "timeouts": { "install_secs": 0 } }),
            ".timeouts.install_secs",
        ),
        (json!({ "source": { "wsus": "" } }), ".source.wsus"),
        (
            json!({ "source": { "wsus": "ftp://wsus.example.com" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "wsus.example.com:8531" } }),
            ".source.wsus",
        ),
        (json!({ "source": { "wsus": "https://" } }), ".source.wsus"),
        (
            json!({ "source": { "wsus": "https://:8531/" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com :8531" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com:8531\n" } }),
            ".source.wsus",
        ),
        // RFC 3986 allows a `\` or a `"` in no URI.
        (
            json!({ "source": { "wsus": "https://wsus.example.com\\path" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://ho\"st" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com/%zz" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com:notaport" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com:8531:99" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com:65536" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com:+8531" } }),
            ".source.wsus",
        ),
        // An IPv6 address outside brackets.
        (
            json!({ "source": { "wsus": "https://fd00::5" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://[fd00::5" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://[]:8531" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://[fd00::5]8531" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://[hello]:8531" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus[1].example.com" } }),
            ".source.wsus",
        ),
        (
            json!({ "source": { "wsus": "https://wsus.example.com/updates[1]" } }),
            ".source.wsus",
        ),
        // An `@` fails anywhere in the URL, even in the path.
        (
            json!({ "source": { "wsus": "https://wsus.example.com:8531/updates@site" } }),
            ".source.wsus",
        ),
        // A WSUS source without its URL.
        (json!({ "source": "wsus" }), ".source.wsus"),
        (json!({ "source": "microsoft_update" }), ".source"),
        (json!({ "source": { "microsoft_update": null } }), ".source"),
        (
            json!({ "source": { "windows_update": "x" } }),
            ".source.windows_update",
        ),
        (json!({ "missing": "retry" }), ".missing"),
        (json!({ "exclude": [{ "kb": 0 }] }), ".exclude[0].kb"),
        (
            json!({ "exclude": [{ "kb": "KB5034441" }] }),
            ".exclude[0].kb",
        ),
        (
            json!({ "exclude": [{ "kb": 5034441 }, { "update_id": "not-a-guid" }] }),
            ".exclude[1].update_id",
        ),
        (
            json!({ "exclude": [{ "update_id": "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f2" }] }),
            ".exclude[0].update_id",
        ),
    ];

    for (settings, field) in cases {
        let document = install_updates(settings);

        assert_fails_at(&document, &format!("{INSTALL_UPDATES}{field}"));
    }
}

#[test]
fn an_unknown_update_setting_fails_and_the_error_names_it() {
    let cases = [
        (json!({ "passes": 3 }), ".passes", "passes"),
        (
            json!({ "timeouts": { "scan_secs": 60 } }),
            ".timeouts.scan_secs",
            "scan_secs",
        ),
        // An unknown kind of exclusion fails at the exclusion.
        (
            json!({ "exclude": [{ "title": "x" }] }),
            ".exclude[0]",
            "title",
        ),
    ];

    for (settings, field, unknown) in cases {
        let document = install_updates(settings.clone());

        let error = assert_fails_at(&document, &format!("{INSTALL_UPDATES}{field}"));

        assert!(
            error.to_string().contains(&format!("`{unknown}`")),
            "{settings}: {error}"
        );
    }
}

#[test]
fn a_wsus_url_with_user_information_fails_and_the_error_holds_none_of_it() {
    let urls = [
        "https://admin:Hunter2@wsus.example.com:8531",
        "http://admin@wsus.example.com",
        "https://admin:Hunter2@wsus.example.com:8531/ClientWebService",
        "https://wsus.example.com\\admin:Hunter2@wsus.example.com",
        "ftp://admin:Hunter2@wsus.example.com",
        "https://admin:Hunter 2@wsus.example.com",
        // A `#`, `/`, or `?` in the password ends the authority early, so the
        // `@` falls after the authority.
        "https://admin:p#ss@wsus.example.com:8531",
        "https://admin:pa/ss@wsus.example.com:8531",
        "https://admin:p?ss@wsus.example.com",
        // The text before the `#` is a valid host and port here, `admin` with
        // the port `123` or an empty port.
        "https://admin:123#x@wsus.example.com",
        "https://admin:#x@wsus.example.com",
    ];
    let mut cases: Vec<(Value, &str)> = urls
        .iter()
        .map(|url| (json!({ "wsus": url }), ".source.wsus"))
        .collect();
    // The URL itself written as the source, where `{"wsus": ...}` belongs.
    cases.extend([
        (json!("https://admin:Hunter2@wsus.example.com"), ".source"),
        (
            json!({ "https://admin:Hunter2@wsus.example.com": null }),
            ".source",
        ),
        (
            json!({ "windows_update": "https://admin:Hunter2@wsus.example.com" }),
            ".source.windows_update",
        ),
    ]);

    for (source, field) in cases {
        let document = install_updates(json!({ "source": source }));

        let error = assert_fails_at(&document, &format!("{INSTALL_UPDATES}{field}"));

        for text in [error.to_string(), format!("{error:?}")] {
            assert!(
                !text.contains("admin") && !text.contains("Hunter"),
                "{source}: {text}"
            );
        }
    }
    for url in urls {
        let error = url.parse::<WsusUrl>().expect_err(url);
        for text in [error.to_string(), format!("{error:?}")] {
            assert!(
                !text.contains("admin") && !text.contains("Hunter"),
                "{url}: {text}"
            );
        }
    }
}

#[test]
fn a_wsus_url_with_an_at_sign_anywhere_fails() {
    for url in [
        "https://admin:Hunter2@wsus.example.com:8531",
        "https://admin:123#x@wsus.example.com",
        "https://admin:#x@wsus.example.com",
        "https://wsus.example.com:8531/updates@site",
        "https://wsus.example.com:8531/?site=a@b",
        "https://wsus.example.com:8531/#a@b",
    ] {
        assert_eq!(url.parse::<WsusUrl>(), Err(WsusUrlError::UserInfo), "{url}");
    }
}

#[test]
fn a_wsus_url_holds_only_the_characters_that_rfc_3986_allows() {
    // RFC 3986 allows these ASCII symbols in no URI.
    for symbol in ['"', '<', '>', '\\', '^', '`', '{', '|', '}'] {
        let url = format!("https://wsus.example.com/a{symbol}b");

        assert_eq!(
            url.parse::<WsusUrl>(),
            Err(WsusUrlError::InvalidChar),
            "{url}"
        );
    }
    // A `%` starts a percent-encoded byte, a `%` and two hex digits.
    for url in [
        "https://wsus.example.com/%",
        "https://wsus.example.com/%2",
        "https://wsus.example.com/%zz",
        "https://wsus.example.com/%%41",
    ] {
        assert_eq!(
            url.parse::<WsusUrl>(),
            Err(WsusUrlError::InvalidChar),
            "{url}"
        );
    }
}

#[test]
fn a_wsus_url_has_brackets_only_around_an_ipv6_address() {
    for url in [
        "https://[hello]:8531",
        "https://[10.0.0.5]:8531",
        // A future format with a version flag, which deployarr does not know.
        "https://[v1.fe]:8531",
        "https://wsus.example.com/updates[1]",
        "https://wsus.example.com/?site=[1]",
        "https://wsus.example.com/#[1]",
    ] {
        assert_eq!(
            url.parse::<WsusUrl>(),
            Err(WsusUrlError::IpLiteral),
            "{url}"
        );
    }
}

#[test]
fn a_wsus_url_is_an_http_or_https_url_that_names_a_host() {
    for url in [
        "http://wsus:8530",
        "https://wsus.example.com:8531",
        "https://wsus.example.com",
        "https://10.0.0.5:8531/",
        "https://[fd00::5]:8531",
        "https://[fd00::5]/",
        "https://[::ffff:10.0.0.5]:8531",
        "https://wsus.example.com:65535",
        // An empty port means the default port.
        "https://wsus.example.com:/",
        // Every symbol that RFC 3986 allows, except the `@` that the parser
        // rejects and the brackets that belong to an IP literal.
        "https://wsus.example.com/-._~:!$&'()*+,;=%2F%c3?q=/?#f/?",
    ] {
        let parsed: WsusUrl = url.parse().unwrap_or_else(|error| panic!("{url}: {error}"));

        assert_eq!(parsed.as_str(), url);
    }
}

#[test]
fn a_wsus_url_scheme_in_upper_case_is_written_in_lower_case() {
    for (url, written) in [
        (
            "HTTPS://WSUS.example.com:8531/Path",
            "https://WSUS.example.com:8531/Path",
        ),
        ("Http://wsus:8530", "http://wsus:8530"),
    ] {
        let document = install_updates(json!({ "source": { "wsus": url } }));

        let sequence = parse(&document).unwrap_or_else(|error| panic!("{url}: {error}"));

        assert_eq!(
            written_settings(&sequence)["source"],
            json!({ "wsus": written }),
            "{url}"
        );
        let text = serde_json::to_string(&sequence).expect("a task sequence serializes");
        assert_eq!(AuthoredSequence::from_json(&text), Ok(sequence), "{url}");
        let parsed: WsusUrl = url.parse().unwrap_or_else(|error| panic!("{url}: {error}"));
        assert_eq!(parsed.as_str(), written);
    }
}
