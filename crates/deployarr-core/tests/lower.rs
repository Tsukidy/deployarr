//! `deferred` and `lower`, which put the built-in steps that add-ons return in
//! place of add-on steps and join slots.

use deployarr_core::action::{Action, BuiltinAction, SecretSource};
use deployarr_core::compile::compile;
use deployarr_core::join::JoinStage;
use deployarr_core::lower::{DeferredKind, Expansions, LowerError, LowerRule, deferred, lower};
use deployarr_core::sequence::{AuthoredSequence, Group, Node, NodeId, RunIn, Step};
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

fn disabled(mut node: Value) -> Value {
    let key = if node.get("step").is_some() {
        "step"
    } else {
        "group"
    };
    node[key]["meta"]["enabled"] = json!(false);
    node
}

fn group(n: u32, children: Vec<Value>) -> Value {
    json!({ "group": { "meta": meta(n), "children": children } })
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

fn join_slot(stage: &str) -> Value {
    json!({ "apply_join_profile": { "stage": stage } })
}

fn command() -> Value {
    json!({ "run_command": { "command": "cmd /c exit 0", "working_dir": null } })
}

/// A step of an expansion. Its ID is `n`, which `lower` replaces.
fn child(n: u32, run_in: &str, action: Value) -> Step<Action> {
    let value = json!({ "meta": meta(n), "run_in": run_in, "action": action });
    serde_json::from_value(value).expect("the step parses")
}

fn collect_hash() -> Value {
    json!({ "run_powershell": { "script": "assets/collect-hash.ps1", "args": [] } })
}

fn server_call() -> Value {
    json!({ "server_call": { "send": ["HardwareHash"] } })
}

fn provisioning_package() -> Value {
    package_from("step_config")
}

/// An `AddProvisioningPackage` whose package is the `package` field of
/// `source`.
fn package_from(source: &str) -> Value {
    json!({
        "add_provisioning_package": {
            "package": { "source": source, "field": "package" },
        },
    })
}

/// The root holds a built-in step, the add-on step 2, and the join slot 3.
fn planned() -> (AuthoredSequence, Expansions) {
    let sequence = sequence(vec![
        step(5, "win_pe", command()),
        step(2, "full_os", add_on()),
        step(3, "full_os", join_slot("full_os")),
    ]);
    let expansions = Expansions::from([
        (
            id(2),
            vec![
                child(0x100, "full_os", collect_hash()),
                child(0x101, "full_os", server_call()),
            ],
        ),
        (id(3), Vec::new()),
    ]);
    (sequence, expansions)
}

#[test]
fn deferred_lists_add_on_steps_and_join_slots_in_document_order() {
    let (sequence, _) = planned();

    let found = deferred(&sequence);

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].node.id, id(2));
    assert_eq!(found[0].run_in, RunIn::FullOs);
    let DeferredKind::AddOn(step) = found[0].what else {
        panic!("expected an add-on step, found {:?}", found[0].what);
    };
    assert_eq!(step.kind.as_str(), "register");
    assert_eq!(found[1].node.id, id(3));
    assert_eq!(found[1].what, DeferredKind::Join(JoinStage::FullOs));
}

#[test]
fn deferred_skips_disabled_steps_and_steps_in_disabled_groups() {
    let sequence = sequence(vec![
        disabled(step(2, "full_os", add_on())),
        disabled(group(3, vec![step(4, "full_os", add_on())])),
        group(6, vec![step(7, "win_pe", join_slot("offline_image"))]),
    ]);

    let found: Vec<NodeId> = deferred(&sequence).iter().map(|d| d.node.id).collect();

    assert_eq!(found, [id(7)]);
}

#[test]
fn lower_puts_each_expansion_in_a_group_with_the_steps_meta() {
    let (sequence, expansions) = planned();

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    let [Node::Step(first), Node::Group(register), Node::Group(join)] =
        lowered.root.children.as_slice()
    else {
        panic!("expected a step and two groups, found {:?}", lowered.root);
    };
    assert_eq!(first.meta.id, id(5));
    let Node::Step(authored) = &sequence.root.children[1] else {
        panic!("the second node is a step");
    };
    assert_eq!(register.meta, authored.meta);
    assert_eq!(join.meta.id, id(3));
    assert_eq!(join.children, []);

    let [Node::Step(hash), Node::Step(call)] = register.children.as_slice() else {
        panic!("expected two steps, found {:?}", register.children);
    };
    // Version 5 UUIDs from the step's ID and the names "0" and "1", worked
    // out with Python's `uuid.uuid5`.
    assert_eq!(
        hash.meta.id.0.to_string(),
        "b71c7c4b-a648-5a37-a8cc-af9d0f7b34c3"
    );
    assert_eq!(
        call.meta.id.0.to_string(),
        "6aae72a3-38e4-5200-9127-71721dc9bb8b"
    );
    assert_eq!(hash.meta.name, "Node 256");
    assert_eq!(hash.run_in, RunIn::FullOs);
    assert!(matches!(hash.action, BuiltinAction::RunPowerShell { .. }));
    assert!(matches!(call.action, BuiltinAction::ServerCall { .. }));

    compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn the_same_input_gives_the_same_node_ids() {
    let (sequence, expansions) = planned();

    let first = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));
    let second = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    assert_eq!(first, second);
    assert_eq!(compile(&first), compile(&second));
}

