//! MDT 8456's standard client task sequence, rebuilt in deployarr's model.
//!
//! `data/mdt-8456-client.json` is a hand-written port of
//! `Source/MDT.8456/Templates/Client.xml` in
//! <https://github.com/keithga/DeployShared> at commit
//! `460f6f9f60e7ac43bedeb20e6ff17e1dbdd30310`. This repository does not hold
//! Microsoft's file, so fetch it from there to compare. The MDT paths below
//! are relative to `Source/MDT.8456`.
//!
//! The port follows these rules:
//!
//! - Every group and step keeps its place, its name, its condition, and its
//!   `disable` and `continueOnError` values. A node's ID ends in the line of
//!   `Client.xml` where the node starts, so the node whose ID ends in
//!   `000000000106` is the step at line 106. The root stands for the
//!   `<sequence>` element at line 2.
//! - Every condition in `Client.xml` is a `Variable` condition, which becomes
//!   a `var` condition. The `or` around the Validation group's one condition
//!   becomes an `any` with one child.
//! - A step becomes the action for its feature, with a `run_in` that the
//!   compiler's table allows. MDT marks most steps `WinPEandFullOS`, which is
//!   `either` for the actions that allow it.
//! - "Next Phase" runs `ZTINextPhase.wsf`, which moves `PHASE` to the next
//!   phase (`scripts/ZTINextPhase.wsf:74-91`). Each one becomes a
//!   `set_variable` of `PHASE` to the next phase. After Preinstall that is
//!   `INSTALL`. The script skips Install only when `DeploymentMethod` is
//!   `OSD` (`scripts/ZTINextPhase.wsf:79-84`).
//! - "Restart computer" becomes `restart`. MDT's step only asks for a reboot
//!   ([MDT 8450 binaries reference](../../../docs/research/mdt-8450-binaries.md#reboots)),
//!   so the PC starts what the steps before it set up. The steps at lines 106
//!   and 480 follow `LTIApply.wsf /PE`, which makes WinPE on the disk the
//!   default OS (`scripts/LTIApply.wsf:155-157`), so they restart into WinPE.
//!   The step at line 345 ends Postinstall, after the image is applied, so it
//!   restarts into the installed OS (inferred).
//! - Both "Configure" steps run `ZTIConfigure.wsf`, which fills in the answer
//!   file ([MDT 8456 reference](../../../docs/research/mdt-8456.md#unattend)),
//!   and both become `apply_unattend`.
//! - The "Inject Drivers" step at line 275 becomes `apply_drivers` with
//!   `auto`, the mode that matches its `DriverInjectionMode=AUTO`
//!   ([Drivers](../../../docs/architecture.md#selection)). The one at line 330
//!   stays a placeholder. In the Lite Touch Postinstall phase,
//!   `ZTIDrivers.wsf` copies no drivers and changes only XP answer files
//!   (`scripts/ZTIDrivers.wsf:171-176, 266-268`).
//! - The port keeps MDT's order, and the compiler does not check the order of
//!   these actions. MDT stages drivers and fills its answer file in
//!   Preinstall, and `LTIApply.wsf` applies both after the image
//!   ([MDT 8456 reference](../../../docs/research/mdt-8456.md#drivers)). So
//!   the `apply_drivers` and the first `apply_unattend` come before
//!   `apply_os_image`. A sequence written for deployarr puts them after it.
//! - A step whose feature has no deployarr action is a `run_command`
//!   placeholder that keeps MDT's command line. A comment in [`OUTLINE`] names
//!   each one's feature. User state migration, BitLocker, and Windows Update
//!   are in the architecture's "Not in the first release". The comment
//!   `refresh` marks a step that only a refresh runs. A refresh starts in the
//!   old Windows (`scripts/LiteTouch.wsf:380-389`), and its State Capture phase
//!   saves the user state (`Templates/Client.xml:61-70`).
//! - `LiteTouch.wsf` sets `PHASE` and `DeploymentType` when a deployment
//!   starts (`scripts/LiteTouch.wsf:371-391`). deployarr has no such script,
//!   so the port sets both as sequence defaults, to the values for a start in
//!   WinPE.
//! - The port leaves out what the model has no place for. The
//!   `<globalVarList>` holds the OS install step's parameters
//!   (`Templates/Client.xml:3-11`), and each `<defaultVarList>` holds its
//!   step's parameters. A typed action holds its parameters as fields, and a
//!   placeholder has none. A `description` is only text. Only the editor uses
//!   `expand` ([MDT 8456 reference](../../../docs/research/mdt-8456.md#task-sequence-format)),
//!   and the engine never reads `startIn`
//!   ([MDT 8450 binaries reference](../../../docs/research/mdt-8450-binaries.md#instruction-table-and-failure-handling)).

