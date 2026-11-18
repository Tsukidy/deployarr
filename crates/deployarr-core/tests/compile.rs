use std::collections::BTreeSet;

use deployarr_core::action::BuiltinAction;
use deployarr_core::compile::OnError::{Continue, FailDeployment};
use deployarr_core::compile::{
    CompileError, CompileErrors, Instr, NodePath, OnError, Pc, Program, Rule, compile,
};
use deployarr_core::sequence::{
    AuthoredSequence, Group, LoweredSequence, Node, NodeId, RunIn, Step,
};
use deployarr_core::validate::{Finding, Issue, validate};
use serde_json::{Value, json};

use self::Shape::{Enter, Exec};

const SAMPLE: &str = include_str!("data/windows-11-pro.json");

// Builders for small sequences. Node `n` has the ID that ends in `n`, written
// in hex, and the root is node 1.

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

fn root(children: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "id": "TEST",
        "version": 3,
        "name": "Test",
        "default_vars": [],
        "root": { "meta": meta(1), "children": children },
    })
}

/// Sets one field of a node's `meta`. `node` is a node or a whole sequence,
/// whose root then gets the field.
fn with(mut node: Value, field: &str, value: Value) -> Value {
    let meta = if node.get("root").is_some() {
        &mut node["root"]["meta"]
    } else if node.get("step").is_some() {
        &mut node["step"]["meta"]
    } else {
        &mut node["group"]["meta"]
    };
    meta[field] = value;
    node
}

fn disabled(node: Value) -> Value {
    with(node, "enabled", json!(false))
}

fn continue_on_error(node: Value) -> Value {
    with(node, "continue_on_error", json!(true))
}

fn when(node: Value, condition: Value) -> Value {
    with(node, "condition", condition)
}

