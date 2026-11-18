//! `deferred` and `plan`, which put the built-in steps that add-ons return in
//! place of add-on steps and join slots, the three rules about add-on steps,
//! join slots, and the actions that only an expansion may hold, and the
//! agreement of `validate` with `plan`.

use deployarr_core::action::{Action, BuiltinAction, SecretSource};
use deployarr_core::catalog::Catalog;
use deployarr_core::join::JoinStage;
use deployarr_core::plan::{
    DeferredKind, Expansions, NodePath, PlanCheck, PlanErrors, PlanIssue, Rule, SaveCheck,
    SaveIssue, Severity, deferred, plan, validate,
};
use deployarr_core::program::{Instr, OnError, Program};
use deployarr_core::sequence::{AuthoredSequence, Node, NodeId, RunIn, Step};
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

fn when(mut node: Value, condition: Value) -> Value {
    let key = if node.get("step").is_some() {
        "step"
    } else {
        "group"
    };
    node[key]["meta"]["condition"] = condition;
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

/// A catalog whose `register` step kind runs with `run_in`, so that an add-on
/// step from [`add_on`] with that `run_in` gets no finding of its own.
fn known(run_in: &str) -> Catalog {
    serde_json::from_value(json!({
        "acme.autopilot": {
            "step_kinds": {
                "register": {
                    "label": "Register with Autopilot",
                    "run_in": run_in,
                    "availability": "enabled",
                    "config": {
                        "current": 1,
                        "versions": {
                            "1": {"field": [
                                {"key": "group_tag", "label": "Group tag", "kind": "template", "required": true},
                            ]},
                        },
                    },
                },
            },
        },
    }))
    .expect("the catalog parses")
}

fn join_slot(stage: &str) -> Value {
    json!({ "apply_join_profile": { "stage": stage } })
}

fn command() -> Value {
    json!({ "run_command": { "command": "cmd /c exit 0", "working_dir": null } })
}

fn restart_into_os() -> Value {
    json!({ "restart": { "into": "installed_os" } })
}

/// A step of an expansion. Its ID is `n`, which `plan` replaces.
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

#[track_caller]
fn plan_ok(sequence: &AuthoredSequence, expansions: &Expansions) -> Program {
    plan(sequence, expansions).unwrap_or_else(|errors| panic!("{errors}"))
}

#[track_caller]
fn plan_err(sequence: &AuthoredSequence, expansions: &Expansions) -> PlanErrors {
    match plan(sequence, expansions) {
        Ok(program) => panic!("expected errors, planned {program:?}"),
        Err(errors) => errors,
    }
}

#[track_caller]
fn assert_rejects(sequence: &AuthoredSequence, expansions: &Expansions, expected: &[PlanIssue]) {
    let errors = plan_err(sequence, expansions);
    assert_eq!(errors.issues(), expected);
    assert_eq!(errors.unused_expansions(), []);
}

fn issue(n: u32, path: Vec<usize>, check: PlanCheck) -> PlanIssue {
    PlanIssue {
        node: id(n),
        path: NodePath::from(path),
        check,
    }
}

/// An issue at child `child` of the expansion of node `n`, which is at
/// `path`.
fn in_child(n: u32, path: Vec<usize>, child: usize, check: PlanCheck) -> PlanIssue {
    PlanIssue {
        node: id(n),
        path: NodePath::from(path).with_child(child),
        check,
    }
}

fn save_error(n: u32, path: Vec<usize>, rule: Rule) -> SaveIssue {
    SaveIssue {
        node: id(n),
        path: NodePath::from(path),
        check: SaveCheck::Rule(rule),
    }
}

/// The instructions of a program, with each `Exec`'s action left out.
fn execs(program: &Program) -> Vec<(NodeId, RunIn, OnError)> {
    program
        .instrs()
        .iter()
        .filter_map(|instr| match instr {
            Instr::Exec {
                node,
                run_in,
                on_error,
                ..
            } => Some((*node, *run_in, *on_error)),
            Instr::Enter { .. } => None,
        })
        .collect()
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
fn plan_puts_each_expansion_in_a_group_that_keeps_the_steps_node() {
    let (mut sequence, expansions) = planned();
    let Node::Step(register) = &mut sequence.root.children[1] else {
        panic!("the second node is a step");
    };
    register.meta.condition =
        Some(serde_json::from_value(win_pe_condition()).expect("a condition"));
    let register_condition = register.meta.condition.clone();

    let program = plan_ok(&sequence, &expansions);

    let [
        Instr::Enter { node: root, .. },
        Instr::Exec { node: first, .. },
        Instr::Enter {
            node: group,
            condition,
            skip_to,
            ..
        },
        Instr::Exec {
            node: hash,
            run_in,
            action: hash_action,
            ..
        },
        Instr::Exec {
            node: call,
            action: call_action,
            ..
        },
        Instr::Enter {
            node: join,
            skip_to: join_end,
            ..
        },
    ] = program.instrs()
    else {
        panic!("expected six instructions, found {:?}", program.instrs());
    };
    assert_eq!((*root, *first), (id(1), id(5)));
    assert_eq!(*group, id(2));
    assert_eq!(condition, &register_condition);
    assert_eq!(skip_to.0, 5);
    // Version 5 UUIDs from the step's ID and the names "0" and "1", worked
    // out with Python's `uuid.uuid5`.
    assert_eq!(hash.0.to_string(), "b71c7c4b-a648-5a37-a8cc-af9d0f7b34c3");
    assert_eq!(call.0.to_string(), "6aae72a3-38e4-5200-9127-71721dc9bb8b");
    assert_eq!(*run_in, RunIn::FullOs);
    assert!(matches!(hash_action, BuiltinAction::RunPowerShell { .. }));
    assert!(matches!(call_action, BuiltinAction::ServerCall { .. }));
    // The join slot's empty expansion is an empty group.
    assert_eq!((*join, join_end.0), (id(3), 6));
}

#[test]
fn the_same_input_gives_the_same_program() {
    let (sequence, expansions) = planned();

    let first = plan_ok(&sequence, &expansions);
    let second = plan_ok(&sequence, &expansions);

    assert_eq!(first, second);
}

#[test]
fn a_child_keeps_its_own_meta_except_its_id() {
    let (sequence, mut expansions) = planned();
    let children = expansions.get_mut(&id(2)).expect("the add-on's expansion");
    children[0].meta.continue_on_error = true;
    children[1].meta.enabled = false;

    let program = plan_ok(&sequence, &expansions);

    let hash: NodeId =
        serde_json::from_value(json!("b71c7c4b-a648-5a37-a8cc-af9d0f7b34c3")).expect("an ID");
    assert_eq!(
        execs(&program),
        [
            (id(5), RunIn::WinPe, OnError::FailDeployment),
            (hash, RunIn::FullOs, OnError::Continue),
        ]
    );
}

#[test]
fn plan_rejects_a_missing_expansion() {
    let (sequence, mut expansions) = planned();
    expansions.remove(&id(2));

    assert_rejects(
        &sequence,
        &expansions,
        &[issue(2, vec![1], PlanCheck::MissingExpansion)],
    );
}

#[test]
fn plan_rejects_an_action_outside_the_allowed_six() {
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
        &[in_child(
            2,
            vec![1],
            1,
            PlanCheck::NotAllowed {
                action: "install_applications",
            },
        )],
    );
}

#[test]
fn plan_rejects_an_install_updates_step_in_an_expansion() {
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
        &[in_child(
            2,
            vec![1],
            0,
            PlanCheck::NotAllowed {
                action: "install_updates",
            },
        )],
    );
}