use deployarr_core::compile::{Instr, OnError, Pc, compile};
use deployarr_core::sequence::TaskSequence;
use serde_json::Value;

const CLIENT: &str = include_str!("data/mdt-8456-client.json");

/// The phases in the order that `ZTINextPhase.wsf` moves through them
/// (`scripts/ZTINextPhase.wsf:74-91`).
const PHASES: [&str; 6] = [
    "VALIDATION",
    "STATECAPTURE",
    "PREINSTALL",
    "INSTALL",
    "POSTINSTALL",
    "STATERESTORE",
];

/// One line for each group and step of the port, in document order. Each
/// line starts with the line of `Client.xml` where the node starts. The
/// comment on a `run_command` placeholder names its feature.
#[rustfmt::skip]
const OUTLINE: &[&str] = &[
    "  2 group Standard Client Task Sequence",
    " 12   group Initialization",
    " 13     step Gather local only: run_command, run_in=either", // the agent gathers facts itself
    " 21   group Validation, if any(PHASE equals VALIDATION)",
    " 31     step Validate: check_readiness, run_in=win_pe",
    " 40     step Check BIOS: run_command, run_in=either", // BIOS block list, no deployarr feature
    " 43     step Next Phase: set_variable name=PHASE value=STATECAPTURE, run_in=either",
    " 47   group State Capture, if PHASE equals STATECAPTURE",
    " 55     step Generate Application Migration File: run_command, run_in=either", // user state migration
    " 61     step Capture User State: run_command, run_in=either, if USMTOfflineMigration not_equals TRUE", // user state migration
    " 71     step Capture Groups: run_command, run_in=either", // user state migration
    " 74     step Capture Network Settings: run_command, run_in=either", // refresh
    " 77     group Refresh only, if DeploymentType equals REFRESH",
    " 85       step Disable BDE Protectors: run_command, run_in=either", // BitLocker
    " 91       step Apply Windows PE: run_command, run_in=either", // refresh
    " 95     step Next Phase: set_variable name=PHASE value=PREINSTALL, run_in=either",
    " 98     group Refresh only, if DeploymentType equals REFRESH",
    "106       step Restart computer: restart into=win_pe, run_in=either",
    "116   group Preinstall, if PHASE equals PREINSTALL",
    "124     step Gather local only: run_command, run_in=either", // the agent gathers facts itself
    "131     group New Computer only, if DeploymentType equals NEWCOMPUTER",
    "139       step Validate: check_readiness, run_in=win_pe",
    "148       step Format and Partition Disk (BIOS): partition_disk layout=bios-mbr, run_in=win_pe, if IsUEFI not_equals True",
    "189       step Format and Partition Disk (UEFI): partition_disk layout=uefi-gpt, run_in=win_pe, if IsUEFI equals True",
    "238       step Copy scripts: run_command, run_in=either", // the agent moves its own state to the disk
    "242     group Offline User State Capture, if USMTOfflineMigration equals TRUE",
    "250       step Offline User State Capture: run_command, run_in=either", // user state migration
    "253       step Unload USMT Hive: run_command, run_in=either, continue on error", // user state migration
    "257     group Refresh only, if DeploymentType equals REFRESH",
    "265       step Backup: run_command, run_in=either", // refresh
    "269     step Configure: apply_unattend template=standard, run_in=win_pe",
    "272     step Enable BitLocker (Offline): run_command, run_in=either, continue on error", // BitLocker
    "275     step Inject Drivers: apply_drivers selection=auto, run_in=win_pe",
    "282     step Apply Patches: run_command, run_in=either", // offline update packages, no deployarr feature
    "288     step Next Phase: set_variable name=PHASE value=INSTALL, run_in=either",
    "292   group Install, if PHASE equals INSTALL",
    "300     step Install Operating System: apply_os_image image=win11-24h2-pro-x64, run_in=win_pe",
    "312     step Next Phase: set_variable name=PHASE value=POSTINSTALL, run_in=either",
    "316   group Postinstall, if PHASE equals POSTINSTALL",
    "324     step Copy Scripts: run_command, run_in=either", // the agent moves its own state to the disk
    "327     step Configure: apply_unattend template=standard, run_in=win_pe",
    "330     step Inject Drivers: run_command, run_in=either", // copies no drivers in Lite Touch, see above
    "333     step Apply Patches: run_command, run_in=either", // offline update packages, no deployarr feature
    "339     step Add Windows Recovery (WinRE): run_command, run_in=either", // deployarr sets up WinRE with the disk layout
    "342     step Next Phase: set_variable name=PHASE value=STATERESTORE, run_in=either",
    "345     step Restart computer: restart into=installed_os, run_in=either",
    "354   group State Restore, if PHASE equals STATERESTORE",
    "362     step Gather local only: run_command, run_in=either", // the agent gathers facts itself
    "369     step Post-Apply Cleanup: run_command, run_in=either", // the agent cleans up at the end
    "372     step Recover From Domain: run_command, run_in=either", // checks the join that the answer file made, no deployarr feature
    "378     step Tattoo: run_command, run_in=either, continue on error", // no deployarr feature
    "381     step Opt In to CEIP and WER: run_command, run_in=either, continue on error, disabled", // no deployarr feature
    "384     step Windows Update (Pre-Application Installation): run_command, run_in=either, continue on error, disabled", // Windows Update
    "387     step Install Applications: install_applications apps={\"from_list\":\"Applications\"}, run_in=full_os",
    "394     step Windows Update (Post-Application Installation): run_command, run_in=either, continue on error, disabled", // Windows Update
    "397     group Custom Tasks",
    "398     step Enable BitLocker: run_command, run_in=either, if BdeInstallSuppress not_equals YES, continue on error", // BitLocker
    "415     step Restore User State: run_command, run_in=either", // user state migration
    "418     step Restore Groups: run_command, run_in=either", // user state migration
    "421     step Apply Local GPO Package: run_command, run_in=either, if ApplyGPOPack not_equals NO", // no deployarr feature
    "431     group Imaging",
    "433       group Prepare Only, if DoCapture equals PREPARE",
    "441         step Copy Sysprep files: run_command, run_in=either", // image capture, no deployarr feature
    "445       group Sysprep Only, if DoCapture equals SYSPREP",
    "453         step Add mass storage drivers to sysprep.inf for XP and 2003: run_command, run_in=either", // image capture
    "456         step Execute Sysprep: run_command, run_in=either", // image capture
    "460       group Capture Image, if DoCapture equals YES",
    "468         step Apply Windows PE: run_command, run_in=either", // image capture
    "471         step Add mass storage drivers to sysprep.inf for XP and 2003: run_command, run_in=either", // image capture
    "474         step Execute Sysprep: run_command, run_in=either", // image capture
    "477         step Apply Windows PE (BCD): run_command, run_in=either", // image capture
    "480         step Restart computer: restart into=win_pe, run_in=either",
    "488         step Gather local only: run_command, run_in=either", // image capture
    "495         step Create WIM: run_command, run_in=either", // image capture
];