fn command() -> Value {
    json!({ "run_command": { "command": "cmd /c exit 0" } })
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

fn drivers() -> Value {
    json!({ "apply_drivers": { "selection": "auto" } })
}

fn unattend() -> Value {
    json!({ "apply_unattend": { "template": "standard" } })
}

fn installed_msi() -> Value {
    json!({ "installed_msi": { "product_code": "23170f69-40c1-2702-2408-000001000000" } })
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

fn var_exists() -> Value {
    json!({ "var": { "name": "AssetTag", "test": "exists" } })
}

fn parse(document: &Value) -> LoweredSequence {
    LoweredSequence::from_json(&document.to_string()).expect("the document parses")
}

/// Compiles the document, and checks that `validate` agrees with `compile`.
fn compile_and_validate(document: &Value) -> Result<Program, CompileErrors> {
    let result = compile(&parse(document));
    let authored = AuthoredSequence::from_json(&document.to_string()).expect("the document parses");
    let errors = match &result {
        Ok(_) => &[][..],
        Err(errors) => errors.as_slice(),
    };
    assert_validate_agrees(&authored, errors);
    result
}

/// `validate` reports the same rules at the same nodes as `compile`, as
/// errors, for a sequence with no add-on step and no join slot.
#[track_caller]
fn assert_validate_agrees(sequence: &AuthoredSequence, errors: &[CompileError]) {
    let expected: Vec<Finding> = errors
        .iter()
        .map(|error| Finding {
            node: error.node,
            path: error.path.clone(),
            issue: Issue::Compile(error.rule.clone()),
        })
        .collect();
    assert_eq!(validate(sequence, &BTreeSet::new()), expected);
}

fn compile_ok(document: &Value) -> Program {
    let program = compile_and_validate(document).unwrap_or_else(|errors| panic!("{errors}"));
    assert_jumps_forward(&program);
    program
}

fn compile_err(document: &Value) -> Vec<CompileError> {
    match compile_and_validate(document) {
        Ok(program) => panic!("expected errors, compiled {program:?}"),
        Err(errors) => errors.as_slice().to_vec(),
    }
}

fn error(n: u32, path: Vec<usize>, rule: Rule) -> CompileError {
    CompileError {
        node: id(n),
        path: NodePath::from(path),
        rule,
    }
}

fn run_in(name: &str) -> RunIn {
    serde_json::from_value(json!(name)).expect("a valid run_in")
}

/// Every jump lands after its own instruction and no later than the end. That
/// covers the `skip_to` and the `on_error` of each `Enter`, and no `Enter`
/// holds `Continue`.
fn assert_jumps_forward(program: &Program) {
    for (pc, instr) in program.instrs().iter().enumerate() {
        let (skip_to, on_error) = match instr {
            Instr::Enter {
                skip_to, on_error, ..
            } => {
                assert_ne!(*on_error, Continue, "the Enter at {pc} holds Continue");
                (Some(*skip_to), *on_error)
            }
            Instr::Exec { on_error, .. } => (None, *on_error),
        };
        let on_error = match on_error {
            OnError::JumpTo(target) => Some(target),
            Continue | FailDeployment => None,
        };
        for target in [skip_to, on_error].into_iter().flatten() {
            assert!(
                Pc(pc) < target && target <= program.end(),
                "the jump at {pc} goes to {target:?}"
            );
        }
    }
}

/// An instruction without its condition and action, with each node ID
/// shortened to its last 32 bits.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Enter(u32, usize),
    Exec(u32, OnError),
}

fn short(node: NodeId) -> u32 {
    (node.0.as_u128() & 0xffff_ffff) as u32
}

fn shapes(program: &Program) -> Vec<Shape> {
    program
        .instrs()
        .iter()
        .map(|instr| match instr {
            Instr::Enter { node, skip_to, .. } => Shape::Enter(short(*node), skip_to.0),
            Instr::Exec { node, on_error, .. } => Shape::Exec(short(*node), *on_error),
        })
        .collect()
}

fn jump_to(pc: usize) -> OnError {
    OnError::JumpTo(Pc(pc))
}

/// Each `Enter` as its node, shortened like [`short`], and its `on_error`.
fn enters(program: &Program) -> Vec<(u32, OnError)> {
    program
        .instrs()
        .iter()
        .filter_map(|instr| match instr {
            Instr::Enter { node, on_error, .. } => Some((short(*node), *on_error)),
            Instr::Exec { .. } => None,
        })
        .collect()
}

/// The `on_error` of the instruction for node `n`.
fn on_error_of(program: &Program, n: u32) -> OnError {
    program
        .instrs()
        .iter()
        .find_map(|instr| match instr {
            Instr::Enter { node, on_error, .. } | Instr::Exec { node, on_error, .. }
                if short(*node) == n =>
            {
                Some(*on_error)
            }
            Instr::Enter { .. } | Instr::Exec { .. } => None,
        })
        .unwrap_or_else(|| panic!("no instruction for node {n:#x}"))
}

/// The steps of the tree in document order, with the disabled ones and the
/// ones inside a disabled group left out.
fn kept_steps<'a>(group: &'a Group<BuiltinAction>, out: &mut Vec<&'a Step<BuiltinAction>>) {
    for child in &group.children {
        match child {
            Node::Group(group) if group.meta.enabled => kept_steps(group, out),
            Node::Step(step) if step.meta.enabled => out.push(step),
            Node::Group(_) | Node::Step(_) => {}
        }
    }
}

#[test]
fn the_sample_compiles_to_a_flat_program() {
    let sequence = LoweredSequence::from_json(SAMPLE).expect("the sample parses");

    let program = compile(&sequence).unwrap_or_else(|errors| panic!("{errors}"));

    assert_jumps_forward(&program);
    let authored = AuthoredSequence::from_json(SAMPLE).expect("the sample parses");
    assert_validate_agrees(&authored, &[]);
    assert_eq!(program.source().0.as_str(), "W11-PRO-STD");
    assert_eq!(program.source().1, 7);
    // The root, then "Install Windows" with its 8 steps, then "Finish in
    // Windows" with 5 of its 6 steps. The last step, 0x10, is disabled.
    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 16),
            Enter(0x0a, 10),
            Exec(0x02, FailDeployment),
            Exec(0x03, FailDeployment),
            Exec(0x04, FailDeployment),
            Exec(0x05, FailDeployment),
            Exec(0x06, FailDeployment),
            Exec(0x07, FailDeployment),
            Exec(0x08, Continue),
            Exec(0x09, FailDeployment),
            Enter(0x11, 16),
            Exec(0x0b, FailDeployment),
            Exec(0x0c, FailDeployment),
            Exec(0x0d, FailDeployment),
            Exec(0x0e, FailDeployment),
            Exec(0x0f, FailDeployment),
        ]
    );
    assert_eq!(program.end(), Pc(16));

    // Each instruction carries its node's condition, and each step its
    // `run_in` and action.
    let Node::Group(finish) = &sequence.root.children[1] else {
        panic!("the sample's second child is a group");
    };
    let Instr::Enter { condition, .. } = &program.instrs()[10] else {
        panic!("expected Enter at 10");
    };
    assert_eq!(condition, &finish.meta.condition);
    assert!(condition.is_some());

    let mut steps = Vec::new();
    kept_steps(&sequence.root, &mut steps);
    let execs: Vec<_> = program
        .instrs()
        .iter()
        .filter_map(|instr| match instr {
            Instr::Exec {
                node,
                condition,
                run_in,
                action,
                ..
            } => Some((*node, condition, *run_in, action)),
            Instr::Enter { .. } => None,
        })
        .collect();
    let expected: Vec<_> = steps
        .iter()
        .map(|step| {
            (
                step.meta.id,
                &step.meta.condition,
                step.run_in,
                &step.action,
            )
        })
        .collect();
    assert_eq!(execs, expected);
}

