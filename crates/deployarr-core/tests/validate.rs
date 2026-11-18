//! `validate`, the save-time check of an authored sequence. It reports the
//! compiler's rules with an empty group in place of each add-on step and join
//! slot, and the rules about add-on steps, join slots, and the actions that
//! only an add-on's expansion may hold.

use std::collections::BTreeSet;

use deployarr_core::addon::AddOnRef;
use deployarr_core::compile::{NodePath, Rule};
use deployarr_core::join::JoinStage;
use deployarr_core::sequence::{AuthoredSequence, NodeId, RunIn};
use deployarr_core::validate::{Finding, Issue, Severity, validate};
use serde_json::{Value, json};

fn id_text(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn id(n: u32) -> NodeId {
    serde_json::from_value(json!(id_text(n))).expect("a valid node ID")
}

fn meta(n: u32) -> Value {
    json!({
        "id": id_text(n),
        "name": format!("Node {n}"),
        "enabled": true,
        "continue_on_error": false,
    })
}

fn step(n: u32, run_in: &str, action: Value) -> Value {
    json!({ "step": { "meta": meta(n), "run_in": run_in, "action": action } })
}

fn group(n: u32, children: Vec<Value>) -> Value {
    json!({ "group": { "meta": meta(n), "children": children } })
}

fn disabled(mut node: Value) -> Value {
    let key = if node.get("step").is_some() {
        "step"
    } else {
        "group"
    };
    node[key]["meta"]["enabled"] = json!(false);
    node
}

fn when(mut node: Value, condition: Value) -> Value {
    node["step"]["meta"]["condition"] = condition;
    node
}

fn sequence(children: Vec<Value>) -> AuthoredSequence {
    let document = json!({
        "schema_version": 1,
        "id": "TEST",
        "version": 1,
        "name": "Test",
        "default_vars": [],
        "root": { "meta": meta(1), "children": children },
    });
    AuthoredSequence::from_json(&document.to_string()).expect("the document parses")
}

fn add_on() -> Value {
    json!({
        "add_on": {
            "addon": "acme.autopilot",
            "kind": "register",
            "config_version": 1,
            "config": { "group_tag": "lab" },
        },
    })
}

fn register() -> AddOnRef {
    AddOnRef {
        addon: "acme.autopilot".parse().expect("a valid add-on ID"),
        kind: "register".parse().expect("a valid kind ID"),
    }
}

/// The step kinds that the add-on steps in these tests use, so that a test
/// sees no warning about an unknown kind.
fn known() -> BTreeSet<AddOnRef> {
    BTreeSet::from([register()])
}

fn join_slot(stage: &str) -> Value {
    json!({ "apply_join_profile": { "stage": stage } })
}

fn command() -> Value {
    json!({ "run_command": { "command": "cmd /c exit 0", "working_dir": null } })
}

fn partition() -> Value {
    json!({ "partition_disk": { "layout": "uefi-gpt" } })
}

fn apply_image() -> Value {
    json!({ "apply_os_image": { "image": "win11-24h2-pro-x64" } })
}

fn restart() -> Value {
    json!({ "restart": { "into": "installed_os" } })
}

fn server_call() -> Value {
    json!({ "server_call": { "send": ["HardwareHash"] } })
}

fn provisioning_package() -> Value {
    json!({ "add_provisioning_package": { "package": "enrollment_package" } })
}

fn wmi() -> Value {
    json!({
        "wmi": {
            "namespace": "root\\cimv2",
            "query": "SELECT * FROM Win32_Battery",
            "ignore_failure": true,
        },
    })
}

fn installed_msi() -> Value {
    json!({ "installed_msi": { "product_code": "23170f69-40c1-2702-2408-000001000000" } })
}

fn finding(n: u32, path: Vec<usize>, issue: Issue) -> Finding {
    Finding {
        node: id(n),
        path: NodePath::from(path),
        issue,
    }
}

fn stage(name: &str) -> JoinStage {
    serde_json::from_value(json!(name)).expect("a valid stage")
}

fn run_in(name: &str) -> RunIn {
    serde_json::from_value(json!(name)).expect("a valid run_in")
}

#[test]
fn a_run_in_that_the_action_does_not_allow_beside_an_add_on_step_is_an_error() {
    let sequence = sequence(vec![
        step(2, "full_os", add_on()),
        step(3, "full_os", apply_image()),
    ]);

    let findings = validate(&sequence, &known());

    assert_eq!(
        findings,
        [finding(
            3,
            vec![1],
            Issue::Compile(Rule::RunIn {
                action: "apply_os_image",
                run_in: RunIn::FullOs,
                allowed: &[RunIn::WinPe],
            }),
        )]
    );
    assert_eq!(findings[0].severity(), Severity::Error);
    assert_eq!(
        findings[0].to_string(),
        format!(
            "root.children[1], node {}: error: `apply_os_image` runs only with `run_in` `win_pe`, and this step has `full_os`",
            id_text(3)
        )
    );
}

#[test]
fn an_add_on_step_or_join_slot_that_shares_an_id_gets_the_duplicate_id_error() {
    let cases = [
        // A built-in step, then an add-on step with its ID.
        (
            sequence(vec![
                step(2, "either", command()),
                step(2, "full_os", add_on()),
            ]),
            finding(
                2,
                vec![1],
                Issue::Compile(Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                }),
            ),
        ),
        // An add-on step, then a disabled join slot with its ID in a group.
        (
            sequence(vec![
                step(2, "full_os", add_on()),
                group(3, vec![disabled(step(2, "full_os", join_slot("full_os")))]),
            ]),
            finding(
                2,
                vec![1, 0],
                Issue::Compile(Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                }),
            ),
        ),
    ];

    for (sequence, expected) in cases {
        let findings = validate(&sequence, &known());

        assert_eq!(findings, [expected]);
        assert!(
            findings[0]
                .to_string()
                .contains("the node at `root.children[0]` has the same ID"),
            "{}",
            findings[0]
        );
    }
}