#[test]
fn plan_rejects_an_expansion_that_holds_a_bitlocker_action() {
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
            &[in_child(
                2,
                vec![1],
                1,
                PlanCheck::NotAllowed { action: name },
            )],
        );
    }
}

#[test]
fn plan_rejects_a_child_with_another_run_in() {
    let (sequence, mut expansions) = planned();
    expansions.insert(id(2), vec![child(0x100, "either", collect_hash())]);

    assert_rejects(
        &sequence,
        &expansions,
        &[in_child(
            2,
            vec![1],
            0,
            PlanCheck::ChildRunIn {
                run_in: RunIn::Either,
                expected: RunIn::FullOs,
            },
        )],
    );
}

#[test]
fn plan_rejects_a_nested_add_on_step_or_join_slot() {
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
            in_child(2, vec![1], 0, PlanCheck::Nested),
            in_child(2, vec![1], 1, PlanCheck::Nested),
        ],
    );
}

#[test]
fn plan_lists_the_expansions_that_no_kept_deferred_step_used() {
    let (sequence, mut expansions) = planned();
    expansions.insert(id(5), vec![child(0x100, "win_pe", command())]);
    expansions.insert(id(0x999), Vec::new());

    let errors = plan_err(&sequence, &expansions);

    assert_eq!(errors.issues(), []);
    assert_eq!(errors.unused_expansions(), [id(5), id(0x999)]);
    assert_eq!(
        errors.to_string(),
        format!(
            "node {}: an expansion was given for a node that is not an enabled add-on step or join slot\nnode {}: an expansion was given for a node that is not an enabled add-on step or join slot",
            id_text(5),
            id_text(0x999)
        )
    );
}

