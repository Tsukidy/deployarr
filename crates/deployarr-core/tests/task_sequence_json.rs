use std::collections::BTreeSet;
use std::num::{NonZeroU8, NonZeroU32};

use deployarr_core::action::{
    Action, AppSelection, BuiltinAction, CheckMode, DiskCheck, DiskSelector, EncryptionMethod,
    ExitCodes, MemoryCheck, MissingUpdatePolicy, ProcessorCheck, ReadinessChecks, StepLimits,
    SuspendRestarts, SuspendRestartsError, TpmFailure, UpdateExclusion, UpdateSource,
    UpdateTimeouts, WsusUrl, WsusUrlError,
};
use deployarr_core::condition::{
    Comparison, Condition, EmptyNamespaceError, EmptyPathError, EmptyQueryError, EmptyRegKeyError,
    FileVersion, PathTemplate, QueryTemplate, RegData, RegKeyTemplate, RegText, RegType,
    RegValueName, RegistryCondition, RegistryConditionError, Test, WmiNamespace,
};
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
        BuiltinAction::EnableBitLocker { .. } => "EnableBitLocker",
        BuiltinAction::SuspendBitLocker { .. } => "SuspendBitLocker",
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
            "EnableBitLocker",
            "InstallApplications",
            "InstallUpdates",
            "PartitionDisk",
            "Restart",
            "RunCommand",
            "RunPowerShell",
            "SetVariable",
            "SuspendBitLocker",
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
            finish("/5/step/action/enable_bitlocker/method"),
            json!("aes"),
        ),
        (
            finish("/5/step/action/enable_bitlocker/full_disk"),
            json!("yes"),
        ),
        (
            finish("/5/step/action/enable_bitlocker/wait_secs"),
            json!(0),
        ),
        (
            finish("/5/step/action/enable_bitlocker/tpm_failure"),
            json!("warn"),
        ),
        (
            finish("/6/step/action/suspend_bitlocker/restarts"),
            json!(0),
        ),
        (
            finish("/6/step/action/suspend_bitlocker/restarts"),
            json!(16),
        ),
        (
            format!("{FINISH_CONDITION}/all/0/any/0/registry/test/greater_equal"),
            json!(true),
        ),
        (
            format!("{FINISH_CONDITION}/all/0/any/1/registry/value_type"),
            json!("reg_link"),
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

/// One of each kind of GUID field in the sample: a node ID, the two kinds of
/// MSI code, and an update ID.
const GUID_FIELDS: [&str; 5] = [
    "/root/meta/id",
    "/root/children/0/group/children/0/step/meta/id",
    "/root/children/1/group/children/2/step/meta/condition/not/installed_msi/product_code",
    "/root/children/1/group/children/7/step/meta/condition/installed_msi/upgrade_code",
    "/root/children/1/group/children/8/step/action/install_updates/exclude/1/update_id",
];

const GUID: &str = "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21";

#[test]
fn a_guid_field_reads_the_hyphenated_form_and_writes_it_in_lower_case_without_braces() {
    for pointer in GUID_FIELDS {
        for text in [
            "e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21",
            "{e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21}",
            "{E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21}",
        ] {
            let mut document = sample();
            *document
                .pointer_mut(pointer)
                .expect("the sample has the field") = json!(text);

            let sequence =
                parse(&document).unwrap_or_else(|error| panic!("{pointer} = {text}: {error}"));

            let written = serde_json::to_value(&sequence).expect("a task sequence serializes");
            assert_eq!(
                written.pointer(pointer),
                Some(&json!(GUID)),
                "{pointer} = {text}"
            );
        }
    }
}

#[test]
fn a_guid_field_rejects_the_urn_and_32_hex_forms_and_the_error_names_it() {
    for pointer in GUID_FIELDS {
        for text in [
            "urn:uuid:e2b6a5f1-4c3d-4b2a-9f8e-7d6c5b4a3f21",
            "URN:UUID:E2B6A5F1-4C3D-4B2A-9F8E-7D6C5B4A3F21",
            "e2b6a5f14c3d4b2a9f8e7d6c5b4a3f21",
            "{e2b6a5f14c3d4b2a9f8e7d6c5b4a3f21}",
        ] {
            let mut document = sample();
            *document
                .pointer_mut(pointer)
                .expect("the sample has the field") = json!(text);

            let error = parse(&document).expect_err(&format!("{pointer} = {text} must not parse"));

            assert_eq!(
                error.path(),
                field_path(pointer),
                "{pointer} = {text}: {error}"
            );
        }
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
        (
            "/root/children/1/group/children/5/step/action/enable_bitlocker",
            "drive",
        ),
        (
            "/root/children/1/group/children/6/step/action/suspend_bitlocker",
            "count",
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

/// A `registry` condition whose `key` is set, with `fields` after it. It is
/// text, because `Value` sorts object keys and these cases need their order.
fn registry_text(fields: &str) -> String {
    format!(r#"{{"registry": {{"key": "HKEY_LOCAL_MACHINE\\SOFTWARE\\Deployarr", {fields}}}}}"#)
}

/// Parses a sequence whose step has `condition`, written as text.
fn parse_with_condition(condition: &str) -> Result<AuthoredSequence, ParseError> {
    let text = sequence_with_condition(json!("CONDITION"))
        .to_string()
        .replace(r#""CONDITION""#, condition);
    AuthoredSequence::from_json(&text)
}

#[test]
fn a_registry_comparison_without_a_value_name_or_with_empty_text_fails() {
    const NO_NAME: &str = "a registry comparison needs a value name, because a key has no data to compare. `exists` without `value` tests the key";
    const EMPTY_NAME: &str = "a registry value name must not be empty";
    const EMPTY_DATA: &str = "the registry data to compare against must not be empty";
    let cases = [
        // A comparison with `value` left out fails at the condition itself.
        (r#""test": {"equals": 1}"#, "", NO_NAME),
        (r#""test": {"like": "Pro*"}"#, "", NO_NAME),
        // With `"value": null`, it fails at whichever of the two comes second.
        (
            r#""value": null, "test": {"equals": 1}"#,
            ".test.equals",
            NO_NAME,
        ),
        (r#""test": {"equals": 1}, "value": null"#, ".value", NO_NAME),
        // An empty value name fails for every test.
        (r#""value": "", "test": "exists""#, ".value", EMPTY_NAME),
        (
            r#""value": "", "test": {"equals": 1}"#,
            ".value",
            EMPTY_NAME,
        ),
        // Empty text data fails at its operator, `like` included.
        (
            r#""value": "Marker", "test": {"equals": ""}"#,
            ".test.equals",
            EMPTY_DATA,
        ),
        (
            r#""value": "Marker", "test": {"like": ""}"#,
            ".test.like",
            EMPTY_DATA,
        ),
    ];

    for (fields, rest, message) in cases {
        let condition = registry_text(fields);

        let error = parse_with_condition(&condition).expect_err(&condition);

        assert_eq!(
            error.path(),
            format!("{STEP}.meta.condition.registry{rest}"),
            "{condition}: {error}"
        );
        assert!(error.to_string().contains(message), "{condition}: {error}");
    }
}

#[test]
fn a_registry_key_test_needs_no_value_name() {
    for (test, expected) in [("exists", Test::Exists), ("not_exists", Test::NotExists)] {
        for fields in [
            format!(r#""test": "{test}""#),
            format!(r#""value": null, "test": "{test}""#),
            format!(r#""test": "{test}", "value": null"#),
        ] {
            let condition = registry_text(&fields);

            let sequence = parse_with_condition(&condition)
                .unwrap_or_else(|error| panic!("{condition}: {error}"));

            let [Node::Step(step)] = sequence.root.children.as_slice() else {
                panic!("expected one step");
            };
            let Some(Condition::Registry(registry)) = &step.meta.condition else {
                panic!("expected a Registry condition");
            };
            assert_eq!(registry.value(), None, "{condition}");
            assert_eq!(*registry.test(), expected, "{condition}");
            let written =
                serde_json::to_value(&step.meta.condition).expect("a condition serializes");
            assert_eq!(
                written,
                json!({ "registry": {
                    "key": "HKEY_LOCAL_MACHINE\\SOFTWARE\\Deployarr",
                    "value": null,
                    "value_type": null,
                    "test": test,
                } }),
                "{condition}"
            );
        }
    }
}

/// The key that the registry tests in code use.
fn deployarr_key() -> RegKeyTemplate {
    RegKeyTemplate::try_from(Template::new(r"HKEY_LOCAL_MACHINE\SOFTWARE\Deployarr"))
        .expect("a key that is not empty")
}

#[test]
fn registry_conditions_built_in_code_cannot_compare_a_key() {
    let key = deployarr_key();
    let pattern: RegText = "Pro*".parse().expect("text that is not empty");
    for test in [
        Test::Equals(RegData::Integer(1)),
        Test::Like(RegData::String(pattern)),
    ] {
        let error =
            RegistryCondition::new(key.clone(), None, None, test).expect_err("no value name");

        assert_eq!(error, RegistryConditionError::Comparison);
        assert!(error.to_string().contains("needs a value name"), "{error}");
    }

    for test in [Test::Exists, Test::NotExists] {
        let condition = RegistryCondition::new(key.clone(), None, None, test.clone())
            .expect("a test of the key");
        assert_eq!(condition.key(), &key);
        assert_eq!(condition.value(), None);
        assert_eq!(condition.value_type(), None);
        assert_eq!(condition.test(), &test);
    }

    let name: RegValueName = "Marker".parse().expect("a name that is not empty");
    let condition = RegistryCondition::new(
        key,
        Some(name.clone()),
        None,
        Test::Equals(RegData::Integer(1)),
    )
    .expect("a comparison that names a value");
    assert_eq!(condition.value(), Some(&name));
    assert!("".parse::<RegValueName>().is_err());
    assert!("".parse::<RegText>().is_err());
}

/// The message that a value type with no value name fails with, in code and
/// in the parser.
const TYPE_WITHOUT_NAME: &str = "a registry value type needs a value name, because a key has no type. Leave out `value_type` to test the key";

#[test]
fn registry_conditions_built_in_code_cannot_give_a_key_a_value_type() {
    let key = deployarr_key();
    for test in [Test::Exists, Test::NotExists] {
        let error = RegistryCondition::new(key.clone(), None, Some(RegType::Sz), test)
            .expect_err("a value type without a value name");

        assert_eq!(error, RegistryConditionError::ValueType);
        assert_eq!(error.to_string(), TYPE_WITHOUT_NAME);
    }

    let name: RegValueName = "Marker".parse().expect("a name that is not empty");
    for test in [Test::Exists, Test::Equals(RegData::Integer(1))] {
        let condition =
            RegistryCondition::new(key.clone(), Some(name.clone()), Some(RegType::Dword), test)
                .expect("a value type that names a value");
        assert_eq!(condition.value(), Some(&name));
        assert_eq!(condition.value_type(), Some(RegType::Dword));
    }
}

/// A comparison and a value type without a value name break both rules.
/// `RegistryCondition::new` reports the comparison.
#[test]
fn registry_conditions_built_in_code_report_a_comparison_before_a_value_type() {
    let key = deployarr_key();
    for test in [
        Test::Equals(RegData::Integer(1)),
        Test::Like(RegData::String("Pro*".parse().expect("text"))),
    ] {
        let error = RegistryCondition::new(key.clone(), None, Some(RegType::Sz), test)
            .expect_err("a comparison and a value type without a value name");

        assert_eq!(error, RegistryConditionError::Comparison);
    }
}

/// With a comparison and a value type but no value name, the parser reports
/// the problem that it finds first, and the comparison when it finds both at
/// once.
#[test]
fn a_registry_comparison_and_value_type_without_a_value_name_fail_at_the_first_problem() {
    let comparison = RegistryConditionError::Comparison.to_string();
    let cases = [
        // With `value` left out, both show at the end.
        (
            r#""value_type": "reg_sz", "test": {"equals": 1}"#,
            "",
            comparison.as_str(),
        ),
        // `value` comes last, after both.
        (
            r#""value_type": "reg_sz", "test": {"equals": 1}, "value": null"#,
            ".value",
            comparison.as_str(),
        ),
        // `value_type` comes before `test`, after `value`.
        (
            r#""value": null, "value_type": "reg_sz", "test": {"equals": 1}"#,
            ".value_type",
            TYPE_WITHOUT_NAME,
        ),
        // `test` comes before `value_type`, after `value`.
        (
            r#""value": null, "test": {"equals": 1}, "value_type": "reg_sz""#,
            ".test.equals",
            comparison.as_str(),
        ),
        // Only `value_type` comes before `value`.
        (
            r#""value_type": "reg_sz", "value": null, "test": {"equals": 1}"#,
            ".value",
            TYPE_WITHOUT_NAME,
        ),
    ];

    for (fields, rest, message) in cases {
        let condition = registry_text(fields);

        let error = parse_with_condition(&condition).expect_err(&condition);

        assert_eq!(
            error.path(),
            format!("{STEP}.meta.condition.registry{rest}"),
            "{condition}: {error}"
        );
        assert!(error.to_string().contains(message), "{condition}: {error}");
    }
}

#[test]
fn a_registry_value_type_without_a_value_name_fails() {
    let cases = [
        // With `value` left out, it fails at the condition itself.
        (r#""value_type": "reg_sz", "test": "exists""#, ""),
        (r#""test": "not_exists", "value_type": "reg_dword""#, ""),
        // With `"value": null`, it fails at whichever of the two comes second.
        (
            r#""value": null, "value_type": "reg_sz", "test": "exists""#,
            ".value_type",
        ),
        (
            r#""value_type": "reg_sz", "value": null, "test": "exists""#,
            ".value",
        ),
        (
            r#""test": "exists", "value_type": "reg_none", "value": null"#,
            ".value",
        ),
    ];

    for (fields, rest) in cases {
        let condition = registry_text(fields);

        let error = parse_with_condition(&condition).expect_err(&condition);

        assert_eq!(
            error.path(),
            format!("{STEP}.meta.condition.registry{rest}"),
            "{condition}: {error}"
        );
        assert!(
            error.to_string().contains(TYPE_WITHOUT_NAME),
            "{condition}: {error}"
        );
    }
}

/// The value type of the one `registry` condition in `sequence`.
fn only_value_type(sequence: &AuthoredSequence) -> Option<RegType> {
    let [Node::Step(step)] = sequence.root.children.as_slice() else {
        panic!("expected one step");
    };
    let Some(Condition::Registry(registry)) = &step.meta.condition else {
        panic!("expected a Registry condition");
    };
    registry.value_type()
}

/// The `value_type` field that `sequence` writes for its one condition.
fn written_value_type(sequence: &AuthoredSequence) -> Value {
    let written = serde_json::to_value(sequence).expect("a task sequence serializes");
    let registry = &written["root"]["children"][0]["step"]["meta"]["condition"]["registry"];
    registry
        .get("value_type")
        .cloned()
        .unwrap_or_else(|| panic!("{registry} has no value_type"))
}

#[test]
fn a_registry_value_type_left_out_or_null_names_no_type() {
    for fields in [
        r#""value": "Marker", "test": "exists""#,
        r#""value": "Marker", "value_type": null, "test": "exists""#,
        r#""value": "Marker", "test": {"equals": 1}, "value_type": null"#,
        r#""value_type": null, "test": "exists""#,
        r#""value": null, "value_type": null, "test": "exists""#,
        r#""value_type": null, "value": null, "test": "not_exists""#,
    ] {
        let condition = registry_text(fields);

        let sequence =
            parse_with_condition(&condition).unwrap_or_else(|error| panic!("{condition}: {error}"));

        assert_eq!(only_value_type(&sequence), None, "{condition}");
        assert_eq!(written_value_type(&sequence), json!(null), "{condition}");
    }
}

#[test]
fn each_registry_value_type_parses_and_is_written_back_unchanged() {
    let cases = [
        ("reg_none", RegType::None),
        ("reg_sz", RegType::Sz),
        ("reg_expand_sz", RegType::ExpandSz),
        ("reg_binary", RegType::Binary),
        ("reg_dword", RegType::Dword),
        ("reg_multi_sz", RegType::MultiSz),
        ("reg_qword", RegType::Qword),
    ];

    for (name, expected) in cases {
        let condition = registry_text(&format!(
            r#""value": "Marker", "value_type": "{name}", "test": "exists""#
        ));

        let sequence =
            parse_with_condition(&condition).unwrap_or_else(|error| panic!("{condition}: {error}"));

        assert_eq!(only_value_type(&sequence), Some(expected), "{name}");
        assert_eq!(written_value_type(&sequence), json!(name), "{name}");
    }
}

#[test]
fn an_unknown_registry_value_type_fails_at_its_field() {
    for name in [
        json!("reg_link"),
        json!("REG_SZ"),
        json!("Reg_Sz"),
        json!("sz"),
        json!(""),
        json!(1),
    ] {
        let document = sequence_with_condition(json!({
            "registry": {
                "key": "HKEY_LOCAL_MACHINE\\SOFTWARE\\Deployarr",
                "value": "Marker",
                "value_type": name,
                "test": "exists",
            },
        }));

        assert_fails_at(
            &document,
            &format!("{STEP}.meta.condition.registry.value_type"),
        );
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

/// MDT fails a condition whose path, key, or query is empty after it expands
/// and trims it. The File check is at `TsCore!0x1801a2c77`, the Folder check
/// at `TsCore!0x1801a3eb6`, the Registry check at `TsCore!0x1801a747a`, and
/// the WMI check at `TsCore!0x1801aa872`. deployarr's parser rejects the empty
/// text when the sequence is saved, as `docs/architecture.md#json-format`
/// says.
#[test]
fn an_empty_path_key_or_query_fails_at_its_field() {
    let cases = [
        (
            json!({ "file": { "path": "" } }),
            "file.path",
            "a file or folder path must not be empty",
        ),
        (
            json!({ "folder": { "path": "" } }),
            "folder.path",
            "a file or folder path must not be empty",
        ),
        (
            json!({ "registry": { "key": "", "test": "exists" } }),
            "registry.key",
            "a registry key must not be empty",
        ),
        (
            json!({ "wmi": { "namespace": "root\\cimv2", "query": "", "ignore_failure": true } }),
            "wmi.query",
            "a WMI query must not be empty",
        ),
    ];

    for (condition, path, message) in cases {
        let document = sequence_with_condition(condition);

        let error = assert_fails_at(&document, &format!("{STEP}.meta.condition.{path}"));
        assert!(error.to_string().contains(message), "{path}: {error}");
    }
}

/// The path, key, and query types reject empty text, so code that builds a
/// sequence cannot hold an empty one. Each error names its field. Text of
/// only white space is not empty.
#[test]
fn path_key_and_query_types_reject_empty_text() {
    let empty = Template::new("");
    assert_eq!(PathTemplate::try_from(empty.clone()), Err(EmptyPathError));
    assert_eq!(
        RegKeyTemplate::try_from(empty.clone()),
        Err(EmptyRegKeyError)
    );
    assert_eq!(QueryTemplate::try_from(empty), Err(EmptyQueryError));
    assert_eq!(
        EmptyPathError.to_string(),
        "a file or folder path must not be empty"
    );
    assert_eq!(
        EmptyRegKeyError.to_string(),
        "a registry key must not be empty"
    );
    assert_eq!(EmptyQueryError.to_string(), "a WMI query must not be empty");

    let space = Template::new(" ");
    let path = PathTemplate::try_from(space.clone()).expect("a space is not empty");
    let key = RegKeyTemplate::try_from(space.clone()).expect("a space is not empty");
    let query = QueryTemplate::try_from(space.clone()).expect("a space is not empty");
    assert_eq!(path.as_template(), &space);
    assert_eq!(key.as_str(), " ");
    assert_eq!(Template::from(query), space);
}

/// MDT fails a WMI condition whose namespace is empty after it expands and
/// trims it, before it opens the namespace (`TsCore!0x1801aa7c6`). deployarr's
/// parser rejects an empty namespace when the sequence is saved, as
/// `docs/architecture.md#json-format` says.
#[test]
fn an_empty_wmi_namespace_fails_at_its_field() {
    let condition = json!({ "wmi": { "namespace": "", "query": "SELECT * FROM Win32_Battery", "ignore_failure": true } });
    let document = sequence_with_condition(condition);

    let error = assert_fails_at(&document, &format!("{STEP}.meta.condition.wmi.namespace"));
    assert!(
        error
            .to_string()
            .contains("a WMI namespace must not be empty"),
        "{error}"
    );
}

/// The namespace type rejects empty text, so code that builds a sequence
/// cannot hold an empty namespace. Text of only white space is not empty.
#[test]
fn the_namespace_type_rejects_empty_text() {
    assert_eq!("".parse::<WmiNamespace>(), Err(EmptyNamespaceError));
    assert_eq!(
        WmiNamespace::try_from(String::new()),
        Err(EmptyNamespaceError)
    );
    assert_eq!(
        EmptyNamespaceError.to_string(),
        "a WMI namespace must not be empty"
    );

    let space = " ".parse::<WmiNamespace>().expect("a space is not empty");
    assert_eq!(space.as_str(), " ");
    let namespace = r"root\cimv2".parse::<WmiNamespace>().expect("a namespace");
    assert_eq!(namespace.as_str(), r"root\cimv2");
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
    let Some(Condition::Registry(registry)) = &step.meta.condition else {
        panic!("expected a Registry condition");
    };
    assert_eq!(*registry.test(), Test::Equals(RegData::Integer(u64::MAX)));
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
        // A fragment holds no `#`.
        (
            json!({ "source": { "wsus": "https://wsus.example.com/#a#b" } }),
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
        // A bracket in the port.
        "https://wsus.example.com:85]",
        "https://wsus.example.com:85[",
        "https://[fd00::5]:85]",
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
fn a_wsus_url_has_at_most_one_number_sign() {
    // The first `#` starts the fragment, and a fragment holds no `#`.
    for url in [
        "https://wsus.example.com/#a#b",
        "https://wsus.example.com/?a#b#c",
        "https://wsus.example.com##",
        "https://wsus.example.com:8531/updates#a/?#",
    ] {
        assert_eq!(url.parse::<WsusUrl>(), Err(WsusUrlError::Fragment), "{url}");
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

// The two BitLocker actions. See the architecture's BitLocker section.

/// A sequence whose root holds one full-OS step for each action. Every node
/// writes every field, so the serializer writes the document back unchanged.
fn sequence_with_full_os_steps(actions: Vec<Value>) -> Value {
    let meta = |n: u32| {
        json!({
            "id": format!("00000000-0000-4000-8000-{n:012x}"),
            "name": format!("Node {n}"),
            "enabled": true,
            "continue_on_error": false,
            "condition": null,
        })
    };
    let children: Vec<Value> = (2..)
        .zip(actions)
        .map(|(n, action)| json!({ "step": { "meta": meta(n), "run_in": "full_os", "action": action } }))
        .collect();
    json!({
        "schema_version": 1,
        "id": "BITLOCKER",
        "version": 1,
        "name": "BitLocker",
        "default_vars": [],
        "root": { "meta": meta(1), "children": children },
    })
}

fn builtin_actions(sequence: &AuthoredSequence) -> Vec<BuiltinAction> {
    sequence
        .root
        .children
        .iter()
        .map(|child| match child {
            Node::Step(step) => match &step.action {
                Action::Builtin(action) => action.clone(),
                other => panic!("expected a built-in action, found {other:?}"),
            },
            Node::Group(_) => panic!("expected only steps"),
        })
        .collect()
}

#[test]
fn bitlocker_actions_with_every_field_set_round_trip_unchanged() {
    let methods = [
        ("os_default", EncryptionMethod::OsDefault),
        ("aes128", EncryptionMethod::Aes128),
        ("aes256", EncryptionMethod::Aes256),
        ("xts_aes128", EncryptionMethod::XtsAes128),
        ("xts_aes256", EncryptionMethod::XtsAes256),
    ];
    let failures = [("fail", TpmFailure::Fail), ("skip", TpmFailure::Skip)];
    let mut actions = Vec::new();
    let mut expected = Vec::new();
    for (index, (method_name, method)) in (0_u32..).zip(methods) {
        let (failure_name, tpm_failure) = failures[(index % 2) as usize];
        let full_disk = index % 2 == 1;
        // None for the first step, then 600, 1200, and so on.
        let wait_secs = NonZeroU32::new(index * 600);
        actions.push(json!({
            "enable_bitlocker": {
                "method": method_name,
                "full_disk": full_disk,
                "wait_secs": wait_secs,
                "tpm_failure": failure_name,
            },
        }));
        expected.push(BuiltinAction::EnableBitLocker {
            method,
            full_disk,
            wait_secs,
            tpm_failure,
        });
    }
    for count in [1, 7, 15] {
        actions.push(json!({ "suspend_bitlocker": { "restarts": count } }));
        expected.push(BuiltinAction::SuspendBitLocker {
            restarts: SuspendRestarts::new(count).expect("a count from 1 to 15"),
        });
    }
    let document = sequence_with_full_os_steps(actions);

    let sequence = parse(&document).expect("the document parses");

    assert_eq!(builtin_actions(&sequence), expected);
    let written = serde_json::to_value(&sequence).expect("a task sequence serializes");
    assert_eq!(written, document);
    assert_eq!(parse(&written), Ok(sequence));
}

#[test]
fn omitted_bitlocker_fields_take_their_defaults_and_are_written_back() {
    let document = sequence_with_full_os_steps(vec![
        json!({ "enable_bitlocker": {} }),
        json!({ "suspend_bitlocker": {} }),
    ]);

    let sequence = parse(&document).expect("the document parses");

    assert_eq!(
        builtin_actions(&sequence),
        [
            BuiltinAction::EnableBitLocker {
                method: EncryptionMethod::OsDefault,
                full_disk: false,
                wait_secs: None,
                tpm_failure: TpmFailure::Fail,
            },
            BuiltinAction::SuspendBitLocker {
                restarts: SuspendRestarts::new(1).expect("a count from 1 to 15"),
            },
        ]
    );
    assert_eq!(EncryptionMethod::default(), EncryptionMethod::OsDefault);
    assert_eq!(TpmFailure::default(), TpmFailure::Fail);
    assert_eq!(SuspendRestarts::default().get(), 1);

    let written = serde_json::to_value(&sequence).expect("a task sequence serializes");
    let children = &written["root"]["children"];
    assert_eq!(
        children[0]["step"]["action"],
        json!({
            "enable_bitlocker": {
                "method": "os_default",
                "full_disk": false,
                "wait_secs": null,
                "tpm_failure": "fail",
            },
        })
    );
    assert_eq!(
        children[1]["step"]["action"],
        json!({ "suspend_bitlocker": { "restarts": 1 } })
    );
}

const RESTARTS: &str = "root.children[0].step.action.suspend_bitlocker.restarts";

#[test]
fn a_suspend_count_outside_1_to_15_fails_at_restarts() {
    for count in [json!(0), json!(16), json!(255), json!(256), json!(-1)] {
        let document = sequence_with_action(json!({ "suspend_bitlocker": { "restarts": count } }));

        assert_fails_at(&document, RESTARTS);
    }
    for count in [0, 16] {
        let document = sequence_with_action(json!({ "suspend_bitlocker": { "restarts": count } }));

        let error = assert_fails_at(&document, RESTARTS);

        assert!(
            error.to_string().contains("1 to 15 restarts"),
            "{count}: {error}"
        );
    }
    for count in [1, 15] {
        let action = only_step_action(sequence_with_action(json!({
            "suspend_bitlocker": { "restarts": count },
        })));

        assert_eq!(
            action,
            BuiltinAction::SuspendBitLocker {
                restarts: SuspendRestarts::new(count).expect("a count from 1 to 15"),
            }
        );
    }
}

#[test]
fn suspend_restarts_built_in_code_holds_1_to_15() {
    for count in [0, 16, u8::MAX] {
        let error = SuspendRestarts::new(count).expect_err("a count outside 1 to 15");

        assert_eq!(error, SuspendRestartsError { count });
        assert_eq!(
            error.to_string(),
            format!("BitLocker stays suspended for 1 to 15 restarts, not {count}")
        );
    }
    for count in 1..=15 {
        let restarts = SuspendRestarts::new(count).expect("a count from 1 to 15");

        assert_eq!(restarts.get(), count);
    }
}

#[test]
fn a_bitlocker_wait_of_zero_seconds_fails() {
    let document = sequence_with_action(json!({ "enable_bitlocker": { "wait_secs": 0 } }));

    assert_fails_at(
        &document,
        "root.children[0].step.action.enable_bitlocker.wait_secs",
    );
}

#[test]
fn an_unknown_field_on_a_bitlocker_action_fails_and_the_error_names_it() {
    let cases = [
        (json!({ "enable_bitlocker": { "drive": "C:" } }), "drive"),
        (
            json!({ "enable_bitlocker": { "recovery_password": "x" } }),
            "recovery_password",
        ),
        (json!({ "suspend_bitlocker": { "count": 2 } }), "count"),
    ];

    for (action, field) in cases {
        let error = parse(&sequence_with_action(action.clone()))
            .expect_err(&format!("{action} must not parse"));

        assert!(error.to_string().contains(&format!("`{field}`")), "{error}");
    }
}