#[test]
fn on_error_jumps_to_the_end_of_the_nearest_group_with_continue_on_error() {
    let document = root(vec![
        continue_on_error(group(
            0x10,
            vec![
                step(0x11, "either", command()),
                group(
                    0x20,
                    vec![
                        step(0x21, "either", command()),
                        continue_on_error(group(
                            0x30,
                            vec![
                                step(0x31, "either", command()),
                                continue_on_error(step(0x32, "either", command())),
                            ],
                        )),
                        step(0x22, "either", command()),
                    ],
                ),
                step(0x12, "either", command()),
            ],
        )),
        step(0x02, "either", command()),
        group(0x40, vec![step(0x41, "either", command())]),
    ]);

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 13),
            Enter(0x10, 10),
            // 0x10 has continue_on_error, so its steps jump to its end.
            Exec(0x11, jump_to(10)),
            Enter(0x20, 9),
            // 0x20 has no continue_on_error, so 0x10 is the nearest.
            Exec(0x21, jump_to(10)),
            Enter(0x30, 8),
            // 0x30 is nearer than 0x10.
            Exec(0x31, jump_to(8)),
            // The step's own continue_on_error wins.
            Exec(0x32, Continue),
            Exec(0x22, jump_to(10)),
            Exec(0x12, jump_to(10)),
            // No enclosing group has continue_on_error.
            Exec(0x02, FailDeployment),
            Enter(0x40, 13),
            Exec(0x41, FailDeployment),
        ]
    );
}

#[test]
fn a_root_with_continue_on_error_catches_failures_at_the_end_of_the_program() {
    let document = continue_on_error(root(vec![
        step(0x02, "either", command()),
        group(0x03, vec![step(0x04, "either", command())]),
    ]));

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 4),
            Exec(0x02, jump_to(4)),
            Enter(0x03, 4),
            Exec(0x04, jump_to(4)),
        ]
    );
    assert_eq!(program.end(), Pc(4));
}

#[test]
fn a_group_whose_condition_fails_to_evaluate_fails_like_a_step_directly_inside_it() {
    let document = root(vec![
        continue_on_error(group(
            0x10,
            vec![
                step(0x11, "either", command()),
                group(
                    0x20,
                    vec![
                        step(0x21, "either", command()),
                        continue_on_error(group(0x30, vec![step(0x31, "either", command())])),
                        group(0x40, vec![step(0x41, "either", command())]),
                    ],
                ),
                step(0x12, "either", command()),
            ],
        )),
        group(0x50, vec![step(0x51, "either", command())]),
        step(0x02, "either", command()),
    ]);

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 13),
            Enter(0x10, 10),
            Exec(0x11, jump_to(10)),
            Enter(0x20, 9),
            Exec(0x21, jump_to(10)),
            Enter(0x30, 7),
            Exec(0x31, jump_to(7)),
            Enter(0x40, 9),
            Exec(0x41, jump_to(10)),
            Exec(0x12, jump_to(10)),
            Enter(0x50, 12),
            Exec(0x51, FailDeployment),
            Exec(0x02, FailDeployment),
        ]
    );
    assert_eq!(
        enters(&program),
        [
            // No enclosing group has continue_on_error.
            (0x01, FailDeployment),
            // A group with continue_on_error jumps to its own skip_to.
            (0x10, jump_to(10)),
            // 0x20 has no continue_on_error, so it passes the failure to 0x10.
            (0x20, jump_to(10)),
            (0x30, jump_to(7)),
            // Two levels up, past 0x20 to 0x10. 0x40 and 0x20 both end at 9.
            (0x40, jump_to(10)),
            (0x50, FailDeployment),
        ]
    );
    // Each group's `Enter` goes where a failed step without continue_on_error
    // directly inside the group goes.
    for (group, step) in [
        (0x01, 0x02),
        (0x10, 0x11),
        (0x20, 0x21),
        (0x30, 0x31),
        (0x40, 0x41),
        (0x50, 0x51),
    ] {
        assert_eq!(
            on_error_of(&program, group),
            on_error_of(&program, step),
            "group {group:#x}"
        );
    }
}