#[test]
fn plan_returns_every_error_in_document_order() {
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
            in_child(
                2,
                vec![0],
                0,
                PlanCheck::ChildRunIn {
                    run_in: RunIn::WinPe,
                    expected: RunIn::FullOs,
                },
            ),
            in_child(
                2,
                vec![0],
                0,
                PlanCheck::NotAllowed {
                    action: "check_readiness",
                },
            ),
            issue(4, vec![1, 0], PlanCheck::MissingExpansion),
        ],
    );
}

#[test]
fn a_rule_broken_inside_an_expansion_names_the_deferred_step_its_path_and_the_child() {
    let sequence = sequence(vec![group(
        3,
        vec![
            step(4, "win_pe", command()),
            when(step(2, "win_pe", add_on()), win_pe_condition()),
        ],
    )]);
    let expansions = Expansions::from([(
        id(2),
        vec![
            child(0x100, "win_pe", command()),
            when_child(child(0x101, "win_pe", restart_into_os()), wmi()),
        ],
    )]);

    let errors = plan_err(&sequence, &expansions);

    // Child 1 breaks two rules. Neither names the child's derived ID.
    assert_eq!(
        errors.issues(),
        [
            in_child(
                2,
                vec![0, 1],
                1,
                PlanCheck::Rule(Rule::FullOsCondition { condition: "wmi" }),
            ),
            in_child(
                2,
                vec![0, 1],
                1,
                PlanCheck::Rule(Rule::RestartBeforePartition)
            ),
        ]
    );
    assert_eq!(
        errors.issues()[0].to_string(),
        format!(
            "root.children[0].group.children[1].expansion[1], node {}: a `wmi` condition works only in the full OS, so a step with `run_in` `win_pe` cannot have one",
            id_text(2)
        )
    );
}

/// A condition that WinPE can answer.
fn win_pe_condition() -> Value {
    json!({ "var": { "name": "AssetTag", "test": "exists" } })
}

fn when_child(mut step: Step<Action>, condition: Value) -> Step<Action> {
    step.meta.condition = Some(serde_json::from_value(condition).expect("a condition"));
    step
}

#[test]
fn a_disabled_add_on_step_needs_no_expansion_and_the_program_drops_it() {
    let sequence = sequence(vec![
        disabled(step(2, "full_os", add_on())),
        disabled(group(3, vec![step(4, "full_os", add_on())])),
        step(5, "win_pe", command()),
    ]);

    let program = plan_ok(&sequence, &Expansions::new());

    // The root's `Enter`, then step 5.
    assert_eq!(program.instrs().len(), 2);
    assert_eq!(
        execs(&program),
        [(id(5), RunIn::WinPe, OnError::FailDeployment)]
    );
}