#[test]
fn a_child_keeps_its_own_meta_except_its_id() {
    let (sequence, mut expansions) = planned();
    let children = expansions.get_mut(&id(2)).expect("the add-on's expansion");
    children[0].meta.continue_on_error = true;
    children[0].meta.enabled = false;

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    let Node::Group(register) = &lowered.root.children[1] else {
        panic!("expected a group");
    };
    let Node::Step(hash) = &register.children[0] else {
        panic!("expected a step");
    };
    assert!(hash.meta.continue_on_error);
    assert!(!hash.meta.enabled);
}

#[track_caller]
fn assert_rejects(sequence: &AuthoredSequence, expansions: &Expansions, expected: &[LowerError]) {
    match lower(sequence, expansions) {
        Ok(lowered) => panic!("expected errors, lowered {lowered:?}"),
        Err(errors) => assert_eq!(errors.as_slice(), expected),
    }
}

fn error(n: u32, rule: LowerRule) -> LowerError {
    LowerError { node: id(n), rule }
}

#[test]
fn lower_rejects_a_missing_expansion() {
    let (sequence, mut expansions) = planned();
    expansions.remove(&id(2));

    assert_rejects(
        &sequence,
        &expansions,
        &[error(2, LowerRule::MissingExpansion)],
    );
}

#[test]
fn lower_rejects_an_action_outside_the_allowed_six() {
    let (sequence, mut expansions) = planned();
    expansions.insert(
        id(2),
        vec![
            child(0x100, "full_os", collect_hash()),
            child(
                0x101,
                "full_os",
                json!({ "install_applications": { "apps": { "one": "7zip" } } }),
            ),
        ],
    );

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            2,
            LowerRule::NotAllowed {
                child: 1,
                action: "install_applications",
            },
        )],
    );
}

#[test]
fn lower_rejects_an_install_updates_step_in_an_expansion() {
    let (sequence, mut expansions) = planned();
    expansions.insert(
        id(2),
        vec![child(
            0x100,
            "full_os",
            json!({ "install_updates": { "source": "windows_update" } }),
        )],
    );

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            2,
            LowerRule::NotAllowed {
                child: 0,
                action: "install_updates",
            },
        )],
    );
}

#[test]
fn lower_rejects_an_expansion_that_holds_a_bitlocker_action() {
    let cases = [
        (json!({ "enable_bitlocker": {} }), "enable_bitlocker"),
        (
            json!({ "suspend_bitlocker": { "restarts": 1 } }),
            "suspend_bitlocker",
        ),
    ];

    for (action, name) in cases {
        let (sequence, mut expansions) = planned();
        expansions.insert(
            id(2),
            vec![
                child(0x100, "full_os", collect_hash()),
                child(0x101, "full_os", action),
            ],
        );

        assert_rejects(
            &sequence,
            &expansions,
            &[error(
                2,
                LowerRule::NotAllowed {
                    child: 1,
                    action: name,
                },
            )],
        );
    }
}

#[test]
fn lower_rejects_a_child_with_another_run_in() {
    let (sequence, mut expansions) = planned();
    expansions.insert(id(2), vec![child(0x100, "either", collect_hash())]);

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            2,
            LowerRule::RunIn {
                child: 0,
                run_in: RunIn::Either,
                expected: RunIn::FullOs,
            },
        )],
    );
}

#[test]
fn lower_rejects_a_nested_add_on_step_or_join_slot() {
    let (sequence, mut expansions) = planned();
    expansions.insert(
        id(2),
        vec![
            child(0x100, "full_os", add_on()),
            child(0x101, "full_os", join_slot("full_os")),
        ],
    );

    assert_rejects(
        &sequence,
        &expansions,
        &[
            error(2, LowerRule::Nested { child: 0 }),
            error(2, LowerRule::Nested { child: 1 }),
        ],
    );
}

#[test]
fn lower_rejects_an_expansion_for_a_step_it_does_not_expand() {
    let (sequence, mut expansions) = planned();
    expansions.insert(id(5), vec![child(0x100, "win_pe", command())]);
    expansions.insert(id(0x999), Vec::new());

    assert_rejects(
        &sequence,
        &expansions,
        &[
            error(5, LowerRule::UnexpectedExpansion),
            error(0x999, LowerRule::UnexpectedExpansion),
        ],
    );
}

