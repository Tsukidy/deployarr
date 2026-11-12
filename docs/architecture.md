# Architecture

deployarr installs Windows on bare-metal PCs from one central server. A technician types the Windows product key. When lookup cannot find the PC, the technician also types its order number or picks a task sequence from a list. It copies the parts of Microsoft Deployment Toolkit (MDT) that work and fixes the faults listed in [MDT 8456 reference](research/mdt-8456.md) and [MDT 8450 binaries reference](research/mdt-8450-binaries.md). This page explains the design and why each choice was made. For the build order, see [Roadmap](roadmap.md).

## Requirements

These come from the product owner and drive every decision below.

- Zero-touch deployment, Windows first, except for the product key.
- Each PC has a Windows product key on a sticker. A technician types it during the deployment, on the same screen as the order number when lookup asks for one.
- Order lookup in an SQL database, as an optional feature with high priority. The order's OS identifier selects the task sequence. A manual list of task sequences is the fallback.
- deployarr works with no order database. It then deploys from pre-staged records and the manual list.
- deployarr only reads the order database. It never writes to it, not even deployment status.
- A PC is matched in three ways, and all three must work: the order table holds the motherboard serial number, a technician scans or types the order number, or someone pre-stages the PC in the web UI. Pre-staging needs no order database.
- The order database engine is not fixed. Support Microsoft SQL Server, PostgreSQL, and MySQL or MariaDB.
- Task sequences that work like MDT's: ordered groups and steps, conditions, variables, scripts, commands, and reboots.
- Every device has a working driver when the deployment finishes.
- Configurable applications, with a simple UI to manage drivers and software.
- A finished PC ends in one of two states, picked for each deployment. A PC that is ready to use skips OOBE and has a local admin account. A PC that is ready for the customer deploys in audit mode and then runs `sysprep /oobe /shutdown`, so the customer sees OOBE on first boot.
- PCs stay in a workgroup by default. Domain join, Entra ID join, and Autopilot are options.
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
 |  rules engine -> optional order DBs (SQL Server, PostgreSQL, MySQL) |
 |  libraries: OS images, drivers, apps, task sequences, boot images   |
 |  storage: SQLite database + content store directory                 |
 +---------------------------------------------------------------------+
            ^ HTTPS, server certificate pinned by the agent
            |
 target PC: PXE -> WinPE (deployarr-agent is the shell)
            -> new Windows (deployarr-agent runs as a Windows service)
```

There are two programs. `deployarr-server` runs on the deployment server. `deployarr-agent` is one Windows executable that runs on the target PC, first as the WinPE shell and then as a Windows service in the new OS. Everything else is a library crate that these two programs share.

## Deployment flow

1. The PC PXE-boots. The proxyDHCP service reads the client architecture (DHCP option 93) and sends Microsoft-signed boot programs for that firmware type, with DHCP option 252 naming the BCD. See [PXE](#pxe).
2. The boot manager loads WinPE from the server over TFTP. WinPE starts `deployarr-agent` from `winpeshl.ini`.
3. The agent initializes WinPE (`wpeinit`), loads the boot image's network and storage drivers with `drvload`, and connects to the server over HTTPS.
4. The agent gathers facts and sends them to the server. Facts include the SMBIOS data (system, baseboard, and chassis serials, UUID, make, model, and SKU), MAC addresses, firmware type, Secure Boot state, TPM version, memory, processor cores, disks, and the hardware IDs of every device.
5. The server runs the lookup chain (see [Order lookup](#order-lookup)). The result is a task sequence and a set of variables, or a request for the technician to type an order number or pick from the list. If the task sequence activates Windows, the agent then asks for the product key. See [Activation](#activation).
6. The server pins the task sequence version and sends the compiled program and variables to the agent.
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
	PreStaged,                                              // machines table in deployarr's own database
	OrderByIdentity { source: DataSourceId, query: QueryId, key: IdentityKind },  // for example, baseboard serial
	PromptOrderNumber { source: DataSourceId, query: QueryId },                   // the technician scans or types it
	ManualList,                                             // the technician picks a task sequence
}
```

The default order is `PreStaged`, then `OrderByIdentity` on the baseboard serial, then `PromptOrderNumber`, then `ManualList`. An order row's OS identifier maps to a task sequence through an editable table, for example `W11-PRO -> W11-PRO-STD`. Other columns can map to variables, such as the computer name or an application list.