#[test]
fn the_steps_that_plan_expands_are_the_ones_deferred_lists() {
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
    plan_ok(&sequence, &expansions);
}

#[test]
fn a_full_os_condition_on_a_full_os_add_on_step_is_fine() {
    // The group that `plan` builds keeps the step's condition, so the step
    // comes after a restart into the installed OS.
    let sequence = sequence(vec![
        step(3, "either", restart_into_os()),
        when(step(2, "full_os", add_on()), wmi()),
    ]);
    let expansions = Expansions::from([(id(2), vec![child(0x100, "full_os", collect_hash())])]);

    plan_ok(&sequence, &expansions);
}

#[test]
fn a_full_os_condition_on_a_full_os_add_on_step_before_the_restart_into_the_os_fails() {
    let sequence = sequence(vec![when(step(2, "full_os", add_on()), wmi())]);
    let expansions = Expansions::from([(id(2), vec![child(0x100, "full_os", collect_hash())])]);

    // The group that `plan` builds keeps the step's condition, whatever the
    // step's `run_in`.
    assert_rejects(
        &sequence,
        &expansions,
        &[issue(
            2,
            vec![0],
            PlanCheck::Rule(Rule::FullOsConditionBeforeInstalledOs { condition: "wmi" }),
        )],
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

    let program = plan_ok(&sequence, &expansions);

    let actions: Vec<&BuiltinAction> = program
        .instrs()
        .iter()
        .filter_map(|instr| match instr {
            Instr::Exec { action, .. } => Some(action),
            Instr::Enter { .. } => None,
        })
        .collect();
    let [call, package] = actions.as_slice() else {
        panic!("expected two steps, found {actions:?}");
    };
    assert!(matches!(call, BuiltinAction::ServerCall { .. }));
    assert!(matches!(
        package,
        BuiltinAction::AddProvisioningPackage { .. }
    ));
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

    plan_ok(&sequence, &expansions);
}

#[test]
fn plan_rejects_a_join_profile_secret_in_an_add_on_steps_expansion() {
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
        &[in_child(
            2,
            vec![0],
            1,
            PlanCheck::SecretSource {
                source: SecretSource::JoinProfile,
            },
        )],
    );
    let errors = plan_err(&sequence, &expansions);
    assert!(
        errors
            .to_string()
            .contains("root.children[0].expansion[1], node 00000000-0000-4000-8000-000000000002: the child names a secret in `join_profile`"),
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

    plan_ok(&sequence, &expansions);
}

#[test]
fn plan_rejects_a_step_config_secret_in_a_join_slots_expansion() {
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
        &[in_child(
            3,
            vec![0],
            0,
            PlanCheck::SecretSource {
                source: SecretSource::StepConfig,
            },
        )],
    );
}

// The three rules about add-on steps, join slots, and the actions that only an
// expansion may hold. Each one has one test, which runs it through `validate`
// and `plan`.

#[test]
fn a_win_pe_add_on_step_or_join_slot_with_a_full_os_condition_breaks_one_rule() {
    let sequence = sequence(vec![
        when(step(2, "win_pe", add_on()), wmi()),
        when(
            step(3, "win_pe", join_slot("offline_image")),
            nested_installed_msi(),
        ),
        // A disabled step counts, and needs no expansion.
        disabled(when(step(4, "win_pe", add_on()), wmi())),
    ]);
    let expansions = Expansions::from([
        (id(2), vec![child(0x100, "win_pe", collect_hash())]),
        (id(3), Vec::new()),
    ]);
    let wmi = Rule::FullOsCondition { condition: "wmi" };
    let msi = Rule::FullOsCondition {
        condition: "installed_msi",
    };

    // The step gets one issue for its condition, though it comes before any
    // restart into the installed OS.
    assert_eq!(
        validate(&sequence, &known("win_pe")),
        [
            save_error(2, vec![0], wmi.clone()),
            save_error(3, vec![1], msi.clone()),
            save_error(4, vec![2], wmi.clone()),
        ]
    );
    assert_rejects(
        &sequence,
        &expansions,
        &[
            issue(2, vec![0], PlanCheck::Rule(wmi.clone())),
            issue(3, vec![1], PlanCheck::Rule(msi)),
            issue(4, vec![2], PlanCheck::Rule(wmi)),
        ],
    );
}