#[test]
fn a_root_whose_condition_fails_to_evaluate_ends_the_program_or_the_deployment() {
    let children = || {
        vec![
            step(0x02, "either", command()),
            group(0x03, vec![step(0x04, "either", command())]),
        ]
    };

    let caught = compile_ok(&continue_on_error(root(children())));
    let failed = compile_ok(&root(children()));

    // With continue_on_error, the root jumps to the end of the program.
    assert_eq!(caught.end(), Pc(4));
    assert_eq!(enters(&caught), [(0x01, jump_to(4)), (0x03, jump_to(4))]);
    // Without it, the deployment fails.
    assert_eq!(
        enters(&failed),
        [(0x01, FailDeployment), (0x03, FailDeployment)]
    );
}

#[test]
fn disabled_nodes_are_dropped_with_everything_inside_them() {
    let document = root(vec![
        disabled(step(0x02, "either", command())),
        continue_on_error(group(
            0x03,
            vec![
                step(0x04, "either", command()),
                disabled(group(
                    0x05,
                    vec![
                        step(0x06, "either", command()),
                        step(0x07, "win_pe", apply_image()),
                    ],
                )),
                step(0x08, "either", command()),
            ],
        )),
        step(0x09, "either", command()),
    ]);

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 5),
            Enter(0x03, 4),
            Exec(0x04, jump_to(4)),
            Exec(0x08, jump_to(4)),
            Exec(0x09, FailDeployment),
        ]
    );
}

#[test]
fn a_disabled_step_is_still_checked_by_itself() {
    let document = root(vec![
        disabled(step(0x02, "full_os", apply_image())),
        disabled(group(
            0x03,
            vec![when(step(0x04, "win_pe", command()), wmi())],
        )),
    ]);

    assert_eq!(
        compile_err(&document),
        [
            error(
                0x02,
                vec![0],
                Rule::RunIn {
                    action: "apply_os_image",
                    run_in: RunIn::FullOs,
                    allowed: &[RunIn::WinPe],
                },
            ),
            error(0x04, vec![1, 0], Rule::FullOsCondition { condition: "wmi" },),
        ]
    );
}

#[test]
fn a_disabled_root_compiles_to_an_empty_program() {
    let document = disabled(root(vec![step(0x02, "either", command())]));

    let program = compile_ok(&document);

    assert_eq!(program.instrs(), []);
    assert_eq!(program.end(), Pc(0));
}