fn fixture() -> Value {
    serde_json::from_str(CLIENT).expect("the fixture is JSON")
}

/// The one key of an object with one key, such as an action or a condition,
/// and its value.
fn only_entry(value: &Value) -> (&str, &Value) {
    let object = value.as_object().expect("an object");
    assert_eq!(object.len(), 1, "an object with one key: {value}");
    let (key, value) = object.iter().next().expect("one entry");
    (key, value)
}

fn text(value: &Value) -> &str {
    value.as_str().expect("a string")
}

/// The line of `Client.xml` that a node's ID names.
fn line_of(meta: &Value) -> u32 {
    let id = text(&meta["id"]);
    let digits = id.rsplit('-').next().expect("a UUID has groups");
    digits
        .parse()
        .expect("the last group of the ID is a line number")
}

fn condition_text(condition: &Value) -> String {
    match only_entry(condition) {
        ("var", var) => {
            let (op, operand) = only_entry(&var["test"]);
            format!("{} {op} {}", text(&var["name"]), text(operand))
        }
        ("any", children) => {
            let children: Vec<_> = children
                .as_array()
                .expect("a list")
                .iter()
                .map(condition_text)
                .collect();
            format!("any({})", children.join(", "))
        }
        (other, _) => panic!("Client.xml has no condition that becomes `{other}`"),
    }
}

/// The action's key and its fields. A placeholder's command line is left out.
fn action_text(action: &Value) -> String {
    let (kind, fields) = only_entry(action);
    let mut out = kind.to_owned();
    if kind == "run_command" {
        return out;
    }
    for (field, value) in fields.as_object().expect("an object") {
        let value = match value {
            Value::String(value) => value.clone(),
            value => value.to_string(),
        };
        out.push_str(&format!(" {field}={value}"));
    }
    out
}