A barcode scanner acts as a keyboard, so the order-number prompt in the WinPE screen accepts scanner input with no extra code.

Data sources are optional. A server with none runs `PreStaged`, then `ManualList`, and opens no database connection besides its own SQLite file. Adding a data source makes `OrderByIdentity` and `PromptOrderNumber` available, and the default order above applies.

An admin can reorder the chain or remove any resolver. When the chain is saved, the server rejects an empty chain and a resolver that names a missing data source or query.

When no resolver returns a task sequence, the agent shows the PC's identity facts on a no-match screen and waits. It does not touch the disk.

### Data sources

The order database is reached only from the server. The agent never holds database credentials. MDT puts them in `CustomSettings.ini` and writes the connection string, password included, to its log.

```rust
#[async_trait]
pub trait DataSource {
	async fn query(&self, query: &NamedQuery, params: &Params) -> Result<Vec<Row>, DataSourceError>;
}
```

- SQL Server uses `tiberius`, with SQL authentication or Windows authentication.
- PostgreSQL and MySQL use `sqlx`. Recent `sqlx` releases dropped SQL Server support, which is why SQL Server has its own driver.
- An admin writes each query with named parameters, such as `SELECT os_code, order_no FROM orders WHERE board_serial = :board_serial`. The server rewrites the names into each engine's placeholder syntax and always binds values. Values are never pasted into the SQL text.
- A lookup error or an empty result moves to the next resolver and appears in the trace. MDT logs the error and continues as if nothing happened.
- The order database is read-only to deployarr. The setup guide requires a database account that can only read. On PostgreSQL and MySQL, the server also runs each named query in a read-only transaction, so a query that tries to write fails even when the account allows it. On SQL Server, deployarr relies on the read-only account alone.

### Wipe safety

A zero-touch server can erase any PC that network-boots by mistake. deployarr erases a disk only after one of these:

- a positive identity match, from a pre-staged record or an order row
- a technician entering an order number
- a technician picking a task sequence

An unknown PC stops at the order-number prompt, the manual list, or the no-match screen, whichever the chain reaches first, and does not touch its disk. After a deployment succeeds, the server stops offering PXE boot to that PC's MAC address and SMBIOS UUID (DHCP option 97), until an admin re-arms it. This stops a PC that boots from the network first from reinstalling in a loop.

## Agent

- The agent is one static Rust executable built for `x86_64-pc-windows-msvc` with the static C runtime.
- It uses Win32 APIs, not WMI, so base WinPE needs no optional components:
  - SMBIOS: `GetSystemFirmwareTable`
  - network adapters: `GetAdaptersAddresses`
  - firmware type: `GetFirmwareType`. The fallback is MDT's `GetFirmwareEnvironmentVariableW` check, and only with `SeSystemEnvironmentPrivilege` enabled first. Without the privilege, newer WinPE versions make that check report BIOS PCs as UEFI, which is the bug the MDT 8456 hotfix fixed.
  - disks and bus type: `IOCTL_STORAGE_QUERY_PROPERTY`. Bus type 7 is USB.
  - device hardware IDs: SetupAPI with `DIGCF_PRESENT | DIGCF_ALLCLASSES`, which is what MDT's `Microsoft.BDD.PnpEnum.exe` does. Unlike PnpEnum, the agent reports an error when enumeration fails, so a failed scan never looks like a PC with no devices.
  - CPU features: the `cpuid` instruction, for the same virtualization, NX, and 64-bit facts that MDT's `Microsoft.BDD.Utility.dll` reports.
  - memory and processor cores: `GetPhysicallyInstalledSystemMemory` and `GetLogicalProcessorInformationEx`.
  - TPM version: `Tbsi_GetDeviceInfo` from `tbs.dll`. ConfigMgr reads the TPM through WMI `Win32_Tpm` instead. Whether base WinPE includes `tbs.dll` is not yet checked, so the agent loads it at run time and reports the TPM as unknown when it is missing.
  - Secure Boot: the `UEFISecureBootEnabled` value under `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State`, which ConfigMgr also reads. When the value is missing, the agent reads the `SecureBoot` UEFI variable with `GetFirmwareEnvironmentVariableW`, with `SeSystemEnvironmentPrivilege` enabled. The fact has three values: on, off, and unknown. A failed read is unknown, never off. ConfigMgr keeps the same third value as `NA`.
