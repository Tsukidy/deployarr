use std::collections::BTreeSet;
use std::num::NonZeroU32;

use deployarr_core::action::{
    Action, CheckMode, DiskCheck, DiskSelector, ExitCodes, MemoryCheck, ProcessorCheck,
    ReadinessChecks, StepLimits,
};
use deployarr_core::condition::{Comparison, Condition, FileVersion};
use deployarr_core::sequence::{Node, NodeMeta, TaskSequence, TaskSequenceId};
use deployarr_core::variable::VarName;
use serde_json::{Value, json};

const SAMPLE: &str = include_str!("data/windows-11-pro.json");

const INSTALL: &str = "/root/children/0/group/children";
const FINISH: &str = "/root/children/1/group/children";
const FINISH_CONDITION: &str = "/root/children/1/group/meta/condition";

fn sample() -> Value {
    serde_json::from_str(SAMPLE).expect("the sample is JSON")
}

fn parse(document: &Value) -> Result<TaskSequence, deployarr_core::sequence::ParseError> {
    TaskSequence::from_json(&document.to_string())
}

#[test]
fn sample_round_trips_unchanged() {
    let sequence = TaskSequence::from_json(SAMPLE).expect("the sample parses");

    let written = serde_json::to_string(&sequence).expect("a task sequence serializes");

    let written_value: Value = serde_json::from_str(&written).expect("the output is JSON");
    assert_eq!(written_value, sample());
    assert_eq!(TaskSequence::from_json(&written), Ok(sequence));
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::RunCommand { .. } => "RunCommand",
        Action::RunPowerShell { .. } => "RunPowerShell",
        Action::SetVariable { .. } => "SetVariable",
        Action::Restart { .. } => "Restart",
        Action::PartitionDisk { .. } => "PartitionDisk",
        Action::ApplyOsImage { .. } => "ApplyOsImage",
        Action::ApplyDrivers { .. } => "ApplyDrivers",
        Action::ApplyUnattend { .. } => "ApplyUnattend",
        Action::InstallApplications { .. } => "InstallApplications",
        Action::VerifyDrivers { .. } => "VerifyDrivers",
        Action::CheckReadiness { .. } => "CheckReadiness",
        Action::ActivateWindows { .. } => "ActivateWindows",
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

    fn visit(&mut self, node: &Node) {
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

/// The round trip only proves what the sample holds, so the sample must hold
/// every variant. A new variant fails to compile in the matches above until it
/// has a name, and then fails here until the sample uses it.
#[test]
fn sample_uses_every_action_and_condition_variant() {
    let sequence = TaskSequence::from_json(SAMPLE).expect("the sample parses");
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

fn only_step_action(document: Value) -> Action {
    let sequence = parse(&document).expect("the document parses");
    match sequence.root.children.as_slice() {
        [Node::Step(step)] => step.action.clone(),
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

    let Action::RunCommand {
        working_dir,
        exit,
        limits,
        ..
    } = action
    else {
        panic!("expected RunCommand, found {action:?}");
    };
    assert_eq!(working_dir, None);
    assert_eq!(
        exit,
        ExitCodes {
            success: vec![0],
            reboot: vec![3010, 1641],
        }
    );
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
        Action::CheckReadiness {
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
        Action::CheckReadiness {
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

    let Action::PartitionDisk { disk, .. } = action else {
        panic!("expected PartitionDisk, found {action:?}");
    };
    assert_eq!(disk, DiskSelector::FirstInternal);
}

#[test]
fn a_runtime_limit_is_whole_seconds() {
    let action = only_step_action(sequence_with_action(json!({
        "run_command": { "command": "setup.exe", "limits": { "max_runtime_secs": 90 } },
    })));

    let Action::RunCommand { limits, .. } = action else {
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

/// Turns a JSON pointer such as `/root/children/0/group` into the path that a
/// parse error reports, `root.children[0].group`.
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
        (install("/1/step/meta/condition/var/op"), json!("equal")),
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
            format!("{FINISH_CONDITION}/all/0/any/0/registry/data"),
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
    assert!(TaskSequence::from_json("").is_err());
    assert!(TaskSequence::from_json("{\"schema_version\": 1,").is_err());
    assert!(TaskSequence::from_json(&format!("{SAMPLE} {SAMPLE}")).is_err());
}