#[test]
fn lower_returns_every_error_in_document_order() {
    let sequence = sequence(vec![
        step(2, "full_os", add_on()),
        group(3, vec![step(4, "win_pe", join_slot("offline_image"))]),
    ]);
    let expansions = Expansions::from([(
        id(2),
        vec![child(0x100, "win_pe", json!({ "check_readiness": {} }))],
    )]);

    assert_rejects(
        &sequence,
        &expansions,
        &[
            error(
                2,
                LowerRule::RunIn {
                    child: 0,
                    run_in: RunIn::WinPe,
                    expected: RunIn::FullOs,
                },
            ),
            error(
                2,
                LowerRule::NotAllowed {
                    child: 0,
                    action: "check_readiness",
                },
            ),
            error(4, LowerRule::MissingExpansion),
        ],
    );
}

#[test]
fn a_disabled_add_on_step_needs_no_expansion_and_the_program_drops_it() {
    let sequence = sequence(vec![
        disabled(step(2, "full_os", add_on())),
        disabled(group(3, vec![step(4, "full_os", add_on())])),
        step(5, "win_pe", command()),
    ]);

    let lowered = lower(&sequence, &Expansions::new()).unwrap_or_else(|errors| panic!("{errors}"));

    let Node::Group(Group { meta, children }) = &lowered.root.children[0] else {
        panic!("expected a group");
    };
    assert_eq!((meta.id, meta.enabled, children.len()), (id(2), false, 0));
    let program = compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
    // The root's `Enter`, then step 5.
    assert_eq!(program.instrs().len(), 2);
}

#[test]
fn the_steps_that_lower_expands_are_the_ones_deferred_lists() {
    let sequence = sequence(vec![
        step(2, "full_os", add_on()),
        disabled(step(3, "full_os", add_on())),
        group(
            4,
            vec![
                step(5, "win_pe", join_slot("offline_image")),
                disabled(group(6, vec![step(7, "full_os", join_slot("full_os"))])),
            ],
        ),
    ]);
    let expansions: Expansions = deferred(&sequence)
        .iter()
        .map(|found| (found.node.id, Vec::new()))
        .collect();

    assert_eq!(
        expansions.keys().copied().collect::<Vec<_>>(),
        [id(2), id(5)]
    );
    lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));
}

fn when(mut node: Value, condition: Value) -> Value {
    node["step"]["meta"]["condition"] = condition;
    node
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

fn nested_installed_msi() -> Value {
    json!({
        "any": [
            { "var": { "name": "AssetTag", "test": "exists" } },
            { "not": { "installed_msi": { "product_code": "23170f69-40c1-2702-2408-000001000000" } } },
        ],
    })
}

#[test]
fn the_compiler_rejects_a_win_pe_command_with_a_full_os_condition() {
    // The rule that the next tests extend to add-on steps and join slots.
    let sequence = sequence(vec![when(step(2, "win_pe", command()), wmi())]);

    let lowered = lower(&sequence, &Expansions::new()).unwrap_or_else(|errors| panic!("{errors}"));

    assert!(compile(&lowered).is_err());
}

#[test]
fn lower_rejects_a_win_pe_add_on_step_with_a_full_os_condition() {
    let sequence = sequence(vec![when(step(2, "win_pe", add_on()), wmi())]);
    let expansions = Expansions::from([(id(2), vec![child(0x100, "win_pe", collect_hash())])]);

    assert_rejects(
        &sequence,
        &expansions,
        &[error(2, LowerRule::FullOsCondition { condition: "wmi" })],
    );
}

#[test]
fn lower_rejects_a_win_pe_join_slot_with_a_full_os_condition_and_an_empty_expansion() {
    let sequence = sequence(vec![when(
        step(3, "win_pe", join_slot("offline_image")),
        nested_installed_msi(),
    )]);
    let expansions = Expansions::from([(id(3), Vec::new())]);

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            3,
            LowerRule::FullOsCondition {
                condition: "installed_msi",
            },
        )],
    );
}

#[test]
fn lower_checks_the_full_os_condition_rule_on_a_disabled_add_on_step() {
    let sequence = sequence(vec![disabled(when(step(2, "win_pe", add_on()), wmi()))]);

    assert_rejects(
        &sequence,
        &Expansions::new(),
        &[error(2, LowerRule::FullOsCondition { condition: "wmi" })],
    );
}