#[test]
fn each_action_allows_only_the_run_in_values_in_its_table() {
    let table: [(&str, Value, &[&str]); 12] = [
        ("partition_disk", partition(), &["win_pe"]),
        ("apply_os_image", apply_image(), &["win_pe"]),
        (
            "apply_drivers",
            json!({ "apply_drivers": { "selection": "auto" } }),
            &["win_pe"],
        ),
        (
            "apply_unattend",
            json!({ "apply_unattend": { "template": "standard" } }),
            &["win_pe"],
        ),
        (
            "check_readiness",
            json!({ "check_readiness": {} }),
            &["win_pe"],
        ),
        (
            "install_applications",
            json!({ "install_applications": { "apps": { "one": "7zip-24.08" } } }),
            &["full_os"],
        ),
        (
            "verify_drivers",
            json!({ "verify_drivers": { "policy": "warn" } }),
            &["full_os"],
        ),
        (
            "activate_windows",
            json!({ "activate_windows": { "online": true } }),
            &["full_os"],
        ),
        ("run_command", command(), &["win_pe", "full_os", "either"]),
        (
            "run_powershell",
            json!({ "run_powershell": { "script": "scripts/a.ps1", "args": [] } }),
            &["win_pe", "full_os", "either"],
        ),
        (
            "set_variable",
            json!({ "set_variable": { "name": "OSDComputerName", "value": "PC-1" } }),
            &["win_pe", "full_os", "either"],
        ),
        ("restart", restart(), &["win_pe", "full_os", "either"]),
    ];

    for (name, action, allowed) in table {
        let allowed: Vec<RunIn> = allowed.iter().map(|name| run_in(name)).collect();
        for written in ["win_pe", "full_os", "either"] {
            // The partition and image steps come first, so a `win_pe` restart,
            // `apply_drivers`, and `apply_unattend` are allowed.
            let document = root(vec![
                step(0x02, "win_pe", partition()),
                step(0x04, "win_pe", apply_image()),
                step(0x03, written, action.clone()),
            ]);

            let result = compile_and_validate(&document);

            if allowed.contains(&run_in(written)) {
                assert!(result.is_ok(), "{name} with {written}: {result:?}");
            } else {
                let errors = result.expect_err(&format!("{name} with {written} must fail"));
                let [
                    CompileError {
                        node,
                        path,
                        rule:
                            Rule::RunIn {
                                action,
                                run_in: found,
                                allowed: listed,
                            },
                    },
                ] = errors.as_slice()
                else {
                    panic!("{name} with {written}: {errors}");
                };
                assert_eq!(short(*node), 0x03);
                assert_eq!(path.indices(), [2]);
                assert_eq!(*action, name);
                assert_eq!(*found, run_in(written));
                assert_eq!(listed.to_vec(), allowed);
            }
        }
    }
}

#[test]
fn a_win_pe_step_with_an_installed_msi_or_wmi_condition_fails() {
    let cases = [
        (installed_msi(), "installed_msi"),
        (wmi(), "wmi"),
        (json!({ "not": wmi() }), "wmi"),
        (
            json!({ "all": [var_exists(), { "any": [var_exists(), installed_msi()] }, wmi()] }),
            "installed_msi",
        ),
    ];

    for (condition, name) in cases {
        let document = root(vec![when(
            step(0x02, "win_pe", command()),
            condition.clone(),
        )]);

        assert_eq!(
            compile_err(&document),
            [error(
                0x02,
                vec![0],
                Rule::FullOsCondition { condition: name },
            )],
            "{condition}"
        );

        for other in ["full_os", "either"] {
            let document = root(vec![when(step(0x02, other, command()), condition.clone())]);
            compile_ok(&document);
        }
    }
}

#[test]
fn a_win_pe_step_with_a_condition_that_win_pe_can_answer_compiles() {
    let conditions = [
        var_exists(),
        json!({ "registry": { "key": "HKEY_LOCAL_MACHINE\\SYSTEM\\Setup", "value": "SystemSetupInProgress", "test": { "equals": 1 } } }),
        json!({ "file": { "path": "X:\\Windows\\System32\\wpeinit.exe" } }),
        json!({ "folder": { "path": "X:\\Windows" } }),
        json!({ "not": { "any": [var_exists()] } }),
    ];

    for condition in conditions {
        compile_ok(&root(vec![when(
            step(0x02, "win_pe", command()),
            condition,
        )]));
    }
}

#[test]
fn a_win_pe_restart_before_the_first_partition_disk_fails() {
    let cases = [
        // Directly before the partition step.
        (
            root(vec![
                step(0x02, "win_pe", restart()),
                step(0x03, "win_pe", partition()),
            ]),
            0x02,
            vec![0],
        ),
        // With no partition step at all.
        (root(vec![step(0x02, "win_pe", restart())]), 0x02, vec![0]),
        // In a group that comes before the partition step's group.
        (
            root(vec![
                group(0x02, vec![step(0x03, "win_pe", restart())]),
                group(0x04, vec![step(0x05, "win_pe", partition())]),
            ]),
            0x03,
            vec![0, 0],
        ),
        // After a disabled partition step, which the program drops.
        (
            root(vec![
                disabled(step(0x02, "win_pe", partition())),
                step(0x03, "win_pe", restart()),
                step(0x04, "win_pe", partition()),
            ]),
            0x03,
            vec![1],
        ),
    ];

    for (document, node, path) in cases {
        assert_eq!(
            compile_err(&document),
            [error(node, path, Rule::RestartBeforePartition)],
            "{document}"
        );
    }
}

