use deployarr_core::compile::OnError::{Continue, FailDeployment};
use deployarr_core::compile::{
    CompileError, CompileErrors, Instr, NodePath, OnError, Pc, Program, Rule, compile,
};
use deployarr_core::sequence::{Group, Node, NodeId, RunIn, Step, TaskSequence};
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

fn parse(document: &Value) -> TaskSequence {
    TaskSequence::from_json(&document.to_string()).expect("the document parses")
}

fn compile_ok(document: &Value) -> Program {
    let program = compile(&parse(document)).unwrap_or_else(|errors| panic!("{errors}"));
    assert_jumps_forward(&program);
    program
}

fn compile_err(document: &Value) -> Vec<CompileError> {
    match compile(&parse(document)) {
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

/// Every jump lands after its own instruction and no later than the end.
fn assert_jumps_forward(program: &Program) {
    for (pc, instr) in program.instrs().iter().enumerate() {
        let target = match instr {
            Instr::Enter { skip_to, .. } => Some(*skip_to),
            Instr::Exec {
                on_error: OnError::JumpTo(target),
                ..
            } => Some(*target),
            Instr::Exec { .. } => None,
        };
        if let Some(target) = target {
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

/// The steps of the tree in document order, with the disabled ones and the
/// ones inside a disabled group left out.
fn kept_steps<'a>(group: &'a Group, out: &mut Vec<&'a Step>) {
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
    let sequence = TaskSequence::from_json(SAMPLE).expect("the sample parses");

    let program = compile(&sequence).unwrap_or_else(|errors| panic!("{errors}"));

    assert_jumps_forward(&program);
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
            // The partition step comes first, so a `win_pe` restart is allowed.
            let document = root(vec![
                step(0x02, "win_pe", partition()),
                step(0x03, written, action.clone()),
            ]);

            let result = compile(&parse(&document));

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
                assert_eq!(path.indices(), [1]);
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

    let errors = compile(&parse(&document)).expect_err("the sequence breaks five rules");

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

    let errors: CompileErrors = compile(&parse(&document)).expect_err("the sequence breaks rules");

    let nodes: Vec<_> = (&errors)
        .into_iter()
        .map(|error| error.path.to_string())
        .collect();
    assert_eq!(
        nodes,
        ["root.children[0]", "root.children[1]", "root.children[1]"]
    );
}