#[test]
fn a_join_slot_whose_run_in_does_not_fit_its_stage_breaks_a_rule() {
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
            let findings = validate(&sequence, &Catalog::default());
            if fits {
                assert_eq!(findings, [], "{stage} {run_in}");
                plan_ok(&sequence, &expansions);
            } else {
                let rule = Rule::JoinStageRunIn {
                    stage: serde_json::from_value(json!(stage)).expect("a valid stage"),
                    run_in: serde_json::from_value(json!(run_in)).expect("a valid run_in"),
                };
                assert_eq!(
                    findings,
                    [save_error(3, vec![0], rule.clone())],
                    "{stage} {run_in}"
                );
                assert_eq!(findings[0].severity(), Severity::Error);
                assert_rejects(
                    &sequence,
                    &expansions,
                    &[issue(3, vec![0], PlanCheck::Rule(rule))],
                );
            }
        }
    }
    let sequence = sequence(vec![step(3, "win_pe", join_slot("full_os"))]);
    assert_eq!(
        validate(&sequence, &Catalog::default())[0].to_string(),
        format!(
            "root.children[0], node {}: error: a join slot with `stage` `full_os` runs only with `run_in` `full_os`, and this step has `win_pe`",
            id_text(3)
        )
    );
}

#[test]
fn an_authored_server_call_or_provisioning_package_breaks_a_rule() {
    let sequence = sequence(vec![
        step(2, "full_os", server_call()),
        group(3, vec![disabled(step(4, "win_pe", provisioning_package()))]),
        disabled(group(5, vec![step(6, "win_pe", provisioning_package())])),
    ]);
    let server_call = Rule::AddOnOnlyAction {
        action: "server_call",
    };
    let package = Rule::AddOnOnlyAction {
        action: "add_provisioning_package",
    };

    let findings = validate(&sequence, &Catalog::default());

    assert_eq!(
        findings,
        [
            save_error(2, vec![0], server_call.clone()),
            save_error(4, vec![1, 0], package.clone()),
            save_error(6, vec![2, 0], package.clone()),
        ]
    );
    assert_eq!(
        findings[0].to_string(),
        format!(
            "root.children[0], node {}: error: only an add-on step's expansion may hold `server_call`, so a task sequence cannot hold it as a step of its own",
            id_text(2)
        )
    );
    assert_rejects(
        &sequence,
        &Expansions::new(),
        &[
            issue(2, vec![0], PlanCheck::Rule(server_call)),
            issue(4, vec![1, 0], PlanCheck::Rule(package.clone())),
            issue(6, vec![2, 0], PlanCheck::Rule(package)),
        ],
    );
}

// `validate` agrees with `plan`: every error that `validate` reports also
// fails `plan`, whatever the add-ons return.

/// Group `n` with a `wmi` condition, which holds one step with `run_in`
/// `either`.
fn wmi_group(n: u32) -> Value {
    when(group(n, vec![step(n + 1, "either", command())]), wmi())
}