#[test]
fn a_win_pe_step_with_an_installed_msi_condition_beside_an_add_on_step_is_an_error() {
    let sequence = sequence(vec![
        step(2, "full_os", add_on()),
        when(step(3, "win_pe", command()), installed_msi()),
    ]);

    assert_eq!(
        validate(&sequence, &known()),
        [finding(
            3,
            vec![1],
            Issue::Compile(Rule::FullOsCondition {
                condition: "installed_msi",
            }),
        )]
    );
}

#[test]
fn a_win_pe_restart_before_the_first_partition_beside_an_add_on_step_is_an_error() {
    let sequence = sequence(vec![
        step(2, "win_pe", add_on()),
        step(3, "win_pe", restart()),
        step(4, "win_pe", partition()),
    ]);

    assert_eq!(
        validate(&sequence, &known()),
        [finding(
            3,
            vec![1],
            Issue::Compile(Rule::RestartBeforePartition)
        )]
    );
}

#[test]
fn a_win_pe_add_on_step_or_join_slot_with_a_wmi_condition_gets_one_error_for_it() {
    let sequence = sequence(vec![
        when(step(2, "win_pe", add_on()), wmi()),
        disabled(when(step(3, "win_pe", join_slot("offline_image")), wmi())),
    ]);

    let findings = validate(&sequence, &known());

    let wmi = Issue::FullOsCondition { condition: "wmi" };
    assert_eq!(
        findings,
        [finding(2, vec![0], wmi.clone()), finding(3, vec![1], wmi)]
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.severity() == Severity::Error)
    );
}