#[test]
fn a_win_pe_restart_after_the_first_partition_disk_compiles() {
    let document = root(vec![
        group(
            0x02,
            vec![
                step(0x03, "win_pe", json!({ "check_readiness": {} })),
                step(0x04, "win_pe", partition()),
            ],
        ),
        group(
            0x05,
            vec![
                step(0x06, "win_pe", apply_image()),
                step(0x07, "win_pe", restart()),
            ],
        ),
    ]);

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 7),
            Enter(0x02, 4),
            Exec(0x03, FailDeployment),
            Exec(0x04, FailDeployment),
            Enter(0x05, 7),
            Exec(0x06, FailDeployment),
            Exec(0x07, FailDeployment),
        ]
    );
}

#[test]
fn a_restart_that_is_not_a_win_pe_step_passes_before_the_partition_step() {
    for written in ["either", "full_os"] {
        compile_ok(&root(vec![
            step(0x02, written, restart()),
            step(0x03, "win_pe", partition()),
        ]));
    }
    // A disabled restart is dropped, so it is not checked.
    compile_ok(&root(vec![
        disabled(step(0x02, "win_pe", restart())),
        step(0x03, "win_pe", partition()),
    ]));
}

#[test]
fn an_apply_drivers_or_apply_unattend_step_before_the_first_apply_os_image_fails() {
    for (action, name) in [(drivers(), "apply_drivers"), (unattend(), "apply_unattend")] {
        let cases = [
            // Directly before the image step.
            (
                root(vec![
                    step(0x02, "win_pe", action.clone()),
                    step(0x03, "win_pe", apply_image()),
                ]),
                0x02,
                vec![0],
            ),
            // In a group that comes before the image step's group.
            (
                root(vec![
                    group(
                        0x02,
                        vec![
                            step(0x03, "win_pe", partition()),
                            step(0x04, "win_pe", action.clone()),
                        ],
                    ),
                    group(0x05, vec![step(0x06, "win_pe", apply_image())]),
                ]),
                0x04,
                vec![0, 1],
            ),
            // After a disabled image step, which the program drops.
            (
                root(vec![
                    disabled(step(0x02, "win_pe", apply_image())),
                    step(0x03, "win_pe", action.clone()),
                    step(0x04, "win_pe", apply_image()),
                ]),
                0x03,
                vec![1],
            ),
            // After an image step inside a disabled group.
            (
                root(vec![
                    disabled(group(0x02, vec![step(0x03, "win_pe", apply_image())])),
                    step(0x04, "win_pe", action.clone()),
                    step(0x05, "win_pe", apply_image()),
                ]),
                0x04,
                vec![1],
            ),
        ];

        for (document, node, path) in cases {
            let errors = compile_err(&document);

            assert_eq!(
                errors,
                [error(
                    node,
                    path.clone(),
                    Rule::BeforeOsImage { action: name }
                )],
                "{document}"
            );
            assert_eq!(
                errors[0].to_string(),
                format!(
                    "{}, node {}: `{name}` acts on the applied image, so it must come after the first `apply_os_image` step",
                    NodePath::from(path),
                    id_text(node)
                )
            );
        }
    }
}

