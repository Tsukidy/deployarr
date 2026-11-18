//! `validate`, the save-time check of an authored sequence, beside add-on
//! steps and join slots. It reports the rules that planning checks, with each
//! add-on step and join slot unexpanded. With a catalog, it checks each add-on
//! step against its kind's descriptor. `validate_join_profile` checks a join
//! profile's config the same way. `tests/plan.rs` tests the three rules about
//! add-on steps, join slots, and the actions that only an expansion may hold.

use deployarr_core::addon::AddOnRef;
use deployarr_core::catalog::Catalog;
use deployarr_core::form::{ValueError, ValueProblem};
use deployarr_core::join::{JoinProfile, JoinProfileIssue, JoinStage, validate_join_profile};
use deployarr_core::plan::{NodePath, Rule, SaveCheck, SaveIssue, Severity, validate};
use deployarr_core::sequence::{AuthoredSequence, NodeId, RunIn};
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

fn register() -> AddOnRef {
    AddOnRef {
        addon: "acme.autopilot".parse().expect("a valid add-on ID"),
        kind: "register".parse().expect("a valid kind ID"),
    }
}

/// A catalog whose `register` step kind runs with `run_in`, so that the
/// add-on steps in a test with that `run_in` see no finding about their kind.
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

/// Group `n` with a `wmi` condition, which holds one step with `run_in`
/// `either`.
fn wmi_group(n: u32) -> Value {
    when(group(n, vec![step(n + 1, "either", command())]), wmi())
}

fn before_installed_os() -> Rule {
    Rule::FullOsConditionBeforeInstalledOs { condition: "wmi" }
}

fn finding(n: u32, path: Vec<usize>, check: SaveCheck) -> SaveIssue {
    SaveIssue {
        node: id(n),
        path: NodePath::from(path),
        check,
    }
}