#[test]
fn a_full_os_condition_on_a_full_os_add_on_step_is_fine() {
    let sequence = sequence(vec![when(step(2, "full_os", add_on()), wmi())]);
    let expansions = Expansions::from([(id(2), vec![child(0x100, "full_os", collect_hash())])]);

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn lower_rejects_a_join_slot_whose_run_in_does_not_fit_its_stage() {
    let cases = [
        ("offline_image", "full_os", false),
        ("offline_image", "either", false),
        ("full_os", "win_pe", false),
        ("full_os", "either", false),
        ("offline_image", "win_pe", true),
        ("full_os", "full_os", true),
    ];

    for (stage, run_in, fits) in cases {
        let enabled = sequence(vec![step(3, run_in, join_slot(stage))]);
        let expansions = Expansions::from([(id(3), Vec::new())]);
        // A disabled join slot needs no expansion and still counts.
        let dropped = sequence(vec![disabled(step(3, run_in, join_slot(stage)))]);

        for (sequence, expansions) in [(enabled, expansions), (dropped, Expansions::new())] {
            if fits {
                let lowered =
                    lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));
                compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
            } else {
                assert_rejects(
                    &sequence,
                    &expansions,
                    &[error(
                        3,
                        LowerRule::JoinStageRunIn {
                            stage: serde_json::from_value(json!(stage)).expect("a valid stage"),
                            run_in: serde_json::from_value(json!(run_in)).expect("a valid run_in"),
                        },
                    )],
                );
            }
        }
    }
}

#[test]
fn lower_rejects_an_authored_server_call_or_provisioning_package() {
    let sequence = sequence(vec![
        step(2, "full_os", server_call()),
        disabled(group(3, vec![step(4, "win_pe", provisioning_package())])),
    ]);

    assert_rejects(
        &sequence,
        &Expansions::new(),
        &[
            error(
                2,
                LowerRule::AddOnOnlyAction {
                    action: "server_call",
                },
            ),
            error(
                4,
                LowerRule::AddOnOnlyAction {
                    action: "add_provisioning_package",
                },
            ),
        ],
    );
}

#[test]
fn an_expansion_may_hold_a_server_call_and_a_provisioning_package() {
    let sequence = sequence(vec![step(2, "win_pe", add_on())]);
    let expansions = Expansions::from([(
        id(2),
        vec![
            child(0x100, "win_pe", server_call()),
            child(0x101, "win_pe", provisioning_package()),
        ],
    )]);

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    let Node::Group(register) = &lowered.root.children[0] else {
        panic!("expected a group");
    };
    let [Node::Step(call), Node::Step(package)] = register.children.as_slice() else {
        panic!("expected two steps, found {:?}", register.children);
    };
    assert!(matches!(call.action, BuiltinAction::ServerCall { .. }));
    assert!(matches!(
        package.action,
        BuiltinAction::AddProvisioningPackage { .. }
    ));
    compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn an_add_on_steps_expansion_may_name_its_step_config_or_its_add_on_settings() {
    let sequence = sequence(vec![step(2, "win_pe", add_on())]);
    let expansions = Expansions::from([(
        id(2),
        vec![
            child(0x100, "win_pe", package_from("step_config")),
            child(0x101, "win_pe", package_from("addon_settings")),
        ],
    )]);

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn lower_rejects_a_join_profile_secret_in_an_add_on_steps_expansion() {
    let sequence = sequence(vec![step(2, "win_pe", add_on())]);
    let expansions = Expansions::from([(
        id(2),
        vec![
            child(0x100, "win_pe", package_from("step_config")),
            child(0x101, "win_pe", package_from("join_profile")),
        ],
    )]);

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            2,
            LowerRule::SecretSource {
                child: 1,
                source: SecretSource::JoinProfile,
            },
        )],
    );
    let errors = lower(&sequence, &expansions).expect_err("the expansion must not lower");
    assert!(
        errors
            .to_string()
            .contains("child 1 names a secret in `join_profile`"),
        "{errors}"
    );
}

#[test]
fn a_join_slots_expansion_may_name_the_join_profile_or_its_add_on_settings() {
    let sequence = sequence(vec![step(3, "win_pe", join_slot("offline_image"))]);
    let expansions = Expansions::from([(
        id(3),
        vec![
            child(0x100, "win_pe", package_from("join_profile")),
            child(0x101, "win_pe", package_from("addon_settings")),
        ],
    )]);

    let lowered = lower(&sequence, &expansions).unwrap_or_else(|errors| panic!("{errors}"));

    compile(&lowered).unwrap_or_else(|errors| panic!("{errors}"));
}

#[test]
fn lower_rejects_a_step_config_secret_in_a_join_slots_expansion() {
    let sequence = sequence(vec![step(3, "full_os", join_slot("full_os"))]);
    let expansions = Expansions::from([(
        id(3),
        vec![
            child(0x100, "full_os", package_from("step_config")),
            child(0x101, "full_os", package_from("join_profile")),
        ],
    )]);

    assert_rejects(
        &sequence,
        &expansions,
        &[error(
            3,
            LowerRule::SecretSource {
                child: 0,
                source: SecretSource::StepConfig,
            },
        )],
    );
}