#[test]
fn apply_drivers_and_apply_unattend_steps_after_the_first_apply_os_image_compile() {
    // A second image step later does not matter. The first one comes before.
    let document = root(vec![
        group(
            0x02,
            vec![
                step(0x03, "win_pe", partition()),
                step(0x04, "win_pe", apply_image()),
            ],
        ),
        group(
            0x05,
            vec![
                step(0x06, "win_pe", drivers()),
                step(0x07, "win_pe", unattend()),
            ],
        ),
        step(0x08, "win_pe", apply_image()),
        step(0x09, "win_pe", drivers()),
    ]);

    let program = compile_ok(&document);

    assert_eq!(
        shapes(&program),
        [
            Enter(0x01, 9),
            Enter(0x02, 4),
            Exec(0x03, FailDeployment),
            Exec(0x04, FailDeployment),
            Enter(0x05, 7),
            Exec(0x06, FailDeployment),
            Exec(0x07, FailDeployment),
            Exec(0x08, FailDeployment),
            Exec(0x09, FailDeployment),
        ]
    );
}

#[test]
fn an_apply_os_image_step_with_a_condition_still_comes_before() {
    // The rule ignores conditions. If the image step is skipped at run time,
    // the agent fails the later step.
    let cases = [
        // The image step is in a group with a condition, and the later steps
        // are outside that group.
        root(vec![
            when(
                group(0x02, vec![step(0x03, "win_pe", apply_image())]),
                var_exists(),
            ),
            step(0x04, "win_pe", drivers()),
            step(0x05, "win_pe", unattend()),
        ]),
        // The image step has a condition of its own.
        root(vec![
            when(step(0x02, "win_pe", apply_image()), var_exists()),
            step(0x03, "win_pe", drivers()),
            step(0x04, "win_pe", unattend()),
        ]),
    ];

    for document in cases {
        compile_ok(&document);
    }
}

#[test]
fn a_disabled_apply_drivers_or_apply_unattend_step_before_the_image_is_not_checked() {
    // The program drops disabled steps and everything inside a disabled
    // group, so the order rule does not see them.
    compile_ok(&root(vec![
        disabled(step(0x02, "win_pe", drivers())),
        disabled(step(0x03, "win_pe", unattend())),
        disabled(group(
            0x04,
            vec![
                step(0x05, "win_pe", drivers()),
                step(0x06, "win_pe", unattend()),
            ],
        )),
        step(0x07, "win_pe", apply_image()),
    ]));
}

#[test]
fn a_sequence_with_no_apply_os_image_fails_for_each_apply_drivers_and_apply_unattend() {
    let document = root(vec![
        step(0x02, "win_pe", partition()),
        step(0x03, "win_pe", drivers()),
        group(
            0x04,
            vec![
                step(0x05, "win_pe", unattend()),
                step(0x06, "win_pe", drivers()),
            ],
        ),
        disabled(step(0x07, "win_pe", drivers())),
        step(0x08, "win_pe", unattend()),
    ]);

    assert_eq!(
        compile_err(&document),
        [
            error(
                0x03,
                vec![1],
                Rule::BeforeOsImage {
                    action: "apply_drivers",
                },
            ),
            error(
                0x05,
                vec![2, 0],
                Rule::BeforeOsImage {
                    action: "apply_unattend",
                },
            ),
            error(
                0x06,
                vec![2, 1],
                Rule::BeforeOsImage {
                    action: "apply_drivers",
                },
            ),
            error(
                0x08,
                vec![4],
                Rule::BeforeOsImage {
                    action: "apply_unattend",
                },
            ),
        ]
    );
}

#[test]
fn a_node_id_shared_by_two_sibling_steps_fails_and_the_error_names_the_id() {
    let document = root(vec![
        step(0x02, "either", command()),
        step(0x02, "either", command()),
    ]);

    let errors = compile_err(&document);

    assert_eq!(
        errors,
        [error(
            0x02,
            vec![1],
            Rule::DuplicateNodeId {
                first: NodePath::from(vec![0]),
            },
        )]
    );
    assert_eq!(
        errors[0].to_string(),
        format!(
            "root.children[1], node {}: the node at `root.children[0]` has the same ID",
            id_text(0x02)
        )
    );
}

#[test]
fn a_node_id_shared_by_a_group_and_its_child_step_fails_and_the_error_names_the_id() {
    let document = root(vec![group(0x02, vec![step(0x02, "either", command())])]);

    let errors = compile_err(&document);

    assert_eq!(
        errors,
        [error(
            0x02,
            vec![0, 0],
            Rule::DuplicateNodeId {
                first: NodePath::from(vec![0]),
            },
        )]
    );
    assert!(errors[0].to_string().contains(&id_text(0x02)));
}