#[test]
fn a_run_in_that_the_action_does_not_allow_beside_an_add_on_step_is_an_error() {
    let sequence = sequence(vec![
        step(2, "full_os", add_on()),
        step(3, "full_os", apply_image()),
    ]);

    let findings = validate(&sequence, &known("full_os"));

    assert_eq!(
        findings,
        [finding(
            3,
            vec![1],
            SaveCheck::Rule(Rule::RunIn {
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
                SaveCheck::Rule(Rule::DuplicateNodeId {
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
                SaveCheck::Rule(Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                }),
            ),
        ),
    ];

    for (sequence, expected) in cases {
        let findings = validate(&sequence, &known("full_os"));

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
        validate(&sequence, &known("full_os")),
        [finding(
            3,
            vec![1],
            SaveCheck::Rule(Rule::FullOsCondition {
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
        validate(&sequence, &known("win_pe")),
        [finding(
            3,
            vec![1],
            SaveCheck::Rule(Rule::RestartBeforePartition)
        )]
    );
}

#[test]
fn an_add_on_steps_own_wmi_condition_before_the_first_restart_into_the_os_is_an_error() {
    // An add-on step's own condition comes before its expansion, so the rule
    // checks it at save.
    let sequence = sequence(vec![
        when(step(2, "either", add_on()), wmi()),
        when(step(3, "win_pe", join_slot("offline_image")), wmi()),
        wmi_group(4),
    ]);

    let findings = validate(&sequence, &known("either"));

    // The `win_pe` join slot gets one finding for its condition. The group
    // comes after an add-on step, so planning checks it.
    assert_eq!(
        findings,
        [
            finding(2, vec![0], SaveCheck::Rule(before_installed_os())),
            finding(
                3,
                vec![1],
                SaveCheck::Rule(Rule::FullOsCondition { condition: "wmi" })
            ),
        ]
    );
}

#[test]
fn a_wmi_group_before_the_first_add_on_step_is_an_error_at_save() {
    let sequence = sequence(vec![
        wmi_group(2),
        step(4, "either", add_on()),
        wmi_group(5),
    ]);

    assert_eq!(
        validate(&sequence, &known("either")),
        [finding(2, vec![0], SaveCheck::Rule(before_installed_os()))]
    );
}

#[test]
fn a_disabled_add_on_step_defers_nothing() {
    let cases = [
        disabled(step(2, "either", add_on())),
        disabled(group(2, vec![step(5, "either", add_on())])),
    ];

    for earlier in cases {
        let sequence = sequence(vec![earlier.clone(), wmi_group(3)]);

        assert_eq!(
            validate(&sequence, &known("either")),
            [finding(3, vec![1], SaveCheck::Rule(before_installed_os()))],
            "{earlier}"
        );
    }
}

#[test]
fn an_unknown_step_kind_alone_is_one_warning() {
    let sequence = sequence(vec![
        step(2, "win_pe", partition()),
        step(3, "full_os", add_on()),
        step(4, "full_os", join_slot("full_os")),
    ]);

    let findings = validate(&sequence, &Catalog::default());

    assert_eq!(
        findings,
        [finding(
            3,
            vec![1],
            SaveCheck::UnknownStepKind { kind: register() }
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

    let findings = validate(&sequence, &Catalog::default());

    let unknown = SaveCheck::UnknownStepKind { kind: register() };
    assert_eq!(
        findings,
        [
            finding(
                3,
                vec![0, 0],
                SaveCheck::Rule(Rule::FullOsCondition {
                    condition: "installed_msi",
                }),
            ),
            finding(3, vec![0, 0], unknown.clone()),
            finding(
                4,
                vec![0, 1],
                SaveCheck::Rule(Rule::RunIn {
                    action: "apply_os_image",
                    run_in: RunIn::FullOs,
                    allowed: &[RunIn::WinPe],
                }),
            ),
            finding(
                5,
                vec![1],
                SaveCheck::Rule(Rule::JoinStageRunIn {
                    stage: JoinStage::FullOs,
                    run_in: RunIn::WinPe,
                }),
            ),
            finding(
                6,
                vec![2],
                SaveCheck::Rule(Rule::AddOnOnlyAction {
                    action: "server_call",
                }),
            ),
            finding(3, vec![3], unknown),
            finding(
                3,
                vec![3],
                SaveCheck::Rule(Rule::DuplicateNodeId {
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

// The checks against the kind's descriptor in the catalog.

/// A catalog whose `register` step kind runs in the full OS. Config version 1
/// needs `group_tag`. Version 2 adds an optional integer `timeout` from 1 to
/// 600. The `bulk-enrollment` join kind has the same schemas.
fn catalog(availability: &str) -> Catalog {
    let schemas = json!({
        "current": 2,
        "versions": {
            "1": {"field": [
                {"key": "group_tag", "label": "Group tag", "kind": "template", "required": true},
            ]},
            "2": {"field": [
                {"key": "group_tag", "label": "Group tag", "kind": "template", "required": true},
                {"key": "timeout", "label": "Timeout", "kind": "integer", "min": 1, "max": 600},
            ]},
        },
    });
    serde_json::from_value(json!({
        "acme.autopilot": {
            "step_kinds": {
                "register": {
                    "label": "Register with Autopilot",
                    "run_in": "full_os",
                    "availability": availability,
                    "config": schemas,
                },
            },
            "join_kinds": {
                "bulk-enrollment": {
                    "label": "Bulk enrollment",
                    "availability": availability,
                    "config": schemas,
                },
            },
        },
    }))
    .expect("the catalog parses")
}

/// A `register` step with this config version and config.
fn register_step(n: u32, run_in: &str, config_version: u32, config: Value) -> Value {
    step(
        n,
        run_in,
        json!({
            "add_on": {
                "addon": "acme.autopilot",
                "kind": "register",
                "config_version": config_version,
                "config": config,
            },
        }),
    )
}

/// A sequence whose one child is a `register` step with ID 2, parsed from
/// JSON text, so the step's config keeps `config`, its text.
fn sequence_with_config_text(config_version: u32, config: &str) -> AuthoredSequence {
    let text = format!(
        r#"{{"schema_version":1,"id":"TEST","version":1,"name":"Test","default_vars":[],"root":{{"meta":{},"children":[{{"step":{{"meta":{},"run_in":"full_os","action":{{"add_on":{{"addon":"acme.autopilot","kind":"register","config_version":{config_version},"config":{config}}}}}}}}}]}}}}"#,
        meta(1),
        meta(2),
    );
    AuthoredSequence::from_json(&text).expect("the document parses")
}

fn config_error(path: &str, problem: ValueProblem) -> SaveCheck {
    SaveCheck::Config(ValueError {
        path: path.to_owned(),
        problem,
    })
}

#[test]
fn a_config_that_its_schema_accepts_gives_no_finding() {
    let sequence = sequence(vec![
        register_step(2, "full_os", 1, json!({"group_tag": "lab"})),
        register_step(3, "full_os", 2, json!({"group_tag": "lab", "timeout": 30})),
    ]);

    assert_eq!(validate(&sequence, &catalog("enabled")), []);
}

#[test]
fn a_config_that_lacks_a_required_field_is_an_error_at_the_field() {
    let sequence = sequence(vec![register_step(2, "full_os", 2, json!({"timeout": 30}))]);

    let findings = validate(&sequence, &catalog("enabled"));

    assert_eq!(
        findings,
        [finding(
            2,
            vec![0],
            config_error("/group_tag", ValueProblem::Missing)
        )]
    );
    assert_eq!(findings[0].severity(), Severity::Error);
    assert_eq!(
        findings[0].to_string(),
        format!(
            "root.children[0], node {}: error: the config does not fit its schema: `/group_tag`: the field is required",
            id_text(2)
        )
    );
}

#[test]
fn a_config_key_that_the_schema_does_not_declare_is_an_error_at_the_key() {
    // Version 1 has no `timeout` field.
    let sequence = sequence(vec![register_step(
        2,
        "full_os",
        1,
        json!({"group_tag": "lab", "timeout": 30}),
    )]);

    assert_eq!(
        validate(&sequence, &catalog("enabled")),
        [finding(
            2,
            vec![0],
            config_error("/timeout", ValueProblem::UnknownKey)
        )]
    );
}

#[test]
fn each_problem_in_a_config_is_its_own_error() {
    let sequence = sequence(vec![register_step(
        2,
        "full_os",
        2,
        json!({"extra": true, "timeout": 0}),
    )]);

    assert_eq!(
        validate(&sequence, &catalog("enabled")),
        [
            finding(2, vec![0], config_error("/extra", ValueProblem::UnknownKey)),
            finding(
                2,
                vec![0],
                config_error("/group_tag", ValueProblem::Missing)
            ),
            finding(
                2,
                vec![0],
                config_error("/timeout", ValueProblem::BelowMin { min: 1 })
            ),
        ]
    );
}

#[test]
fn a_disabled_step_with_a_bad_config_gets_the_same_error() {
    let bad = || register_step(3, "full_os", 2, json!({"timeout": 30}));
    let error = config_error("/group_tag", ValueProblem::Missing);

    for (sequence, path) in [
        (sequence(vec![bad()]), vec![0]),
        (sequence(vec![disabled(bad())]), vec![0]),
        (sequence(vec![disabled(group(2, vec![bad()]))]), vec![0, 0]),
    ] {
        assert_eq!(
            validate(&sequence, &catalog("enabled")),
            [finding(3, path, error.clone())]
        );
    }
}

#[test]
fn a_step_whose_run_in_differs_from_its_kinds_is_an_error_that_names_both() {
    let sequence = sequence(vec![register_step(
        2,
        "either",
        1,
        json!({"group_tag": "lab"}),
    )]);

    let findings = validate(&sequence, &catalog("enabled"));

    assert_eq!(
        findings,
        [finding(
            2,
            vec![0],
            SaveCheck::StepKindRunIn {
                kind: register(),
                declared: RunIn::FullOs,
                run_in: RunIn::Either,
            }
        )]
    );
    assert_eq!(findings[0].severity(), Severity::Error);
    assert_eq!(
        findings[0].to_string(),
        format!(
            "root.children[0], node {}: error: the step kind `register` of `acme.autopilot` runs only with `run_in` `full_os`, and this step has `either`",
            id_text(2)
        )
    );
}

#[test]
fn a_config_version_with_no_schema_is_one_warning() {
    // The catalog holds versions 1 and 2. Version 0 is older, and version 3
    // is newer, as in a step from a server with a newer add-on. The config
    // fits no schema that the catalog holds, and still gets no error.
    for config_version in [0, 3] {
        let sequence = sequence(vec![register_step(
            2,
            "full_os",
            config_version,
            json!({"tag": "lab"}),
        )]);

        let findings = validate(&sequence, &catalog("enabled"));

        assert_eq!(
            findings,
            [finding(
                2,
                vec![0],
                SaveCheck::NoConfigSchema {
                    kind: register(),
                    config_version,
                }
            )]
        );
        assert_eq!(findings[0].severity(), Severity::Warning);
        assert_eq!(
            findings[0].to_string(),
            format!(
                "root.children[0], node {}: warning: the config could not be checked, because this server holds no schema for config version {config_version} of the step kind `register` of `acme.autopilot`",
                id_text(2)
            )
        );
    }
}

#[test]
fn a_config_version_newer_than_the_kinds_current_one_is_checked_against_its_schema() {
    // The catalog keeps the schema of version 2, and the installed add-on
    // writes version 1.
    let mut value = serde_json::to_value(catalog("enabled")).expect("a catalog serializes");
    value["acme.autopilot"]["step_kinds"]["register"]["config"]["current"] = json!(1);
    let catalog: Catalog = serde_json::from_value(value).expect("the catalog parses");
    let sequence = sequence(vec![register_step(
        2,
        "full_os",
        2,
        json!({"group_tag": "lab", "timeout": 601}),
    )]);

    assert_eq!(
        validate(&sequence, &catalog),
        [finding(
            2,
            vec![0],
            config_error("/timeout", ValueProblem::AboveMax { max: 600 })
        )]
    );
}

#[test]
fn a_step_kind_that_no_catalog_knows_is_still_one_warning() {
    let gone = AddOnRef {
        addon: "acme.gone".parse().expect("a valid add-on ID"),
        kind: "register".parse().expect("a valid kind ID"),
    };
    let sequence = sequence(vec![step(
        2,
        "win_pe",
        json!({
            "add_on": {
                "addon": "acme.gone",
                "kind": "register",
                "config_version": 7,
                "config": {"anything": 1},
            },
        }),
    )]);

    let findings = validate(&sequence, &catalog("enabled"));

    assert_eq!(
        findings,
        [finding(
            2,
            vec![0],
            SaveCheck::UnknownStepKind { kind: gone }
        )]
    );
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn the_kinds_availability_does_not_change_the_findings() {
    let sequence = sequence(vec![
        register_step(2, "full_os", 1, json!({"group_tag": "lab"})),
        register_step(3, "win_pe", 2, json!({"timeout": 30})),
        register_step(4, "full_os", 9, json!({})),
    ]);

    let enabled = validate(&sequence, &catalog("enabled"));

    assert_eq!(enabled.len(), 3, "{enabled:?}");
    for availability in [
        "disabled",
        "failed",
        "incompatible",
        "revoked",
        "not_installed",
    ] {
        assert_eq!(
            validate(&sequence, &catalog(availability)),
            enabled,
            "{availability}"
        );
    }
}

#[test]
fn a_steps_own_findings_come_before_the_compilers_at_one_node() {
    let sequence = sequence(vec![
        register_step(2, "full_os", 1, json!({"group_tag": "lab"})),
        when(
            register_step(2, "win_pe", 2, json!({"timeout": 30})),
            installed_msi(),
        ),
    ]);

    assert_eq!(
        validate(&sequence, &catalog("enabled")),
        [
            finding(
                2,
                vec![1],
                SaveCheck::Rule(Rule::FullOsCondition {
                    condition: "installed_msi",
                }),
            ),
            finding(
                2,
                vec![1],
                SaveCheck::StepKindRunIn {
                    kind: register(),
                    declared: RunIn::FullOs,
                    run_in: RunIn::WinPe,
                },
            ),
            finding(
                2,
                vec![1],
                config_error("/group_tag", ValueProblem::Missing)
            ),
            finding(
                2,
                vec![1],
                SaveCheck::Rule(Rule::DuplicateNodeId {
                    first: NodePath::from(vec![0]),
                }),
            ),
        ]
    );
}

#[test]
fn validate_leaves_the_config_text_unchanged() {
    let config = r#"{"timeout":12345678901234567890123,"group_tag":"lab"}"#;
    let sequence = sequence_with_config_text(2, config);

    let findings = validate(&sequence, &catalog("enabled"));

    assert_eq!(
        findings,
        [finding(
            2,
            vec![0],
            config_error(
                "/timeout",
                ValueProblem::WrongType {
                    expected: "a whole number from -2^63 to 2^63 - 1"
                }
            )
        )]
    );
    let text = serde_json::to_string(&sequence).expect("the sequence serializes");
    assert!(text.contains(config), "{text}");
}

#[test]
fn a_config_that_the_check_cannot_read_is_an_error() {
    // The saved text holds a number that a `serde_json::Value` cannot hold.
    let sequence = sequence_with_config_text(2, r#"{"group_tag":"lab","timeout":1e400}"#);

    let findings = validate(&sequence, &catalog("enabled"));

    assert_eq!(findings, [finding(2, vec![0], SaveCheck::UnreadableConfig)]);
    assert_eq!(findings[0].severity(), Severity::Error);
}

// The check of a join profile against the catalog.

fn add_on_profile(addon: &str, config_version: u32, config: &str) -> JoinProfile {
    serde_json::from_str(&format!(
        r#"{{"add_on":{{"kind":{{"addon":"{addon}","kind":"bulk-enrollment"}},"config_version":{config_version},"config":{config}}}}}"#
    ))
    .expect("the profile parses")
}

fn bulk_enrollment(addon: &str) -> AddOnRef {
    AddOnRef {
        addon: addon.parse().expect("a valid add-on ID"),
        kind: "bulk-enrollment".parse().expect("a valid kind ID"),
    }
}

#[test]
fn a_join_profile_whose_config_fits_its_schema_gives_no_finding() {
    let profile = add_on_profile("acme.autopilot", 2, r#"{"group_tag":"lab","timeout":5}"#);

    assert_eq!(validate_join_profile(&profile, &catalog("enabled")), []);
}

#[test]
fn a_join_profile_of_a_kind_that_no_catalog_knows_is_a_warning() {
    let profile = add_on_profile("acme.gone", 2, r#"{"anything":1}"#);

    let issues = validate_join_profile(&profile, &catalog("enabled"));

    assert_eq!(
        issues,
        [JoinProfileIssue::UnknownJoinKind {
            kind: bulk_enrollment("acme.gone"),
        }]
    );
    assert_eq!(issues[0].severity(), Severity::Warning);
    assert_eq!(
        issues[0].to_string(),
        "no add-on that this server knows declares the join kind `bulk-enrollment` of `acme.gone`"
    );
}

#[test]
fn a_join_profiles_config_version_with_no_schema_is_a_warning() {
    let profile = add_on_profile("acme.autopilot", 3, r#"{"tag":"lab"}"#);

    let issues = validate_join_profile(&profile, &catalog("enabled"));

    assert_eq!(
        issues,
        [JoinProfileIssue::NoConfigSchema {
            kind: bulk_enrollment("acme.autopilot"),
            config_version: 3,
        }]
    );
    assert_eq!(issues[0].severity(), Severity::Warning);
    assert_eq!(
        issues[0].to_string(),
        "the config could not be checked, because this server holds no schema for config version 3 of the join kind `bulk-enrollment` of `acme.autopilot`"
    );
}

#[test]
fn each_problem_in_a_join_profiles_config_is_an_error_with_its_pointer() {
    let profile = add_on_profile("acme.autopilot", 2, r#"{"extra":true,"timeout":601}"#);

    let issues = validate_join_profile(&profile, &catalog("not_installed"));

    let error = |path: &str, problem| {
        JoinProfileIssue::Config(ValueError {
            path: path.to_owned(),
            problem,
        })
    };
    assert_eq!(
        issues,
        [
            error("/extra", ValueProblem::UnknownKey),
            error("/group_tag", ValueProblem::Missing),
            error("/timeout", ValueProblem::AboveMax { max: 600 }),
        ]
    );
    assert!(
        issues
            .iter()
            .all(|issue| issue.severity() == Severity::Error)
    );
    assert_eq!(
        issues[1].to_string(),
        "the config does not fit its schema: `/group_tag`: the field is required"
    );
}

#[test]
fn a_join_profile_config_that_the_check_cannot_read_is_an_error() {
    let profile = add_on_profile(
        "acme.autopilot",
        2,
        r#"{"group_tag":"lab","timeout":1e400}"#,
    );

    let issues = validate_join_profile(&profile, &catalog("enabled"));

    assert_eq!(issues, [JoinProfileIssue::UnreadableConfig]);
    assert_eq!(issues[0].severity(), Severity::Error);
}

#[test]
fn a_workgroup_or_domain_profile_gives_no_finding() {
    for text in [
        r#"{"workgroup":{"name":"WORKGROUP"}}"#,
        r#"{"domain":{"domain":"corp.example","ou":null,"account":"join","password":"domain-join-password"}}"#,
    ] {
        let profile: JoinProfile = serde_json::from_str(text).expect("the profile parses");
        assert_eq!(
            validate_join_profile(&profile, &catalog("enabled")),
            [],
            "{text}"
        );
    }
}