- `RunPowerShell` steps in WinPE need the WinPE PowerShell component, which only DISM can add. See [Boot image](#boot-image).
- `RunPowerShell` runs `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <script>` and passes each argument separately. MDT's PowerShell host joins the arguments into one string and runs it as PowerShell code, which loses their quoting.
- A CI check lists the agent's DLL imports and fails if one is missing from base WinPE.
- In WinPE the agent is the shell. It shows a full-screen console screen with the current step, a progress bar, and the order-number, task-sequence, and product-key prompts.
- Ending the WinPE shell reboots the PC, so the agent never exits on a fatal error. It shows the error and waits.
- A setting in the boot image makes F8 open a command prompt, for debugging. The agent does not reboot WinPE while a prompt it opened is still running. MDT's `BDDRun.exe` works the same way.
- The agent keeps the PC awake for the whole run with `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`, as MDT does.
- HTTPS uses `rustls`. The agent trusts only the server CA certificate that is built into the boot image.
- Logs use the CMTrace line format, so admins can read them with CMTrace. The final copy goes to `C:\Windows\Temp\Deployarr` and to the server.

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

- A task sequence that contains an `ActivateWindows` step needs a key. The built-in Windows client template contains one at the start of its full-OS phase. A sequence for a PC that needs no typed key leaves the step out, such as one for an edition with its own key, or for a lab VM that stays unactivated. Installing needs no key. The agent applies the chosen edition's image, so no key has to select the edition, as one does in Windows Setup ([Work with product keys and activation](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/work-with-product-keys-and-activation-auth-phases)). The agent asks for the key in WinPE right after lookup, before the disk changes. When lookup used the order-number prompt, the key prompt follows on the same screen, so the technician types both in one visit.
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

A PC stays in a workgroup by default. Domain join, Entra ID join, and Autopilot are options in the first release. An admin defines join profiles on the server, and the `JoinProfile` variable names the profile a deployment uses. As with `EndState`, a task sequence sets the default, and an order column or a pre-staged record can override it. With no setting, the PC uses the built-in `Workgroup` profile.

```rust
pub enum JoinProfile {
	Workgroup { name: String },   // "WORKGROUP" unless an admin changes it
	Domain { domain: String, ou: Option<String>, account: String, password: SecretRef },
	EntraId { package: SecretRef, made_on: Date },   // bulk enrollment package from Windows Configuration Designer
	Autopilot { upload: AutopilotUpload },   // CsvExport, or Graph through a tenant connection
}
```

- `Workgroup` and `Domain` go into the unattend file, in the specialize pass of the `Microsoft-Windows-UnattendedJoin` component. `JoinWorkgroup` names the workgroup. `JoinDomain`, `Credentials`, and `MachineObjectOU` name the domain, the join account, and the full LDAP path of the OU ([UnattendedJoin](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin), [Identification](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin-identification), [MachineObjectOU](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-unattendedjoin-identification-machineobjectou)). The PC must reach a domain controller during the specialize pass.
- The join account's password is a secret. The server stores it encrypted, and the agent deletes the cached answer files before the finish action (see [End state](#end-state)).
- `EntraId` uses bulk enrollment. An admin makes the provisioning package with Windows Configuration Designer and uploads it. The package joins the PC to the Entra tenant and enrolls it in Intune ([Bulk enrollment](https://learn.microsoft.com/en-us/intune/intune-service/enrollment/windows-bulk-enroll)). It carries a token for a `package_{GUID}` account in Entra ID (same page), so the server stores it encrypted as a secret.
- In WinPE, the agent adds the package to the applied image with `DISM /Image=W:\ /Add-ProvisioningPackage`, which works on an applied image ([DISM provisioning package options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-provisioning-package-command-line-options)). Windows applies a package that is embedded in the image during the OOBE pass ([How provisioning works](https://learn.microsoft.com/en-us/windows/configuration/provisioning-packages/provisioning-how-it-works)). The M8 lab checks that a `ReadyToUse` PC, which skips the OOBE screens, applies it too. If it does not, the agent runs `Install-ProvisioningPackage` in the full OS instead (inferred).
- The token in a bulk enrollment package is valid for 90 days (Bulk enrollment page). The admin enters the date the package was made, and the UI warns before the 90 days pass.
- `Autopilot` only customizes OOBE ([Autopilot FAQ](https://learn.microsoft.com/en-us/autopilot/faq)), so it needs `EndState` set to `CustomerOobe`. The server rejects any other pair before the disk changes.
- A Home image can use only `Workgroup`. Microsoft lists "The ability to join a domain or Microsoft Entra ID" among the features that Pro adds to Home ([Upgrade Windows Home to Windows Pro](https://support.microsoft.com/en-us/windows/upgrade-windows-home-to-windows-pro-ef34d520-e73f-3198-c525-d1a218cc2818)). Entra join works on Windows 11 "except for Home editions", and bulk enrollment is one way to do it ([Microsoft Entra joined devices](https://learn.microsoft.com/en-us/entra/identity/devices/concept-directory-join)). Autopilot's list of supported Windows 11 editions has no Home edition ([Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)).
- The server rejects a `Domain`, `EntraId`, or `Autopilot` profile for a Home image when a task sequence or a pre-staged record is saved, and again after lookup, before the disk changes. An order column can set the profile, so the second check is the one that catches it. A sequence counts as Home when any of its `ApplyOsImage` steps names a Home image, because conditions pick the step only at run time. The error names the image's edition ID and the profile.
- A step can also set `JoinProfile`, with `SetVariable` or `var set`. The agent checks the new value when the step sets it, and fails the step if the image is Home. A step that runs before the disk steps keeps the disk unchanged. Without these checks, a domain join on Home would fail in the specialize pass, after the disk is wiped (inferred).
- A bulk enrollment package can also carry a product key that upgrades the edition (Bulk enrollment page). Microsoft's table marks Home to Pro by provisioning package as not supported, and Home to Education or Pro Education as supported with a reboot ([Windows Edition Upgrade](https://learn.microsoft.com/en-us/windows/deployment/upgrade/windows-edition-upgrades)). Whether the Entra join follows such an upgrade is not checked, so the rule rejects `EntraId` for Home whatever the package holds.
- Capturing the hardware hash requires a booted Windows ([Manually register devices](https://learn.microsoft.com/en-us/autopilot/add-devices)), so the agent collects the hash in audit mode, before `sysprep`, and sends it to the server. The M8 lab picks the collection method.
- With `CsvExport`, an admin downloads the hashes as a CSV file and imports it into Intune, which takes up to 500 rows per file (same page). With `Graph`, the server registers each hash through `importedWindowsAutopilotDeviceIdentities`, which needs the `DeviceManagementServiceConfig.ReadWrite.All` permission ([Create importedWindowsAutopilotDeviceIdentity](https://learn.microsoft.com/en-us/graph/api/intune-enrollment-importedwindowsautopilotdeviceidentity-create?view=graph-rest-1.0)). The tenant connection's credentials are secrets.
- Microsoft calls manual hash registration "primarily for testing and evaluation scenarios" (same page). A builder that sells into many customer tenants may need partner registration instead. That path is not yet checked.

## Readiness check

deployarr applies `install.wim` with DISM and never runs `setup.exe`, so Windows Setup never checks the hardware. The `CheckReadiness` action does that check instead. ConfigMgr has the same step, but its defaults are below the Windows 11 minimums and it has no Secure Boot check ([ConfigMgr 2509 reference](research/configmgr-2509.md#check-readiness)).

- The step runs in WinPE before any disk step, so a PC that fails keeps its disk as it was.
- Its defaults are Microsoft's Windows 11 minimums: 4 GB of memory, a 64 GB target disk, two or more processor cores, UEFI firmware, and TPM 2.0.
- Each check either fails the step or logs a warning. The Secure Boot check warns by default, because Windows 11 requires firmware that can run Secure Boot, not Secure Boot turned on.
- A task sequence can change a minimum, switch a check between fail and warn, or turn it off. A BIOS PC needs the UEFI check off, and a PC without TPM 2.0 needs the TPM check off.
- A fact that the agent cannot read fails its check, and the log says that the value was unknown.
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
- deployarr answers a DISCOVER or a REQUEST that carries option 93 (client architecture) and option 97 (machine GUID), as ConfigMgr does. It ignores INFORM.
- The OFFER sets `siaddr` to deployarr's address and carries options 53, 54, 97 (echoed), 60 (`PXEClient`), and 82 (echoed when present). It carries no boot file and no option 43. Only the ACK to a REQUEST carries the boot file, in the BOOTP `file` field, with option 250 or option 252.
- Replies go to the request's source address and port. deployarr broadcasts only when the source is `0.0.0.0`, and it answers a relayed request by unicast to the relay. On routed networks, a DHCP relay (IP helper) forwards PXE clients to deployarr.
- The first release supports PXE on one subnet, and the lab tests only that. The relay handling above stays in the design, so routed subnets can come later without a redesign. Before they do, the design must choose where a relayed reply goes. deployarr sends it to the request's source address, as ConfigMgr does. RFC 2131 section 4.1 sends it to port 67 at the relay address in `giaddr`. The two differ when a relay sends from another interface (inferred).
- A PC with no order, or one whose deployment succeeded, gets no reply on UEFI, so the firmware moves on to the next boot device. On BIOS it gets `abortpxe.com`. ConfigMgr does the same.
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
- The agent API covers sessions, the compiled plan, content manifests, events, logs, and the post-install device report.
- The admin API serves the web UI. Live progress reaches the UI over server-sent events.
- Each deployment gets its own ID from the server. Succeeded and Failed are final states that later events cannot change. MDT's monitoring service keeps one record per computer, sets a finished record back to Running on its next event, and keeps no history.
- The agent queues events and sends them in the background with retries, plus a heartbeat on a timer. A slow or unreachable server never delays a step. MDT sends each event as a blocking call with a 60-second timeout, and its PowerShell steps send no events at all.
- Installation creates a private CA and a server certificate. Each boot image carries the CA certificate.
- Secrets are encrypted at rest with a server key. This covers data source passwords, per-PC local admin passwords, product keys, domain join passwords, bulk enrollment packages, and Autopilot tenant credentials. Each deployed PC gets its own random local admin password, which an admin can read in the UI. MDT uses one shared password.
- Staff sign in to the admin web UI with local deployarr accounts. The server stores each password as an argon2id hash. The first release has no other sign-in. Active Directory and OIDC sign-in can come later.

## Web UI

The UI is written in Leptos and compiled to WebAssembly, so it uses the same `deployarr-core` types as the server. The editor validates a task sequence with the same code the server runs when you save.

The web UI is the only admin interface in the first release. After it, a desktop program built with Tauri shows the same UI in its own window on the server. Tauri draws the window with the operating system's web view, so the program bundles no browser and stays in the Rust workspace.

The screens are these:

- Deployments: live progress, step timeline, logs, variable trace, and missing drivers.
- Task sequences: a tree editor with a step palette, a typed form for each action, a condition builder, version history, and templates.
- OS images: import an ISO or WIM, then choose editions.
- Drivers: import packages, browse by hardware ID, manage profiles, and view missing-driver reports.
- Applications: upload files, set the command, the detection rule, and dependencies, and test detection.
- Order lookup: the lookup chain, plus the optional data source connections with a test button, named queries, and the OS-code map. A test box takes an order number or a serial number and shows the resolved task sequence and its trace.
- Machines: pre-staged PCs, with CSV import.
- Join profiles: workgroup and domain settings, Entra ID packages, Autopilot tenant connections, and the Autopilot hash export.
- Boot images and PXE: build boot images, set the active image, and choose the PXE interfaces and the port 67 setting.

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
xtask/               boot image builds, lab automation, packaging
```

`deployarr-core` is `no_std` with `alloc`, so library code that names `std` does not compile. That rules out files, sockets, processes, environment variables, threads, and the clock. It also forbids unsafe code, so it cannot call the operating system through an `extern` block. Its unit tests may use `std`, which the crate root grants them with `cfg_attr(not(test), no_std)`. Every dependency must build without `std`, so the manifest turns off default features that pull it in. CI builds the crate for `x86_64-unknown-none`, a target that ships no `std`. That build fails when library code or a dependency needs `std`, and when the crate root loses its `no_std` attribute. The `wasm32-unknown-unknown` build alone cannot catch I/O, because `std` on that target compiles file and socket calls and fails them only at run time.

`deployarr-inf` and `deployarr-smbios` are separate crates because they are pure parsers tested against real files on Linux. The agent targets `x86_64-pc-windows-msvc` with the static C runtime. On other targets it builds a stub that exits with an error, so `cargo build --workspace` works on Linux.

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
- vendor driver catalogs
- a desktop program for the server

The [Roadmap](roadmap.md) lists them.