/// Sequences with add-on steps and join slots, each with an error that
/// `validate` reports, or none.
fn fixtures() -> Vec<AuthoredSequence> {
    vec![
        // No error.
        sequence(vec![
            step(
                2,
                "win_pe",
                json!({ "partition_disk": { "layout": "uefi-gpt" } }),
            ),
            step(3, "win_pe", add_on()),
            step(4, "win_pe", join_slot("offline_image")),
            step(
                5,
                "win_pe",
                json!({ "restart": { "into": "installed_os" } }),
            ),
            step(6, "full_os", join_slot("full_os")),
        ]),
        // A `wmi` group after an add-on step waits for planning.
        sequence(vec![step(2, "either", add_on()), wmi_group(3)]),
        sequence(vec![step(2, "full_os", join_slot("full_os")), wmi_group(3)]),
        // A `wmi` group before the first add-on step.
        sequence(vec![
            wmi_group(2),
            step(4, "either", add_on()),
            wmi_group(5),
        ]),
        // A disabled add-on step defers nothing.
        sequence(vec![disabled(step(2, "either", add_on())), wmi_group(3)]),
        // The add-on step's own condition comes before its expansion.
        sequence(vec![when(step(2, "full_os", add_on()), wmi())]),
        // A `win_pe` restart before the first partition beside an add-on step.
        sequence(vec![
            step(2, "win_pe", add_on()),
            step(
                3,
                "win_pe",
                json!({ "restart": { "into": "installed_os" } }),
            ),
        ]),
        // A run_in that the action does not allow.
        sequence(vec![
            step(2, "full_os", add_on()),
            step(
                3,
                "full_os",
                json!({ "apply_os_image": { "image": "w11" } }),
            ),
        ]),
        // A duplicate ID on an add-on step and a disabled join slot.
        sequence(vec![
            step(2, "full_os", add_on()),
            group(3, vec![disabled(step(2, "full_os", join_slot("full_os")))]),
        ]),
        // The three rules about deferred steps.
        sequence(vec![when(step(2, "win_pe", add_on()), wmi())]),
        sequence(vec![step(2, "either", join_slot("full_os"))]),
        sequence(vec![
            step(2, "either", add_on()),
            step(3, "full_os", server_call()),
        ]),
    ]
}

/// Each kept deferred step expands to `children(run_in)`.
fn expand_each(
    sequence: &AuthoredSequence,
    children: impl Fn(RunIn) -> Vec<Step<Action>>,
) -> Expansions {
    deferred(sequence)
        .iter()
        .map(|found| (found.node.id, children(found.run_in)))
        .collect()
}

fn run_in_name(run_in: RunIn) -> &'static str {
    match run_in {
        RunIn::WinPe => "win_pe",
        RunIn::FullOs => "full_os",
        RunIn::Either => "either",
    }
}

/// Gives the expansion of a deferred step with this `run_in`.
type Expand = fn(RunIn) -> Vec<Step<Action>>;

#[test]
fn every_error_that_validate_reports_also_fails_plan() {
    let samples: [(&str, Expand); 2] = [
        ("empty", |_| Vec::new()),
        ("restart into the installed OS", |run_in| {
            vec![child(0x100, run_in_name(run_in), restart_into_os())]
        }),
    ];
    let mut checked = 0;

    for sequence in fixtures() {
        let errors: Vec<SaveIssue> = validate(&sequence, &Catalog::default())
            .into_iter()
            .filter(|found| found.severity() == Severity::Error)
            .collect();
        for (name, children) in &samples {
            let expansions = expand_each(&sequence, children);
            let planned = plan(&sequence, &expansions);
            if !errors.is_empty() {
                checked += 1;
                assert!(
                    planned.is_err(),
                    "validate reports {errors:?}, and plan with the {name} expansion builds a program for {sequence:?}"
                );
            }
        }
    }
    // Nine fixtures have an error, each with two sample expansions.
    assert_eq!(checked, 18);
}

#[test]
fn a_wmi_group_after_a_deferred_step_waits_for_planning() {
    let cases = [
        step(2, "either", add_on()),
        step(2, "full_os", join_slot("full_os")),
    ];

    for deferred in cases {
        let sequence = sequence(vec![deferred.clone(), wmi_group(3)]);

        // The step's expansion might hold a restart into the installed OS.
        assert_eq!(validate(&sequence, &known("either")), [], "{deferred}");

        let restarts = expand_each(&sequence, |run_in| {
            vec![child(0x100, run_in_name(run_in), restart_into_os())]
        });
        plan_ok(&sequence, &restarts);

        let does_not = expand_each(&sequence, |run_in| {
            vec![child(0x100, run_in_name(run_in), command())]
        });
        assert_rejects(
            &sequence,
            &does_not,
            &[issue(
                3,
                vec![1],
                PlanCheck::Rule(Rule::FullOsConditionBeforeInstalledOs { condition: "wmi" }),
            )],
        );
    }
}
