# Architecture

deployarr installs Windows on bare-metal PCs from one central server. A technician types the Windows product key. When lookup cannot find the PC, the technician also picks a task sequence from a list, or types its order number when the SQL order lookup add-on is installed. It copies the parts of Microsoft Deployment Toolkit (MDT) that work and fixes the faults listed in [MDT 8456 reference](research/mdt-8456.md) and [MDT 8450 binaries reference](research/mdt-8450-binaries.md). This page explains the design and why each choice was made. For the build order, see [Roadmap](roadmap.md).

## Requirements

These come from the product owner and drive every decision below.

- Zero-touch deployment, Windows first, except for the product key.
- A base program does the deployment and every standard task sequence feature. Optional features are add-ons that an admin installs separately, from an **Add add-on...** button in the web UI. See [Add-ons](#add-ons).
- Each PC has a Windows product key on a sticker. A technician types it during the deployment, on the same screen as the order number when lookup asks for one.
- Order lookup in an SQL database, as an optional add-on with high priority. It is the first add-on the owner named. The order's OS identifier selects the task sequence. A manual list of task sequences is the fallback.
- deployarr works with no order database. It then deploys from pre-staged records and the manual list.
- deployarr only reads the order database. It never writes to it, not even deployment status.
- A PC is matched in three ways, and all three must work: the order table holds the motherboard serial number, a technician scans or types the order number, or someone pre-stages the PC in the web UI. Pre-staging needs no order database. The first two ways come from the SQL order lookup add-on.
- The order database engine is not fixed. Support Microsoft SQL Server, PostgreSQL, and MySQL or MariaDB.
- Task sequences that work like MDT's: ordered groups and steps, conditions, variables, scripts, commands, and reboots.
- Every device has a working driver when the deployment finishes.
- Configurable applications, with a simple UI to manage drivers and software.
- A finished PC ends in one of two states, picked for each deployment. A PC that is ready to use skips OOBE and has a local admin account. A PC that is ready for the customer deploys in audit mode and then runs `sysprep /oobe /shutdown`, so the customer sees OOBE on first boot.
- PCs stay in a workgroup by default. Domain join is an option in the base program. Entra ID join and Autopilot are add-ons.
- Admins configure and run deployarr through a web UI. After the first release, a desktop program on the server shows the same UI in its own window.
- Install from an ISO or a WIM.
- A WinPE-based preinstall environment.
- PXE boot for UEFI with Secure Boot on and for legacy BIOS.
- PXE on one subnet, with as many PCs deploying at once as the server can serve. The design keeps routed subnets possible for a later release.
- One central deployment server that runs on Windows or Linux.

## Components

```
                 deployment server (Windows or Linux)
 +---------------------------------------------------------------------+
 |  deployarr-server (one binary)                                      |
 |                                                                     |
 |  PXE: proxyDHCP (UDP 67, 4011), TFTP (UDP 69)                       |
 |  HTTPS API for agents and the web UI, content downloads             |
 |  lookup chain and deployment planning                               |
 |  add-on host (wasmtime) -> connections: order DBs, HTTPS APIs       |
 |  libraries: OS images, drivers, apps, task sequences, boot images   |
 |  storage: SQLite database + content store directory                 |
 +---------------------------------------------------------------------+
            ^ HTTPS, server certificate pinned by the agent
            |
 target PC: PXE -> WinPE (deployarr-agent is the shell)
            -> new Windows (deployarr-agent runs as a Windows service)
```

There are two programs. `deployarr-server` runs on the deployment server. `deployarr-agent` is one Windows executable that runs on the target PC, first as the WinPE shell and then as a Windows service in the new OS. Everything else is a library crate that these two programs share. Add-ons are WebAssembly components that the server runs. The agent never loads add-on code. See [Add-ons](#add-ons).

## Deployment flow

1. The PC PXE-boots. The proxyDHCP service reads the client architecture (DHCP option 93) and sends Microsoft-signed boot programs for that firmware type, with DHCP option 252 naming the BCD. See [PXE](#pxe).
2. The boot manager loads WinPE from the server over TFTP. WinPE starts `deployarr-agent` from `winpeshl.ini`.
3. The agent initializes WinPE (`wpeinit`), loads the boot image's network and storage drivers with `drvload`, and connects to the server over HTTPS.
4. The agent gathers facts and sends them to the server. Facts include the SMBIOS data (system, baseboard, and chassis serials, UUID, make, model, and SKU), MAC addresses, firmware type, Secure Boot state, TPM version, memory, processor cores, disks, and the hardware IDs and compatible IDs of every device.
5. The server runs the lookup chain (see [Order lookup](#order-lookup)). The result is a task sequence and a set of variables, or a prompt for the technician, such as an add-on's order-number prompt or the manual list. If the task sequence activates Windows, the agent then asks for the product key. See [Activation](#activation).
6. The server pins the task sequence version. It asks each add-on to expand its steps into built-in steps, compiles the result, and sends the compiled program and variables to the agent. See [Task sequences stay loadable](#task-sequences-stay-loadable).
7. The agent runs the WinPE steps. A typical run checks readiness, partitions the disk, downloads and applies the OS image, injects drivers, writes the unattend file, runs `bcdboot`, and installs itself as a service in the offline image.
8. The PC reboots into Windows Setup. When Setup finishes, or reaches audit mode, the agent service resumes the sequence at the next step.
9. The agent installs applications, runs scripts, reboots as steps request, and checks that every device has a driver.
10. The agent uploads its logs, removes itself, and runs the finish action. A PC that is ready to use restarts or shuts down. A PC for the customer runs `sysprep /oobe /shutdown`. See [End state](#end-state).
11. The server marks the deployment complete. Later PXE boots of that PC fall through to the local disk.

## Core data shapes

The task sequence model lives in `deployarr-core`, a `no_std` crate with no I/O. See [Crate layout](#crate-layout). The server, the agent, and the web UI all use it. These sketches show the shape, not final code.

```rust
pub struct TaskSequence {
	pub id: TaskSequenceId,   // upper-case, like "W11-PRO-STD"
	pub version: u32,         // every save creates a new version
	pub name: String,
	pub default_vars: Vec<VarAssignment>,
	pub root: Group,
}

pub enum Node {
	Group(Group),
	Step(Step),
}

pub struct NodeMeta {
	pub id: NodeId,           // stable UUID. Survives a rename or a move
	pub name: String,
	pub enabled: bool,
	pub continue_on_error: bool,
	pub condition: Option<Condition>,
}

pub struct Group {
	pub meta: NodeMeta,
	pub children: Vec<Node>,
}

pub struct Step {
	pub meta: NodeMeta,
	pub run_in: RunIn,        // WinPe, FullOs, or Either. Checked against the action at save
	pub action: Action,
}

pub enum Action {
	RunCommand { command: Template, working_dir: Option<Template>, exit: ExitCodes, limits: StepLimits },
	RunPowerShell { script: ContentRef, args: Vec<Template>, exit: ExitCodes, limits: StepLimits },
	SetVariable { name: VarName, value: Template },
	Restart { into: RestartTarget },
	PartitionDisk { layout: DiskLayoutId, disk: DiskSelector },
	ApplyOsImage { image: OsImageRef },
	ApplyDrivers { selection: DriverSelection },
	ApplyUnattend { template: UnattendTemplateId },
	InstallApplications { apps: AppSelection },   // One(AppId) or FromList(VarName)
	VerifyDrivers { policy: MissingDriverPolicy },
	CheckReadiness { checks: ReadinessChecks },   // Windows 11 minimums by default
	ActivateWindows { online: bool },   // full OS only. Installs the product key typed in WinPE
}

pub struct ExitCodes {
	pub success: Vec<i32>,    // never empty. Default [0]
	pub reboot: Vec<i32>,     // success, then a reboot. Default [3010, 1641]
}

pub struct StepLimits {
	pub max_runtime_secs: Option<NonZeroU32>,   // whole seconds. Default none
	pub rerun_if_interrupted: bool,   // default false
}

pub enum Condition {
	All(Vec<Condition>),
	Any(Vec<Condition>),
	Not(Box<Condition>),
	Var { name: VarName, op: CompareOp, value: Template },
	Registry { key: Template, value: Option<String>, op: CompareOp, data: Option<RegData> },
	File { path: Template, version: Option<Comparison<FileVersion>>, modified: Option<Comparison<Timestamp>> },
	Folder { path: Template, modified: Option<Comparison<Timestamp>> },
	InstalledMsi(MsiProduct),   // ProductCode or UpgradeCode. Full OS only
	Wmi { namespace: String, query: Template, ignore_failure: bool },   // full OS only
}

pub enum CompareOp { Exists, NotExists, Equals, NotEquals, Less, LessEqual, Greater, GreaterEqual, Like }
```

Each MDT step type that does real work becomes a typed `Action` variant with typed parameters. `RunCommand` and `RunPowerShell` cover everything else. MDT passes every step's settings as untyped variables and command lines, so a typo becomes a runtime failure. With typed actions, the editor and the server reject a bad step when you save it.

The condition types match what MDT's engine evaluates. MDT also has make, model, location, and computer conditions, which compare by substring, so a gateway of `10.0.0.1` also matches `10.0.0.10`. deployarr expresses those as `Var` conditions on gathered facts, which compare exactly.

Add-ons extend this model in three ways. A stored step can hold a step from an add-on. A join slot marks where the deployment's join profile adds its steps. The agent's actions gain `ServerCall`, `AddProvisioningPackage`, and step files. The task sequence types take the action type as a parameter, so the compiler accepts only built-in actions. [Task sequences stay loadable](#task-sequences-stay-loadable) shows these shapes.

### Parameter types

```rust
pub struct NodeId(pub Uuid);
pub struct Template(String);   // the text as written. Substitution happens at run time
pub struct VarAssignment { pub name: VarName, pub value: Template }

pub enum RunIn { WinPe, FullOs, Either }
pub enum RestartTarget { WinPe, InstalledOs }
pub enum DiskSelector { FirstInternal }   // the default
pub enum DriverSelection { Auto, Profile, ProfileAll }
pub enum MissingDriverPolicy { Fail, Warn, Ignore }
pub enum AppSelection { One(AppId), FromList(VarName) }

pub struct ReadinessChecks {
	pub memory: MemoryCheck,
	pub disk: DiskCheck,
	pub processor: ProcessorCheck,
	pub uefi: CheckMode,             // default Fail
	pub tpm_2_0: CheckMode,          // default Fail
	pub secure_boot: CheckMode,      // default Warn
}

pub struct MemoryCheck { pub min_mib: u32, pub mode: CheckMode }        // mebibytes. Default 4096, Fail
pub struct DiskCheck { pub min_gb: u32, pub mode: CheckMode }           // decimal gigabytes. Default 64, Fail
pub struct ProcessorCheck { pub min_cores: u32, pub mode: CheckMode }   // processor cores. Default 2, Fail

pub enum CheckMode { Fail, Warn, Off }

pub struct Comparison<T> { pub op: CompareOp, pub value: T }
pub struct FileVersion { pub major: u16, pub minor: u16, pub build: u16, pub revision: u16 }
pub struct Timestamp(pub OffsetDateTime);
pub enum MsiProduct { ProductCode(Uuid), UpgradeCode(Uuid) }
pub enum RegData { String(String), Integer(i64) }   // compared by the `Var` rules
```

- `NodeId` wraps a UUID. The core crate parses and writes IDs and never generates one, so it leaves out the `uuid` crate's `v4` feature. On `wasm32-unknown-unknown`, `uuid` stops a build that has `v4` unless the build also turns on one of its randomness features: `js`, `rng-getrandom`, or `rng-rand`.
- `TaskSequenceId` holds one or more of `A` to `Z`, digits, `_`, or `-`.
- `VarName` holds 1 to 256 ASCII letters, digits, `_`, or `-`. Two names that differ only in case are equal. The name keeps the spelling its author wrote. The length, the character classes, and the case rule are MDT's. The ASCII limit is deployarr's choice. See [Variables](#variables).
- `DiskLayoutId`, `OsImageRef`, `UnattendTemplateId`, `ContentRef`, and `AppId` are strings that reject an empty value.
- `DiskSelector` has one rule so far, the first internal, non-removable disk.
- `MemoryCheck`, `DiskCheck`, and `ProcessorCheck` each hold a minimum and a `mode`, so the JSON for one is an object such as `{"min_mib": 4096, "mode": "fail"}`. The `uefi`, `tpm_2_0`, and `secure_boot` checks have no minimum, so each is a bare `CheckMode`, such as `"warn"`.
- `min_mib` is in mebibytes of 2^20 bytes, so the default 4096 is 4 GiB. `min_gb` is in decimal gigabytes of 10^9 bytes, the unit that disks are sold in, because a disk sold as 64 GB holds less than 64 GiB. `min_cores` counts processor cores.
- `max_runtime_secs` is a whole number of seconds and is never zero, because a zero limit fails every run. An earlier sketch had `max_runtime: Option<Duration>`. serde writes a `Duration` as an object with `secs` and `nanos`. deployarr limits a step in whole seconds, so the limit is one JSON integer and the field name carries the unit.
- `Comparison<T>` has named `op` and `value` fields. An earlier sketch had the tuple `(CompareOp, T)`. serde writes a tuple as a JSON array, so a parse error would point at `version[1]`. With named fields it points at `version.value`.
- `FileVersion` is written as `a.b.c.d` and orders by its four fields.
- `Timestamp` is written in RFC 3339, such as `2026-01-31T08:00:00Z`. An earlier sketch had `DateTime` and named no format. RFC 3339 requires a UTC offset, so every value names one instant.
- The core crate parses a `Timestamp` with the `time` crate and writes the text itself, because `time`'s formatter needs `std`. It writes the same text that `time` writes. A fraction of a second loses its trailing zeros, and a zero offset is `Z`.
- `MsiProduct` is written as a GUID without braces. `RegData` is written as a bare JSON string or integer.

### JSON format

A task sequence is stored and sent as one JSON document. `TaskSequence::from_json` parses it into the types above.

- The top-level `schema_version` is 1. The parser rejects any other version.
- Field names and variant names are in snake case, such as `continue_on_error` and `win_pe`. `RunPowerShell` is written as `run_powershell`.
- A variant with data is an object with one key, the variant's name, such as `{"run_command": {"command": "ipconfig"}}`. A variant without data is a string, such as `"win_pe"`. Each node in `children` is `{"group": ...}` or `{"step": ...}`. This is serde's externally tagged form. The other choice puts a `"type"` key inside the object. serde buffers such an object before it reads the variant, so the path of a parse error stops at `action` and does not reach the field that failed.
- A field that the document omits takes its default when it has one above: `exit`, `limits`, `disk`, `checks`, and each field inside them. An omitted `Option` field is none. Every other field is required.
- An unknown field is an error, so a misspelled field name never passes as a default.
- The serializer writes every field, defaults included. A stored sequence then keeps its meaning if a default changes later.
- A parse error gives the path of the field that failed, such as `root.children[0].step.action.run_command.exit.success[1]`, and the reason.
- A value that is valid but not in canonical form still parses, and the serializer writes it back in canonical form. A GUID in upper case or in braces is written in lower case without braces. A file version part with leading zeros, such as `010`, is written as `10`. A timestamp with a lower-case `t` or `z`, a space in place of `T`, or a `+00:00` offset is written with `T` and `Z`. Any other offset is kept.
- The parser checks each value by itself. It accepts an empty success list, an empty `All` or `Any`, and a `run_in` that the action cannot honor. The compiler rejects those when the sequence is saved, so that each error names the node. See [Compile the tree into a flat program](#compile-the-tree-into-a-flat-program).

`crates/deployarr-core/tests/data/windows-11-pro.json` is a sample that uses every action and every condition.

## Task sequence engine

### Compile the tree into a flat program

The server compiles a task sequence into a flat list of instructions before it sends it to the agent:

```rust
pub struct Program {
	pub source: (TaskSequenceId, u32),
	pub instrs: Vec<Instr>,
}

pub enum Instr {
	// If the condition is false, jump to `skip_to`, past the group's last child.
	Enter { node: NodeId, condition: Option<Condition>, skip_to: Pc },
	Exec { node: NodeId, on_error: OnError },
}

pub enum OnError {
	Continue,        // the step has continue_on_error
	JumpTo(Pc),      // the end of the nearest enclosing group with continue_on_error
	FailDeployment,
}
```

The compiler drops disabled nodes and works out `on_error` for each step, so a failure needs no group stack at run time. MDT's engine keeps disabled entries in its table and tracks failures with a stack of open groups, which it must save and rebuild on every resume (`_SMSTSInstructionStackString`). Working out `on_error` ahead of time removes that state.

Like MDT, deployarr checks a group's condition only when it reaches the group, and resumes from a saved instruction pointer. A variable that a step sets in WinPE can therefore enable a group that runs after a reboot. That is how MDT's phase groups work.

The compiler also rejects these mistakes when you save, which MDT finds only at run time or never:

- An empty success list. MDT's engine has no default, so an empty `successCodeList` fails every exit code, including 0.
- A step whose `run_in` its action cannot honor, such as `ApplyOsImage` in the full OS. MDT fails such a step with error 50 when the engine reaches it.
- A `Restart` in WinPE before the state has moved to disk. See [State on disk and resume](#state-on-disk-and-resume).
- An empty `All`, `Any`, or `Not`. MDT treats an empty OR as true, which is rarely what the author meant.

### Conditions

- The agent evaluates a condition when it reaches the group or step. A false group condition skips the group. A false step condition skips the step and leaves `last` unchanged, as MDT does.
- The agent expands `Template` fields in a condition when it evaluates it, not when the server compiles the sequence.
- Evaluation returns `Result<bool, ConditionError>`. An error is not false. It fails the step and goes through the step's `on_error`, which is MDT's rule.
- `Var` compares as integers when both sides are integers, and otherwise compares strings without regard to case, like MDT. `Like` matches `*` and `?` wildcards. `Exists` means the variable is set and not empty.
- A missing file, folder, registry key, or registry value makes the condition false.
- `Not` takes one child. MDT's `not` with several children means "none of them is true", so an MDT importer turns it into `Not(Any(children))`.

[MDT 8450 binaries reference](research/mdt-8450-binaries.md#conditions) lists the exact behavior of MDT's evaluator.

### State on disk and resume

The agent keeps one state file and one append-only journal:

```rust
pub struct RunState {
	pub deployment: DeploymentId,
	pub program: Program,             // frozen when the deployment starts
	pub pc: Pc,
	pub vars: Vars,
	pub in_flight: Option<InFlight>,  // written before a step starts
	pub last: Option<StepResult>,     // like _SMSTSLastActionSucceeded
}
```

- The agent writes the state file to a temporary path, flushes it, and renames it over the old file. A crash leaves either the old state or the new state, never a partial file. MDT's engine rewrites its state file in place.
- If a save fails, the run stops before the next step or reboot. MDT's engine logs a warning and continues, so a later power loss resumes from older state.
- Before the disk is partitioned, the state lives on `X:`, the WinPE RAM disk, and a reboot loses it. That is safe, because nothing on the disk has changed yet, and the deployment starts over. MDT's engine cannot save at all until a fixed volume exists. After the partition step, the agent moves the state to `W:\Deployarr\state` on the new Windows volume.
- The state file holds the deployment ID, and the agent looks for it only at its fixed path. MDT's engine resumes from whichever `TSEnv.dat` on any fixed drive has the newest access time.
- A named mutex keeps a second agent from running the same deployment. MDT's `TsmBootstrap.exe` does the same with `Global\TSMBootStrapMutex`.
- The agent sets `in_flight` before it starts a step and clears it when it records the result. If the agent starts and finds `in_flight` set, the step was interrupted by a crash or a power loss. Typed actions are written to converge when they run again. For example, `PartitionDisk` checks the current layout first. `RunCommand` and `RunPowerShell` are not assumed to be safe to repeat, so they fail unless the step sets `rerun_if_interrupted`. MDT saves nothing before a step, so an interrupted step silently runs again. A sequence ported from MDT may need `rerun_if_interrupted` on some steps.
- A step can return `Reboot { rerun: false }` to reboot and continue with the next step, or `Reboot { rerun: true }` to run the same step again. These map to MDT's `SMSTSRebootRequested` and `SMSTSRetryRequested`.
- The exit codes in `ExitCodes.reboot`, 3010 and 1641 by default, mean success and then a reboot. This is a deliberate difference from MDT, whose engine never reboots because of an exit code.
- The agent saves state before every reboot, including a re-run. MDT's engine skips that save for a re-run, so the step's variable changes are lost. After saving, the agent writes a marker that the reboot clears: a volatile registry key in Windows, or a file on `X:` in WinPE. It refuses to resume while the marker exists. MDT's engine uses a volatile key the same way.
- The agent runs each command in a Windows job object. When `max_runtime_secs` passes, it ends the whole process tree and fails the step. MDT's templates set no time limit, so a hung step waits forever.
- The agent streams each step start, step result, and log line to the server. The server is the record of what happened, and the local journal covers network outages.

The engine is a pure state machine over an `Executor` trait, so it is tested on Linux with a fake executor. The key test crashes the agent at every step boundary and checks that each resumed run ends in the same state as a run with no crash.

### Variables

- A name is 1 to 256 letters, digits, `_`, or `-`, and names are case-insensitive. These are MDT's rules. deployarr also limits the letters and digits to ASCII. That limit is deployarr's choice, because the [MDT 8450 binaries reference](research/mdt-8450-binaries.md#variables) does not say whether MDT accepts other letters. A strict set can loosen later without breaking a stored sequence. Names that start with `_` belong to the agent, and steps can read them but not set them.
- A value is a scalar or a list. MDT's `Applications001`-style lists become real lists.
- A value that is already set beats a sequence default. MDT's `globalVarList` follows the same rule.
- `%Name%` inside a `Template` substitutes a value, and `%%` stands for a literal `%`. Substitution makes one pass from left to right and never scans inserted text again. A name that is not a task sequence variable falls back to the process environment, as in MDT. A name that neither defines fails the step. MDT leaves such a name in the text, and its expander scans inserted text again, so one stored value can make it loop forever.
- There is no `#expr#` evaluation. MDT runs `#expr#` through VBScript `Eval`, which executes anything. Conditions use the typed `Condition` model instead.
- Step processes read and set variables through the agent, with the `deployarr-agent var get` and `var set` commands over a local channel that the agent opens for each step. There is no command that clears every variable. MDT scripts change a shared-memory store directly, and its `Clear()` method erases the engine's own state.
- A variable can be marked secret. Secrets never reach a log line or an event. The CMTrace writer and the event stream both redact them. Logs show templates before substitution, as MDT's engine does.
- Every variable records where its value came from: a gathered fact, a rule, a data source column, a prompt, or a step. The UI shows this trace for each deployment. MDT writes a similar trace into `ZTIGather.log`.

## Order lookup

### Identity facts

The agent reads SMBIOS directly with `GetSystemFirmwareTable('RSMB')` and parses it in the `deployarr-smbios` crate. MDT reads the serial number from `Win32_BIOS`, which is the SMBIOS type 1 system serial. Your orders record the motherboard serial, which is SMBIOS type 2, so deployarr reads types 1, 2, and 3 and keeps them as separate facts.

Custom-built PCs often carry placeholder values such as "To Be Filled By O.E.M.", "Default string", "System Serial Number", or the all-zero UUID. The identity matcher drops known placeholders, so two PCs that share a placeholder never match the same order. MDT only drops blank values.

### Lookup chain

The server tries resolvers in a configured order and stops at the first one that returns a task sequence:

```rust
pub enum Resolver {
	PreStaged,          // machines table in deployarr's own database
	AddOn(AddOnRef),    // a resolver that an add-on declares, such as SQL order lookup by identity
	ManualList,         // the technician picks a task sequence
}
```

The chain's rules belong to the base program. An add-on resolver only answers when the chain calls it.

- The default chain is `PreStaged`, then `ManualList`. A server with no add-on runs that chain and opens no database connection besides its own SQLite file. The base binary holds the SQL drivers and uses none until an add-on asks for a connection.
- Installing an add-on does not change the chain. An admin adds its resolvers on the Order lookup screen. With the SQL order lookup add-on, the usual chain is `PreStaged`, then "by identity" on the baseboard serial, then "by order number", then `ManualList`.
- An admin can reorder the chain or remove any resolver. When the chain is saved, the server rejects an empty chain and a resolver that no installed add-on declares.
- The server runs the chain from the top each time the agent sends new prompt answers, so the chain keeps no state between calls (design decision).
- The server skips the resolver of a disabled or removed add-on, and the trace says so.
- An identity resolver gets only the PC's usable identity facts, with known placeholders dropped. With none left, the server skips it. See [Add-on resolvers and wipe authority](#add-on-resolvers-and-wipe-authority).
- A resolver that declares a prompt and has no answer yet sends its prompt to the agent. The agent renders it. See [What runs on the PC](#what-runs-on-the-pc).
- A timeout, a trap, a refused host call, or a match without wipe authority counts as an error. The trace shows it, and the chain moves to the next resolver.
- The server checks that a matched task sequence exists. A variable that a match sets records the add-on and the resolver as its origin.

The SQL order lookup add-on maps an order row's OS identifier to a task sequence through an editable table in its settings, for example `W11-PRO -> W11-PRO-STD`. Other columns can map to variables, such as the computer name or an application list. The table lives in the add-on, because a resolver returns a task sequence, not an OS identifier (design decision).

A barcode scanner acts as a keyboard, so a prompt in the WinPE screen, such as the order number, accepts scanner input with no extra code.

When no resolver returns a task sequence, the agent shows the PC's identity facts on a no-match screen and waits. It does not touch the disk.

### Data sources

The order database is reached only from the server. The agent never holds database credentials. MDT puts them in `CustomSettings.ini` and writes the connection string, password included, to its log.

A data source is a SQL connection that an add-on declares and an admin fills in on the add-on's settings page. The add-on host owns it. The host stores the address, the account, the password, and the CA certificate, opens the connection, and keeps the pool. The add-on holds only the connection's name. See [SQL connections](#sql-connections).

```rust
#[async_trait]
pub trait DataSource {   // inside the add-on host. No add-on sees this type
	async fn query(&self, query: &NamedQuery, params: &Params) -> Result<Vec<Row>, DataSourceError>;
}
```

- SQL Server uses `tiberius`. It signs in with a SQL login, or with Windows authentication when the deployarr server runs on Windows. No research source covers Windows authentication from a Linux server ([Spin 4.3.0 reference](research/spin-4.3.0.md#open-questions)), and the roadmap lists the choice as an [open question](roadmap.md#open-questions).
- PostgreSQL and MySQL use `sqlx`. Recent `sqlx` releases dropped SQL Server support, which is why SQL Server has its own driver.
- An admin writes each query with named parameters, such as `SELECT os_code, order_no FROM orders WHERE board_serial = :board_serial`. The query is a `sql_text` setting of the add-on. The add-on names it and never supplies SQL. The server rewrites the names into each engine's placeholder syntax and always binds values. Values are never pasted into the SQL text.
- A lookup error or an empty result moves to the next resolver and appears in the trace. MDT logs the error and continues as if nothing happened.
- The order database is read-only to deployarr. The setup guide requires a database account that can only read. That account is the guard that holds on every engine. The engine guards in [Write guards](#write-guards) are a second guard, and each one has limits.

#### Write guards

The test results below come from runs on 2026-11-12 and 2026-11-13 with an account that may write, on PostgreSQL 16.15, MySQL 8.0.46, and MariaDB 10.11.14. Comments on [#77](https://github.com/Tsukidy/deployarr/issues/77) record the setup and every result, in the [first runs](https://github.com/Tsukidy/deployarr/issues/77#issuecomment-6839109962) and the [compound statement runs](https://github.com/Tsukidy/deployarr/issues/77#issuecomment-6845427631). Without a guard, that account's `INSERT`, `TRUNCATE`, and `DROP TABLE` changed the data on each engine.

The server sends each named query to PostgreSQL, MySQL, and MariaDB as one prepared statement. PostgreSQL and MySQL accept one statement in each prepared statement ([Extended Query](https://www.postgresql.org/docs/16/protocol-flow.html#PROTOCOL-FLOW-EXT-QUERY), [mysql_stmt_prepare()](https://dev.mysql.com/doc/c-api/8.0/en/mysql-stmt-prepare.html), [PREPARE Statement](https://dev.mysql.com/doc/refman/8.0/en/prepare.html)). On those two engines, a query text that holds a second statement fails before it runs. MariaDB also refused a text with two statements. A MariaDB compound statement still holds several statements in one, as the gaps below show.

Sent as plain text, a second statement can end the guard's transaction or reset the session. It can then write. On PostgreSQL, `COMMIT; DROP TABLE orders` went as one query inside `BEGIN READ ONLY` and dropped the table. On MySQL and MariaDB, `SET SESSION TRANSACTION READ WRITE; DROP TABLE orders` went as one query under the guard below and dropped the table. Sent as a prepared statement, each text failed and changed nothing.

The guard on each engine:

- PostgreSQL. The server runs each named query in its own transaction. It opens the transaction with `BEGIN READ ONLY` and rolls it back after the query. A read-only transaction refuses `INSERT`, `UPDATE`, and `DELETE` on a table that is not temporary. It also refuses `TRUNCATE` and every `CREATE`, `ALTER`, and `DROP` command ([SET TRANSACTION](https://www.postgresql.org/docs/16/sql-set-transaction.html)). It refused every write to the database that the tests tried, including a `CALL` of a procedure that writes. A transaction per query also holds where a setting made once per connection does not. In one test, a named query ran `SET SESSION CHARACTERISTICS AS TRANSACTION READ WRITE`. Under a once-per-connection setting, the next named query on that connection then dropped a table. Under a transaction per query, the same `DROP TABLE` failed.
- MySQL and MariaDB. Before each named query, the server sets the session's transaction access mode with `SET SESSION TRANSACTION READ ONLY`. After each named query, it runs `ROLLBACK`, which ends any transaction that the query left open. Under this guard, `INSERT`, `DELETE`, `TRUNCATE`, `DROP TABLE`, and a `CALL` of a procedure that runs `TRUNCATE` each failed with error 1792 and changed nothing.
- SQL Server. deployarr relies on the read-only account alone. SQL Server has no engine-level read-only setting to fall back on (inferred in [Add-on design attacks, item 2](research/addon-design-attacks-wasmtime-50.0.1.md#item-2-read-only-enforcement-depends-on-the-engine)). The server also runs each named query in a transaction and always rolls it back. No test shows that the rollback undoes `TRUNCATE` or `DROP TABLE`, so it does not count as a guard. The M5 integration test records whether it does.

The MySQL and MariaDB guard takes this form for two reasons:

- A read-only transaction alone does not stop DDL. A DDL statement such as `TRUNCATE TABLE` or `DROP TABLE` commits the open transaction and then runs ([Statements That Cause an Implicit Commit](https://dev.mysql.com/doc/refman/8.0/en/implicit-commit.html)). Inside `START TRANSACTION READ ONLY`, `TRUNCATE TABLE` emptied the table. `DROP TABLE` dropped it. The session's read-only access mode refuses DDL on every table. It also refuses data changes to a table that is not temporary ([Transaction Access Mode](https://dev.mysql.com/doc/refman/8.0/en/set-transaction.html#set-transaction-access-mode)).
- A setting made once per connection does not hold. A named query can run `SET SESSION TRANSACTION READ WRITE`. Under a once-per-connection setting, the next named query on that pooled connection then dropped a table. On MariaDB, a named query can also run `START TRANSACTION READ WRITE`. A session setting does not change a transaction that is already open ([Transaction Characteristic Scope](https://dev.mysql.com/doc/refman/8.0/en/set-transaction.html#set-transaction-scope)). The next named query then wrote inside that transaction. A later named query committed the write. Setting the access mode before each query stops the first case. The `ROLLBACK` after each query stops the second. MySQL 8.0 refused to prepare `START TRANSACTION`, with error 1295.

The engine guards do not stop these:

- A query that writes a file or runs a program on the database server. On MySQL and MariaDB, `SELECT ... INTO OUTFILE` wrote a file under every guard tested. On PostgreSQL, `COPY ... TO` a file and `COPY ... TO PROGRAM` both ran inside `BEGIN READ ONLY`. PostgreSQL's read-only mode "does not prevent all writes to disk" ([SET TRANSACTION](https://www.postgresql.org/docs/16/sql-set-transaction.html)).
- On MySQL and MariaDB, a call to a stored procedure that sets the session back to read-write and then writes. A procedure that runs `SET SESSION TRANSACTION READ WRITE` and then `TRUNCATE TABLE` emptied the table under every guard tested. An account with only `SELECT` and `EXECUTE` emptied it the same way, through a procedure that runs with its definer's rights. On PostgreSQL, the same kind of procedure failed inside `BEGIN READ ONLY`.
- On MariaDB, a named query that starts with `SET STATEMENT tx_read_only=0 FOR`. MariaDB runs the statement after `FOR` with the variable changed for that statement only. Its list of variables that cannot be set this way does not include `tx_read_only` ([SET STATEMENT](https://mariadb.com/docs/server/reference/sql-statements/administrative-sql-statements/set-commands/set-statement)). Sent as one prepared statement under the guard, it ran `INSERT`, `TRUNCATE TABLE`, and `DROP TABLE`. MySQL 8.0 refused to prepare it, with error 1064.
- On MariaDB, a named query that is a compound statement, such as `BEGIN NOT ATOMIC SET SESSION TRANSACTION READ WRITE; TRUNCATE TABLE orders; END`. A compound statement can hold several statements ([BEGIN END](https://mariadb.com/docs/server/reference/sql-statements/programmatic-compound-statements/begin-end)). A statement inside it can set the session back to read-write. A later statement inside it then writes. Sent as one prepared statement under the guard, this form ran `INSERT`, `TRUNCATE TABLE`, and `DROP TABLE`. A compound statement that held only `TRUNCATE TABLE` failed with error 1792. An account with only `SELECT` got error 1142 on the `TRUNCATE` form. MySQL 8.0 refused to prepare each form, with error 1064.

The read-only account that the setup guide requires must also meet these rules:

- On MySQL and MariaDB, it holds neither the `FILE` privilege ([FILE](https://dev.mysql.com/doc/refman/8.0/en/privileges-provided.html#priv_file)) nor `EXECUTE` on a stored procedure that writes.
- On PostgreSQL, it is not a superuser, and it is not a member of `pg_write_server_files` or `pg_execute_server_program` ([Predefined Roles](https://www.postgresql.org/docs/16/predefined-roles.html)).

### Wipe safety

A zero-touch server can erase any PC that network-boots by mistake. deployarr erases a disk only after one of these:

- a positive identity match, from a pre-staged record or from an add-on resolver such as an order row lookup
- a technician's typed answer to an add-on resolver's prompt, such as an order number
- a technician picking a task sequence

An add-on resolver returns a match and never wipe authority. `deployarr-core` builds the authority from the fact or the answer that the server sent. See [Add-on resolvers and wipe authority](#add-on-resolvers-and-wipe-authority).

An unknown PC stops at an add-on's prompt, the manual list, or the no-match screen, whichever the chain reaches first, and does not touch its disk.

After a deployment succeeds, the server stops offering PXE boot to that PC's MAC address and SMBIOS UUID (DHCP option 97), until an admin re-arms it. This stops a PC that boots from the network first from reinstalling in a loop. A match on either key marks the PC as finished. For a request without option 97, the server checks the MAC address in `chaddr` alone, which every request carries. Option 61 (client identifier) is not a key. The iPXE ROM fills it with the same MAC address. OVMF sends none ([#56](https://github.com/Tsukidy/deployarr/issues/56)).

## Agent

- The agent is one static Rust executable built for `x86_64-pc-windows-msvc` with the static C runtime.
- It uses Win32 APIs, not WMI, so base WinPE needs no optional components:
  - SMBIOS: `GetSystemFirmwareTable`
  - network adapters: `GetAdaptersAddresses`
  - firmware type: `GetFirmwareType`. The fallback is MDT's `GetFirmwareEnvironmentVariableW` check, and only with `SeSystemEnvironmentPrivilege` enabled first. Without the privilege, newer WinPE versions make that check report BIOS PCs as UEFI, which is the bug the MDT 8456 hotfix fixed.
  - disks and bus type: `IOCTL_STORAGE_QUERY_PROPERTY`. Bus type 7 is USB.
  - device hardware IDs and compatible IDs: SetupAPI with `DIGCF_PRESENT | DIGCF_ALLCLASSES`, which is what MDT's `Microsoft.BDD.PnpEnum.exe` does. Unlike PnpEnum, the agent reports an error when enumeration fails, so a failed scan never looks like a PC with no devices.
  - CPU features: the `cpuid` instruction, for the same virtualization, NX, and 64-bit facts that MDT's `Microsoft.BDD.Utility.dll` reports.
  - memory and processor cores: `GetPhysicallyInstalledSystemMemory` and `GetLogicalProcessorInformationEx`.
  - TPM version: `Tbsi_GetDeviceInfo` from `tbs.dll` when the boot image has it and the call succeeds, and the TPM device's IDs otherwise. Base WinPE 10.0.26100.1 has no `tbs.dll`. ConfigMgr reads the TPM through WMI `Win32_Tpm` instead. See [TPM version](#tpm-version).
  - Secure Boot: the `UEFISecureBootEnabled` value under `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State`, which ConfigMgr also reads. When the value is missing, the agent reads the `SecureBoot` UEFI variable with `GetFirmwareEnvironmentVariableW`, with `SeSystemEnvironmentPrivilege` enabled. The fact has three values: on, off, and unknown. A failed read is unknown, never off. ConfigMgr keeps the same third value as `NA`.
- `RunPowerShell` steps in WinPE need the WinPE PowerShell component, which only DISM can add. See [Boot image](#boot-image).
- `RunPowerShell` runs `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <script>` and passes each argument separately. MDT's PowerShell host joins the arguments into one string and runs it as PowerShell code, which loses their quoting.
- A CI check, `cargo xtask winpe-imports`, reads the module names in the agent's import table and delay-load import table. It fails if a name is missing from base WinPE's [list file](research/winpe-10.1.26100.9457.md#the-list-file).
  - It lower-cases each name.
  - It compares an `api-` or `ext-` name up to its last hyphen with `api-` and `ext-` list entries cut the same way. The loader ignores the last number of an API set name, so an import of `api-ms-win-core-synch-l1-2-0.dll` resolves through the schema entry `api-ms-win-core-synch-l1-2-1` (inferred in [API sets](research/winpe-10.1.26100.9457.md#api-sets)).
  - It compares every other name exactly.
  - It checks module names, not functions. The M3 lab tests the functions.
- In WinPE the agent is the shell. It shows a full-screen console screen with the current step, a progress bar, and the prompts. Those are the task sequence list, the product key, and any prompt that an add-on declares, such as the order number. The agent renders an add-on's prompt from data. See [What runs on the PC](#what-runs-on-the-pc).
- Ending the WinPE shell reboots the PC, so the agent never exits on a fatal error. It shows the error and waits.
- A setting in the boot image makes F8 open a command prompt, for debugging. The agent does not reboot WinPE while a prompt it opened is still running. MDT's `BDDRun.exe` works the same way.
- The agent keeps the PC awake for the whole run with `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`, as MDT does.
- HTTPS uses `rustls`. The agent trusts only the server CA certificate that is built into the boot image.
- Logs use the CMTrace line format, so admins can read them with CMTrace. The final copy goes to `C:\Windows\Temp\Deployarr` and to the server.

### TPM version

Base WinPE 10.0.26100.1 has no `tbs.dll`. It ships in the `WinPE-SecureStartup` optional component. DISM can add that component only after `WinPE-WMI` (inferred from the package's `parent` element). The [WinPE 10.1.26100.9457 reference](research/winpe-10.1.26100.9457.md#tbsdll) records these findings. Reading the TPM version requires neither component. The agent reads the TPM version from `Tbsi_GetDeviceInfo` when the boot image has `tbs.dll` and the call succeeds, and from the TPM device's IDs otherwise.

1. The agent loads `tbs.dll` at run time with `LoadLibraryExW` and `LOAD_LIBRARY_SEARCH_SYSTEM32`, so the loader searches only `System32` ([LoadLibraryExW](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw)). `tbs.dll` is in neither the import table nor the delay-load import table, because the import check reads both and rejects it.
2. When `tbs.dll` and its `Tbsi_GetDeviceInfo` export load, the agent calls `Tbsi_GetDeviceInfo`. On `TBS_SUCCESS`, `tpmVersion` holds `TPM_VERSION_12` or `TPM_VERSION_20` ([TPM_DEVICE_INFO](https://learn.microsoft.com/en-us/windows/win32/api/tbs/ns-tbs-tpm_device_info)), and the agent reports that version.
3. When `tbs.dll` or its `Tbsi_GetDeviceInfo` export does not load, the agent maps device IDs to a TPM version. It also maps them when `Tbsi_GetDeviceInfo` returns an error, such as `TBS_E_TPM_NOT_FOUND` ([Tbsi_GetDeviceInfo](https://learn.microsoft.com/en-us/windows/win32/api/tbs/nf-tbs-tbsi_getdeviceinfo)), or a `tpmVersion` with any other value. The log keeps the load error, the TBS error, or the `tpmVersion` value. A boot image with `tbs.dll` therefore reports unknown only when base WinPE would too.

The mapping:

- It uses the IDs that base WinPE's `tpm.inf` binds to the TPM driver, from the [table in the research page](research/winpe-10.1.26100.9457.md#tbsdll). An ID from a row that `tpm.inf` describes as TPM 1.2 means 1.2. An ID from a row it describes as TPM 2.0 means 2.0. The agent carries the table in its code. It does not read `tpm.inf` at run time.
- It reads the hardware IDs and the compatible IDs of every present device from the SetupAPI scan above. It compares them with the table without regard to case.
- A device with no compatible IDs is not a scan error. `SetupDiGetDeviceRegistryPropertyW` reports `ERROR_INVALID_DATA` for a property that the device does not have ([SetupDiGetDeviceRegistryPropertyW](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceregistrypropertyw)).
- Compatible IDs matter when the firmware gives the TPM a vendor `_HID`, as EDK2 does. Windows uses compatible IDs to find a driver when none matches a hardware ID ([Compatible ID](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/compatible-ids)). An ACPI device's compatible IDs come from its `_CID` object ([Device management namespace objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/bringup/device-management-namespace-objects), section Compatible ID (_CID)). EDK2 at tag `edk2-stable202408` declares its TPM 2.0 device with `_HID` `NNNN0000` and `_CID` `MSFT0101` (`SecurityPkg/Tcg/Tcg2Acpi/Tpm.asl:30-34`). Its `UpdateHID` replaces `NNNN0000` with an ID built from the TPM's manufacturer and firmware version (`SecurityPkg/Tcg/Tcg2Acpi/Tcg2Acpi.c:558-652`). A check of hardware IDs alone reads such a PC as unknown.
- Nobody has checked the table against every firmware TPM. The M3 check tests it on the UEFI VM and on one real PC.
- The UEFI VM's TPM is QEMU's `tpm-crb` device (`tools/lab/vm.sh`). QEMU 8.2.2, the version in Ubuntu 24.04, declares that device with `_HID` `MSFT0101` and no `_CID` (`hw/i386/acpi-build.c:1794-1810`). The VM therefore tests only the hardware-ID path. The real PC tests the compatible-ID path only if its TPM has a vendor `_HID`.

The fact has three values: 1.2, 2.0, and unknown. The log names the source of each value.

| Case | TPM version | The log names |
| --- | --- | --- |
| `Tbsi_GetDeviceInfo` returns `TBS_SUCCESS` with `TPM_VERSION_20` or `TPM_VERSION_12` | 2.0 or 1.2 | `Tbsi_GetDeviceInfo` |
| `tbs.dll` or `Tbsi_GetDeviceInfo` does not load, `Tbsi_GetDeviceInfo` returns an error, or `tpmVersion` holds another value | The mapping's result, from the rows below | The load error, the TBS error, or the `tpmVersion` value, then the mapping's row |
| A present device lists a 2.0 ID from the table | 2.0 | The device, the ID, and whether it is a hardware ID or a compatible ID |
| No present device lists a 2.0 ID, and one lists a 1.2 ID | 1.2 | The device, the ID, and whether it is a hardware ID or a compatible ID |
| No present device lists an ID from the table | Unknown | That no ID matched |
| The device scan fails | Unknown | The scan error |

- A scan that fails partway is a failed scan. The TPM version is unknown, even when a device read before the failure matched.
- When one device lists IDs for both versions, 2.0 wins. The same holds when two devices match with different versions. This is a design choice, not a finding. No known device lists IDs for both versions.
- The agent never reports that a PC has no TPM. A TPM whose IDs are all outside the table looks the same as a PC with no TPM (inferred).
- The agent does not read `TBS_E_TPM_NOT_FOUND` as no TPM. It runs the mapping, as base WinPE does.
- A matched ID shows the TPM interface that the firmware declares. It does not show that the TPM works.

### Handoff from WinPE to the new OS

MDT resumes in the new OS through auto-logon as Administrator with a logon count of 999, plus a startup shortcut. deployarr runs the agent as a Windows service instead:

1. Before the reboot, the agent copies itself to `W:\Deployarr\bin`.
2. It loads the offline `SYSTEM` registry hive from `W:\Windows\System32\config` with `RegLoadKeyW`.
3. It creates the `DeployarrAgent` service key: automatic start, running as LocalSystem, set to restart on failure.
4. The service starts at boot. It waits until Windows Setup reports that it has finished, by watching `ImageState` and `SystemSetupInProgress`, and then continues the sequence. In the customer end state it continues in audit mode instead. See [End state](#end-state).

This removes the stored Administrator password and the logon count from the deployment. It also makes the PC deploy the same way whether or not anyone is in front of it.

The trade-off is that steps run as LocalSystem in session 0, with no desktop. A few installers fail without a logged-on user. For those, a later milestone adds a per-step option that runs the step in an auto-logon session with a one-time password. The agent removes that password after the step.

## End state

The owner ships some PCs ready to use and others ready for the customer, so each deployment picks one of two end states. The `EndState` variable holds the choice. A task sequence sets its default, and an order column or a pre-staged record can override it, because a value that is already set beats a sequence default. The server accepts only `ReadyToUse` and `CustomerOobe`, and it rejects any other value before the disk changes.

- `ReadyToUse`: the unattend file skips OOBE and creates the local admin account with the PC's own password (see [Server](#server)). The finish action restarts the PC or shuts it down.
- `CustomerOobe`: the unattend file sets `Microsoft-Windows-Deployment | Reseal | Mode` to `Audit` in the oobeSystem pass. Setup then ignores the other oobeSystem settings and starts the auditSystem pass ([Reseal Mode](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-deployment-reseal-mode)). The agent service runs the rest of the sequence in audit mode. Its last action is `sysprep /oobe /shutdown`, which Microsoft documents for a PC that goes to a customer ([Boot Windows to audit mode or OOBE](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/boot-windows-to-audit-mode-or-oobe)). It does not pass `/generalize`, because the image stays on the PC it was applied to.

In `CustomerOobe`:

- The agent service must continue in audit mode. `IMAGE_STATE_COMPLETE` means that the specialize and oobeSystem passes are complete ([Windows Setup states](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/windows-setup-states)). A PC in audit mode has not finished OOBE, so the service cannot wait for that value alone (inferred). The M3 lab records the value that audit mode shows.
- The agent reports success, uploads its logs, and removes its service before it runs `sysprep`, because the shutdown ends its run. The customer's first boot starts no deployarr code.
- The PC keeps no account or password that deployarr created. The M3 lab confirms it.

In both states, the agent deletes the cached answer files in `%WINDIR%\Panther` before the finish action. Microsoft says to delete the cached answer file before a PC goes to a customer ([Windows Setup automation overview](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/windows-setup-automation-overview)), and deployarr's answer file can hold passwords.

## Activation

Each PC has a Windows product key on a sticker, and a technician types it during the deployment.

- A task sequence that contains an `ActivateWindows` step needs a key. The built-in Windows client template contains one at the start of its full-OS phase. A sequence for a PC that needs no typed key leaves the step out, such as one for an edition with its own key, or for a lab VM that stays unactivated. Installing needs no key. The agent applies the chosen edition's image, so no key has to select the edition, as one does in Windows Setup ([Work with product keys and activation](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/work-with-product-keys-and-activation-auth-phases)). The agent asks for the key in WinPE right after lookup, before the disk changes. When lookup used an add-on's prompt, such as the order number, the key is a field on the same screen, so the technician types both in one visit.
- The agent checks only the format: 25 letters and digits in five groups of five. A format check cannot tell a mistyped key from a real one.
- The typed key is the secret variable `ProductKey`, so it never reaches a log line or an event. The server stores it encrypted with the other secrets and shows it to admins next to the deployment.
- The key does not go into the unattend file. An invalid key in the `ProductKey` setting of `Microsoft-Windows-Shell-Setup` makes Windows Setup fail ([ProductKey](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-productkey)), so one typo would stop the PC in Setup.
- `ActivateWindows` runs in the full OS instead. It installs the key with `slmgr /ipk`, which returns an error when the key is not valid for the installed operating system ([Slmgr.vbs options](https://learn.microsoft.com/en-us/windows-server/get-started/activation-slmgr-vbs-options)). With `online` set, it then runs `slmgr /ato` to activate online.
- In `ReadyToUse`, the step then runs `slmgr /cpky` to remove the key from the registry. Microsoft calls this the best practice for retail installations that deploy keys. The step skips `/cpky` in `CustomerOobe`, because Microsoft says that some servicing operations need the key in the registry during OOBE (same page).
- A rejected key fails the step, and the UI shows the error with the deployment. Because the template runs the step first in the full OS, a mistyped key shows up minutes after the first reboot.
- Microsoft's rule on whether a system builder or the customer activates a PC is not yet checked. The `online` flag lets each task sequence follow that rule.
- Lab tests of the step type Microsoft's KMS client setup key for Windows 11 Pro, `W269N-WFGWX-YVC9B-4J6C9-T83GX`. Microsoft publishes none for Home ([KMS client activation keys](https://learn.microsoft.com/en-us/windows-server/get-started/kms-client-activation-keys)). Installing it turns a retail edition into a KMS client. A KMS client activates only against a KMS host on the local network (same page). The lab has no KMS host, so a lab sequence that runs the step leaves `online` off. That `slmgr /ipk` accepts the key on the Pro image of the multi-edition ISO is inferred until M3 confirms it. A Home test needs a key from the owner.
- A KMS client setup key for an edition that Pro upgrades to is not a rejected key. `slmgr.vbs` with such a key can upgrade Pro to Enterprise, Education, Pro Education, or Pro for Workstations ([Windows Edition Upgrade](https://learn.microsoft.com/en-us/windows/deployment/upgrade/windows-edition-upgrades)). A test of a rejected key changes one character of the Pro key instead.

## Directory join

A PC stays in a workgroup by default. Domain join is an option in the base program. Entra ID join and Autopilot are add-ons, planned for M8 (see [Planned add-ons](#planned-add-ons)). An admin defines join profiles on the server, and the `JoinProfile` variable names the profile a deployment uses. As with `EndState`, a task sequence sets the default, and an order column or a pre-staged record can override it. With no setting, the PC uses the built-in `Workgroup` profile.

```rust
pub enum JoinProfile {
	Workgroup { name: String },   // "WORKGROUP" unless an admin changes it
	Domain { domain: String, ou: Option<String>, account: String, password: SecretRef },
	AddOn { kind: AddOnRef, config_version: u32, config: Box<RawValue> },   // a join kind from an add-on, such as Entra ID or Autopilot
}
```

- `Workgroup` and `Domain` go into the unattend file, in the specialize pass of the `Microsoft-Windows-UnattendedJoin` component. `JoinWorkgroup` names the workgroup. `JoinDomain`, `Credentials`, and `MachineObjectOU` name the domain, the join account, and the full LDAP path of the OU ([UnattendedJoin](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin), [Identification](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin-identification), [MachineObjectOU](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin-identification-machineobjectou)). The PC must reach a domain controller during the specialize pass.
- The join account's password is a secret. The server stores it encrypted, and the agent deletes the cached answer files before the finish action (see [End state](#end-state)).
- A join kind from an add-on adds no code path of its own. It names a profile form and, for each stage of the built-in `ApplyJoinProfile` action, one of the add-on's step kinds. The stages are the offline image in WinPE and the full OS. The built-in Windows client template holds an `ApplyJoinProfile` step for each stage. When a deployment is planned, each one expands to the step kind that the profile's join kind names for that stage, or to nothing. `Workgroup` and `Domain` expand to nothing, because the unattend file does the join. See [Extension points](#extension-points).
- A join kind can name the end states it needs. The server rejects any other pair before the disk changes.
- A Home image can use only `Workgroup`. Microsoft lists "The ability to join a domain or Microsoft Entra ID" among the features that Pro adds to Home ([Upgrade Windows Home to Windows Pro](https://support.microsoft.com/en-us/windows/upgrade-windows-home-to-windows-pro-ef34d520-e73f-3198-c525-d1a218cc2818)). Entra join works on Windows 11 "except for Home editions", and bulk enrollment is one way to do it ([Microsoft Entra joined devices](https://learn.microsoft.com/en-us/entra/identity/devices/concept-directory-join)). Autopilot's list of supported Windows 11 editions has no Home edition ([Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)).
- The server rejects any profile but `Workgroup` for a Home image when a task sequence or a pre-staged record is saved, and again after lookup, before the disk changes. That covers `Domain` and every join kind from an add-on. A rule in the base needs no field in each add-on's manifest (design decision). An order column can set the profile, so the second check is the one that catches it. A sequence counts as Home when any of its `ApplyOsImage` steps names a Home image, because conditions pick the step only at run time. The error names the image's edition ID and the profile.
- A step can also set `JoinProfile`, with `SetVariable` or `var set`. The agent checks the new value when the step sets it, and fails the step if the image is Home. A step that runs before the disk steps keeps the disk unchanged. Without these checks, a domain join on Home would fail in the specialize pass, after the disk is wiped (inferred).

### Entra ID join add-on

`deployarr.entra-join` joins a PC through bulk enrollment. Its join kind's profile form holds the provisioning package as a secret file setting. Its one step in the offline image expands to one `AddProvisioningPackage` action with that package as its secret input, and to no command. See [What runs on the PC](#what-runs-on-the-pc).

- An admin makes the provisioning package with Windows Configuration Designer and uploads it. The package joins the PC to the Entra tenant and enrolls it in Intune ([Bulk enrollment](https://learn.microsoft.com/en-us/intune/intune-service/enrollment/windows-bulk-enroll)). It carries a token for a `package_{GUID}` account in Entra ID (same page), so the server stores it encrypted as a secret.
- In WinPE, the agent adds the package to the applied image with `DISM /Image=W:\ /Add-ProvisioningPackage`, which works on an applied image ([DISM provisioning package options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-provisioning-package-command-line-options)). Windows applies a package that is embedded in the image during the OOBE pass ([How provisioning works](https://learn.microsoft.com/en-us/windows/configuration/provisioning-packages/provisioning-how-it-works)). The M8 lab checks that a `ReadyToUse` PC, which skips the OOBE screens, applies it too. If it does not, the package goes in during the full OS stage instead, through a built-in action as well, because no command that an add-on writes gets a secret (inferred).
- The token in a bulk enrollment package is valid for 90 days (Bulk enrollment page). The admin enters the date the package was made, and the UI warns before the 90 days pass.
- A bulk enrollment package can also carry a product key that upgrades the edition (Bulk enrollment page). Microsoft's table marks Home to Pro by provisioning package as not supported, and Home to Education or Pro Education as supported with a reboot ([Windows Edition Upgrade](https://learn.microsoft.com/en-us/windows/deployment/upgrade/windows-edition-upgrades)). Whether the Entra join follows such an upgrade is not checked, so the Home rule above rejects this join kind for Home whatever the package holds.

### Autopilot add-on

`deployarr.autopilot` registers a PC's hardware hash with Autopilot. Its step collects the hash with a PowerShell script from the add-on's assets, then sends it to the server with a `ServerCall`. The server side records the hash and queues a job, so the PC never waits on Microsoft Graph. See [Extension points](#extension-points).

- Autopilot only customizes OOBE ([Autopilot FAQ](https://learn.microsoft.com/en-us/autopilot/faq)), so its join kind needs `EndState` set to `CustomerOobe`.
- Capturing the hardware hash requires a booted Windows ([Manually register devices](https://learn.microsoft.com/en-us/autopilot/add-devices)), so the step runs in the full OS, in audit mode, before `sysprep`. The M8 lab picks the collection method.
- With CSV export, an admin downloads the hashes as a CSV file through the add-on's export action and imports it into Intune, which takes up to 500 rows per file (same page). With Graph, the add-on's job registers each hash through `importedWindowsAutopilotDeviceIdentities`, which needs the `DeviceManagementServiceConfig.ReadWrite.All` permission ([Create importedWindowsAutopilotDeviceIdentity](https://learn.microsoft.com/en-us/graph/api/intune-enrollment-importedwindowsautopilotdeviceidentity-create?view=graph-rest-1.0)). The tenant is an HTTP connection whose OAuth client credentials the host holds and uses. See [HTTP connections](#http-connections).
- Microsoft calls manual hash registration "primarily for testing and evaluation scenarios" (same page). A builder that sells into many customer tenants may need partner registration instead. That path is not yet checked.

## Readiness check

deployarr applies `install.wim` with DISM and never runs `setup.exe`, so Windows Setup never checks the hardware. The `CheckReadiness` action does that check instead. ConfigMgr has the same step, but its defaults are below the Windows 11 minimums and it has no Secure Boot check ([ConfigMgr 2509 reference](research/configmgr-2509.md#check-readiness)).

- The step runs in WinPE before any disk step, so a PC that fails keeps its disk as it was.
- Its defaults are Microsoft's Windows 11 minimums: 4 GB of memory, a 64 GB target disk, two or more processor cores, UEFI firmware, and TPM 2.0.
- Each check either fails the step or logs a warning. The Secure Boot check warns by default, because Windows 11 requires firmware that can run Secure Boot, not Secure Boot turned on.
- A task sequence can change a minimum, switch a check between fail and warn, or turn it off. A BIOS PC needs the UEFI check off, and a PC without TPM 2.0 needs the TPM check off.
- A fact that the agent cannot read fails its check, and the log says that the value was unknown.
- The TPM check passes when the agent reports 2.0, from either source in [TPM version](#tpm-version). In base WinPE the version comes from the TPM device's IDs. A PC whose TPM lists `ACPI\MSFT0101` as a hardware ID or as a compatible ID passes the default check. A TPM that lists no ID from the table reads as unknown and fails the check.
- Each check sets read-only variables with its result, the required value, and the value found, as ConfigMgr's `_TS_CR` variables do. Conditions and the UI use them to show why a PC stopped.
- The step does not check processor speed or Microsoft's list of supported processors.

## Drivers

The goal is a working driver for every device. MDT matches drivers only to devices that WinPE can see, never checks OS versions, and never confirms the result. deployarr fixes those three gaps.

### Library

- The `deployarr-inf` crate parses INF files itself, including `%strings%` substitution, so import works on a Linux server. MDT parses INF files through Windows SetupAPI and takes class names from the server's registry. deployarr ships a table of the standard device class GUIDs instead.
- The index keeps each ID with its kind (hardware or compatible), its architecture, and its minimum OS build, all taken from the `[Manufacturer]` decoration that lists it, such as `NTamd64.10.0...19041`. MDT flattens every ID into one list and loses all three, and its substring test files an ARM64 driver as x86.
- A package is the folder tree that holds an INF. Import removes nested duplicates. MDT copies nested vendor folders more than once.
- An INF that fails to parse is recorded as rejected, with the reason, and the rest of the import continues. One malformed INF stops an entire MDT import.
- You can import a folder, a ZIP file, or a CAB file. Vendor EXE driver packs need to be extracted first. Drivers that only install through an EXE become applications.
- Driver profiles group packages for a PC type. A profile can match on system make and model, on baseboard manufacturer and product, or on SMBIOS SKU. The baseboard fields matter for custom-built PCs, whose make and model are often placeholders.

### Selection

The `ApplyDrivers` action has three modes:

| Mode | Behavior | MDT equivalent |
| --- | --- | --- |
| `Auto` | Match every device's hardware and compatible IDs against the whole library | `DriverInjectionMode=AUTO` |
| `Profile` | Match devices only against the PC's profile | `DriverGroup` with PnP matching |
| `ProfileAll` | Inject every package in the profile, whatever the hardware | `DriverInjectionMode=ALL` |

In every mode, the server skips packages that are for the wrong architecture or that need a newer OS build than the image. It does not rank the packages that remain. Like MDT, it stages every matching package and lets Windows Plug and Play pick the best driver for each device.

A per-model profile replaces the library-wide match for PCs it covers. In MDT, the stock Inject Drivers step uses the profile "All Drivers", which makes `DriverGroup` settings do nothing unless the profile is narrowed.

### Injection and verification

1. WinPE gets the network and storage drivers it needs to reach the disk and the server, such as Intel VMD/RST, from the boot image. The default set is every driver of class Net, SCSIAdapter, or HDC, which is MDT's default. Display and System drivers are optional. The agent loads them at start with `drvload` and reports any that fail. MDT ignores WinPE driver failures.
2. The agent downloads the selected packages and injects them one at a time with `dism /Image:W:\ /Add-Driver /Driver:<package> /Recurse`. A failing package is recorded and skipped. MDT injects all drivers in one DISM call, so one bad driver fails the deployment.
3. Windows Setup installs the injected drivers while it detects devices.
4. Late in the sequence, `VerifyDrivers` lists every device with a problem code through `CM_Get_DevNode_Status`. The agent sends each device's hardware IDs to the server. If the library has a matching package that was not injected, the agent installs it with `pnputil /add-driver <inf> /install`, then checks again. This catches devices that WinPE could not see. A policy can also try Windows Update drivers.
5. The step then applies its policy: fail the deployment, warn, or ignore. The UI lists missing drivers by PC type, with hardware IDs, so the next deployment of that model succeeds.

## OS images

- To import an ISO, the server extracts `sources\install.wim` or `sources\install.esd` with 7-Zip. 7-Zip reads the UDF file system that Windows ISOs use and runs on Windows and Linux. The server also keeps `sources\boot.wim` as a possible WinPE base, plus the PXE boot files.
- To import a WIM or ESD, the server takes the file directly.
- `wimlib-imagex info` lists the editions in the image. The server exports each chosen edition into its own WIM with `wimlib-imagex export`, so the agent downloads only that edition.
- `wimlib-imagex info` also prints each edition's ID, from the `EDITIONID` element of the image's XML, and `wimlib-imagex export` keeps it (measured with wimlib 1.14.5). The server stores the ID with the exported image. An edition ID that starts with `Core` marks a Home image. In the Windows 11 ISO that the lab uses, Home, Home N, and Home Single Language are `Core`, `CoreN`, and `CoreSingleLanguage`, and Pro is `Professional`. An image with no edition ID counts as Home for the [directory join](#directory-join) rule.
- The content store keeps files by SHA-256. The agent checks each download against its hash.
- The agent downloads the image over HTTPS into `W:\Deployarr\cache` on the new Windows partition, then runs `dism /Apply-Image /ImageFile:<wim> /Index:1 /ApplyDir:W:\ /ScratchDir:W:\Deployarr\scratch`. The scratch directory on disk avoids WinPE's small RAM scratch space. Downloads resume after a network drop. The target disk needs room for the image file plus the applied OS. Windows 11 already requires a 64 GB disk, which has room for both.
- A clean install never runs `setup.exe`. MDT 8456 also applies `install.wim` for an OS imported from an ISO. Its `setup.exe` path has no caller.

## Disk layout and boot files

- Two built-in layouts follow Microsoft's documented defaults:
  - UEFI: GPT with EFI (FAT32, 300 MB), MSR (16 MB), Windows, and Recovery partitions. 300 MB meets Microsoft's EFI minimum for both 512-byte and 4K native sectors.
  - BIOS: MBR with an active System partition (NTFS), Windows, and Recovery partitions. ConfigMgr's engine requires the active partition on an MBR disk to be NTFS.
- Recovery gets a fixed size, and Windows gets the rest of the disk. The server computes the size when it imports an image. It is the size of `\Windows\System32\Recovery\winre.wim` in that image, plus any WinRE customizations, plus 250 MB for servicing, as Microsoft documents. ConfigMgr's console instead gives Recovery about 1% of the disk ([ConfigMgr 2509 reference](research/configmgr-2509.md#default-disk-layouts)).
- The agent writes a `diskpart` script for the layout, runs it with `diskpart /s`, and then reads the partition table back to confirm the result. MDT pipes commands into `diskpart` and never checks them. The read-back uses `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` and matches partitions to volumes with `IOCTL_STORAGE_GET_DEVICE_NUMBER`, as MDT's `HiddenPartitionsToDrives` does. The step log keeps the script and the output of `diskpart`.
- Letters are fixed during the WinPE phase. `S:` is the system partition, `W:` is Windows, and `R:` is Recovery.
- The target disk is chosen by rule. The default is the first internal, non-removable disk. The agent refuses USB disks and the disk it booted from.
- Boot files are written with `bcdboot W:\Windows /s S: /f UEFI` or `/f BIOS`, explicitly. MDT omits `/s` and `/f` on UEFI.
- WinRE is copied to `R:\Recovery\WindowsRE` and registered with `reagentc /setreimage /path R:\Recovery\WindowsRE /target W:\Windows`.

## Applications

- An application record holds its name, version, files, install command, success codes, reboot behavior, detection rule, dependencies, and supported architectures.
- For MSI files, the server reads the product code and suggests the silent command and the detection rule.
- Detection rules check an MSI product code, an `Uninstall` registry key, a file version, or a PowerShell script. A detected app is skipped.
- Exit codes:
  - 0 means success.
  - 3010 and 1641 mean success, then a reboot, then the next app.
  - Any other code fails the step unless the app lists it as a success code. MDT logs unexpected codes and marks the app as installed.
- Dependencies install first. The server rejects dependency cycles when you save. MDT's editor rejects only an app that depends on itself.
- A bundle is an application with no command, only an ordered list of dependencies. MDT uses the same idea.
- `InstallApplications` takes either one application or a variable that holds a list. App lists can come from the task sequence, from a pre-staged record, or from an order row column. MDT's Install Application step chooses between these by whether a GUID field is empty.

## Boot image

- The base is the WinPE image from the Windows ADK (`winpe.wim`), or index 1 of the imported ISO's `sources\boot.wim`, which is WinPE. deployarr never ships Microsoft files. They come from media you provide.
- `wimlib-imagex update` adds these files on Windows and on Linux:
  - the agent
  - `winpeshl.ini`
  - a config file with the server URL
  - the server CA certificate
  - the WinPE driver set
- The boot image holds no passwords or credentials. MDT's unattended setup needs share credentials in `Bootstrap.ini` inside the boot image.
- wimlib can add files but cannot add WinPE optional components such as PowerShell. Those are servicing packages that only DISM installs, and MDT adds each one with `dism /Add-Package`, followed by its language package. On a Linux server, a sequence that runs PowerShell in WinPE therefore needs a base image already serviced with WinPE-WMI, WinPE-NetFx, WinPE-Scripting, and WinPE-PowerShell on a Windows machine. A server running on Windows can do that servicing itself. The importer records which components a base image contains, and the server rejects a sequence that needs one the image lacks.
- The builder rebuilds the boot image from the untouched base every time, and keys each build by a hash of its recipe: the base, the files, and the drivers. Any failed step fails the build. MDT patches the previous image instead, ignores servicing failures, and records failed items as present, so it never retries them.
- Outputs are `boot.wim` for PXE, plus an ISO for USB sticks and remote-management virtual media. The ISO is built with `xorriso`, using the El Torito boot files from the imported Windows media. It uses the no-prompt EFI boot image, `efisys_noprompt.bin`, so it starts without a key press. MDT's ISO uses `efisys.bin`, which waits for a key press (inferred from ADK behavior).

## PXE

deployarr's PXE design follows ConfigMgr's PXE responder, `sccmpxe.exe`, which boots WinPE with its own DHCP and TFTP code and no WDS service. [ConfigMgr 2509 reference](research/configmgr-2509.md#pxe-responder) records how it works.

### Boot chain

- Secure Boot stays on, because every boot program is Microsoft-signed. The importer takes them from the boot image: `Windows\Boot\EFI\bootmgfw.efi`, and `wdsmgfw.efi`, `wdsnbp.com`, `pxeboot.n12`, `pxeboot.com`, `bootmgr.exe`, and `abortpxe.com` from `Windows\Boot\PXE`, plus `Windows\Boot\Fonts` and `boot.sdi`. deployarr never ships Microsoft files.
- ConfigMgr never gives the firmware the boot manager directly. Its chain has two stages:
  1. The firmware's request gets WDS's network boot program, `wdsmgfw.efi` on UEFI or `wdsnbp.com` on BIOS, plus DHCP option 250.
  2. That program sends a second request that carries option 250. The reply holds the boot manager (`bootmgfw.efi` on UEFI, `pxeboot.n12` on BIOS, which loads `bootmgr.exe`) and option 252, the TFTP path of the BCD.
  3. The boot manager reads the BCD and loads `boot.sdi` and `boot.wim` over TFTP into a RAM disk.
- A single stage, with `bootmgfw.efi` and option 252 in the first reply, would be simpler, but no Microsoft product is known to do it. The prototype tests both chains. deployarr ships the single stage only if it boots every lab firmware with Secure Boot on. Otherwise it ships the two-stage chain.
- Every BIOS PC sends option 93 = 0, whatever its CPU. In the two-stage chain, `wdsnbp.com` reports the real CPU in its second request. deployarr supports only x64, so the single stage would send the x64 BIOS files to every BIOS PC.
- For zero touch, nothing waits for a key press: `pxeboot.n12` on BIOS, and option 250 sub-option 11 = 2 for `wdsmgfw.efi` on UEFI. These are what ConfigMgr sends for a required deployment.
- In WinPE, the agent reads the DHCP reply that booted it from `HKLM\SYSTEM\ControlSet001\Control\PXE\BootServerReply` and logs which server sent it. The boot image's config still names the server. DHCP and TFTP carry no secrets. ConfigMgr hides a password in that reply under keys that are hard-coded in its binaries, so anyone who captures the reply can recover it.

### DHCP

- deployarr works only as a proxyDHCP server, because on one subnet it hears every PXE broadcast itself. The existing DHCP server keeps handing out addresses, and deployarr answers only PXE clients. That server needs no options 66 and 67. It binds UDP 67 and 4011 on each chosen interface address, so it knows which address to put in its replies. An admin chooses interfaces by MAC address. A setting skips port 67, so deployarr can run on a host that also runs a DHCP server.
- deployarr answers a DISCOVER or a REQUEST that carries option 93 (client architecture), with or without option 97 (machine GUID). It ignores INFORM. On port 67 it answers a REQUEST only when the REQUEST carries option 250, which a WDS network boot program sends in its second request. It does not answer the REQUEST with which a PC accepts the DHCP server's lease. A firmware's boot file REQUEST must therefore reach port 4011. ConfigMgr differs on both points. It drops a request without option 97. On port 67 it answers any PXE REQUEST, the lease REQUEST included ([ConfigMgr 2509 reference](research/configmgr-2509.md#requests-it-answers)).
- The port 67 rule keeps PXE working when deployarr and the DHCP server answer from one address. An ACK to the lease REQUEST from that address carries the same option 54 as the DHCP server's ACK, with `yiaddr` 0.0.0.0. The review of the PXE prototype for [#24](https://github.com/Tsukidy/deployarr/issues/24) tested this case, with the prototype and dnsmasq 2.91 on one address. [#55](https://github.com/Tsukidy/deployarr/issues/55) records the results. With a scripted client, the prototype's ACK arrived first in 20 of 20 trials. OVMF 2024.02 took the prototype's ACK as the DHCP server's. It sent DHCPDECLINE and stopped with `PXE-E07`. With the lease REQUEST unanswered, OVMF sent its PXE REQUEST to port 4011. It then downloaded the network boot program. In the same review, the iPXE ROM of a SeaBIOS guest also sent its PXE REQUEST to port 4011 ([#56](https://github.com/Tsukidy/deployarr/issues/56)). A firmware that sends its boot file REQUEST only to port 67 gets no boot file. No firmware tested so far does that. If a lab run finds one, the owner reopens [#55](https://github.com/Tsukidy/deployarr/issues/55).
- deployarr answers a request without option 97 because the option 97 condition adds no wipe safety. The PXE reply only decides whether a PC boots WinPE. [Wipe safety](#wipe-safety) still decides whether deployarr erases the PC's disk. It recognizes a finished PC by its MAC address, which every request carries. Some boot ROMs send no option 97. With ipxe-qemu 1.21.1 on QEMU 8.2.2, a SeaBIOS guest sends none on the default `pc` and `q35` machine types, even when it has an SMBIOS UUID ([#56](https://github.com/Tsukidy/deployarr/issues/56)). OVMF sends option 97. A reply to a request without option 97 carries none. RFC 4578 section 2.3 says a PXE-compliant client or server must send option 97 in every DHCP and PXE packet. Neither that request nor the reply complies. The rule changes nothing for compliant firmware, which sends option 97.
- The OFFER sets `siaddr` to deployarr's address and carries options 53, 54, 97 (echoed when present), 60 (`PXEClient`), and 82 (echoed when present). It carries no boot file and no option 43. Only the ACK to a REQUEST carries the boot file, in the BOOTP `file` field, with option 250 or option 252.
- Replies go to the request's source address and port. deployarr broadcasts only when the source is `0.0.0.0`, and it answers a relayed request by unicast to the relay. On routed networks, a DHCP relay (IP helper) forwards PXE clients to deployarr.
- The first release supports PXE on one subnet, and the lab tests only that. The relay handling above stays in the design, so routed subnets can come later without a redesign. Before they do, the design must choose where a relayed reply goes. deployarr sends it to the request's source address, as ConfigMgr does. RFC 2131 section 4.1 sends it to port 67 at the relay address in `giaddr`. The two differ when a relay sends from another interface (inferred).
- deployarr offers the [boot chain](#boot-chain) to every request that the rules above accept, except one from a PC whose deployment succeeded. [Wipe safety](#wipe-safety) names the keys that recognize that PC. A finished PC gets no reply on UEFI, so the firmware moves on to the next boot device. On BIOS it gets `abortpxe.com`.
- The reply does not depend on an order, because the service cannot see one. A PXE request carries no baseboard serial and no order number. With the SQL order lookup add-on, the order table matches a PC by its baseboard serial, which the agent reads in WinPE, or by an order number that a technician types in WinPE. The [lookup chain](#lookup-chain) decides what a PC runs once it boots WinPE. [Wipe safety](#wipe-safety) still holds, so deployarr erases that PC's disk only after an identity match, a typed order number, or a picked task sequence.
- ConfigMgr looks a PC up through its management point by the option 97 GUID and the hardware address before it replies. It answers an unknown PC only when unknown computer support is on, and then with the deployments for unknown computers ([ConfigMgr 2509 reference](research/configmgr-2509.md#deployment-decision)). deployarr answers an unknown PC as ConfigMgr does with that support on and a required task sequence deployed to unknown computers. A finished PC gets what ConfigMgr sends a PC with no deployment.
- deployarr cannot share UDP 67, 69, or 4011 with a Windows DHCP server or WDS on the same host. The installer checks for them.
- IPv6 PXE is not in the first release. ConfigMgr's DHCPv6 reply carries option 59 with a `tftp://` URL and the BCD path in a Microsoft vendor option. It does not handle relayed DHCPv6.

### BCD

- The BCD stores are fixed templates, one per firmware type, with fixed paths such as `\boot\x64\boot.wim`. Option 252 names the template for the client's firmware. The server never edits a BCD file. It serves a different boot image at the same path. The templates are made once with `bcdedit`, and the repository keeps the `bcdedit` script that created them. ConfigMgr builds each BCD at run time through the BCD WMI provider, which exists only on Windows.
- `{bootmgr}` has a description, a timeout, `displayorder` and `default` set to the loader, and `fontpath` set to the served fonts folder.
- `{ramdiskoptions}` has `ramdisksdidevice boot` and `ramdisksdipath \boot\boot.sdi`, as in ConfigMgr's store and MDT's offline media.
- The loader has `device` and `osdevice` set to `ramdisk=[boot]\boot\x64\boot.wim,{ramdiskoptions}`, `systemroot \windows`, `detecthal yes`, and `winpe yes`. It also has `path \windows\system32\boot\winload.efi` (or `winload.exe` for BIOS). ConfigMgr's loader has no `path`, and the prototype checks whether deployarr can drop it too.
- `ramdisktftpblocksize` and `ramdisktftpwindowsize` are written only when an admin sets them. ConfigMgr leaves both out by default, so the boot manager picks its own values.

### TFTP

- The TFTP server negotiates `blksize` (RFC 2348) up to 32768, `tsize` (RFC 2349), and `windowsize` (RFC 7440) up to 32. It clamps each value before it builds the OACK. ConfigMgr echoes the requested window size and clamps it afterwards.
- After an ACK for a block inside the window, it resends from the next block, as RFC 7440 says. ConfigMgr resends the whole window.
- File sizes and counters are 64-bit. ConfigMgr's 32-bit sizes cannot serve a file of 2 GiB or more.
- One session exists per client address, port, and file, so a retransmitted request does not start a second transfer. ConfigMgr starts one for each.
- No setting limits how many transfers run at once. Each transfer is its own async task, so the network link and the disk set the limit.
- Each transfer has its own socket on the address that received the request. The retransmit timer is 5 seconds, with 5 retries, as in ConfigMgr. Both values are settings.
- A request without options gets plain 512-byte blocks and no OACK. ConfigMgr's own WinPE TFTP client sends requests like that.
- A missing file gets ERROR code 1 at once, so the boot manager's probes for files that do not exist fail fast.
- Paths are normalized: `/` becomes `\`, leading `\` is removed, case is ignored, and `..` and `:` are rejected.
- WinPE images are 300 MB or larger. ConfigMgr records no transfer speeds, so the lab measures `boot.wim` transfer time for each block and window size.

### Risks to settle with a prototype first

The lab answers these with packet captures before the PXE services are built:

1. Does `bootmgfw.efi`, sent in the first reply with option 252, load its BCD? This decides between the single-stage and two-stage chains.
2. What do `wdsmgfw.efi` and `wdsnbp.com` send in their second request (destination port, option 250 sub-options), and which files does the boot manager request over TFTP?
3. Does a loader with no `path` element boot on both firmware types?
4. Which block and window size does the boot manager use when the BCD sets none, and how long does `boot.wim` take at each setting?
5. Does the firmware accept an OFFER with no option 43?
6. Do boot programs signed with the Windows UEFI CA 2023 boot? Microsoft's 2011 Secure Boot certificates expire during 2026, and firmware that revokes them boots only 2023-signed programs. ConfigMgr 2509 has no handling for this. In the two-stage chain, `wdsmgfw.efi` needs a 2023 signature as well as `bootmgfw.efi`. The importer reports which certificate signed each boot file.

If the signed chain fails, the fallbacks are UEFI HTTP boot, or the boot ISO through USB or remote-management virtual media. ConfigMgr has no UEFI HTTP boot to learn from. An iPXE and wimboot path over HTTP is faster, but it works only on BIOS or with Secure Boot off, so it stays optional.

## Server

- The server is one `tokio` binary. It uses `axum` for HTTPS, `rustls` for TLS, `sqlx` with SQLite for its own data, and a content store directory for files. It runs as a Windows service through `windows-service`, or as a systemd unit on Linux.
- Add-on calls run on a second `tokio` runtime with its own threads, so PXE, TFTP, and the agent API never share a worker with add-on code. See [Runtime](#runtime).
- The agent API covers sessions, the compiled plan, content manifests, server calls, events, logs, and the post-install device report.
- The admin API serves the web UI. Live progress reaches the UI over server-sent events.
- Each deployment gets its own ID from the server. Succeeded and Failed are final states that later events cannot change. MDT's monitoring service keeps one record per computer, sets a finished record back to Running on its next event, and keeps no history.
- The agent queues events and sends them in the background with retries, plus a heartbeat on a timer. A slow or unreachable server never delays a step. MDT sends each event as a blocking call with a 60-second timeout, and its PowerShell steps send no events at all.
- Installation creates a private CA and a server certificate. Each boot image carries the CA certificate.
- Secrets are encrypted at rest with a server key. This covers per-PC local admin passwords, product keys, domain join passwords, and add-on secrets. Add-on secrets are connection credentials, such as an order database's password or a tenant's client secret, and secret settings, such as a bulk enrollment package. Each deployed PC gets its own random local admin password, which an admin can read in the UI. MDT uses one shared password.
- Staff sign in to the admin web UI with local deployarr accounts. The server stores each password as an argon2id hash. The first release has no other sign-in. Active Directory and OIDC sign-in can come later.

## Add-ons

The base program deploys Windows and covers every standard task sequence feature. An add-on adds an optional feature, such as SQL order lookup. An admin installs one from a package file in the web UI, and the server does not restart. The add-on API becomes a contract that is hard to change once add-ons ship, so [One-way doors](#one-way-doors) lists the choices that become fixed.

Each research claim in this section cites its page. [Add-on design attacks on wasmtime 50.0.1](research/addon-design-attacks-wasmtime-50.0.1.md) records the runs that attacked this design, and the citations name its items. A statement marked "design decision" comes from the design session and has no research behind it. That session's [design](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832653114), [build plan](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832676025), and [decision log](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832689907) are comments on [#83](https://github.com/Tsukidy/deployarr/issues/83).

### Planned add-ons

| Add-on | ID | Milestone | What it uses |
| --- | --- | --- | --- |
| Webhooks | `deployarr.webhooks` | M-A | settings, a secret, an HTTP connection, an event subscription, jobs, and an action |
| SQL order lookup | `deployarr.sql-order-lookup` | M5 | two resolvers, a prompt, SQL connections, and an action |
| Entra ID join | `deployarr.entra-join` | M8 | a join kind, and a step kind that takes a secret file |
| Autopilot registration | `deployarr.autopilot` | M8 | a join kind, a step kind with a `ServerCall`, an HTTP connection, jobs, and an action that returns a file |
| MDT task sequence importer | `deployarr.mdt-import` | M8 | an importer |
| Vendor driver catalogs | `deployarr.driver-catalogs` | after the first release | API level 2 |

Webhooks comes first because a CI test can check it on Windows and Linux with no PC and no database. SQL order lookup comes second. Two different add-ons then exist before anything else is built on the API (design decision). The [Roadmap](roadmap.md) gives the order.

### Runtime

An add-on's code is one WebAssembly component for `wasm32-wasip2`. `wasmtime` runs it inside `deployarr-server`. There is no runtime for native add-on processes. A separate process adds no sandbox for files or the network ([HashiCorp go-plugin 1.8.0 reference](research/go-plugin-1.8.0.md#isolation-and-permissions)), so nothing would back the permissions that the admin approves. A component reaches only what the host hands it.

- One component serves both server systems. Zed ships one component that runs on macOS, Linux, and Windows ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#code-platforms-and-the-api-version)). The add-on prototype ran only on Linux ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#not-tested)), so the M-A check runs the same file on Windows.
- Every call gets a new instance. The host instantiates the component, makes one call, and drops the store. A new store, WASI context, and instance took a median of 22 microseconds over 2000 calls. Over 3000 calls that all trapped, resident memory stopped growing after the first 1000 ([Add-on design attacks, supported claims](research/addon-design-attacks-wasmtime-50.0.1.md#supported-claims)).
- An add-on keeps nothing in memory. Its state is its settings, its store, and its jobs, and the host holds all three. The host therefore needs no supervisor, health polling, restart backoff, or graceful shutdown. go-plugin leaves health checks and restarts to the host ([HashiCorp go-plugin 1.8.0 reference](research/go-plugin-1.8.0.md#crash-handling-health-and-restart)), and Grafana restarts a crashed plugin process with backoff ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#lifecycle-crashes-and-health)).
- A trap ends one call. An upgrade swaps one reference, and a call in flight keeps the version it started with (design decision).
- An add-on has no files and no sockets. The host links WASI with an empty context, which refused a file open, a folder listing, and a TCP connect ([Add-on design attacks, item 11](research/addon-design-attacks-wasmtime-50.0.1.md#item-11-a-pure-call-sees-the-clock-and-fresh-randomness)). The only ways out are six host interfaces: `sql`, `http`, `store`, `jobs`, `settings`, and `log`.
- All egress goes through connections that the host owns. A manifest declares a connection, the admin fills it in, and the host stores its credentials with the server key. The add-on holds only the connection's name. The host checks the grant inside the one function that opens a connection. Spin checks its allow list in the callers, and one entry point skips it ([Spin 4.3.0 reference](research/spin-4.3.0.md#host-provided-connections)).

The host limits each call:

- **Memory.** One budget covers every linear memory and every table that a call creates, 64 MiB by default. A host `ResourceLimiter` sums them and caps the number of instances, memories, tables, and table elements. A table counts at its declared size, because a guest can fill it later. `wasmtime`'s `StoreLimits` caps each memory alone and does not count tables. Under its 64 MiB limit, 16 instances held 1000 MiB, and 8 filled tables held 1831 MiB ([Add-on design attacks, item 1](research/addon-design-attacks-wasmtime-50.0.1.md#item-1-memory-limits-do-not-bound-an-add-on)).
- **Time.** An epoch deadline traps a looping guest, and a `tokio` timeout wraps the whole call. A timeout fires only when its task is polled. A host function that blocked for 3000 ms held a call with a 100 ms timeout for the whole 3000 ms ([Add-on design attacks, item 8](research/addon-design-attacks-wasmtime-50.0.1.md#item-8-add-on-calls-can-starve-the-servers-runtime)). Every host function is therefore async. Driver work that can block runs under `spawn_blocking` with its own timeout.
- **The server's own work.** Add-on calls run on their own `tokio` runtime, with threads apart from PXE, TFTP, and the agent API. The epoch callback yields through `UpdateDeadline::YieldCustom` with `tokio::task::yield_now`, and traps at the deadline. With 8 looping calls on a shared runtime of 4 workers, a 50 ms heartbeat stalled for 5964 ms without a yield, and for 4757 ms with `wasmtime`'s plain yield. On a separate runtime with `yield_now`, its longest gap was 54 ms. With the plain yield on a separate runtime, the heartbeat stayed on time, but no call ended at its own timeout ([Add-on design attacks, item 8](research/addon-design-attacks-wasmtime-50.0.1.md#item-8-add-on-calls-can-starve-the-servers-runtime)).
- **Concurrency.** Each add-on has a limit on calls in flight, 8 by default (design decision).

Zed uses epochs only to yield, and Mattermost sets no timeout on any hook but one ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#what-changes-in-deployarr) item 9, [Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#limits)). A failed call is an error for its caller. It never disables the add-on, because input from a PC must not be able to switch an add-on off. In Mattermost, a member crashed a plugin a few times, and it stayed disabled ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#known-failures-and-lessons)).

### SQL connections

The SQL drivers live in the server, as [Data sources](#data-sources) describes. They build natively for Windows and Linux. In a component, `tiberius` needed WASI libc headers and a patched `rustls-native-certs`, and no SQL Server session ever ran ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#sql-server)). A `sqlx` build in a component needed `--cfg tokio_unstable` ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#tokio-on-wasm32-wasip2)). Spin also keeps its database drivers in the host and gives the guest an interface ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).

`deployarr:sql` has one function, `query(connection, query, params)`. There is no `execute` and no connection string.

- `connection` names a connection that the admin configured. Spin's guest builds the address, password included, and deployarr's guest never does ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).
- `query` names one of the add-on's own `sql_text` settings by its settings path, such as `rules/0/statement`. The admin writes the statement on the settings page. At save, the host checks that it holds one statement and only the parameters that the field declares. The host reads the text from the saved settings itself, so a compromised add-on can run only the admin's statements (design decision).
- The host rewrites named parameters, binds every value, and caps rows and time. A guest that names a path that is not one of its `sql_text` settings gets `no-such-query`.
- One engine-neutral interface serves every engine. Spin has one package per engine, and its MySQL and PostgreSQL types differ ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).

A statement belongs to the admin across upgrades. A settings migration returns a document that the host stores. In a model of the first design, version 2's migration put its own statement where the admin's was, and the next lookup returned payroll rows ([Add-on design attacks, item 3](research/addon-design-attacks-wasmtime-50.0.1.md#item-3-an-upgrade-can-replace-the-admins-sql)). The host therefore compares every `sql_text` value in a migration's result with the saved document, by settings path. A value must be the same text at the same path, or absent, or empty at a path that the old document lacked. Any other result blocks the upgrade, and the review dialog names the statement. An empty statement makes its rule wait for the admin.

The interface bounds which statements run. It does not make a statement read-only. The read-only account and the engine guards in [Write guards](#write-guards) stop a write ([Add-on design attacks, item 2](research/addon-design-attacks-wasmtime-50.0.1.md#item-2-read-only-enforcement-depends-on-the-engine)).

### HTTP connections

`deployarr:http` sends one request on a connection that the admin configured or approved.

- A connection has a base URL. The manifest can fix it, so the review dialog shows it, or the admin types it on the settings page.
- The guest gives path segments and query pairs, never a URL. The host percent-encodes each segment, refuses an empty segment, `.`, and `..`, and appends the segments to the base path. It never parses guest text as a URL, and it checks that the final host is the connection's host. With `Url::join`, the paths `//attacker.example/x` and `\\attacker.example\x` sent the request to `attacker.example`. With string joining, `@attacker.example/x` did the same ([Add-on design attacks, item 13](research/addon-design-attacks-wasmtime-50.0.1.md#item-13-a-guest-path-can-redirect-credentials)).
- The host resolves the name, applies the block list and the proxy setting, verifies TLS, and adds the credentials. Spin applies its network block list to HTTP but not to its database drivers, and its HTTP path has no proxy setting ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)). A Nextcloud app bypassed the core SSRF guard from inside the app ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#security-advisories)).
- The host runs the auth scheme itself: bearer, basic, a header key, OAuth 2 client credentials, or an HMAC-SHA256 signature of the body. The guest cannot set the `Authorization`, `Host`, or `Cookie` header. The add-on never holds a client secret or an access token (design decision).

### Package and signing

A package is one `.daddon` file. It is a ZIP with two stored entries, `payload.zip` and `payload.zip.minisig`. The payload holds `addon.toml`, `addon.wasm`, and the folders `schemas/`, `assets/`, and `docs/`. An unsigned package has no `.minisig` entry. The stock minisign tool checks a package with `minisign -Vm payload.zip` (design decision).

The server checks a package in this order. Each check runs before the next one reads more input:

1. Size caps on the file, the signature, the trusted comment, the entry count, and the unpacked size of the payload. TUF caps sizes before any other work ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 12).
2. The signature over the raw bytes of `payload.zip`.
3. The payload's SHA-256 against the trust bundle's revoked hashes.
4. `addon.toml`, parsed strictly. Its ID and version must equal the signature's trusted comment.
5. The signing key's scope, which must cover the ID.
6. The entry names. An absolute path, `..`, a link, a duplicate, or an encrypted entry is refused, and every file the manifest names must exist. Both Zed CVEs came from archive extraction in host code ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#known-failures-and-lessons)).
7. `addon.wasm`, validated with `wasmparser` against the compile caps below. The host reads the API level from the component's imports here.

What the server does with a package:

- **Nothing is unpacked.** After the checks pass, the payload goes into the content store under its SHA-256, and that blob is the installed add-on. The server reads entries from it and registers assets in the content store by hash. There is no add-on folder to scan, to half-write, or to tamper with. In Jellyfin, a file written into a plugin folder loaded as code at the next start ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#a-file-write-became-code-execution)). Mattermost keeps the uploaded package as the source of truth and the unpacked folder as a cache ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#where-files-land)). deployarr keeps no unpacked copy.
- **No compiled code is stored.** The server compiles each component in memory when it loads the add-on, and keeps no cache. A 4-byte edit to a serialized component changed the value that its function returned ([Add-on design attacks, item 5](research/addon-design-attacks-wasmtime-50.0.1.md#item-5-a-compiled-cache-is-code-on-disk)), so a cache on disk would let a file write become code execution.
- **Compiling is guarded.** A hostile component of 485,757 bytes took 28.7 seconds and 4153 MiB to compile, and a larger one aborted the process ([Add-on design attacks, item 12](research/addon-design-attacks-wasmtime-50.0.1.md#item-12-compiling-a-hostile-component-exhausts-the-server)). The host compiles only after the trust checks pass. It refuses a component over a cap on its size, on one function body's size, or on one function's locals. One thread compiles one component at a time. The prototype's components compiled in 0.48 to 1.22 seconds each ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#sizes-and-times)). The M-A spike sets the cap values.
- **The manifest** is TOML, parsed strictly into `deployarr-core` types. Every table and every field kind refuses an unknown key. A derived `deny_unknown_fields` on a tagged enum accepted `allow_write = true` on a `sql_text` field ([Add-on design attacks, item 10](research/addon-design-attacks-wasmtime-50.0.1.md#item-10-the-manifest-parser-accepts-unknown-keys)). A field therefore parses through one flat struct, and a conversion gives each kind only its own keys.
- **Permissions come from the manifest.** The manifest declares what the add-on contributes: connections, prompts, resolvers, step kinds, join kinds, event subscriptions, jobs, actions, importers, and assets. The host derives the permissions from those declarations, so there is no second list to keep in step. Mattermost's and Jellyfin's research both ask for grants from the manifest ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#what-changes-in-deployarr) item 10, [Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 12).
- **The add-on ID** is `<publisher>.<name>` in lower case. The server parses it with a strict grammar where the package enters. Jellyfin needed a security fix for unsafe package names ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 13).

How the server decides whom to trust:

- The server binary embeds the root public keys. The root keys sign `addon-trust.json`, the trust bundle. It lists publishers with ID scopes, revoked keys, revoked package hashes, and allowed package hashes. k of n root keys sign it, each as a separate `.minisig` file ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) items 1, 6, and 11).
- A publisher key signs the raw bytes of `payload.zip` with a minisign prehashed signature. The server checks it with `minisign-verify` and `allow_legacy = false` ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 3). Version 0.3.0 verified such a signature with that setting, and refused a changed bit and an edited trusted comment ([Add-on design attacks, supported claims](research/addon-design-attacks-wasmtime-50.0.1.md#supported-claims)).
- The trusted comment is `deployarr-addon id=<id> version=<semver>`, and it must equal the manifest. A signed package then cannot install under another ID or version ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 4).
- A key signs only IDs in its scope, such as `deployarr.*`. No two publishers cover the same ID ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 2). The server matches a key by its full public key, never by its 8-byte key ID alone, and revokes a whole key ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) items 5 and 11).
- The bundle has a version that never goes down. Each server release ships one, and an admin can import a newer one from a file. The UI shows the bundle's signing date and age, and the server does not trust that date for security ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) items 7 to 9).
- The checks run again at every server start, every enable, and every bundle import. The server disables a revoked add-on and shows the reason. Nextcloud checks its revocation list only when it downloads an app ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr) Avoid items 4 and 7, [TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 10).
- An admin can add a local key with an ID scope on the Trust page. It follows the bundle's rules, shows as local, and is still subject to the bundle's revoked keys ([TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 13).
- An add-on that was installed signed never accepts an unsigned update or one from another publisher ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr) Avoid item 8).
- Who holds the root keys, and who signs add-ons, are open questions with defaults in the [roadmap](roadmap.md#open-questions).

Unsigned packages:

- The server refuses an unsigned package by default. It installs one only when the server setting "Allow unsigned add-ons" is on and the admin confirms the package's SHA-256 with their password. A package signed by an unknown key is handled the same way. A bad signature never installs.
- The host compiles an unsigned package only after that confirmation, so its code checks run in `apply`, and the review dialog says that they have not run yet ([Add-on design attacks, item 12](research/addon-design-attacks-wasmtime-50.0.1.md#item-12-compiling-a-hostile-component-exhausts-the-server)).
- The add-on shows an Unsigned badge for good. Each load logs a warning, and the audit log records who allowed it. Grafana also logs a warning when it loads an unsigned plugin that an admin allowed ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#unsigned-and-modified-plugins)). Mattermost never verifies an uploaded bundle, and Nextcloud installs a folder copied onto disk with no signature check ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#unsigned-handling), [Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#without-the-app-store)). deployarr has only the upload path, so the check always runs there.
- The web UI may allow this because the sandbox bounds what an add-on can do to the server, and an admin can already run any command on a PC with a `RunCommand` step (design decision).

### API levels and the support window

- **The contract is WIT.** Each capability is its own package, such as `deployarr:sql@1.0.0` and `deployarr:http@1.0.0`, so one capability can get a new major version alone. A released function or type never changes. A minor version only adds, and marks each addition `@since`. Spin moved from one package to one per capability ([Spin 4.3.0 reference](research/spin-4.3.0.md#wit-evolution)). `wasmtime` 50.0.1 linked an import of 1.0.0 to a host that served 1.3.0, and refused an import of 2.0.0 ([Add-on design attacks, supported claims](research/addon-design-attacks-wasmtime-50.0.1.md#supported-claims)).
- **An API level is one number for people.** It names a frozen set: the package versions, the export world, the manifest features, and the task sequence JSON schema versions. `wit/levels.toml` is the one table of levels. `cargo xtask addon pack` computes an add-on's level from the component's imports and the manifest features it uses, and writes `api_level` into the manifest. The host computes it again from the component at `stage`, so the binary is the truth. Zed also reads the API version from the binary ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#code-platforms-and-the-api-version)).
- **The server checks the level.** `stage` refuses a package outside the levels that the server serves, and shows both ranges. `reconcile` checks again at every start. A server upgrade marks an add-on Incompatible, keeps its saved enabled flag, and restores it when a compatible version is installed ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr) Copy item 3). The server installer lists the add-ons that an upgrade would disable before it proceeds ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 15).
- **Support window.** A capability's major version stays served for at least 24 months after the stable release that ships its successor. It is removed only in a server major release whose predecessor announced the removal. This is the default, and the roadmap lists the window as an [open question](roadmap.md#open-questions). Zed has no written window and still loads every API version since 0.0.1 ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#deprecation-policy)). The Jellyfin and go-plugin research asks for at least one server major release of overlap ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 17, [HashiCorp go-plugin 1.8.0 reference](research/go-plugin-1.8.0.md#what-changes-in-deployarr) item 8).
- **Old levels keep running.** The host keeps one binding module per level and converts each older level to its newest code, as Zed does ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#how-old-versions-keep-working)).
- **The API grows by addition.** A new host ability is a new function, or a new package at a new level. A new extension point is a new export in the next level's world. A new manifest table or field kind belongs to a level. An older server refuses it with "this add-on needs a newer deployarr server", because its parser is strict. Spin's `must_understand` does the same for its manifest ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)). Documents that must evolve, such as task sequences, event payloads, and config, are JSON with their own schema versions, so they grow without a WIT change (design decision).
- **No unreleased API ships in a release.** Until the first release, the API is level 0, and only a development build serves it. Breaking changes wait in `wit/PENDING_CHANGES.md`. Zed gates unreleased API versions the same way ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#versions-and-negotiation)). M9 freezes level 1.

### Install lifecycle

- **Two calls.** `stage(upload)` runs the checks and returns everything the review dialog shows. It changes no add-on's state. `apply(staged, approval)` installs, upgrades, downgrades, or reinstalls, whichever applies (design decision).
- **Desired state and observed state are separate.** The database stores what the admin asked for: the active package hash, the previous one, and `enabled`. The state that the UI shows, such as Enabled, Disabled, Incompatible, Revoked, or Failed with a reason, is computed and never stored. Jellyfin re-enabled disabled plugins until 12.0, because it saved a memory-only status in its stored manifest ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#state-bugs-in-the-plugin-manager)).
- **One function reconciles.** `reconcile` makes the assets and the in-memory registry match the database. It runs at start and after every change, and it is safe to run any number of times. Mattermost reconciles every plugin to its desired state the same way ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#failure-and-rollback)).
- **One pure planner decides every change.** `plan(registry, change, facts)` in `deployarr-core` returns the steps and the effects of an install, upgrade, rollback, disable, uninstall, or purge, or a refusal with its reasons. The review dialog shows the plan's effects, and the executor runs its steps, so the dialog shows what will happen. The planner does no I/O, so table tests cover every refusal (design decision).
- **Every change is a journaled operation.** A row in `addon_ops` records each phase before its work starts. A unique index allows one open operation per add-on, so a second request gets a conflict. Grafana's research asks for that conflict ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#what-changes-in-deployarr), Avoid), and a SQLite partial unique index refused a second open row ([Add-on design attacks, supported claims](research/addon-design-attacks-wasmtime-50.0.1.md#supported-claims)). The commit is one SQLite transaction that sets the active hash, the previous hash, the grants, and the settings. A crash before it leaves the old version active, and `reconcile` repeats or abandons the operation.
- **Install and upgrade check before they commit.** The host prepares the new version beside the old one, instantiates it once, compares `describe` with the manifest, and checks the settings migration. Only then does it commit. `stage` runs these code checks for a signed package, so the dialog can show the result. `apply` runs them again for every package, because settings can change between the two calls. A failure abandons the operation with its reason, and the old version stays active. The old package stays until the next successful upgrade. Grafana and Mattermost delete the old version first and have no rollback ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#failure-and-rollback), [Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#failure-and-rollback)).
- **An upgrade waits for deployments.** Planning records which add-on versions each deployment uses. An upgrade, disable, or uninstall of an add-on in use waits, is refused, or is forced, as the admin chooses. There are no automatic updates. Jellyfin updates every plugin at startup and every 24 hours by default ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#automatic-updates)). The Jellyfin and Zed research asks that no update apply while a deployment uses the add-on ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 9, [Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#what-changes-in-deployarr) item 15).
- **A waiting operation can be cancelled or taken over.** A waiting upgrade holds the one open operation. In a model of the first design, Force, Roll back, and Uninstall therefore each got a conflict ([Add-on design attacks, item 14](research/addon-design-attacks-wasmtime-50.0.1.md#item-14-a-waiting-upgrade-blocks-every-other-operation)). `cancel` abandons a waiting operation, and `apply` with Force for the same package takes over the waiting row. Any other change still gets a conflict that names the waiting operation.
- **Roll back** is its own operation. It switches to the previous package. When the two versions use different data versions, it restores the store snapshot from before the migration, and it tells the admin how many keys changed since. A downgrade by upload needs an explicit confirmation ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#what-changes-in-deployarr) item 17, [TUF 1.0.37 and minisign 0.12 reference](research/tuf-1.0.37-minisign-0.12.md#what-changes-in-deployarr) item 7).
- **Disable** flips `enabled` and swaps the registry. It runs no add-on code and deletes nothing. Nextcloud runs an app's uninstall steps on every disable ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#disable-and-remove)).
- **Uninstall** is refused, with the list, while a current task sequence version, the lookup chain, a join profile, or a running deployment uses the add-on ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#what-changes-in-deployarr) item 2). It keeps the settings, secrets, and data. **Purge** is a separate confirmed action that lists what it deletes ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr) Copy item 9). Step kind and join kind descriptors are never deleted. See [Task sequences stay loadable](#task-sequences-stay-loadable).
- **The admin confirms again.** Install, upgrade, enable, uninstall, and any change to a secret or a connection address need the admin's password again. At Nextcloud, a stolen session could change storage settings without a password confirmation ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#security-advisories)).

### Capabilities and isolation

| What an add-on can reach | How the admin grants it | How the host enforces it |
| --- | --- | --- |
| Which task sequence a PC gets | Approves "Decide which task sequence a PC gets. A wrong match erases a PC." at install | Calls `resolve` only for declared resolvers, only with a usable identity fact or a typed answer, and checks that the task sequence exists |
| Commands on PCs | Approves "Run commands on PCs as LocalSystem" at install | `deployarr-core` parses the `expand` output and allows six built-in actions. No command gets a secret. |
| A SQL database | Fills in the connection on the settings page | The add-on names a connection and one of the admin's statements. The host connects, binds the values, and applies the engine's guard. |
| An HTTPS host | Approves a fixed base URL at install, or types the URL on the settings page | The host builds the URL from segments, checks the host, applies the block list and the proxy, verifies TLS, and adds the credentials |
| Deployment events | Approves the event kinds at install | The queue delivers only the subscribed kinds, with secrets redacted |
| Its own data | Always | One store file per add-on, with a quota |

There is no process execution, no file access, no access to the server's database, no other add-on's data, and no admin session. Zed lets an extension run a native process outside its sandbox, and its grants are global ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#capabilities-and-grants)). Two Grafana CVEs passed a user's token or cookie to a plugin ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#known-failures-and-lessons)).

Every host function starts with one check, `permit(scope, capability, target)`, and nothing else decides. The host sets the scope from the export it calls:

| Export | Scope | What `permit` allows |
| --- | --- | --- |
| `describe`, `steps.expand`, `importer.import`, `migrate.config` | `Pure` | `settings` and `log` |
| `resolver.resolve`, `steps.call`, `events.handle`, `jobs-handler.run`, `actions.run` | `Granted` | `settings`, `log`, `store`, `jobs`, and the SQL and HTTP connections that the admin granted |
| `migrate.data` | `Migration` | `settings`, `log`, and `store`, inside the migration's transaction |

- **The table covers every export.** The first design gave `expand` no capability at all and put `steps.call` in no scope, which blocked calls that the sketched add-ons make ([Add-on design attacks, item 7](research/addon-design-attacks-wasmtime-50.0.1.md#item-7-the-permit-scopes-blocked-the-sketched-add-ons)). `Scope::of` is one exhaustive match over this table, so a new export does not compile until it has a scope.
- **A `Pure` call gives the same output for the same input.** An empty WASI context still hands out the real clocks and a fresh random seed. A `Pure` call therefore gets a fixed wall clock, a fixed monotonic clock, and seeded random generators. With them, two runs of the same guest printed the same output ([Add-on design attacks, item 11](research/addon-design-attacks-wasmtime-50.0.1.md#item-11-a-pure-call-sees-the-clock-and-fresh-randomness)). A plan then does not differ between two runs on the same input.
- **`describe` says what a component implements.** Every component exports the same interfaces, and kinds are strings at run time, so the export list says nothing ([Add-on design attacks, item 15](research/addon-design-attacks-wasmtime-50.0.1.md#item-15-stage-cannot-compare-exports-with-the-manifest)). The `describe` export returns the kinds that the component implements. The SDK generates that answer from the manifest. `stage` and `apply` refuse a package whose answer differs from its manifest. A call for a kind that the component does not implement returns the typed error `unknown-kind`.
- **The admin approves what the dialog shows.** The review dialog lists the permissions in plain sentences. An upgrade that asks for more shows the difference, and the admin approves a digest of exactly the set shown (design decision). Zed checks only one of its three capabilities against the manifest ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#capabilities-and-grants)).

### Configuration, secrets, and data

- **One form language.** `FormSchema` in `deployarr-core` describes add-on settings, step config, join profiles, and action input. The Leptos UI renders it, the server validates with it, and it ships in the package as a file. MDT kept one step's rules in a MOF, a WinForms control, and a script ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#the-step-contract)). The field kinds are text, integer, bool, choice, template, `sql_text`, secret, secret file, connection reference, task sequence reference, and list. A later level can add a kind, and no level changes one (design decision).
- **No secret reaches add-on code, or a command that an add-on wrote.** The host stores a secret field with the server key. The UI and the add-on see only "set" or "not set" ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr) Avoid item 14). The host uses a secret in two places only:
  - It applies a connection's credentials.
  - It hands a secret to a built-in action that takes one as its input. In level 1 that action is `AddProvisioningPackage`, and the agent builds its DISM command line itself.
- **Commands get no secret.** The host attaches no secret file and no secret variable to a `RunCommand` or `RunPowerShell` step, and `ServerCall` refuses to send a variable marked secret. In a model of the first design, an add-on's own command read its secret file and sent the bytes back through `ServerCall` ([Add-on design attacks, item 9](research/addon-design-attacks-wasmtime-50.0.1.md#item-9-an-add-on-can-read-its-own-secret-file)). Spin marks secrets, and no code reads the mark ([Spin 4.3.0 reference](research/spin-4.3.0.md#variables-and-secrets)).
- **One limit remains.** An add-on with the "Run commands on PCs as LocalSystem" permission has full rights on the PC. It can read what is on the disk, including a provisioning package after DISM adds it. The install dialog shows that permission for this reason (design decision).
- **Data is a document store per add-on.** `store` offers `get`, `put` with a revision check, `delete`, and `list` by prefix. Behind it is one SQLite file per add-on. An add-on then never holds the write lock of the server's database, and the host can show the store's size, export it, back it up, snapshot it, and purge it. Nextcloud's research asks whether add-ons should own SQL tables at all, because that ownership is what removal leaves behind ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#what-changes-in-deployarr), Open questions).
- **Schema changes are versioned and forward-only.** The manifest states `settings.version`, `data.version`, and a `config_version` for each step kind and join kind. `migrate.config` converts one document up one version, in the `Pure` scope. `migrate.data` converts the store. The host runs each step and writes the new data version in one transaction on the store file. Install and upgrade run the same steps. Nextcloud wraps no migration's data steps in a transaction, and it skips them on a fresh install ([Nextcloud apps 35.0.3 reference](research/nextcloud-apps-35.0.3.md#database-migrations-and-repair-steps)).

### Extension points

| Extension point | In API level 1 | How it works |
| --- | --- | --- |
| Lookup resolvers | Yes | `resolver.resolve`. The chain holds `Resolver::AddOn`. See [Add-on resolvers and wipe authority](#add-on-resolvers-and-wipe-authority). |
| Agent prompts | Yes, declarative | A `[[prompt]]` in the manifest. The agent renders it as a `PromptSpec`, in the lookup phase only. |
| Settings pages | Yes, declarative | A `FormSchema`, plus connection forms that the host renders, each with a test button |
| Admin actions | Yes | `actions.run`, with a schema for the input and one of four outputs: a message, a table, a trace, or a file download. The host checks sign-in, CSRF, and size limits first. No raw HTTP reaches an add-on. |
| Task sequence steps | Yes | `steps.expand` when a deployment is planned, and `steps.call` for `ServerCall` |
| Directory join kinds | Yes | A `[[join_kind]]` names a profile form and, for each stage of the built-in `ApplyJoinProfile` action, one of the add-on's step kinds. It adds no export. |
| Event subscriptions | Yes | `events.handle`, delivered at least once and in no promised order, through the host's job queue |
| Background work | Yes | `jobs.enqueue` and `jobs-handler.run`, with backoff. Webhook delivery and Graph calls use it. |
| Importers | Yes | `importer.import` returns a task sequence in the [JSON format](#json-format). The host validates it and opens it as a draft. |
| Driver library access | No, level 2 | A `deployarr:library` package that fetches a driver pack into the content store and calls the base importer |
| Timed schedules | No, level 2 | A `[[schedule]]` table. A level 1 add-on enqueues a delayed job instead. |
| Pages with add-on UI code | No | If one is ever needed, a sandboxed frame on its own origin |
| Inbound HTTP without sign-in, conditions, facts, and agent code | No | None of the planned add-ons needs them |

- Admin actions are typed, and the host checks the request first. Mattermost's plugin routes skipped MFA and read bodies with no size limit ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#known-failures-and-lessons)).
- No add-on code runs in the admin origin. Three Grafana CVEs came from plugin content in the main origin ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#known-failures-and-lessons)). Mattermost's web app plugins broke when the host moved to React 19 ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#known-failures-and-lessons)).
- The importer point is in level 1 so that M8's MDT importer proves it before the API freezes. If the owner moves the importer after the first release, the importer point leaves level 1 (design decision).

### Add-on resolvers and wipe authority

The lookup chain's rules stay in the base, as [Lookup chain](#lookup-chain) describes. An add-on resolver returns a match and never wipe authority. `WipeAuthority::from_addon_match` in `deployarr-core` is the only way an add-on's match becomes authority. It takes the resolver's declared input kind, the input the server sent, and the match's `matched_on`.

- `matched_on` is `Option<IdentityKind>`, an enum. In the first design it was a string, and the reference add-on filled it with free text ([Add-on design attacks, item 4](research/addon-design-attacks-wasmtime-50.0.1.md#item-4-wipe-authority-cannot-come-from-free-text)).
- For an identity resolver, `matched_on` must name a kind, and the server must have sent a fact of that kind. `UsableIdentity` holds the facts that the server may send. Its only constructor drops known placeholders and returns none when nothing is left.
- For a resolver with a prompt, `matched_on` must be none. The authority is the technician's typed answer.
- Any other combination is an error, and the chain moves on.
- The host writes the trace line from the fact it sent, never from add-on text.
- `IdentityKind` lists the system serial, the baseboard serial, the chassis serial, the UUID, and the MAC address. The WIT enum `identity-kind` repeats the list, and a test compares the two. A new kind needs a new API level (design decision).

### Task sequences stay loadable

A step from an add-on must load, save, show, diff, and export when its add-on is disabled, removed, or upgraded. Removing MDT's ConfigMgr extension before its steps corrupts task sequences, and a copied sequence breaks where the extension is missing ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#known-failures-and-lessons)).

The model is generic over what a step holds:

```rust
pub type AuthoredSequence = TaskSequence<Action>;         // what an admin edits and the server stores
pub type LoweredSequence = TaskSequence<BuiltinAction>;   // what the compiler and the agent see

pub enum Action {
	Builtin(BuiltinAction),   // written in JSON exactly as a built-in action
	AddOn(AddOnStep),
	ApplyJoinProfile { stage: JoinStage },   // OfflineImage or FullOs. The join profile fills it when the plan is made
}

pub struct AddOnStep {
	pub addon: AddOnId,           // <publisher>.<name>
	pub kind: KindId,             // a step kind that the add-on declares
	pub config_version: u32,      // never rewritten in a saved version
	pub config: Box<RawValue>,    // the JSON text that the editor saved
}

pub enum BuiltinAction {
	// Every action of `Action` in Core data shapes. `RunCommand` and `RunPowerShell`
	// gain `files: Vec<StepFile>`, which holds content, never a secret.
	ServerCall { send: Vec<VarName>, limits: StepLimits },
	AddProvisioningPackage { package: SecretRef, limits: StepLimits },
}
```

`TaskSequence`, `Node`, `Group`, and `Step` from [Core data shapes](#core-data-shapes) take the action type as a parameter.

- **An add-on step is plain data.** In the [JSON format](#json-format) it is `{"add_on": {"addon": "deployarr.autopilot", "kind": "register", "config_version": 1, "config": {"group_tag": "%OrderGroupTag%"}}}`.
- **The config keeps its bytes.** The step keeps the JSON text that was saved, a `Box<RawValue>`. Parsed into a `serde_json::Value`, a config came back with its keys reordered and a 23-digit integer turned into a float ([Add-on design attacks, item 6](research/addon-design-attacks-wasmtime-50.0.1.md#item-6-stored-steps-lose-bytes-and-fields)). A `Box<RawValue>` field does not parse inside an internally tagged enum, and it does in a plain struct (same item). The externally tagged form of the JSON format does not buffer the variant, so it can hold one (inferred, and M0's round-trip check tests it).
- **Unknown fields are errors.** The envelope refuses unknown fields, as every built-in action does. A step from a newer schema is then an error, not a silent loss ([Add-on design attacks, item 6](research/addon-design-attacks-wasmtime-50.0.1.md#item-6-stored-steps-lose-bytes-and-fields)).
- **Saving runs no add-on code.** `validate` checks the config against the kind's schema at its config version. An unknown kind is a warning, not an error. Only planning a deployment runs add-on code.
- **A removed add-on's step still shows.** The server keeps a descriptor, the label and the schema, for every step kind and config version that was ever installed, and never deletes it. The editor therefore shows a removed add-on's step as a read-only form with a banner ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#what-changes-in-deployarr) item 1). The admin can move, disable, or delete the step. The compiler drops a disabled step, so the sequence deploys again.
- **A missing add-on stops a deployment before the disk changes.** When an add-on is disabled, removed, incompatible, or revoked, planning fails with `AddOnUnavailable`. The deployment stops at the lookup screen with the reason. The task sequence list marks such sequences, and the disable dialog lists them first.
- **Upgrades migrate config in memory.** After an upgrade, a stored step keeps its `config_version`. The host migrates the config in memory when it plans a deployment and when the editor opens the step. A saved version is never rewritten, so history stays true. A config older than the add-on accepts, or newer than it knows, is a plan error. MDT never upgraded stored steps ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#known-failures-and-lessons)).
- **An export says what it needs.** An export carries the descriptors of the add-on steps it uses and a derived list of the add-ons it needs. Another server can show the steps and say what to install.
- **A running deployment is not affected.** Its program was lowered and frozen when it started.

Planning runs in two parts. `AddOnHost::lower` asks each add-on to expand its steps and the join slots, and `substitute` in `deployarr-core` puts each expansion in place of its step. `compile` then takes the `LoweredSequence`, so a program that names an add-on does not type-check. `substitute` keeps the step's `NodeMeta` on a group that holds the expansion, so the step's condition and `continue_on_error` still apply. It rejects a missing expansion, an action outside the six that an add-on may return, a child with another `run_in`, and a nested add-on step. The IDs of the new nodes derive from the step's ID, so the same input gives the same program (design decision).

### What runs on the PC

On the PC, only the agent and the commands of the task sequence run. The server expands an add-on step into built-in steps, as MDT's stored step resolves to a run of a program from a content package ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#on-the-pc)). The agent stays one static executable for base WinPE and never loads add-on code ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#what-changes-in-deployarr) item 13, [MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#what-changes-in-deployarr) item 9). It gains two generic abilities and one built-in action, and it learns nothing about add-ons:

- **Step files.** `RunCommand` and `RunPowerShell` can list files that the agent puts in a private step folder before the step and deletes after it. A step file is content, such as a script from the add-on's assets, and never a secret ([Add-on design attacks, item 9](research/addon-design-attacks-wasmtime-50.0.1.md#item-9-an-add-on-can-read-its-own-secret-file)).
- **`AddProvisioningPackage`.** The agent writes a secret package to a private folder, runs DISM with a command line that it builds itself, and deletes the file.
- **`ServerCall`.** The agent sends named variables to the server and assigns the variables that come back. It refuses to send a variable marked secret. The server finds the add-on and the config from the deployment and the node, so no config or credential travels to the PC. The server stores the first result for each deployment and node and replays it, so a rerun after a crash is safe.

An add-on step may expand to six built-in actions in level 1: `RunCommand`, `RunPowerShell`, `SetVariable`, `Restart`, `ServerCall`, and `AddProvisioningPackage`. `BuiltinAction::allowed_from_addon` is one exhaustive match, so a new built-in action does not compile until someone decides whether add-ons may use it (design decision).

Prompts are data too. The agent renders a `PromptSpec` in WinPE, and a barcode scanner still works as a keyboard. Entra ID join becomes one `AddProvisioningPackage` step with the bulk enrollment package as its secret input. Autopilot becomes a PowerShell step that collects the hardware hash, then a `ServerCall`. See [Directory join](#directory-join).

### Known limits

- An abort inside a native SQL driver stops the whole server, PXE included. The host catches panics at the capability boundary, but not an abort or an out-of-memory failure. `tiberius` and `sqlx` are Rust, with C only in `ring` (inferred from their dependency lists, not measured).
- Each server start compiles every enabled add-on, one at a time. The prototype's components took 0.48 to 1.22 seconds each ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#sizes-and-times)).
- `wasmtime` adds about 25 MB to the server binary, an upper bound from the stripped prototype host ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#what-changes-in-deployarr)). The base binary also carries the SQL drivers, unused until a SQL add-on is installed.
- No run shows that the compile caps bound a compile for every input. If the spike finds a component under the caps that still exhausts memory, the next step is a compile in a short-lived child process under an operating system memory limit. This design does not cover that.
- A new HTTP auth scheme, a new way to use a secret on a PC, and a new identity kind each need a server release or a new API level.
- A settings migration cannot reorder rules that hold statements, because the host matches a statement by its settings path. List items would need stable IDs, which level 1 does not have.
- The attack runs ran on Linux only ([Add-on design attacks, Sources](research/addon-design-attacks-wasmtime-50.0.1.md#sources)), and no SQL Server ran ([Add-on design attacks, Not rerun](research/addon-design-attacks-wasmtime-50.0.1.md#not-rerun)). The M-A check repeats the runtime runs on Windows, and the M5 check runs SQL Server.

### One-way doors

These choices are hard or impossible to change after level 1 freezes in M9. The owner decides each one, and each entry names the default. The design session chose this design from four candidate designs. Its [decision log](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832689907) records the base it picked and what it took from the others, in the rows from 2026-11-12 at 14:53:47 and 14:53:48 UTC.

1. **The runtime contract.** This covers WebAssembly components on WASI 0.2, synchronous guest calls, no sockets, no files, a new instance for every call, the scope of each export, and a fixed clock in `Pure` calls. The default is the design in this section, and M-A's check decides it.
2. **Where SQL lives.** SQL lives in the host as `deployarr:sql`, with no `execute`, and with statements that only the admin writes and no migration can change. The default is the design in this section, and M5's check against SQL Server decides it.
3. **The add-on ID grammar**, and the rule that a signing key is scoped to IDs. The default is `<publisher>.<name>`, in lower case.
4. **The package envelope and the signature format**, including the trusted comment. The default is a `.daddon` file with a minisign prehashed signature.
5. **The root keys.** This covers who holds them and how many must sign. Every server embeds them. The default is that the owner holds them offline, and that 2 of 3 sign the trust bundle.
6. **The unsigned policy.** The default refuses an unsigned package unless a server setting allows it and the admin confirms its hash, and it shows a permanent badge.
7. **The level 1 WIT.** This covers the package names, the function signatures, the `sql` value cases, the HTTP request shape with path segments, the `identity-kind` cases, the `describe` record, and the error variants.
8. **The task sequence JSON format as part of the contract.** This covers the add-on step's shape, the config as raw JSON text, the refusal of unknown fields, and the six actions an add-on may expand to. The server must read every schema version from level 1 on.
9. **The form language.** This covers the field kinds in level 1. A kind can be added, never changed.
10. **No secret goes to add-on code or to a command that an add-on wrote.** This covers the list of HTTP auth schemes that the host runs and the built-in actions that take a secret. A permission that reveals a secret can be added later. Once given, it cannot be taken away.
11. **The store's promises.** These cover the key rules, the revision check, the list order, and the caps on values and on the quota.
12. **The event envelope.** This covers the envelope, the event kind names, and delivery at least once with no promised order.
13. **The support window.** The default is 24 months after a successor major version ships, with removal only in an announced server major release.

The roadmap lists four [open questions](roadmap.md#open-questions) that the design session raised with the owner, each with its default. Two are doors 5 and 13. The other two shape doors 3 and 2. They ask who signs add-ons and how deployarr signs in to SQL Server.

These stay two-way doors and need no early decision:

- the limits and their defaults
- the compile caps
- the database tables
- the binding module for each level
- the UI layout
- the agent protocol for `ServerCall` and step files, because the agent ships with the server in every boot image
- whether a frame for add-on UI pages is ever added

## Web UI

The UI is written in Leptos and compiled to WebAssembly, so it uses the same `deployarr-core` types as the server. The editor validates a task sequence with the same code the server runs when you save.

The web UI is the only admin interface in the first release. After it, a desktop program built with Tauri shows the same UI in its own window on the server. Tauri draws the window with the operating system's web view, so the program bundles no browser and stays in the Rust workspace.

The screens are these:

- Deployments: live progress, step timeline, logs, variable trace, and missing drivers.
- Task sequences: a tree editor with a step palette, a typed form for each action, a condition builder, version history, and templates. Step kinds from add-ons appear in the palette under the add-on's name, with forms drawn from their schemas. Import opens a file through an importer add-on.
- OS images: import an ISO or WIM, then choose editions.
- Drivers: import packages, browse by hardware ID, manage profiles, and view missing-driver reports.
- Applications: upload files, set the command, the detection rule, and dependencies, and test detection.
- Order lookup: the lookup chain, with the resolvers that installed add-ons declare.
- Machines: pre-staged PCs, with CSV import.
- Join profiles: workgroup and domain settings, and the join kinds that add-ons declare, such as Entra ID and Autopilot.
- Boot images and PXE: build boot images, set the active image, and choose the PXE interfaces and the port 67 setting.
- Add-ons: each add-on with its version, a trust badge (Official, Verified publisher, Local key, or Unsigned), its state with the reason, and any pending upgrade. **Add add-on...** opens a file picker and then the review dialog. Each add-on has a page with these tabs:
  - Settings: the form and the connections, each with a test button
  - Actions
  - Activity: jobs, failures, and log lines tagged with the add-on
  - Permissions
  - Versions, with Roll back
  - Data: the store's size, an export, and Purge
  - Used by: task sequences, chain entries, join profiles, and running deployments
- Trust: the trust bundle's version and age, an import for a newer bundle, and the local keys.

The SQL order lookup add-on's settings hold its connections, its named queries, and its OS-code map. Its Test lookup action takes an order number or a serial number and shows the resolved task sequence and its trace.

## Crate layout

```
crates/
	deployarr-core     task sequence model, compiler, conditions, variables, templates,
	                   identity rules, API types. No I/O. no_std with alloc. Builds for
	                   Linux, Windows, wasm32, and x86_64-unknown-none.
	deployarr-engine   interpreter, state file, journal, and resume logic over an Executor trait
	deployarr-inf      INF parser and hardware ID index
	deployarr-smbios   SMBIOS table parser
	deployarr-pxe      proxyDHCP and TFTP
	deployarr-server   server binary
	deployarr-agent    Windows agent binary
	deployarr-ui       Leptos web UI
	deployarr-addons   the add-on host. `AddOnHost` is its only public item
	deployarr-addon-sdk  what an add-on author builds against: one trait with default
	                   bodies, the generated WIT bindings, and deployarr-core's types
wit/                 the add-on API: one WIT package per capability, levels.toml,
                     and PENDING_CHANGES.md
addons/              the in-tree add-ons, each a crate built for wasm32-wasip2
xtask/               boot image builds, lab automation, packaging, and the
                     `addon new`, `addon pack`, and `addon sign` commands
```

`deployarr-core` is `no_std` with `alloc`, so library code that names `std` does not compile. That rules out files, sockets, processes, environment variables, standard input and output, threads, and the clock. It also forbids unsafe code, so it cannot call the operating system through an `extern` block. Its unit tests may use `std`, which the crate root grants them with `cfg_attr(not(test), no_std)`. Every dependency must build without `std`, so the manifest turns off default features that pull it in. CI builds the crate for `x86_64-unknown-none`, a target that ships no `std`. That build fails when library code or a dependency needs `std`, when a file adds `extern crate std`, and when the crate root loses its `no_std` attribute. The `wasm32-unknown-unknown` build does not replace it. That target ships `std`, so it accepts `extern crate std`, a crate root without `no_std`, and a dependency that needs `std`.

`deployarr-inf` and `deployarr-smbios` are separate crates because they are pure parsers tested against real files on Linux. The agent targets `x86_64-pc-windows-msvc` with the static C runtime. On other targets it builds a stub that exits with an error, so `cargo build --workspace` works on Linux.

`deployarr-addons` hides the add-on runtime, the bindings for every API level, the package envelope, trust, the operation journal, grants, limits, the store of each add-on, connections, both egress paths, and the job queue. Every module except `lib.rs` is private, so `deployarr-server` never sees a `wasmtime`, WIT, or package type. An add-on author sees one trait with default bodies and the `deployarr-core` types. go-plugin's research asks for default bodies, so a new host function never breaks add-on code at compile time ([HashiCorp go-plugin 1.8.0 reference](research/go-plugin-1.8.0.md#what-changes-in-deployarr) item 9). The SDK and the in-tree add-ons live in this repository and build in CI against the current API, because Jellyfin's plugin template lagged a major release behind its server ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 16).

## Not in the first release

These are planned for later and kept out of the first release on purpose:

- Windows Update during deployment
- BitLocker
- user state migration and in-place upgrades
- multicast
- PXE across routed subnets through a DHCP relay
- IPv6 PXE
- ARM64
- Linux deployment
- vendor driver catalogs, which arrive as an add-on with API level 2 (see [Extension points](#extension-points))
- a desktop program for the server

The [Roadmap](roadmap.md) lists them.