/// Appends a line for a group or step, and then one for each node inside it.
/// `kind` is `group` or `step`, and `node` is the object under that key.
fn outline(kind: &str, node: &Value, depth: usize, lines: &mut Vec<String>) {
    let meta = &node["meta"];
    let mut line = format!(
        "{:>3} {:indent$}{kind} {}",
        line_of(meta),
        "",
        text(&meta["name"]),
        indent = 2 * depth,
    );
    if kind == "step" {
        line.push_str(&format!(
            ": {}, run_in={}",
            action_text(&node["action"]),
            text(&node["run_in"])
        ));
    }
    if let Some(condition) = meta.get("condition").filter(|value| !value.is_null()) {
        line.push_str(&format!(", if {}", condition_text(condition)));
    }
    if meta["continue_on_error"] == true {
        line.push_str(", continue on error");
    }
    if meta["enabled"] == false {
        line.push_str(", disabled");
    }
    lines.push(line);
    if kind == "group" {
        for child in node["children"].as_array().expect("a list") {
            let (kind, child) = only_entry(child);
            outline(kind, child, depth + 1, lines);
        }
    }
}

/// The phase that a condition tests: `PHASE equals <phase>`, alone or as the
/// one child of an `any`.
fn tested_phase(condition: &Value) -> Option<&str> {
    match only_entry(condition) {
        ("var", var) if var["name"] == "PHASE" => var["test"]["equals"].as_str(),
        ("any", children) => match children.as_array()?.as_slice() {
            [child] => tested_phase(child),
            _ => None,
        },
        _ => None,
    }
}

/// The values that `set_variable` steps in `group`, at any depth, give `PHASE`.
fn phase_sets<'a>(group: &'a Value, out: &mut Vec<&'a str>) {
    for child in group["children"].as_array().expect("a list") {
        match only_entry(child) {
            ("group", group) => phase_sets(group, out),
            ("step", step) => {
                let set = &step["action"]["set_variable"];
                if set["name"] == "PHASE" {
                    out.push(text(&set["value"]));
                }
            }
            (other, _) => panic!("a node is a group or a step, not `{other}`"),
        }
    }
}

#[test]
fn the_port_parses_and_compiles() {
    let sequence = TaskSequence::from_json(CLIENT).unwrap_or_else(|error| panic!("{error}"));

    let program = compile(&sequence).unwrap_or_else(|errors| panic!("{errors}"));

    // An `Enter` for each of the 18 groups, the root included, and an `Exec`
    // for each of the 53 steps that are not disabled. Client.xml has 56.
    assert_eq!(program.end(), Pc(18 + 53));
    // No group continues on error, so a failed step ends the deployment unless
    // the step itself continues on error, as 4 enabled steps do.
    let mut continuing = 0;
    for (pc, instr) in program.instrs().iter().enumerate() {
        match instr {
            Instr::Enter { skip_to, .. } => {
                assert!(Pc(pc) < *skip_to && *skip_to <= program.end());
            }
            Instr::Exec {
                on_error: OnError::Continue,
                ..
            } => continuing += 1,
            Instr::Exec { on_error, .. } => assert_eq!(*on_error, OnError::FailDeployment),
        }
    }
    assert_eq!(continuing, 4);
}

#[test]
fn the_port_keeps_every_group_step_and_condition_of_client_xml() {
    let mut lines = Vec::new();

    outline("group", &fixture()["root"], 0, &mut lines);

    assert_eq!(lines.join("\n"), OUTLINE.join("\n"));
}

#[test]
fn each_phase_moves_the_deployment_to_the_next_phase() {
    let fixture = fixture();
    let phase_groups: Vec<(&str, &Value)> = fixture["root"]["children"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|child| child.get("group"))
        .filter_map(|group| Some((tested_phase(group["meta"].get("condition")?)?, group)))
        .collect();

    let phases: Vec<&str> = phase_groups.iter().map(|(phase, _)| *phase).collect();
    assert_eq!(phases, PHASES);
    for (index, (phase, group)) in phase_groups.iter().enumerate() {
        let mut sets = Vec::new();
        phase_sets(group, &mut sets);
        // The last phase sets none. `ZTINextPhase.wsf` would clear `PHASE`.
        let next: Vec<&str> = PHASES.get(index + 1).into_iter().copied().collect();
        assert_eq!(sets, next, "the {phase} group");
    }
    // A deployment that starts in WinPE starts at Preinstall, as a new computer.
    assert_eq!(
        fixture["default_vars"],
        serde_json::json!([
            { "name": "PHASE", "value": "PREINSTALL" },
            { "name": "DeploymentType", "value": "NEWCOMPUTER" },
        ])
    );
}