#[test]
fn every_node_counts_for_a_duplicate_id() {
    let cases = [
        // The root and a step.
        (
            root(vec![step(0x01, "either", command())]),
            vec![error(
                0x01,
                vec![0],
                Rule::DuplicateNodeId {
                    first: NodePath::default(),
                },
            )],
        ),
        // A disabled step and a kept one.
        (
            root(vec![
                disabled(step(0x02, "either", command())),
                step(0x02, "either", command()),
            ]),
            vec![error(
                0x02,
                vec![1],
                Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                },
            )],
        ),
        // A kept step and a step inside a disabled group.
        (
            root(vec![
                step(0x02, "either", command()),
                disabled(group(0x03, vec![step(0x02, "either", command())])),
            ]),
            vec![error(
                0x02,
                vec![1, 0],
                Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                },
            )],
        ),
        // Three nodes with one ID. Both later ones name the first.
        (
            root(vec![
                step(0x02, "either", command()),
                group(0x02, vec![step(0x02, "either", command())]),
            ]),
            vec![
                error(
                    0x02,
                    vec![1],
                    Rule::DuplicateNodeId {
                        first: NodePath::from(vec![0]),
                    },
                ),
                error(
                    0x02,
                    vec![1, 0],
                    Rule::DuplicateNodeId {
                        first: NodePath::from(vec![0]),
                    },
                ),
            ],
        ),
    ];

    for (document, expected) in cases {
        assert_eq!(compile_err(&document), expected, "{document}");
    }
}

#[test]
fn every_error_is_reported_in_document_order() {
    let document = root(vec![
        step(0x02, "full_os", apply_image()),
        group(
            0x03,
            vec![
                when(step(0x04, "win_pe", command()), wmi()),
                step(0x02, "win_pe", restart()),
            ],
        ),
        step(
            0x05,
            "win_pe",
            json!({ "verify_drivers": { "policy": "fail" } }),
        ),
    ]);

    let errors = compile_and_validate(&document).expect_err("the sequence breaks five rules");

    assert_eq!(
        errors.as_slice(),
        [
            error(
                0x02,
                vec![0],
                Rule::RunIn {
                    action: "apply_os_image",
                    run_in: RunIn::FullOs,
                    allowed: &[RunIn::WinPe],
                },
            ),
            error(0x04, vec![1, 0], Rule::FullOsCondition { condition: "wmi" },),
            error(
                0x02,
                vec![1, 1],
                Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                },
            ),
            error(0x02, vec![1, 1], Rule::RestartBeforePartition),
            error(
                0x05,
                vec![2],
                Rule::RunIn {
                    action: "verify_drivers",
                    run_in: RunIn::WinPe,
                    allowed: &[RunIn::FullOs],
                },
            ),
        ]
    );
    let expected = [
        format!(
            "root.children[0], node {}: `apply_os_image` runs only with `run_in` `win_pe`, and this step has `full_os`",
            id_text(0x02)
        ),
        format!(
            "root.children[1].group.children[0], node {}: a `wmi` condition works only in the full OS, so a step with `run_in` `win_pe` cannot have one",
            id_text(0x04)
        ),
        format!(
            "root.children[1].group.children[1], node {}: the node at `root.children[0]` has the same ID",
            id_text(0x02)
        ),
        format!(
            "root.children[1].group.children[1], node {}: a `restart` with `run_in` `win_pe` must come after the first `partition_disk` step. Until then the state is on `X:`, and the restart loses it",
            id_text(0x02)
        ),
        format!(
            "root.children[2], node {}: `verify_drivers` runs only with `run_in` `full_os`, and this step has `win_pe`",
            id_text(0x05)
        ),
    ];
    assert_eq!(errors.to_string(), expected.join("\n"));
}

#[test]
fn compile_errors_iterate_in_order() {
    let document = root(vec![
        step(0x02, "full_os", apply_image()),
        step(0x02, "full_os", apply_image()),
    ]);

    let errors: CompileErrors =
        compile_and_validate(&document).expect_err("the sequence breaks rules");

    let nodes: Vec<_> = (&errors)
        .into_iter()
        .map(|error| error.path.to_string())
        .collect();
    assert_eq!(
        nodes,
        ["root.children[0]", "root.children[1]", "root.children[1]"]
    );
}