#[test]
fn a_join_slot_whose_run_in_does_not_fit_its_stage_is_an_error() {
    let cases = [
        ("offline_image", "full_os", false),
        ("offline_image", "either", false),
        ("full_os", "win_pe", false),
        ("full_os", "either", false),
        ("offline_image", "win_pe", true),
        ("full_os", "full_os", true),
    ];

    for (written_stage, written_run_in, fits) in cases {
        for node in [
            step(2, written_run_in, join_slot(written_stage)),
            disabled(step(2, written_run_in, join_slot(written_stage))),
        ] {
            let sequence = sequence(vec![node.clone()]);

            let findings = validate(&sequence, &known());

            if fits {
                assert_eq!(findings, [], "{node}");
            } else {
                assert_eq!(
                    findings,
                    [finding(
                        2,
                        vec![0],
                        Issue::JoinStageRunIn {
                            stage: stage(written_stage),
                            run_in: run_in(written_run_in),
                        },
                    )],
                    "{node}"
                );
                assert_eq!(findings[0].severity(), Severity::Error);
            }
        }
    }
}

#[test]
fn an_authored_server_call_or_provisioning_package_is_an_error() {
    let sequence = sequence(vec![
        step(2, "full_os", server_call()),
        group(3, vec![disabled(step(4, "win_pe", provisioning_package()))]),
    ]);

    let findings = validate(&sequence, &known());

    assert_eq!(
        findings,
        [
            finding(
                2,
                vec![0],
                Issue::AddOnOnlyAction {
                    action: "server_call",
                },
            ),
            finding(
                4,
                vec![1, 0],
                Issue::AddOnOnlyAction {
                    action: "add_provisioning_package",
                },
            ),
        ]
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.severity() == Severity::Error)
    );
    assert_eq!(
        findings[0].to_string(),
        format!(
            "root.children[0], node {}: error: only an add-on step's expansion may hold `server_call`, so a task sequence cannot hold it as a step of its own",
            id_text(2)
        )
    );
}

#[test]
fn an_unknown_step_kind_alone_is_one_warning() {
    let sequence = sequence(vec![
        step(2, "win_pe", partition()),
        step(3, "full_os", add_on()),
        step(4, "full_os", join_slot("full_os")),
    ]);

    let findings = validate(&sequence, &BTreeSet::new());

    assert_eq!(
        findings,
        [finding(
            3,
            vec![1],
            Issue::UnknownStepKind { kind: register() }
        )]
    );
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn findings_come_in_document_order_with_the_steps_own_findings_first() {
    let sequence = sequence(vec![
        group(
            2,
            vec![
                when(step(3, "win_pe", add_on()), installed_msi()),
                step(4, "full_os", apply_image()),
            ],
        ),
        step(5, "win_pe", join_slot("full_os")),
        step(6, "either", server_call()),
        step(3, "full_os", add_on()),
    ]);

    let findings = validate(&sequence, &BTreeSet::new());

    let unknown = Issue::UnknownStepKind { kind: register() };
    assert_eq!(
        findings,
        [
            finding(
                3,
                vec![0, 0],
                Issue::FullOsCondition {
                    condition: "installed_msi",
                },
            ),
            finding(3, vec![0, 0], unknown.clone()),
            finding(
                4,
                vec![0, 1],
                Issue::Compile(Rule::RunIn {
                    action: "apply_os_image",
                    run_in: RunIn::FullOs,
                    allowed: &[RunIn::WinPe],
                }),
            ),
            finding(
                5,
                vec![1],
                Issue::JoinStageRunIn {
                    stage: JoinStage::FullOs,
                    run_in: RunIn::WinPe,
                },
            ),
            finding(
                6,
                vec![2],
                Issue::AddOnOnlyAction {
                    action: "server_call",
                },
            ),
            finding(3, vec![3], unknown),
            finding(
                3,
                vec![3],
                Issue::Compile(Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0, 0]),
                }),
            ),
        ]
    );
    assert_eq!(
        findings[3].to_string(),
        format!(
            "root.children[1], node {}: error: a join slot with `stage` `full_os` runs only with `run_in` `full_os`, and this step has `win_pe`",
            id_text(5)
        )
    );
}
