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
4. The agent gathers facts and sends them to the server. Facts include the SMBIOS data (system, baseboard, and chassis serials, UUID, make, model, SKU, and chassis type), the form factor flags (laptop, desktop, and server), the virtual machine (VM) flag, the hypervisor vendor, MAC addresses, firmware type, Secure Boot state, TPM version, memory, processor cores, disks, and the hardware IDs and compatible IDs of every device. See [Chassis and VM facts](#chassis-and-vm-facts).
5. The server runs the lookup chain (see [Order lookup](#order-lookup)). The result is a task sequence and a set of variables, or a prompt for the technician, such as an add-on's order-number prompt or the manual list. If the task sequence activates Windows, the agent then asks for the product key. See [Activation](#activation).
6. The server pins the task sequence version. It asks each add-on to expand its steps into built-in steps, compiles the result, and sends the compiled program and variables to the agent. See [Task sequences stay loadable](#task-sequences-stay-loadable).
7. The agent runs the WinPE steps. A typical run checks readiness, partitions the disk, downloads and applies the OS image, injects drivers, writes the unattend file to `\Windows\Panther\unattend.xml`, runs `bcdboot`, and installs itself as a service in the offline image.
8. The PC reboots into Windows Setup. When Setup finishes, or reaches audit mode, the agent service resumes the sequence at the next step.
9. The agent installs applications and Windows updates, runs scripts, reboots as steps request, and checks that every device has a driver.
10. The agent turns off the built-in Administrator account, deletes every answer file that Windows Setup can find on the PC's disk, and reports success. It then uploads its logs, removes itself, and runs the finish action. A PC that is ready to use restarts or shuts down. A PC for the customer ends any running `sysprep.exe` and runs `sysprep /oobe /shutdown /quiet`. See [End state](#end-state).
11. The server marks the deployment complete. Later PXE boots of that PC fall through to the local disk.

## Core data shapes

The task sequence model lives in `deployarr-core`, a `no_std` crate with no I/O. See [Crate layout](#crate-layout). The server, the agent, and the web UI all use it. These sketches show the shape, not final code.

```rust
pub struct TaskSequence<A> {   // A is the action type that a step holds
	pub id: TaskSequenceId,   // upper-case, like "W11-PRO-STD"
	pub version: u32,         // every save creates a new version
	pub name: String,
	pub default_vars: DefaultVars,   // no two entries name the same variable
	pub root: Group<A>,
}

pub type AuthoredSequence = TaskSequence<Action>;         // what an admin edits and the server stores
pub type LoweredSequence = TaskSequence<BuiltinAction>;   // what the compiler and the agent see

pub enum Node<A> {
	Group(Group<A>),
	Step(Step<A>),
}

pub struct NodeMeta {
	pub id: NodeId,           // stable UUID. Survives a rename or a move
	pub name: String,
	pub enabled: bool,
	pub continue_on_error: bool,
	pub condition: Option<Condition>,
}

pub struct Group<A> {
	pub meta: NodeMeta,
	pub children: Vec<Node<A>>,
}

pub struct Step<A> {
	pub meta: NodeMeta,
	pub run_in: RunIn,        // WinPe, FullOs, or Either. Checked against the action at save
	pub action: A,
}

pub enum Action {   // what a stored step holds. See Task sequences stay loadable
	Builtin(BuiltinAction),   // written in JSON exactly as a built-in action
	AddOn(AddOnStep),
	ApplyJoinProfile { stage: JoinStage },   // OfflineImage or FullOs
}

pub enum BuiltinAction {   // everything the agent runs
	RunCommand { command: Template, working_dir: Option<Template>, exit: ExitCodes, limits: StepLimits, files: StepFiles },
	RunPowerShell { script: ContentRef, args: Vec<Template>, exit: ExitCodes, limits: StepLimits, files: StepFiles },
	SetVariable { name: SettableVarName, value: Template },   // the name cannot start with `_`
	Restart { into: RestartTarget },
	PartitionDisk { layout: DiskLayoutId, disk: DiskSelector },
	ApplyOsImage { image: OsImageRef },
	ApplyDrivers { selection: DriverSelection },
	ApplyUnattend { template: UnattendTemplateId },
	InstallApplications { apps: AppSelection },   // One(AppId) or FromList(VarName)
	InstallUpdates {              // full OS only. Windows Update or WSUS. See Windows Update
		source: UpdateSource,
		missing: MissingUpdatePolicy,
		max_passes: NonZeroU8,    // default 7
		exclude: Vec<UpdateExclusion>,   // default empty
		timeouts: UpdateTimeouts,
	},
	VerifyDrivers { policy: MissingDriverPolicy },
	EnableBitLocker { method: EncryptionMethod, full_disk: bool, wait_secs: Option<NonZeroU32>, tpm_failure: TpmFailure },   // full OS only
	SuspendBitLocker { restarts: SuspendRestarts },   // full OS only. Never decrypts
	CheckReadiness { checks: ReadinessChecks },   // Windows 11 minimums by default
	ActivateWindows { online: bool },   // full OS only. Installs the product key typed in WinPE
	ServerCall { send: Vec<VarName>, limits: StepLimits },   // add-on steps expand to it
	AddProvisioningPackage { package: SecretRef, limits: StepLimits },   // the one action that takes a secret
}

pub struct StepFile {   // a file the agent writes to the step's private folder
	pub name: FileName,
	pub content: ContentRef,   // content only, never a secret
}

pub struct ExitCodes {          // read with success() and reboot(). No code is in both lists
	success: NonEmpty<i32>,   // Default [0]
	reboot: Vec<i32>,         // success, then a reboot. Default [3010, 1641]
}

pub struct StepLimits {
	pub max_runtime_secs: Option<NonZeroU32>,   // whole seconds. Default none
	pub rerun_if_interrupted: bool,   // default false
}

pub enum Condition {
	All(NonEmpty<Condition>),
	Any(NonEmpty<Condition>),
	Not(Box<Condition>),
	Var { name: VarName, test: Test<NonEmptyTemplate> },
	Registry { key: Template, value: Option<String>, test: Test<RegData> },   // REG_DWORD, REG_QWORD, REG_SZ, or REG_EXPAND_SZ
	File { path: Template, version: Option<Comparison<FileVersion>>, modified: Option<Comparison<Timestamp>> },
	Folder { path: Template, modified: Option<Comparison<Timestamp>> },
	InstalledMsi(MsiProduct),   // ProductCode or UpgradeCode. Full OS only
	Wmi { namespace: String, query: Template, ignore_failure: bool },   // full OS only
}

// An operator with its operand. Exists and NotExists take none, and every other operator takes one
pub enum Test<T> { Exists, NotExists, Equals(T), NotEquals(T), Less(T), LessEqual(T), Greater(T), GreaterEqual(T), Like(T) }

pub enum CompareOp { Equals, NotEquals, Less, LessEqual, Greater, GreaterEqual }   // for file versions and timestamps
```

Each MDT step type that does real work becomes a typed `BuiltinAction` variant with typed parameters. `RunCommand` and `RunPowerShell` cover everything else. MDT passes every step's settings as untyped variables and command lines, so a typo becomes a runtime failure. With typed actions, the editor and the server reject a bad step when you save it.

The condition types match what MDT's engine evaluates. MDT also has make, model, location, and computer conditions, which compare by substring, so a gateway of `10.0.0.1` also matches `10.0.0.10`. deployarr expresses those as `Var` conditions on gathered facts, which compare exactly.

Add-ons extend this model in three ways. A stored step can hold a step from an add-on. The `ApplyJoinProfile` join slot marks where the deployment's join profile adds its steps. The agent's actions gain `ServerCall`, `AddProvisioningPackage`, and step files. The task sequence types take the action type as a parameter, so the compiler accepts only built-in actions. [Task sequences stay loadable](#task-sequences-stay-loadable) shows these shapes.

### Parameter types

```rust
pub struct NodeId(pub Uuid);
pub struct Template(String);              // the text as written. Substitution happens at run time
pub struct NonEmptyTemplate(Template);    // a Template that is not empty
pub struct SettableVarName(VarName);      // a VarName that does not start with `_`
pub struct VarAssignment { pub name: SettableVarName, pub value: Template }
pub struct DefaultVars(Vec<VarAssignment>);   // read with as_slice(). No two names are equal in any case
pub struct StepFiles(Vec<StepFile>);      // read with as_slice(). No two names are equal in any case
pub struct FileName(String);              // one Windows file name, never a path
pub struct SecretRef(String);             // names a secret that the server holds
pub struct NonEmpty<T>(Vec<T>);           // a list with at least one item

pub enum RunIn { WinPe, FullOs, Either }
pub enum RestartTarget { WinPe, InstalledOs }
pub enum DiskSelector { FirstInternal }   // the default
pub enum DriverSelection { Auto, Profile, ProfileAll }
pub enum MissingDriverPolicy { Fail, Warn, Ignore }
pub enum MissingUpdatePolicy { Fail, Warn, Ignore }      // default Warn
pub enum UpdateSource { WindowsUpdate, Wsus(WsusUrl) }   // default WindowsUpdate
pub enum UpdateExclusion { Kb(NonZeroU32), UpdateId(Uuid) }
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

pub enum EncryptionMethod { OsDefault, Aes128, Aes256, XtsAes128, XtsAes256 }   // default OsDefault
pub enum TpmFailure { Fail, Skip }   // default Fail
pub struct SuspendRestarts(u8);      // 1 to 15. Default 1

pub struct UpdateTimeouts {          // whole seconds, for one phase of one pass
	pub search_secs: NonZeroU32,     // default 3600
	pub download_secs: NonZeroU32,   // default 7200
	pub install_secs: NonZeroU32,    // default 7200
}

pub struct Comparison<T> { pub op: CompareOp, pub value: T }
pub struct FileVersion { pub major: u16, pub minor: u16, pub build: u16, pub revision: u16 }
pub struct Timestamp(pub OffsetDateTime);
pub enum MsiProduct { ProductCode(Uuid), UpgradeCode(Uuid) }
pub enum RegData { String(String), Integer(u64) }   // compared by the `Var` rules
```

- `NodeId` wraps a UUID. The core crate never generates a random ID, so it leaves out the `uuid` crate's `v4` feature. On `wasm32-unknown-unknown`, `uuid` stops a build that has `v4` unless the build also turns on one of its randomness features: `js`, `rng-getrandom`, or `rng-rand`. `lower` derives the IDs of the nodes it adds with the `v5` feature, which needs no randomness. See [Task sequences stay loadable](#task-sequences-stay-loadable).
- `TaskSequenceId` holds one or more of `A` to `Z`, digits, `_`, or `-`.
- `VarName` holds 1 to 256 ASCII letters, digits, `_`, or `-`. Two names that differ only in case are equal. The name keeps the spelling its author wrote. The length, the character classes, and the case rule are MDT's. The ASCII limit is deployarr's choice. See [Variables](#variables).
- `SettableVarName` is a `VarName` that does not start with `_`. `SetVariable` and the entries of `default_vars` take it, so a sequence cannot set a name that belongs to the agent. Conditions and `FromList` take a `VarName`, so they can still read one.
- `StepFiles` keeps its list private, and `StepFiles::new` rejects two files whose names are equal in any case, because Windows does not assume that names differ by case alone ([Naming Files, Paths, and Namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)). The JSON is a plain array of files.
- `DefaultVars` keeps its list private, and `DefaultVars::new` rejects two entries whose names are equal in any case. Code that builds a sequence, such as an importer or the editor, therefore cannot write a `default_vars` list that the parser would reject. The JSON is a plain array of entries.
- `NonEmpty<T>` holds at least one item. `All`, `Any`, and the success list of `ExitCodes` use it, so none of them can be empty.
- `DiskLayoutId`, `OsImageRef`, `UnattendTemplateId`, `ContentRef`, `AppId`, and `SecretRef` are strings that reject an empty value. A `SecretRef` names a secret that the server holds, never the secret's value. It is a non-empty opaque string for now. [#189](https://github.com/Tsukidy/deployarr/issues/189) decides its shape before API level 1 freezes.
- A step file's `FileName` holds 1 to 255 ASCII letters, digits, `.`, `_`, or `-`, and does not end with `.`. The part before its first `.` is not a name that Windows reserves for a device, such as `NUL`, in any case. Windows treats `NUL.txt` as `NUL` and advises against a name that ends with a period ([Naming Files, Paths, and Namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)). A step file's name is therefore one file in the step folder and never a path.
- `DiskSelector` has one rule so far, the first internal, non-removable disk.
- `MemoryCheck`, `DiskCheck`, and `ProcessorCheck` each hold a minimum and a `mode`, so the JSON for one is an object such as `{"min_mib": 4096, "mode": "fail"}`. The `uefi`, `tpm_2_0`, and `secure_boot` checks have no minimum, so each is a bare `CheckMode`, such as `"warn"`.
- `min_mib` is in mebibytes of 2^20 bytes, so the default 4096 is 4 GiB. `min_gb` is in decimal gigabytes of 10^9 bytes, the unit that disks are sold in, because a disk sold as 64 GB holds less than 64 GiB. `min_cores` counts processor cores.
- `max_runtime_secs` is a whole number of seconds and is never zero, because a zero limit fails every run. An earlier sketch had `max_runtime: Option<Duration>`. serde writes a `Duration` as an object with `secs` and `nanos`. deployarr limits a step in whole seconds, so the limit is one JSON integer and the field name carries the unit.
- `wait_secs` in `EnableBitLocker` follows the same rule. `SuspendRestarts` keeps its count private, and `SuspendRestarts::new` rejects a count outside 1 to 15. Its JSON is a bare integer. [BitLocker](#bitlocker) gives the meaning of each setting.
- `UpdateTimeouts` follows the same rules as `max_runtime_secs`. Each limit is whole seconds and never zero, and `max_passes` is never zero either. See [Windows Update](#windows-update).
- `WsusUrl` is an `http` or `https` URL, such as `https://wsus.example.com:8531`. The parser rejects an empty value, any other scheme, and a URL with user information such as `user:pass@`, so a credential never reaches the registry or a log. `UpdateExclusion::Kb` holds a KB article number without the `KB` prefix, and `UpdateId` holds an update ID, written as a GUID without braces.
- `ExitCodes` holds `i32` values. Windows returns a process's exit code as a 32-bit `DWORD` ([GetExitCodeProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess)), and an `i32` holds those 32 bits. An exit code at or above `0x80000000` is written as its signed value, so `0x80070005` is `-2147024891`. The unsigned spelling fails to parse. An HRESULT is a `LONG`, a 32-bit signed integer ([Windows Data Types](https://learn.microsoft.com/en-us/windows/win32/winprog/windows-data-types)), so the signed value is also the HRESULT's own value.
- `ExitCodes` keeps its fields private, and `ExitCodes::new` rejects a code that is in both `success` and `reboot`. A code means success, or success and then a reboot, never both.
- `Test<T>` carries an operator together with its operand. `Exists` and `NotExists` take no operand, and every other operator takes one, so a `Var` or `Registry` condition can neither lack the operand it needs nor carry one that it ignores. A `Var` condition compares against a `NonEmptyTemplate`, because `NotExists` is the test for an empty variable.
- `Comparison<T>` takes only the six operators of `CompareOp`, so a file version or timestamp comparison cannot use `Like`, `Exists`, or `NotExists`.
- `Comparison<T>` has named `op` and `value` fields. An earlier sketch had the tuple `(CompareOp, T)`. serde writes a tuple as a JSON array, so a parse error would point at `version[1]`. With named fields it points at `version.value`.
- `FileVersion` is written as `a.b.c.d` and orders by its four fields.
- `Timestamp` is written in RFC 3339, such as `2026-01-31T08:00:00Z`. An earlier sketch had `DateTime` and named no format. RFC 3339 requires a UTC offset, so every value names one instant.
- The core crate parses a `Timestamp` with the `time` crate and writes the text itself, because `time`'s formatter needs `std`. It writes the same text that `time` writes. A fraction of a second loses its trailing zeros, and a zero offset is `Z`.
- `MsiProduct` is written as a GUID without braces. `RegData` is written as a bare JSON string or integer.
- A `Registry` condition reads `REG_DWORD`, `REG_QWORD`, `REG_SZ`, and `REG_EXPAND_SZ` values. `REG_DWORD` is a 32-bit number and `REG_QWORD` a 64-bit number ([Registry value types](https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-value-types)), and Windows declares `DWORD` and `QWORD` as unsigned ([Windows Data Types](https://learn.microsoft.com/en-us/windows/win32/winprog/windows-data-types)). `RegData::Integer` therefore holds a `u64`, which holds every value of both types, and a negative number fails to parse.

### JSON format

A task sequence is stored and sent as one JSON document. `TaskSequence::from_json` parses it into the types above.

- The top-level `schema_version` is 1. The parser rejects any other version.
- Field names and variant names are in snake case, such as `continue_on_error` and `win_pe`. `RunPowerShell` is written as `run_powershell`, `EnableBitLocker` as `enable_bitlocker`, and `SuspendBitLocker` as `suspend_bitlocker`.
- A variant with data is an object with one key, the variant's name, such as `{"run_command": {"command": "ipconfig"}}`. A variant without data is a string, such as `"win_pe"`. Each node in `children` is `{"group": ...}` or `{"step": ...}`. This is serde's externally tagged form. The other choice puts a `"type"` key inside the object. serde buffers such an object before it reads the variant, so the path of a parse error stops at `action` and does not reach the field that failed.
- A field that the document omits takes its default when it has one above: `exit`, `limits`, `disk`, `checks`, every field of `install_updates`, `method`, `full_disk`, `tpm_failure`, `restarts`, and each field inside them. An omitted `files` list of a `RunCommand` or `RunPowerShell` action is empty (design decision), so a sequence written before step files existed still parses, and a command with no file needs no list. An omitted `Option` field is none. Every other field is required.
- An unknown field is an error, so a misspelled field name never passes as a default.
- The serializer writes every field, defaults included. A stored sequence then keeps its meaning if a default changes later.
- A parse error gives the path of the field that failed, such as `root.children[0].step.action.run_command.exit.success[1]`, and the reason.
- A value that is valid but not in canonical form still parses, and the serializer writes it back in canonical form. A GUID in upper case or in braces is written in lower case without braces. A file version part with leading zeros, such as `010`, is written as `10`. A timestamp with a lower-case `t` or `z`, a space in place of `T`, or a `+00:00` offset is written with `T` and `Z`. Any other offset is kept. A `Var` or `Registry` test of `{"exists": null}` is written as `"exists"`, and `{"not_exists": null}` as `"not_exists"`.
- A `Var` or `Registry` condition writes its operator and operand under `test`, by the variant rule above. `"test": "exists"` takes no operand, and `"test": {"greater_equal": 26100}` takes one. A file version or timestamp comparison keeps the `op` and `value` fields.
- The parser also rejects the states below. Each error path names the field that holds the state:
  - An empty success list, at `exit.success`. MDT's engine has no default, so an empty `successCodeList` fails every exit code, including 0 ([MDT 8450 binaries reference](research/mdt-8450-binaries.md#instruction-table-and-failure-handling)).
  - A code in both `success` and `reboot`, at the code in the list that comes second, such as `exit.reboot[0]`. An omitted list takes its default first. `{"success": [0, 3010]}` with no `reboot` therefore fails at `exit`, because the default `reboot` holds 3010.
  - An empty `All` or `Any`, at `all` or `any`. MDT treats an empty OR as true, which is rarely what the author meant ([MDT 8450 binaries reference](research/mdt-8450-binaries.md#conditions)).
  - An operand on `exists` or `not_exists`, a missing operand on any other operator, or an empty `Var` operand, at the operator, such as `test.exists`.
  - `Like`, `Exists`, or `NotExists` in a file version or timestamp comparison, at `op`.
  - A negative registry integer, at the operator that holds it.
  - A `SuspendBitLocker` count outside 1 to 15, at `restarts`.
  - A `SetVariable` step or a `default_vars` entry that names a variable that starts with `_`, at its `name`.
  - Two `default_vars` entries that name the same variable, in any case, at the second entry's name, such as `default_vars[1].name`.
  - Two step files with the same name, in any case, at the second file's name, such as `files[1].name`.
- The compiler rejects a `run_in` that the action cannot honor when the sequence is saved, so that the error names the node. See [Compile the tree into a flat program](#compile-the-tree-into-a-flat-program).

`crates/deployarr-core/tests/data/windows-11-pro.json` is a sample that uses every condition, and every action that `deployarr-core` implements but the ones that come from add-ons: the add-on step, the join slot, `ServerCall`, and `AddProvisioningPackage`.

## Task sequence engine

### Compile the tree into a flat program

The server compiles a task sequence into a flat list of instructions before it sends it to the agent:

```rust
pub fn compile(sequence: &LoweredSequence) -> Result<Program, CompileErrors>;

pub struct Program {           // only compile builds one
	source: (TaskSequenceId, u32),
	instrs: Vec<Instr>,
}

pub struct Pc(pub usize);      // an index into instrs. instrs.len() is the end of the program

pub enum Instr {
	// If the condition is false, jump to `skip_to`, past the group's last child.
	Enter { node: NodeId, condition: Option<Condition>, skip_to: Pc },
	// If the condition is false, skip the step.
	Exec { node: NodeId, condition: Option<Condition>, run_in: RunIn, action: BuiltinAction, on_error: OnError },
}

pub enum OnError {
	Continue,        // the step has continue_on_error
	JumpTo(Pc),      // the end of the nearest enclosing group with continue_on_error
	FailDeployment,
}
```

The compiler drops disabled nodes and works out `on_error` for each step, so a failure needs no group stack at run time. MDT's engine keeps disabled entries in its table and tracks failures with a stack of open groups, which it must save and rebuild on every resume (`_SMSTSInstructionStackString`). Working out `on_error` ahead of time removes that state.

A group, the root included, compiles to an `Enter` followed by its children. Each `Exec` carries the step's condition, `run_in`, and action, so the agent runs the program without the tree. An earlier sketch gave `Exec` only the node ID and `on_error`. Every jump lands after the instruction that holds it and no later than the end of the program, so a run moves only forward.

Like MDT, deployarr checks a group's condition only when it reaches the group, and resumes from a saved instruction pointer. A variable that a step sets in WinPE can therefore enable a group that runs after a reboot. That is how MDT's phase groups work.

The compiler also rejects these mistakes when you save, which MDT finds only at run time or never:

- A step whose `run_in` its action does not allow, by the table below, such as `ApplyOsImage` in the full OS. MDT fails such a step with error 50 when the engine reaches it.
- A step with `run_in` `WinPe` whose condition holds an `InstalledMsi` or `Wmi` condition at any depth. Both are full OS only. See [Core data shapes](#core-data-shapes).
- A `Restart` step with `run_in` `WinPe` that comes before the first `PartitionDisk` step in program order. Until that step, the state lives on `X:`, and a reboot loses it, so the deployment starts over from the first step. See [State on disk and resume](#state-on-disk-and-resume). A sequence with no `PartitionDisk` step therefore fails for every such `Restart`. A `Restart` with `run_in` `Either` passes. The agent fails it at run time if the state is still on `X:`.
- A `Restart` step with `into` `WinPe` that comes after an `EnableBitLocker` step in program order. After the partition step, the state lives on the Windows volume. Once BitLocker protects that volume, the agent in WinPE cannot read the state there (inferred). See [BitLocker](#bitlocker).
- Two nodes in one tree that share a `NodeId`. `Instr` and the errors identify a node by its ID, so an ID must be unique.

| Action | Allowed `run_in` |
| --- | --- |
| `PartitionDisk`, `ApplyOsImage`, `ApplyDrivers`, `ApplyUnattend`, `CheckReadiness` | `WinPe` |
| `InstallApplications`, `InstallUpdates`, `VerifyDrivers`, `ActivateWindows` | `FullOs` |
| `RunCommand`, `RunPowerShell`, `SetVariable`, `Restart`, `ServerCall`, `AddProvisioningPackage` | `WinPe`, `FullOs`, or `Either` |
| `EnableBitLocker`, `SuspendBitLocker` | `FullOs` |

- `BuiltinAction::allowed_run_in` holds the table as one exhaustive match, so a new action does not compile until someone decides where it runs.
- `compile` returns every error in document order, not only the first.
- Each error names the node's ID, its path in the tree, such as `root.children[1].group.children[0]`, and the rule. An ID that two nodes share cannot locate either node, so the duplicate error also names the path of the first node with that ID.
- A disabled node is still part of the sequence that you save, so the `run_in` rule, the WinPE condition rule, and the duplicate ID rule check it. The two `Restart` rules read program order, so they skip disabled nodes (design decision).

The parser rejects other mistakes before the compiler runs, such as an empty success list or an empty `All` or `Any`. See [JSON format](#json-format).

### Conditions

- The agent evaluates a condition when it reaches the group or step. A false group condition skips the group. A false step condition skips the step and leaves `last` unchanged, as MDT does.
- The agent expands `Template` fields in a condition when it evaluates it, not when the server compiles the sequence.
- Evaluation returns `Result<bool, ConditionError>`. An error is not false. It fails the step and goes through the step's `on_error`, which is MDT's rule.
- `Var` compares as integers when both sides are integers, and otherwise compares strings without regard to case, like MDT. `Like` matches `*` and `?` wildcards. `Exists` means the variable is set and not empty. A `Registry` comparison applies the `Var` rules to the text of both sides, with a number as its decimal text.
- A comparison or `Like` test on a list value is an error, the same way a list in a `Template` fails the step (see [Variables](#variables)). `Exists` and `NotExists` work on a list, and a list with at least one item exists. A test on list items needs its own operator, which a later ticket can add (owner decision on [#177](https://github.com/Tsukidy/deployarr/issues/177)).
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
	pub progress: Option<StepProgress>,   // an action's own progress across its reruns
}
```

- The agent writes the state file to a temporary path, flushes it, and renames it over the old file. A crash leaves either the old state or the new state, never a partial file. MDT's engine rewrites its state file in place.
- If a save fails, the run stops before the next step or reboot. MDT's engine logs a warning and continues, so a later power loss resumes from older state.
- Before the disk is partitioned, the state lives on `X:`, the WinPE RAM disk, and a reboot loses it. That is safe, because nothing on the disk has changed yet, and the deployment starts over. MDT's engine cannot save at all until a fixed volume exists. After the partition step, the agent moves the state to `W:\Deployarr\state` on the new Windows volume.
- The state file holds the deployment ID, and the agent looks for it only at its fixed path. MDT's engine resumes from whichever `TSEnv.dat` on any fixed drive has the newest access time.
- A named mutex keeps a second agent from running the same deployment. MDT's `TsmBootstrap.exe` does the same with `Global\TSMBootStrapMutex`.
- The agent sets `in_flight` before it starts a step and clears it when it records the result. If the agent starts and finds `in_flight` set, the step was interrupted by a crash or a power loss. Typed actions are written to converge when they run again. For example, `PartitionDisk` checks the current layout first. `RunCommand` and `RunPowerShell` are not assumed to be safe to repeat, so they fail unless the step sets `rerun_if_interrupted`. MDT saves nothing before a step, so an interrupted step silently runs again. A sequence ported from MDT may need `rerun_if_interrupted` on some steps.
- A step can return `Reboot { rerun: false }` to reboot and continue with the next step, or `Reboot { rerun: true }` to run the same step again. These map to MDT's `SMSTSRebootRequested` and `SMSTSRetryRequested`.
- A typed action can keep its own progress in `progress`, such as the pass count of `InstallUpdates`. The action saves it during the step, with the same write, flush, and rename as any save. `progress` outlives `in_flight`. A `Reboot { rerun: true }` result clears `in_flight`, so the run after that reboot is not an interrupted run, but it keeps `progress`. A crash keeps both. The agent clears `progress` when it records the step's final result, success or failure, so no other step reads it. See [Reboots and resume](#reboots-and-resume).
- The exit codes in `ExitCodes.reboot`, 3010 and 1641 by default, mean success and then a reboot. This is a deliberate difference from MDT, whose engine never reboots because of an exit code.
- The agent saves state before every reboot, including a re-run. MDT's engine skips that save for a re-run, so the step's variable changes are lost. After saving, the agent writes a marker that the reboot clears: a volatile registry key in Windows, or a file on `X:` in WinPE. It refuses to resume while the marker exists. MDT's engine uses a volatile key the same way.
- The agent runs each command in a Windows job object. When `max_runtime_secs` passes, it ends the whole process tree and fails the step. MDT's templates set no time limit, so a hung step waits forever.
- The agent streams each step start, step result, and log line to the server. The server is the record of what happened, and the local journal covers network outages.

The engine is a pure state machine over an `Executor` trait, so it is tested on Linux with a fake executor. The key test crashes the agent at every step boundary and checks that each resumed run ends in the same state as a run with no crash.

### Variables

- A name is 1 to 256 letters, digits, `_`, or `-`, and names are case-insensitive. These are MDT's rules. deployarr also limits the letters and digits to ASCII. That limit is deployarr's choice, because the [MDT 8450 binaries reference](research/mdt-8450-binaries.md#variables) does not say whether MDT accepts other letters. A strict set can loosen later without breaking a stored sequence. Names that start with `_` belong to the agent, and steps can read them but not set them. A sequence default cannot name a variable that starts with `_` either.
- A value is a scalar or a list. MDT's `Applications001`-style lists become real lists.
- A value that is already set beats a sequence default. For a sequence default, an empty value counts as not set, so the default fills it. MDT's `globalVarList` follows the same rule and fills only variables that are still empty.
- `%Name%` inside a `Template` substitutes a value, and `%%` stands for a literal `%`. Substitution makes one pass from left to right and never scans inserted text again. A name that is not a task sequence variable falls back to the process environment, as in MDT. A name that neither defines fails the step. MDT leaves such a name in the text, and its expander scans inserted text again, so one stored value can make it loop forever. A name is the text between one `%` and the next, so a `%` with no closing `%` fails the step. A name that is not a valid variable name, such as `ProgramFiles(x86)`, goes straight to the process environment. An empty variable is defined, so it substitutes as empty text and does not fall back to the process environment. That is deployarr's choice. The [MDT 8450 binaries reference](research/mdt-8450-binaries.md#variables) does not say how MDT's expander treats an empty value. A list value fails the step, because no one separator suits every command line.
- There is no `#expr#` evaluation. MDT runs `#expr#` through VBScript `Eval`, which executes anything. Conditions use the typed `Condition` model instead.
- Step processes read and set variables through the agent, with the `deployarr-agent var get` and `var set` commands over a local channel that the agent opens for each step. There is no command that clears every variable. MDT scripts change a shared-memory store directly, and its `Clear()` method erases the engine's own state.
- A variable can be marked secret, and it stays secret when a later write sets it again. Secrets never reach a log line or an event. The CMTrace writer and the event stream both redact them. Redaction masks every stretch of text that any secret value covers, so overlapping secrets leave no fragment. The mask repeats a character that no secret contains, so no secret can show through it. Redaction also masks every value a secret variable held before, because a step can copy a secret into a plain variable before the secret changes. Logs show templates before substitution, as MDT's engine does.
- Every variable records where its value came from: a gathered fact, a rule, a lookup match, a prompt, a step, a sequence default, or the agent. A lookup match names its resolver, and for an add-on resolver also the add-on. The UI shows this trace for each deployment. MDT writes a similar trace into `ZTIGather.log`.
- Some variables fill settings in the unattend file, such as the `ReadyToUse` OOBE settings. Where MDT has a variable for one of those settings, deployarr keeps MDT's name. See [Ready-to-use OOBE settings](#ready-to-use-oobe-settings).

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
- The server skips the resolver of an add-on that is not Enabled, such as one that is disabled, incompatible, revoked, or missing. The trace names the add-on's state.
- An identity resolver gets only the PC's usable identity facts, with known placeholders dropped. With none left, the server skips it. See [Add-on resolvers and wipe authority](#add-on-resolvers-and-wipe-authority).
- A resolver that declares a prompt and has no answer yet sends its prompt to the agent. The agent renders it. See [What runs on the PC](#what-runs-on-the-pc).
- A timeout, a trap, a refused host call, or a match without wipe authority counts as an error. The trace shows it, and the chain moves to the next resolver.
- The server checks that a matched task sequence exists. A variable that a match sets has a lookup match as its origin. A lookup match names its resolver, and for an add-on resolver also the add-on.

The SQL order lookup add-on maps an order row's OS identifier to a task sequence through an editable table in its settings, for example `W11-PRO -> W11-PRO-STD`. Other columns can map to variables, such as the computer name or an application list. The map lives in the add-on, not in the base, so the base never sees an OS identifier. The add-on's resolver returns a task sequence, which the server then checks (owner decision on [#130](https://github.com/Tsukidy/deployarr/issues/130)).

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
- PostgreSQL, MySQL, and MariaDB use `sqlx`. Recent `sqlx` releases dropped SQL Server support, which is why SQL Server has its own driver.
- An admin writes each query with named parameters, such as `SELECT os_code, order_no FROM orders WHERE board_serial = :board_serial`. The query is a `sql_text` setting of the add-on. The add-on names it and never supplies SQL. The server rewrites the names into each engine's placeholder syntax and always binds values. Values are never pasted into the SQL text.
- On MySQL and MariaDB, a named query must be a `SELECT`, which may start with a `WITH` clause. The add-on host applies the `SELECT` check in [Write guards](#write-guards) to the `sql_text` value when an admin saves the add-on's settings. Only text that passes the check becomes a `NamedQuery`.
- A lookup error or an empty result moves to the next resolver and appears in the trace. MDT logs the error and continues as if nothing happened.
- The order database is read-only to deployarr. The setup guide requires a database account that can only read. That account is the guard that holds on every engine. The engine guards and the `SELECT` check in [Write guards](#write-guards) are a second guard, and each one has limits.

#### Write guards

The test results below come from runs on 2026-11-12 and 2026-11-13 with an account that may write, on PostgreSQL 16.15, MySQL 8.0.46, and MariaDB 10.11.14. Comments on [#77](https://github.com/Tsukidy/deployarr/issues/77) record the setup and every result, in the [first runs](https://github.com/Tsukidy/deployarr/issues/77#issuecomment-6839109962) and the [compound statement runs](https://github.com/Tsukidy/deployarr/issues/77#issuecomment-6845427631). The [triage on #118](https://github.com/Tsukidy/deployarr/issues/118#issuecomment-6855355335) records the runs behind the `SELECT` check below, on the same MySQL and MariaDB releases. Without a guard, that account's `INSERT`, `TRUNCATE`, and `DROP TABLE` changed the data on each engine.

The server sends each named query to PostgreSQL, MySQL, and MariaDB as one prepared statement. PostgreSQL and MySQL accept one statement in each prepared statement ([Extended Query](https://www.postgresql.org/docs/16/protocol-flow.html#PROTOCOL-FLOW-EXT-QUERY), [mysql_stmt_prepare()](https://dev.mysql.com/doc/c-api/8.0/en/mysql-stmt-prepare.html), [PREPARE Statement](https://dev.mysql.com/doc/refman/8.0/en/prepare.html)). On those two engines, a query text that holds a second statement fails before it runs. MariaDB also refused a text with two statements. A MariaDB compound statement still holds several statements in one, as shown below.

Sent as plain text, a second statement can end the guard's transaction or reset the session. It can then write. On PostgreSQL, `COMMIT; DROP TABLE orders` went as one query inside `BEGIN READ ONLY` and dropped the table. On MySQL and MariaDB, `SET SESSION TRANSACTION READ WRITE; DROP TABLE orders` went as one query under the guard below and dropped the table. Sent as a prepared statement, each text failed and changed nothing.

The guard on each engine:

- PostgreSQL. The server runs each named query in its own transaction. It opens the transaction with `BEGIN READ ONLY` and rolls it back after the query. A read-only transaction refuses `INSERT`, `UPDATE`, and `DELETE` on a table that is not temporary. It also refuses `TRUNCATE` and every `CREATE`, `ALTER`, and `DROP` command ([SET TRANSACTION](https://www.postgresql.org/docs/16/sql-set-transaction.html)). It refused every write to the database that the tests tried, including a `CALL` of a procedure that writes. A transaction per query also holds where a setting made once per connection does not. In one test, a named query ran `SET SESSION CHARACTERISTICS AS TRANSACTION READ WRITE`. Under a once-per-connection setting, the next named query on that connection then dropped a table. Under a transaction per query, the same `DROP TABLE` failed.
- MySQL and MariaDB. Before each named query, the server sets the session's transaction access mode with `SET SESSION TRANSACTION READ ONLY`. After each named query, it runs `ROLLBACK`, which ends any transaction that the query left open. Under this guard, `INSERT`, `DELETE`, `TRUNCATE`, `DROP TABLE`, and a `CALL` of a procedure that runs `TRUNCATE` each failed with error 1792 and changed nothing. Before the server sends a named query, it also applies the `SELECT` check below.
- SQL Server. deployarr relies on the read-only account alone. SQL Server has no engine-level read-only setting to fall back on (inferred in [Add-on design attacks, item 2](research/addon-design-attacks-wasmtime-50.0.1.md#item-2-read-only-enforcement-depends-on-the-engine)). The server also runs each named query in a transaction and always rolls it back. No test shows that the rollback undoes `TRUNCATE` or `DROP TABLE`, so it does not count as a guard. The M5 integration test records whether it does.

The MySQL and MariaDB guard takes this form for two reasons:

- A read-only transaction alone does not stop DDL. A DDL statement such as `TRUNCATE TABLE` or `DROP TABLE` commits the open transaction and then runs ([Statements That Cause an Implicit Commit](https://dev.mysql.com/doc/refman/8.0/en/implicit-commit.html)). Inside `START TRANSACTION READ ONLY`, `TRUNCATE TABLE` emptied the table. `DROP TABLE` dropped it. The session's read-only access mode refuses DDL on every table. It also refuses data changes to a table that is not temporary ([Transaction Access Mode](https://dev.mysql.com/doc/refman/8.0/en/set-transaction.html#set-transaction-access-mode)).
- A setting made once per connection does not hold. A named query can run `SET SESSION TRANSACTION READ WRITE`. Under a once-per-connection setting, the next named query on that pooled connection then dropped a table. On MariaDB, a named query can also run `START TRANSACTION READ WRITE`. A session setting does not change a transaction that is already open ([Transaction Characteristic Scope](https://dev.mysql.com/doc/refman/8.0/en/set-transaction.html#set-transaction-scope)). The next named query then wrote inside that transaction. A later named query committed the write. Setting the access mode before each query stops the first case. The `ROLLBACK` after each query stops the second. MySQL 8.0 refused to prepare `START TRANSACTION`, with error 1295.

On MariaDB, two forms of named query get past that guard. Each prepares as one statement:

- A query that starts with `SET STATEMENT tx_read_only=0 FOR`. MariaDB runs the statement after `FOR` with the variable changed for that statement only. Its list of variables that cannot be set this way does not include `tx_read_only` ([SET STATEMENT](https://mariadb.com/docs/server/reference/sql-statements/administrative-sql-statements/set-commands/set-statement)). Sent as one prepared statement under the guard, it ran `INSERT`, `TRUNCATE TABLE`, and `DROP TABLE`. MySQL 8.0 refused to prepare it, with error 1064.
- A compound statement, such as `BEGIN NOT ATOMIC SET SESSION TRANSACTION READ WRITE; TRUNCATE TABLE orders; END`. A compound statement can hold several statements ([BEGIN END](https://mariadb.com/docs/server/reference/sql-statements/programmatic-compound-statements/begin-end)). A statement inside it can set the session back to read-write. A later statement inside it then writes. Sent as one prepared statement under the guard, this form ran `INSERT`, `TRUNCATE TABLE`, and `DROP TABLE`. A compound statement that held only `TRUNCATE TABLE` failed with error 1792. An account with only `SELECT` got error 1142 on the `TRUNCATE` form. MySQL 8.0 refused to prepare each form, with error 1064.

The `SELECT` check stops both forms on MySQL and MariaDB. On those two engines, the server checks the text of each named query. A named query passes the check only when both of these hold:

- The text holds no executable comment. Neither `/*!` nor `/*M!` appears anywhere in it.
- Its first word is `SELECT` or `WITH`, in any letter case. Only whitespace, ordinary comments, and one `(` may come before it. The ordinary comments are `/* ... */`, `#` up to the next line feed, and `--` followed by whitespace up to the next line feed. For the check, only a line feed ends a `#` or `--` comment. A carriage return does not.

Any other text fails the check, including text that the check cannot read, such as a `/*` with no `*/`. The failure names the reason. The add-on host applies the check to each MySQL and MariaDB `sql_text` value when an admin saves an add-on's settings, next to its check for one statement and declared parameters ([SQL connections](#sql-connections)). If a value fails, the host does not save the settings and shows the admin the reason. The server never sends a query that fails the check to the database. If saved settings hold one, the lookup treats it as a lookup error. It moves to the next resolver, and the reason appears in the trace. The check does not change the engine guard. The server still runs `SET SESSION TRANSACTION READ ONLY` before each named query and `ROLLBACK` after it.

The check is an allowlist. It names the text that passes, not the text that fails, for three reasons:

- MariaDB runs several kinds of block outside a stored program. Its page on them names `BEGIN`, `IF`, `CASE`, `LOOP`, `WHILE`, and `REPEAT` as the statements that may start one ([Using Compound Statements Outside of Stored Programs](https://mariadb.com/docs/server/reference/sql-statements/programmatic-compound-statements/using-compound-statements-outside-of-stored-programs)). The page leaves out `FOR`. In the [triage on #118](https://github.com/Tsukidy/deployarr/issues/118#issuecomment-6855355335), `IF`, `CASE`, `WHILE`, `REPEAT`, and `FOR` blocks that set the session back to read-write each emptied the table under the guard. A list of refused words would miss forms like the `FOR` block.
- A comment can come before the first word, and an executable comment can hide or change it. MariaDB runs the code inside `/*!...*/` and `/*M!...*/`, unless the version number in the comment rules it out ([Comment Syntax](https://mariadb.com/docs/server/reference/sql-statements/comment-syntax)). A check that skips executable comments as comments reads `/*!BEGIN NOT ATOMIC SET SESSION TRANSACTION READ WRITE; TRUNCATE TABLE orders; */ SELECT 1; END` as a `SELECT`, and that query emptied the table. A check that reads them as code reads `/*M!999999 SELECT 1 FROM orders */ BEGIN NOT ATOMIC ...` as a `SELECT`. MariaDB 10.11.14 skipped that comment, which names a later version, and the block emptied the table. So the check refuses every executable comment. This also rules out nested comments, which the Comment Syntax page says are possible in some situations. MySQL and MariaDB nest a comment only inside an executable comment. So in text that passes the check, an ordinary comment ends at its first `*/` (the `MY_LEX_LONG_COMMENT` case in `sql/sql_lex.cc` of [MariaDB 10.11.14](https://github.com/MariaDB/server/blob/mariadb-10.11.14/sql/sql_lex.cc#L2364-L2492) and [MySQL 8.0.46](https://github.com/mysql/mysql-server/blob/mysql-8.0.46/sql/sql_lex.cc#L1865-L1966)).
- MySQL and MariaDB do not end a `#` or `--` comment at a carriage return. A check that ends one there reads a `SELECT` that the server treats as part of the comment. On MariaDB, `#x`, a carriage return, `SELECT 1 FROM orders`, a line feed, and then the compound statement example above emptied the table. The same text with `-- x` in place of `#x` did too. On MySQL, `#x`, a carriage return, `SELECT 1 FROM orders`, a line feed, and `TRUNCATE TABLE orders` prepared as a `TRUNCATE`, and the guard refused it with error 1792.

Text that passes the check can still try to write, so the engine guard and the account rules stay. On MySQL, `WITH c AS (SELECT 1) DELETE FROM orders` and the same with `UPDATE` pass the check, and the guard refused both with error 1792. MariaDB 10.11.14 refused to prepare the `DELETE` form, with error 1064. On both engines, a `SELECT` of a stored function that writes failed under the guard with error 1792. `SELECT ... INTO OUTFILE` also passes the check. The rule that the account holds no `FILE` privilege covers it.

The engine guards do not stop these:

- A query that writes a file or runs a program on the database server. On MySQL and MariaDB, `SELECT ... INTO OUTFILE` wrote a file under every guard tested. On PostgreSQL, `COPY ... TO` a file and `COPY ... TO PROGRAM` both ran inside `BEGIN READ ONLY`. PostgreSQL's read-only mode "does not prevent all writes to disk" ([SET TRANSACTION](https://www.postgresql.org/docs/16/sql-set-transaction.html)).
- On MySQL and MariaDB, a call to a stored procedure that sets the session back to read-write and then writes. A procedure that runs `SET SESSION TRANSACTION READ WRITE` and then `TRUNCATE TABLE` emptied the table under every guard tested. An account with only `SELECT` and `EXECUTE` emptied it the same way, through a procedure that runs with its definer's rights. On PostgreSQL, the same kind of procedure failed inside `BEGIN READ ONLY`. On MySQL and MariaDB, the `SELECT` check refuses `CALL`, so a named query cannot call such a procedure. On MariaDB 10.11.14, a `SELECT` of a stored function that calls such a procedure failed with error 1792 under the guard.

The read-only account that the setup guide requires must also meet these rules:

- On MySQL and MariaDB, it holds neither the `FILE` privilege ([FILE](https://dev.mysql.com/doc/refman/8.0/en/privileges-provided.html#priv_file)) nor `EXECUTE` on a stored procedure that writes.
- On PostgreSQL, it is not a superuser, and it is not a member of `pg_write_server_files` or `pg_execute_server_program` ([Predefined Roles](https://www.postgresql.org/docs/16/predefined-roles.html)).

### Wipe safety

A zero-touch server can erase any PC that network-boots by mistake. deployarr erases a disk only after one of these:

- a positive identity match, from a pre-staged record or from an add-on resolver such as an order row lookup
- a technician's typed answer to an add-on resolver's prompt, such as an order number
- a technician picking a task sequence

`WipeAuthority` in `deployarr-core` holds one of four authorities. Each variant holds a type with private fields, so only the variant's constructor can build one.

- `PreStagedMatch` holds the PC's usable identity facts of the kind that a pre-staged record matched on. `WipeAuthority::from_pre_staged_match` takes the `UsableIdentity` that the server matched the record against and the `IdentityKind` that the record matched on. It returns an error when the identity holds no fact of that kind. It never takes the gathered facts. `UsableIdentity` holds no placeholder, so a placeholder never gives authority.
- `IdentityMatch` holds an add-on identity resolver and the facts of the matched kind that the server sent to it. `WipeAuthority::from_addon_match` builds it.
- `TechnicianEntry` holds an add-on resolver and the ID of the prompt that the technician answered. It does not hold the answer. `WipeAuthority::from_addon_match` builds it.
- `ManualPick` holds the ID of the task sequence that a technician picked from the manual list. `WipeAuthority::from_manual_pick` takes the task sequences that the server offered on the list and the one that the technician picked. It returns an error for a pick that the server did not offer, so a pick from an empty list is always an error.

An add-on resolver returns a match and never wipe authority. `deployarr-core` builds the authority from the fact or the answer that the server sent. See [Add-on resolvers and wipe authority](#add-on-resolvers-and-wipe-authority).

An unknown PC stops at an add-on's prompt, the manual list, or the no-match screen, whichever the chain reaches first, and does not touch its disk.

After a deployment succeeds, the server stops offering PXE boot to that PC's MAC address and SMBIOS UUID (DHCP option 97), until an admin re-arms it. This stops a PC that boots from the network first from reinstalling in a loop. A match on either key marks the PC as finished. For a request without option 97, the server checks the MAC address in `chaddr` alone, which every request carries. Option 61 (client identifier) is not a key. The iPXE ROM fills it with the same MAC address. OVMF sends none ([#56](https://github.com/Tsukidy/deployarr/issues/56)).

## Agent

- The agent is one static Rust executable built for `x86_64-pc-windows-msvc` with the static C runtime.
- In WinPE it uses Win32 APIs, not WMI, so base WinPE needs no optional components:
  - SMBIOS: `GetSystemFirmwareTable`. The chassis type and the form factor flags read the type 3 structures, and the VM flag reads the type 0 structure. See [Chassis and VM facts](#chassis-and-vm-facts).
  - network adapters: `GetAdaptersAddresses`
  - firmware type: `GetFirmwareType`. The fallback is MDT's `GetFirmwareEnvironmentVariableW` check, and only with `SeSystemEnvironmentPrivilege` enabled first. Without the privilege, newer WinPE versions make that check report BIOS PCs as UEFI, which is the bug the MDT 8456 hotfix fixed.
  - disks and bus type: `IOCTL_STORAGE_QUERY_PROPERTY`. Bus type 7 is USB.
  - device hardware IDs and compatible IDs: SetupAPI with `DIGCF_PRESENT | DIGCF_ALLCLASSES`, which is what MDT's `Microsoft.BDD.PnpEnum.exe` does. Unlike PnpEnum, the agent reports an error when enumeration fails, so a failed scan never looks like a PC with no devices.
  - CPU features: the `cpuid` instruction, for the same virtualization, NX, and 64-bit facts that MDT's `Microsoft.BDD.Utility.dll` reports. The VM flag reads leaf 1 ECX bit 31, the hypervisor present bit, which MDT reports as `IsHypervisorRunning` ([MDT 8450 binaries reference](research/mdt-8450-binaries.md#microsoftbddutilitydll)). The hypervisor vendor reads leaf `0x40000000`.
  - memory and processor cores: `GetPhysicallyInstalledSystemMemory` and `GetLogicalProcessorInformationEx`.
  - TPM version: `Tbsi_GetDeviceInfo` from `tbs.dll` when the boot image has it and the call succeeds, and the TPM device's IDs otherwise. Base WinPE 10.0.26100.1 has no `tbs.dll`. ConfigMgr reads the TPM through WMI `Win32_Tpm` instead. See [TPM version](#tpm-version).
  - Secure Boot: the `UEFISecureBootEnabled` value under `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State`, which ConfigMgr also reads. When the value is missing, the agent reads the `SecureBoot` UEFI variable with `GetFirmwareEnvironmentVariableW`, with `SeSystemEnvironmentPrivilege` enabled. The fact has three values: on, off, and unknown. A failed read is unknown, never off. ConfigMgr keeps the same third value as `NA`.
- In the full OS, the agent also calls WMI through COM, for `Wmi` conditions and the [BitLocker](#bitlocker) actions. It never calls WMI in WinPE. COM adds `ole32.dll` and `oleaut32.dll` to the import table, and base WinPE's [list file](research/winpe-10.1.26100.9457.md#the-list-file) holds both.
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

### Chassis and VM facts

The agent gathers four facts about the PC's form and whether it is a virtual machine (VM). They are gathered facts, like the others in step 4 of the [Deployment flow](#deployment-flow). A `Var` condition can test them, and the variable trace shows them. MDT's gather sets similar properties ([MDT 8456 reference](research/mdt-8456.md#gathered-facts)). ConfigMgr 2509 gathers no chassis type and no VM flag ([ConfigMgr 2509 reference](research/configmgr-2509.md#set-dynamic-variables)).

The SMBIOS fields and their names come from the SMBIOS specification, [DSP0134 3.7.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.7.0.pdf). The CPUID leaves come from [Feature discovery](https://learn.microsoft.com/en-us/virtualization/hyper-v-on-windows/tlfs/feature-discovery) in the Hyper-V Top Level Functional Specification (TLFS). The `deployarr-smbios` crate, which parses the [identity facts](#identity-facts), also reads the type 0 VM bit and the type 3 chassis type.

- Chassis type: bits 6:0 of the Type byte at offset 05h of an SMBIOS type 3 structure (DSP0134 3.7.0, Table 16). Bit 7 flags a chassis lock and is not part of the type. The agent reads the first type 3 structure, in table order, whose type is not 12 (Docking Station) or 21 (Peripheral Chassis). The fact is empty when no such structure exists. Table 17 names the values. A PC can have more than one type 3 structure. A blade has one for itself and one for its Blade Enclosure (Table 17, the note on Blade).
- Form factor flags: a laptop flag, a desktop flag, and a server flag. The agent reads bits 6:0 of every type 3 structure. It skips a structure whose type is 12 or 21. Each other structure sets the flag whose list in the table below holds its type. A type that no list names, such as 1 (Other) or 2 (Unknown), sets no flag. No structure clears a flag, so more than one flag can be true. These are MDT's skip and MDT's lists ([Form factor flags](research/mdt-8456.md#form-factor-flags)), so a condition ported from MDT gives MDT's answer. Where MDT reads the first `ChassisTypes` entry of a `Win32_SystemEnclosure` instance, the agent reads the type of one type 3 structure. Microsoft documents that `ChassisTypes` comes from the Type member of the type 3 structure ([`Win32_SystemEnclosure` class](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-systemenclosure)). That each instance is one type 3 structure is inferred.
- VM flag: true when either of these signals reads true, and false otherwise. Make and model strings never set it. The log names each signal that reads true.
  - CPUID leaf 1 sets ECX bit 31, the hypervisor present bit. The TLFS says that a set bit means a hypervisor is present, and that the bit is clear in a non-virtualized environment.
  - The SMBIOS type 0 structure sets bit 4 of BIOS Characteristics Extension Byte 2, at offset 13h. DSP0134 3.7.0 defines the bit as "SMBIOS table describes a virtual machine", and a clear bit allows no inference (Table 9). A type 0 structure too short to hold that byte counts as a clear bit. Such a structure has a Length below 14h, as in SMBIOS 2.2 and earlier (Table 6).
- Hypervisor vendor: when the hypervisor present bit is set, the 12-byte signature in EBX, ECX, and EDX of CPUID leaf `0x40000000`, in that order, with trailing NUL bytes removed. The fact is empty otherwise. Microsoft's hypervisor returns `Microsoft Hv` (TLFS). KVM's signature is `KVMKVMKVM` followed by three NUL bytes ([`arch/x86/include/uapi/asm/kvm_para.h:11`](https://github.com/torvalds/linux/blob/v6.8/arch/x86/include/uapi/asm/kvm_para.h#L11) in Linux v6.8). QEMU 10.2.1 puts that signature in leaf `0x40000000` of a KVM guest ([`target/i386/kvm/kvm.c:2251-2258`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/kvm/kvm.c#L2251-L2258)). With QEMU's Hyper-V enlightenments on, that leaf holds `Microsoft Hv` by default, and KVM's signature moves to leaf `0x40000100` ([`target/i386/kvm/kvm.c:1564-1569`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/kvm/kvm.c#L1564-L1569), [`target/i386/kvm/kvm.c:2161-2170`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/kvm/kvm.c#L2161-L2170), [`target/i386/cpu.c:9196-9199`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/cpu.c#L9196-L9199)). The lab VMs turn on no enlightenments ([`tools/lab/vm.sh`](../tools/lab/vm.sh)). The TLFS says that "The vendor ID signature should be used only for reporting and diagnostic purposes." The VM flag does not read the vendor.

| Flag | Chassis types | MDT property |
| --- | --- | --- |
| Laptop | 8, 9, 10, 11, 14, 18, 30, 31, 32 | `IsLaptop` |
| Desktop | 3, 4, 5, 6, 7, 13, 15, 16, 35, 36 | `IsDesktop` |
| Server | 23, 28 | `IsServer` |

MDT's laptop list also names 12 and 21, which the skip never lets through. Type 18 is Expansion Chassis, yet it sets the laptop flag. No list names 17 (Main Server Chassis) or 29 (Blade Enclosure). The lists stay as MDT has them. A condition that needs another type tests the chassis type itself.

Strings never set the VM flag. MDT sets `IsVM` from six string tests on the make and the model. A hypervisor that those tests do not name, such as QEMU with KVM, gets `IsVM` false ([Virtual machine flag](research/mdt-8456.md#virtual-machine-flag)). The two signals that deployarr reads come from the CPU and the firmware, and neither depends on a vendor's name.

The VM flag reads both signals, because each one alone can miss a VM. The lab runs QEMU 10.2.1, `ovmf` 2025.11, and `seabios` 1.17.0 ([Versions](research/pxe-lab/single-stage-chain.md#versions)). The citations below use the matching upstream tags. Nobody has checked Ubuntu's patches to them.

- The SMBIOS VM bit alone misses the BIOS lab VM (inferred). QEMU 10.2.1 builds a type 0 structure only when the command line sets a type 0 field ([`hw/smbios/smbios.c:537-549`](https://github.com/qemu/qemu/blob/v10.2.1/hw/smbios/smbios.c#L537-L549), [`hw/smbios/smbios.c:566-603`](https://github.com/qemu/qemu/blob/v10.2.1/hw/smbios/smbios.c#L566-L603)). [`tools/lab/vm.sh`](../tools/lab/vm.sh) sets only a type 2 field, so the firmware adds its own type 0 structure. OVMF at `edk2-stable202511` sets Extension Byte 2 to `0x1C`, which sets bit 4 ([`OvmfPkg/SmbiosPlatformDxe/SmbiosPlatformDxe.c:22-49`](https://github.com/tianocore/edk2/blob/edk2-stable202511/OvmfPkg/SmbiosPlatformDxe/SmbiosPlatformDxe.c#L22-L49), [`:113-191`](https://github.com/tianocore/edk2/blob/edk2-stable202511/OvmfPkg/SmbiosPlatformDxe/SmbiosPlatformDxe.c#L113-L191)). SeaBIOS at `rel-1.17.0` sets it to `4`, which leaves bit 4 clear ([`src/fw/biostables.c:443-483`](https://github.com/coreboot/seabios/blob/rel-1.17.0/src/fw/biostables.c#L443-L483), [`:516-556`](https://github.com/coreboot/seabios/blob/rel-1.17.0/src/fw/biostables.c#L516-L556)).
- KVM sets the hypervisor present bit on every lab VM (inferred). QEMU 10.2.1 reports the bit as supported under KVM ([`target/i386/kvm/kvm.c:456-460`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/kvm/kvm.c#L456-L460)), and the lab runs each VM under KVM with `-cpu host` ([`tools/lab/vm.sh`](../tools/lab/vm.sh)). The script falls back to TCG with `-cpu max` only when the host has no usable `/dev/kvm` and `LAB_ALLOW_TCG=1` is set.
- The hypervisor present bit alone misses a VM whose hypervisor clears it. QEMU names the bit `hypervisor` among the CPU flags that a command line can turn off ([`target/i386/cpu.c:1080-1092`](https://github.com/qemu/qemu/blob/v10.2.1/target/i386/cpu.c#L1080-L1092)).

The statements about the lab VMs stay inferred until the M3 check runs.

The agent reads these facts in WinPE, with the other facts. It does not read them again in the full OS. Virtualization-based security (VBS) "uses hardware virtualization and the Windows hypervisor" ([VBS page for OEMs](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/oem-vbs)). With the TLFS rule above, that hypervisor can set the hypervisor present bit on a physical PC in the full OS (inferred). Nobody has checked the bit in WinPE on a physical PC. The M3 check reads it on the real PC.

The facts need no new module. The agent already reads SMBIOS with `GetSystemFirmwareTable`, which base WinPE's `kernel32.dll` exports ([WinPE 10.1.26100.9457 reference](research/winpe-10.1.26100.9457.md#functions-the-architecture-names)), and `cpuid` is an instruction. So the WinPE import check does not change.

### TPM version

Base WinPE 10.0.26100.1 has no `tbs.dll`. It ships in the `WinPE-SecureStartup` optional component. DISM can add that component only after `WinPE-WMI` (inferred from the package's `parent` element). The [WinPE 10.1.26100.9457 reference](research/winpe-10.1.26100.9457.md#tbsdll) records these findings. Reading the TPM version requires neither component. The agent reads the TPM version from `Tbsi_GetDeviceInfo` when the boot image has `tbs.dll` and the call succeeds, and from the TPM device's IDs otherwise.

1. The agent loads `tbs.dll` at run time with `LoadLibraryExW` and `LOAD_LIBRARY_SEARCH_SYSTEM32`, so the loader searches only `System32` ([LoadLibraryExW](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw)). `tbs.dll` is in neither the import table nor the delay-load import table, because the import check reads both and rejects it.
2. When `tbs.dll` and its `Tbsi_GetDeviceInfo` export load, the agent calls `Tbsi_GetDeviceInfo`. On `TBS_SUCCESS`, `tpmVersion` holds `TPM_VERSION_12` or `TPM_VERSION_20` ([TPM_DEVICE_INFO](https://learn.microsoft.com/en-us/windows/win32/api/tbs/ns-tbs-tpm_device_info)), and the agent reports that version.
3. When `tbs.dll` or its `Tbsi_GetDeviceInfo` export does not load, the agent maps device IDs to a TPM version. It also maps them when `Tbsi_GetDeviceInfo` returns an error, such as `TBS_E_TPM_NOT_FOUND` ([Tbsi_GetDeviceInfo](https://learn.microsoft.com/en-us/windows/win32/api/tbs/nf-tbs-tbsi_getdeviceinfo)), or a `tpmVersion` with any other value. The log keeps the load error, the TBS error, or the `tpmVersion` value. A boot image with `tbs.dll` therefore reports unknown only when base WinPE would too.

The mapping:

- It uses the IDs that base WinPE's `tpm.inf` binds to the TPM driver, from the [table in the research page](research/winpe-10.1.26100.9457.md#tbsdll). An ID from a row that `tpm.inf` describes as TPM 1.2 means 1.2. An ID from a row it describes as TPM 2.0 means 2.0. The agent carries the table in its code. It does not read `tpm.inf` at run time.
- It reads the hardware IDs and the compatible IDs of every present device from the SetupAPI scan above. It compares them with the table without regard to case. A device that lists an ID from the table is a matched device.
- A device with no compatible IDs is not a scan error. `SetupDiGetDeviceRegistryPropertyW` reports `ERROR_INVALID_DATA` for a property that the device does not have ([SetupDiGetDeviceRegistryPropertyW](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceregistrypropertyw)).
- Compatible IDs matter when the firmware gives the TPM a vendor `_HID`, as EDK2 does. Windows uses compatible IDs to find a driver when none matches a hardware ID ([Compatible ID](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/compatible-ids)). An ACPI device's compatible IDs come from its `_CID` object ([Device management namespace objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/bringup/device-management-namespace-objects), section Compatible ID (_CID)). EDK2 at tag `edk2-stable202408` declares its TPM 2.0 device with `_HID` `NNNN0000` and `_CID` `MSFT0101` (`SecurityPkg/Tcg/Tcg2Acpi/Tpm.asl:30-34`). Its `UpdateHID` replaces `NNNN0000` with an ID built from the TPM's manufacturer and firmware version (`SecurityPkg/Tcg/Tcg2Acpi/Tcg2Acpi.c:558-652`). A check of hardware IDs alone reads such a PC as unknown.
- It counts a matched device only when the device has started with no problem code. Such a device is a counted device. Both paths in step 3 run this mapping, so the rule holds when `tbs.dll` or its export does not load, and when `Tbsi_GetDeviceInfo` returns an error or another `tpmVersion` value. A boot image with `tbs.dll` and base WinPE therefore still agree.
- The agent reads a matched device's status with `CM_Get_DevNode_Status`, which `cfgmgr32.dll` in base WinPE exports ([WinPE 10.1.26100.9457 reference](research/winpe-10.1.26100.9457.md#functions-the-architecture-names)). It passes the device instance handle that the scan returns in `SP_DEVINFO_DATA.DevInst`. `CM_` functions such as `CM_Get_DevNode_Status` take that handle as input ([SP_DEVINFO_DATA](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/ns-setupapi-sp_devinfo_data)).
- `CM_Get_DevNode_Status` returns a status made of the `DN_` flags defined in `Cfg.h`, and a problem code, one of the `CM_PROB_` values defined in `Cfg.h`. The problem code is used only when the status has `DN_HAS_PROBLEM` ([CM_Get_DevNode_Status](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_get_devnode_status)). A matched device counts when its status has `DN_STARTED` and lacks `DN_HAS_PROBLEM`. When a device is started, `SetupDiRestartDevices` sets its status to `DN_STARTED` ([SetupDiRestartDevices](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdirestartdevices)).
- The log gives a matched device's status flags and its problem code. When the status lacks `DN_HAS_PROBLEM`, the log says that the device has no problem code.
- Within one device, the first table ID among its hardware IDs, in list order, decides the device's version. Its compatible IDs decide only when none of its hardware IDs is in the table, and then the first table ID among them decides. Windows "always prefers Hardware IDs (returned by _HID) over Compatible IDs (returned by _CID) in INF matching and driver selection" ([Device management namespace objects](https://learn.microsoft.com/en-us/windows-hardware/drivers/bringup/device-management-namespace-objects), section Compatible ID (_CID)). A list of hardware IDs is typically sorted from most to least suitable ([Hardware ID](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/hardware-ids)). Compatible IDs should be listed in order of decreasing suitability ([Compatible ID](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/compatible-ids)). While `tpm.inf` is the only INF that lists these IDs, the rule picks the ID that Windows binds (inferred).
- A started device with a 1.2 hardware ID, such as `ACPI\PNP0C31`, and the 2.0 compatible ID `ACPI\MSFT0101` therefore reads 1.2. Windows binds the 1.2 section of `tpm.inf` for that device, so 1.2 is the version its driver uses (inferred). No known device lists IDs for both versions.
- Nobody has checked the table against every firmware TPM. The M3 check tests it on the UEFI VM and on one real PC.
- The UEFI VM's TPM is QEMU's `tpm-crb` device (`tools/lab/vm.sh`). QEMU 8.2.2, the version in Ubuntu 24.04, declares that device with `_HID` `MSFT0101` and no `_CID` (`hw/i386/acpi-build.c:1794-1810`). The VM therefore tests only the hardware-ID path. The real PC tests the compatible-ID path only if its TPM has a vendor `_HID`.
- Nobody has checked whether `tpm.sys` starts in base WinPE. The image holds `tpm.sys` and `tpm.inf` ([WinPE 10.1.26100.9457 reference](research/winpe-10.1.26100.9457.md#tbsdll)), but only a boot shows whether the driver starts. If it does not, no device counts, and every PC reads unknown in base WinPE. The M3 check tests it, on a boot image built from the ADK's `winpe.wim`.

The fact has three values: 1.2, 2.0, and unknown. The log names the source of each value.

| Case | TPM version | The log names |
| --- | --- | --- |
| `Tbsi_GetDeviceInfo` returns `TBS_SUCCESS` with `TPM_VERSION_20` or `TPM_VERSION_12` | 2.0 or 1.2 | `Tbsi_GetDeviceInfo` |
| `tbs.dll` or `Tbsi_GetDeviceInfo` does not load, `Tbsi_GetDeviceInfo` returns an error, or `tpmVersion` holds another value | The mapping's result, from the rows below | The load error, the TBS error, or the `tpmVersion` value, then the mapping's row |
| A counted device reads 2.0 | 2.0 | The device, the ID, whether it is a hardware ID or a compatible ID, and the device's status and problem code |
| No counted device reads 2.0, and one reads 1.2 | 1.2 | The device, the ID, whether it is a hardware ID or a compatible ID, and the device's status and problem code |
| One or more devices match, but none has started with no problem code | Unknown | Each matched device, its ID, its status, and its problem code |
| No present device lists an ID from the table | Unknown | That no ID matched |
| The device scan fails | Unknown | The scan error |

- A scan that fails partway is a failed scan. The TPM version is unknown, even when a device read before the failure matched.
- A failed `CM_Get_DevNode_Status` call on a matched device is a failed scan too. The call then returns a `CR_` error code ([CM_Get_DevNode_Status](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_get_devnode_status)), and the log names it.
- When two counted devices read different versions, 2.0 wins. This is a design choice, not a finding. Within one device, the hardware ID rule above decides instead.
- The agent never reports that a PC has no TPM. A TPM whose IDs are all outside the table looks the same as a PC with no TPM (inferred).
- The agent does not read `TBS_E_TPM_NOT_FOUND` as no TPM. It runs the mapping, as base WinPE does. A matched device then overrides the TBS answer only when it has started with no problem code.
- A counted device shows that `tpm.sys` started for the device with no problem code. Base WinPE's `tpm.inf` binds the table's IDs to the TPM driver, so the driver that started is `tpm.sys` (inferred). The device's ID shows the TPM interface that the firmware declares. A counted device does not show that every TPM command works.

### Handoff from WinPE to the new OS

MDT resumes in the new OS through auto-logon as Administrator with a logon count of 999, plus a startup shortcut. deployarr runs the agent as a Windows service instead:

1. Before the reboot, the agent copies itself to `W:\Deployarr\bin`.
2. It loads the offline `SYSTEM` registry hive from `W:\Windows\System32\config` with `RegLoadKeyW`.
3. It creates the `DeployarrAgent` service key: automatic start, running as LocalSystem, set to restart on failure.
4. The service starts at boot. It waits until Windows Setup reports that it has finished, by watching `ImageState` and `SystemSetupInProgress`, and then continues the sequence. In the customer end state it continues in audit mode instead. See [End state](#end-state).

This removes the stored Administrator password and the logon count from the deployment. It also makes the PC deploy the same way whether or not anyone is in front of it.

The trade-off is that steps run as LocalSystem in session 0, with no desktop. A few installers fail without a logged-on user. For those, a later milestone adds a per-step option that runs the step in an auto-logon session with a one-time password. The agent removes that password after the step. That option never uses the built-in Administrator account or the unattend file's `AutoLogon` setting. See [Built-in Administrator account](#built-in-administrator-account).

## End state

The owner ships some PCs ready to use and others ready for the customer, so each deployment picks one of two end states. The `EndState` variable holds the choice. A task sequence sets its default, and an order column or a pre-staged record can override it, because a value that is already set beats a sequence default. The server accepts only `ReadyToUse` and `CustomerOobe`, and it rejects any other value before the disk changes.

- `ReadyToUse`: the unattend file skips OOBE with the settings in [Ready-to-use OOBE settings](#ready-to-use-oobe-settings). It creates the local admin account with the PC's own password (see [Server](#server)). The finish action restarts the PC or shuts it down.
- `CustomerOobe`: the unattend file sets `Microsoft-Windows-Deployment | Reseal | Mode` to `Audit` in the oobeSystem pass. Setup then ignores the other oobeSystem settings and starts the auditSystem pass ([Reseal Mode](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-deployment-reseal-mode)). The agent service runs the rest of the sequence in audit mode. Its last action is `sysprep /oobe /shutdown /quiet`. Microsoft documents `sysprep /oobe /shutdown` for a PC that goes to a customer ([Boot Windows to audit mode or OOBE](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/boot-windows-to-audit-mode-or-oobe)). It does not pass `/generalize`, because the image stays on the PC it was applied to.

In both end states, after the sequence's last reboot, the agent turns off the built-in Administrator account and deletes every answer file that Windows Setup can find on the PC's disk. Both finish before the agent reports success, and both are safe to run again after a crash. See [Built-in Administrator account](#built-in-administrator-account) and [Answer files](#answer-files).

In `CustomerOobe`:

- The agent service must continue in audit mode. `IMAGE_STATE_COMPLETE` means that the specialize and oobeSystem passes are complete ([Windows Setup states](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/windows-setup-states)). A PC in audit mode has not finished OOBE, so the service cannot wait for that value alone (inferred). The same table says that an image in `IMAGE_STATE_SPECIALIZE_RESEAL_TO_AUDIT` "has successfully completed the specialize configuration pass and will continue into audit mode when Setup is initiated". Audit mode may show that value (inferred). The M3 lab records the value that audit mode shows.
- The agent reports success, uploads its logs, and removes its service before it runs `sysprep`, because the shutdown ends its run. The customer's first boot starts no deployarr code.
- When the PC boots into audit mode, "the System Preparation (Sysprep) Tool appears" ([Boot Windows to audit mode or OOBE](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/boot-windows-to-audit-mode-or-oobe)). Before it runs `sysprep`, the agent ends any `sysprep.exe` process that is already running, such as that window. Microsoft's system builder guide closes the window before it runs Sysprep from a command prompt. It says "If Sysprep is open, close it and open an elevated command prompt" ([System builder deployment](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/system-builder-deployment?view=windows-10)). In the guide's [Windows 11 view](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/system-builder-deployment?view=windows-11), an automated process starts in audit mode "and the Sysprep window will be closed". Nobody has tested whether `sysprep` from the service works while the window is open. The M3 lab records whether the window was open when the agent ran `sysprep`.
- `/quiet` "Runs the Sysprep tool without displaying on-screen confirmation messages. You can use this option if you automate the Sysprep tool" ([Sysprep command-line options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/sysprep-command-line-options)). The service runs in session 0, where nobody sees such a message (inferred).
- The PC keeps no account or password that deployarr created. The M3 lab confirms it.

### Ready-to-use OOBE settings

Microsoft's Automate OOBE page says "OOBE screens that aren't configured in Unattend will display when a user goes through OOBE." It lists the settings that automate OOBE, all in the oobeSystem pass ([Automate OOBE](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/automate-oobe)). In that pass, the built-in `ReadyToUse` unattend template sets each of them.

The owner sets up OOBE as each client directs ([#113](https://github.com/Tsukidy/deployarr/issues/113#issuecomment-6855426248)). So each setting except `LocalAccounts` comes from a task sequence variable. A task sequence, a rule, an order column, or a pre-staged record can set the variable, as for any variable (see [Variables](#variables)). The template's value is the default. It applies when nothing sets the variable, and a value set for a client overrides it. The agent reads each variable when it writes the unattend file.

The four locale settings belong to `Microsoft-Windows-International-Core`. `LocalAccounts` belongs to `Microsoft-Windows-Shell-Setup | UserAccounts`, and the other settings belong to `Microsoft-Windows-Shell-Setup | OOBE`.

| Setting | Variable | Default |
| --- | --- | --- |
| `InputLocale` | `KeyboardLocale` | `0409:00000409` |
| `SystemLocale` | `SystemLocale` | The value of `UserLocale` |
| `UILanguage` | `UILanguage` | `en-US` |
| `UserLocale` | `UserLocale` | `en-US` |
| `LocalAccounts` | None | The local admin account, with the PC's own password |
| `HideEULAPage` | `HideEULAPage` | `true` |
| `HideLocalAccountScreen` | `HideLocalAccountScreen` | `true` |
| `HideOEMRegistrationScreen` | `HideOEMRegistrationScreen` | `true` |
| `HideOnlineAccountScreens` | `HideOnlineAccountScreens` | `true` |
| `HideWirelessSetupInOOBE` | `HideWirelessSetupInOOBE` | `true` |
| `ProtectYourPC` | `ProtectYourPC` | `3` |

- MDT maps its `KeyboardLocale`, `UILanguage`, and `UserLocale` variables to the same settings (`scripts/ZTIConfigure.xml:181-194`, and the [MDT 8456 reference](research/mdt-8456.md#unattend)). deployarr keeps those names, so rules written for MDT port over. MDT has no `SystemLocale` variable. Its `UserLocale` mapping sets both `SystemLocale` and `UserLocale` (`scripts/ZTIConfigure.xml:185-190`). deployarr's `SystemLocale` takes the value of `UserLocale` when nothing sets it, so an MDT rule that sets only `UserLocale` still sets both.
- MDT has no variable for any `OOBE` setting. Its template sets four of the `Hide` settings to `true` and `ProtectYourPC` to `1` (`Templates/Unattend_x64.xml.10.0:149-156`). The names of the `OOBE` variables and of `SystemLocale` are deployarr's choice. Each one is the setting's own name, as MDT's `UILanguage` and `UserLocale` are.
- The locale defaults are the values in MDT's template (`Templates/Unattend_x64.xml.10.0:162-165`). They match the lab's `en-us` Windows 11 ISO. The MDT paths in this section are relative to `Source/MDT.8456` in the [MDT 8456 sources](research/mdt-8456.md#sources).
- `LocalAccounts` has no variable. It holds the local admin account with the PC's own password, which the server makes for each PC (see [Server](#server)). The account is never the built-in Administrator account.
- The server accepts only `true` and `false` for the `Hide` variables, and only `1`, `2`, or `3` for `ProtectYourPC`. It rejects any other value before the disk changes, as it does for `EndState`.
- `ProtectYourPC` values `1` and `2` turn on Express settings, and `3` turns them off. The setting has "no default value. If a value is not set, the Get going fast page opens during setup" ([ProtectYourPC](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-oobe-protectyourpc)). The template's default, `3`, turns Express settings off.
- `HideLocalAccountScreen` "applies only to the Windows Server editions" ([HideLocalAccountScreen](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-oobe-hidelocalaccountscreen)). The template sets it anyway, because the Automate OOBE page lists it and MDT's template sets it.
- Microsoft says "Windows 11 Pro for personal use and Windows 11 Home require internet connectivity and a Microsoft account during initial device setup" ([Windows 11 specifications](https://www.microsoft.com/en-us/windows/windows-11-specifications)). The owner reports that Home and Pro finish setup without internet through audit mode and the unattend file ([#25](https://github.com/Tsukidy/deployarr/issues/25#issuecomment-6837445895)). The M3 check tests both end states on Home, on the lab's isolated network.
- These variables change only `ReadyToUse`. In `CustomerOobe`, Setup ignores the other oobeSystem settings, and the customer's OOBE runs with no answer file.
- No unattend file that deployarr writes sets `SkipMachineOOBE` or `SkipUserOOBE`. The Automate OOBE page warns "Don't use the SkipMachineOOBE setting to automate OOBE." The SkipMachineOOBE page says that the setting has been deprecated since Windows 8, and that "you should never ship a computer with the SkipMachineOOBE setting configured to true" ([SkipMachineOOBE](https://learn.microsoft.com/en-us/previous-versions/windows/hardware/previsioning-framework/dn965788%28v=vs.85%29)). The SkipUserOOBE page says "This setting has been deprecated since Windows 7" ([SkipUserOOBE](https://learn.microsoft.com/en-us/previous-versions/windows/hardware/previsioning-framework/dn965789%28v=vs.85%29)).
- The server rejects an unattend template that sets `AutoLogon`, `SkipMachineOOBE`, or `SkipUserOOBE`.

`HideEULAPage` hides the Microsoft Software License Terms page. Microsoft's page for the setting says "OEMs and System Builders can only use this setting for testing prior to shipment" ([HideEULAPage](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-oobe-hideeulapage)). Each client's settings decide whether a `ReadyToUse` PC hides the page. When they set `HideEULAPage` to `false`, the license terms page shows at the deployment's first boot. The agent service waits until Setup finishes (see [Handoff from WinPE to the new OS](#handoff-from-winpe-to-the-new-os)), so the deployment waits until someone at the PC accepts the terms (inferred). A PC whose end user must accept the license terms uses `CustomerOobe`, where the customer's OOBE runs with no answer file.

### Built-in Administrator account

Microsoft's page for system builders says "Original equipment manufacturers (OEMs) and system builders are required to disable the built-in administrator account before delivering the computers to customers" ([Enable and disable the built-in Administrator account](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/enable-and-disable-the-built-in-administrator-account)). The page names `sysprep /generalize` as one way to turn the account off. `CustomerOobe` does not pass `/generalize`, and `ReadyToUse` runs no `sysprep`.

Microsoft's pages disagree on when audit mode turns the account on and when Windows turns it off by itself:

- The auditSystem page says "When booting into Audit Mode, the built-in Administrator account is automatically enabled, is used to run scripts during auditUser pass by default, and is used to sign in automatically" ([auditSystem](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/auditsystem)).
- The Audit mode overview says "After you log on to the system, the built-in administrator account is immediately disabled during the auditUser configuration pass. The next time that the computer reboots, the built-in administrator account remains disabled" ([Audit mode overview](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/audit-mode-overview)).
- Boot Windows to audit mode or OOBE says "Booting to audit mode starts the computer in the built-in administrator account, which is then automatically removed during the generalize configuration pass" ([Boot Windows to audit mode or OOBE](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/boot-windows-to-audit-mode-or-oobe)). `CustomerOobe` runs no generalize pass.
- The Enable and disable page says "For new installations, after the end user creates a user account in OOBE, the built-in Administrator account is disabled" (same page as above).

deployarr relies on none of these statements:

- In both end states, the agent turns off the built-in Administrator account after the sequence's last reboot and before it reports success. It waits for the last reboot because, by the auditSystem page, a boot into audit mode turns the account on again (inferred).
- The agent finds the account by its well-known relative ID, 500, not by its name. The account's SID is `S-1-5-domain-500`, and "it can be renamed" ([Security identifiers](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-security-identifiers)).
- The agent then reads the account's state back. If the account is still on, the deployment fails. The Security identifiers page also says that the account "can't be deleted, disabled, or locked out". That conflicts with the Enable and disable page, which Microsoft wrote for system builders. deployarr follows the Enable and disable page, and the read-back catches an account that stays on.
- An account that is already off is not an error, so the step is safe to run again after a crash.
- The M3 lab reads the account's state after each run. In `CustomerOobe`, it reads the state again after the customer's OOBE.

The unattend file never involves the account:

- The local admin account that the `ReadyToUse` unattend file creates is a separate account, in `UserAccounts | LocalAccounts`. It is never the built-in Administrator account.
- No unattend file that deployarr writes sets `AutoLogon` or turns on the built-in Administrator account. The agent runs as a service, so it needs no signed-in user. The auditSystem page says "It is not necessary to explicitly enable the built-in Administrator account or specify an autologon of the account using an answer file in order to run commands in the auditSystem and auditUser passes. Doing so can prevent the image or device from entering the Out-Of-Box Experience (OOBE) successfully." The AutoLogon page says "Make sure AutoLogon is disabled on computers that are delivered to customers" ([AutoLogon](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-autologon)).

### Answer files

The agent writes the unattend file to one place. In WinPE, it writes `\Windows\Panther\unattend.xml` in the applied image. Microsoft's steps for an offline image put the answer file at that path, and "When Windows Setup runs, it finds and uses this answer file" ([Windows Setup automation overview](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/windows-setup-automation-overview)). MDT copies its unattend file to the same path (`scripts/LTIApply.wsf:1032-1036` in the [MDT 8456 sources](research/mdt-8456.md#sources)). The same Microsoft page says of `%WINDIR%\Panther` "Do not use, modify, or overwrite the answer file in this directory." A freshly applied image has no answer file there yet, so the agent's write replaces none (inferred).

The agent never writes an answer file to `\Windows\Panther\Unattend`. The same page says that an answer file there "will replace the cached answer file at the beginning of each configuration pass". It adds "As a result, the sensitive data for the specialize configuration pass is no longer cleared" and "Unless the cached answer file must be overridden, embed the answer files at a location that has a lower precedence."

Microsoft also says "Before you deliver the computer to a customer, you must delete the cached answer file in the %WINDIR%\panther directory" (same page). deployarr's answer file can hold the local admin password and a domain join password. Windows Setup searches several places for an answer file at the beginning of each configuration pass (same page). Sysprep "processes any settings in the oobeSystem configuration pass in an answer file before OOBE starts" ([Sysprep command-line options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/sysprep-command-line-options)). So in both end states, before the agent reports success, it deletes each answer file that the implicit search order can find on the PC's disk. It deletes these files even when deployarr did not write them:

- the file that the `HKLM\System\Setup\UnattendFile` value names, when it is on that disk, and the value itself
- `Unattend.xml` and `Autounattend.xml` in `%WINDIR%\Panther` and in `%WINDIR%\Panther\Unattend`
- `Unattend.xml` in `%WINDIR%\System32\Sysprep`
- `Unattend.xml` and `Autounattend.xml` at the root of `%SYSTEMDRIVE%`

Cleanup follows these rules:

- A file that is already gone is not an error, so cleanup is safe to run again after a crash. A file that cannot be deleted fails the deployment.
- The search order's table says that Setup searches `%WINDIR%\Panther\Unattend` "only on downlevel installations". Boot Windows to audit mode or OOBE still puts an answer file there in an offline image, and Windows then boots into audit mode. So cleanup covers that folder too.
- Setup also searches the root of removable media for `Autounattend.xml`. The boot media holds no `Unattend.xml` or `Autounattend.xml` at its root.

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
- A join kind from an add-on adds no code path of its own. It names a profile form and, for each stage of the `ApplyJoinProfile` join slot, one of the add-on's step kinds. The stages are the offline image in WinPE and the full OS. The built-in Windows client template holds an `ApplyJoinProfile` join slot for each stage. When a deployment is planned, each one expands to the step kind that the profile's join kind names for that stage, or to nothing. `Workgroup` and `Domain` expand to nothing, because the unattend file does the join. See [Extension points](#extension-points).
- A join kind can name the end states it needs. The server rejects any other pair before the disk changes.
- A Home image can use only `Workgroup`. Microsoft lists "The ability to join a domain or Microsoft Entra ID" among the features that Pro adds to Home ([Upgrade Windows Home to Windows Pro](https://support.microsoft.com/en-us/windows/upgrade-windows-home-to-windows-pro-ef34d520-e73f-3198-c525-d1a218cc2818)). Entra join works on Windows 11 "except for Home editions", and bulk enrollment is one way to do it ([Microsoft Entra joined devices](https://learn.microsoft.com/en-us/entra/identity/devices/concept-directory-join)). Autopilot's list of supported Windows 11 editions has no Home edition ([Autopilot requirements](https://learn.microsoft.com/en-us/autopilot/requirements)).
- The server rejects any profile but `Workgroup` for a Home image when a task sequence or a pre-staged record is saved, and again after lookup, before the disk changes. That covers `Domain` and every join kind from an add-on. A rule in the base needs no field in each add-on's manifest (extends the existing Home rule to add-on join kinds). An order column can set the profile, so the second check is the one that catches it. A sequence counts as Home when any of its `ApplyOsImage` steps names a Home image, because conditions pick the step only at run time. The error names the image's edition ID and the profile.
- A step can also set `JoinProfile`, with `SetVariable` or `var set`. The agent checks the new value when the step sets it, and fails the step if the image is Home. A step that runs before the disk steps keeps the disk unchanged. Without these checks, a domain join on Home would fail in the specialize pass, after the disk is wiped (inferred).
- A step that sets `JoinProfile` cannot change which steps an add-on's join kind added. Planning expands the `ApplyJoinProfile` join slots and freezes the program before the first step runs. A change at run time may therefore only switch between `Workgroup` and `Domain` profiles, which the unattend file applies. The agent fails a step that sets `JoinProfile` unless the planned profile and the new one are both `Workgroup` or `Domain` profiles, so the agent needs to know only the base kinds. This rule follows from expanding join kinds when the deployment is planned.

### Entra ID join add-on

`deployarr.entra-join` joins a PC through bulk enrollment. Its join kind's profile form holds the provisioning package as a secret file setting. Its one step in the offline image expands to one `AddProvisioningPackage` action with that package as its secret input, and to no command. See [What runs on the PC](#what-runs-on-the-pc).

- An admin makes the provisioning package with Windows Configuration Designer and uploads it. The package joins the PC to the Entra tenant and enrolls it in Intune ([Bulk enrollment](https://learn.microsoft.com/en-us/intune/intune-service/enrollment/windows-bulk-enroll)). It carries a token for a `package_{GUID}` account in Entra ID (same page), so the server stores it encrypted as a secret.
- In WinPE, the agent adds the package to the applied image with `DISM /Image=W:\ /Add-ProvisioningPackage`, which works on an applied image ([DISM provisioning package options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-provisioning-package-command-line-options)). Windows applies a package that is embedded in the image during the OOBE pass ([How provisioning works](https://learn.microsoft.com/en-us/windows/configuration/provisioning-packages/provisioning-how-it-works)). The M8 lab checks that a `ReadyToUse` PC, which skips the OOBE screens, applies it too. If it does not, the join kind names a full-OS step instead, which expands to `AddProvisioningPackage`. In the full OS the agent runs `DISM /Online /Add-ProvisioningPackage`, which adds a package to the running Windows (same DISM page). The fallback is a built-in action too, because no command that an add-on writes gets a secret. That this online install joins the PC is inferred.
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
- The TPM check passes when the agent reports 2.0, from either source in [TPM version](#tpm-version). In base WinPE the version comes from the TPM device's IDs, and the device counts only when it has started with no problem code.
  - A started TPM device with no problem code passes the default check when it lists `ACPI\MSFT0101` as a hardware ID, as a TPM with the `_HID` `MSFT0101` does. It also passes when it lists `ACPI\MSFT0101` as a compatible ID and no hardware ID from the table, as a TPM with a vendor `_HID` does. [TPM version](#tpm-version) gives the rule for a device that lists IDs for both versions.
  - A TPM device with a problem code, or one that has not started, does not count. With no other counted device, the TPM version is unknown, and the PC fails the default check.
  - A TPM that lists no ID from the table reads as unknown and fails the check too.
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
4. Late in the sequence, `VerifyDrivers` lists every device with a problem code through `CM_Get_DevNode_Status`. The agent sends each device's hardware IDs to the server. If the library has a matching package that was not injected, the agent installs it with `pnputil /add-driver <inf> /install`, then checks again. This catches devices that WinPE could not see. Windows Update drivers are later work. The first release does not try them, and the [Windows Update](#windows-update) step searches only for software updates. `MissingDriverPolicy` holds `Fail`, `Warn`, and `Ignore`.
5. The step then applies its policy: fail the deployment, warn, or ignore. The UI lists missing drivers by PC type, with hardware IDs, so the next deployment of that model succeeds.

## OS images

- To import an ISO, the server extracts `sources\install.wim` or `sources\install.esd` with 7-Zip. 7-Zip reads the UDF file system that Windows ISOs use and runs on Windows and Linux. The server also keeps `sources\boot.wim` as a possible WinPE base, plus the PXE boot files.
- To import a WIM or ESD, the server takes the file directly.
- `wimlib-imagex info` lists the editions in the image. The server exports each chosen edition into its own WIM with `wimlib-imagex export`, so the agent downloads only that edition.
- `wimlib-imagex info` also prints each edition's ID, from the `EDITIONID` element of the image's XML, and `wimlib-imagex export` keeps it (measured with wimlib 1.14.5). The server stores the ID with the exported image. An edition ID that starts with `Core` marks a Home image. In the Windows 11 ISO that the lab uses, Home, Home N, and Home Single Language are `Core`, `CoreN`, and `CoreSingleLanguage`, and Pro is `Professional`. An image with no edition ID counts as Home for the [directory join](#directory-join) rule and for the [`EnableBitLocker`](#home-and-other-limits) rule.
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

## BitLocker

deployarr turns BitLocker on with the `EnableBitLocker` action and suspends it with the `SuspendBitLocker` action. They match ConfigMgr's Enable BitLocker and Disable BitLocker steps. ConfigMgr's third step, Pre-provision BitLocker, has no counterpart ([ConfigMgr 2509 reference](research/configmgr-2509.md#bitlocker)).

- Both actions run only in the full OS, on the volume of the running Windows.
- deployarr's disk layouts have one Windows volume and no data volume, so neither action has a drive setting.
- For BitLocker, the system drive "must not be encrypted" and "must differ from the operating system drive". It must use FAT32 on UEFI and NTFS on BIOS ([BitLocker overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/)). The EFI partition of the UEFI layout and the System partition of the BIOS layout meet these rules. The same page recommends a system drive of "approximately 350 MB", more than the UEFI layout's 300 MB EFI partition. [Disk layout and boot files](#disk-layout-and-boot-files) gives the BIOS System partition no size.
- The built-in Windows client template has no BitLocker step, so an admin adds one to a sequence that needs it (design decision). With a BitLocker step, the template could not build a Home sequence, because of the Home rule in [Home and other limits](#home-and-other-limits). MDT's template has two BitLocker steps ([MDT 8456 reference](research/mdt-8456.md#bitlocker)).
- Both actions belong to M7 in the [roadmap](roadmap.md#m7-applications) (design decision). They need the full-OS handoff and the lab's UEFI VM with a TPM, which M3 brings, and M7 keeps them off the path from M3 to order lookup in M5, which the owner ranks high (see [Requirements](#requirements)).

### No pre-provisioning in WinPE

MDT and ConfigMgr can start encryption in WinPE, before the new Windows first starts. By default both run `manage-bde.exe -on <drive> -used` there, with no key protector ([MDT 8456 reference](research/mdt-8456.md#pre-provisioning-in-winpe), [ConfigMgr 2509 reference](research/configmgr-2509.md#pre-provision-bitlocker)). That needs `manage-bde.exe` from the `WinPE-SecureStartup` component, and the WMI service and the `Win32_EncryptableVolume` provider from `WinPE-WMI`. Base WinPE has none of them ([ConfigMgr 2509 reference](research/configmgr-2509.md#what-pre-provisioning-needs-in-winpe)).

The owner chose a boot image of base WinPE, without `WinPE-WMI` or `WinPE-SecureStartup` ([#51](https://github.com/Tsukidy/deployarr/issues/51#issuecomment-6816910755)). The owner also chose no pre-provisioning ([#156](https://github.com/Tsukidy/deployarr/issues/156#issuecomment-6956569307)). So:

- deployarr enables BitLocker only in the full OS. No action pre-provisions it in WinPE.
- No BitLocker action allows `run_in` `WinPe`. A BitLocker step therefore never needs an optional component, and the rule in [Boot image](#boot-image) never rejects a sequence because of one.
- Base WinPE has `fveapi.dll` and `fvevol.sys`. Nobody has tested them for pre-provisioning, and deployarr does not use them.

### How the agent reaches BitLocker

The agent calls the `Win32_EncryptableVolume` WMI class in `root\CIMV2\Security\MicrosoftVolumeEncryption` (design decision). The other choice runs `manage-bde.exe` and parses its text. The class gives typed results, and the agent never has to see the recovery password, as [Enable BitLocker](#enable-bitlocker) shows. MDT's `ZTIBde.wsf` and ConfigMgr's `tscore.dll` use the same class ([MDT 8456 reference](research/mdt-8456.md#enabling-in-the-full-os), [ConfigMgr 2509 reference](research/configmgr-2509.md#bitlocker)).

- Each method returns a status code, and the volume's state comes back as numbers. The agent checks every code and reads the state back without parsing text.
- The agent calls WMI through COM, and only in the full OS, as [Agent](#agent) says. It gets the WMI locator from `CoCreateInstance` at run time ([Example: Getting WMI Data from the Local Computer](https://learn.microsoft.com/en-us/windows/win32/wmisdk/example--getting-wmi-data-from-the-local-computer)). COM adds `ole32.dll` and `oleaut32.dll` to the import table, and base WinPE's [list file](research/winpe-10.1.26100.9457.md#the-list-file) holds both.
- These calls work only in the full OS. Base WinPE has no WMI service ([ConfigMgr 2509 reference](research/configmgr-2509.md#what-pre-provisioning-needs-in-winpe)). `ProtectKeyWithTPM` "is only applicable for the volume that contains the currently running operating system" ([ProtectKeyWithTPM](https://learn.microsoft.com/en-us/windows/win32/secprov/protectkeywithtpm-win32-encryptablevolume)).

### Enable BitLocker

`EnableBitLocker` has four settings:

| Setting | Default | Meaning |
| --- | --- | --- |
| `method` | `OsDefault` | `OsDefault` leaves the method to Windows, which picks XTS-AES 128 on Windows 10 1511 and later ([Encrypt](https://learn.microsoft.com/en-us/windows/win32/secprov/encrypt-win32-encryptablevolume)). `Aes128`, `Aes256`, `XtsAes128`, and `XtsAes256` pass the `EncryptionMethod` values 3, 4, 6, and 7. ConfigMgr's steps take the same five choices ([ConfigMgr 2509 reference](research/configmgr-2509.md#encryption-methods)) |
| `full_disk` | `false` | `false` encrypts used space only. `true` encrypts the whole volume. `true` fails the step on a volume that is already encrypted in data-only mode |
| `wait_secs` | none | None ends the step while encryption goes on. A number of seconds waits until the volume is fully encrypted, and fails the step if that time passes first |
| `tpm_failure` | `Fail` | `Fail` fails the step when the PC has no usable TPM. `Skip` ends the step with a warning instead. The step has changed nothing by then. Any other TPM failure fails the step, whatever this setting says |

The default of `wait_secs` matches MDT and ConfigMgr, and the default of `tpm_failure` matches ConfigMgr. Neither product waits for encryption by default ([MDT 8456 reference](research/mdt-8456.md#waiting-for-encryption), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)). ConfigMgr's `SkipWhenNoValidTPM` is false by default, so a TPM failure fails its step (same ConfigMgr page).

The step adds two key protectors, and no others:

- A TPM protector, with `ProtectKeyWithTPM`. The agent passes no platform validation profile, so Windows uses its default ([ProtectKeyWithTPM](https://learn.microsoft.com/en-us/windows/win32/secprov/protectkeywithtpm-win32-encryptablevolume)).
- A recovery password, with `ProtectKeyWithNumericalPassword` and no password. "If no numerical password is specified, one is randomly generated" ([ProtectKeyWithNumericalPassword](https://learn.microsoft.com/en-us/windows/win32/secprov/protectkeywithnumericalpassword-win32-encryptablevolume)). The call returns only the new protector's ID.

deployarr leaves the recovery password only on the PC. The owner decided that deployarr keeps no copy of it, on its server or anywhere else, and that the client backs it up ([#156](https://github.com/Tsukidy/deployarr/issues/156#issuecomment-6956569307)). So:

- The agent never calls `GetKeyProtectorNumericalPassword`, the method that reads the password ([GetKeyProtectorNumericalPassword](https://learn.microsoft.com/en-us/windows/win32/secprov/getkeyprotectornumericalpassword-win32-encryptablevolume)). The password therefore never reaches the agent, a log line, an event, a file, or the server.
- A command that an admin writes can still print the password. The agent cannot redact it, because the agent never reads the value.
- No setting or variable supplies a password. MDT and ConfigMgr each take one from `OSDBitLockerRecoveryPassword` ([MDT 8456 reference](research/mdt-8456.md#recovery-key), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).
- deployarr sends the password to no store, AD DS included. MDT writes it in plain text to a file on the key drive ([MDT 8456 reference](research/mdt-8456.md#recovery-key)). ConfigMgr escrows it to AD DS (inferred in the research), to its own database, or to both ([ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).
- The agent writes no BitLocker policy values. MDT writes them only for the modes that need a PIN or a startup key ([MDT 8456 reference](research/mdt-8456.md#enabling-in-the-full-os)). ConfigMgr writes them for every mode and for its AD DS backup ([ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).
- The step result names each protector's ID and says that deployarr kept no copy of the recovery password. The client backs the password up from the PC. For example, `manage-bde -protectors -adbackup C: -id <protector ID>` backs it up to AD DS, and `-aadbackup` with the same `-id` backs it up to Microsoft Entra ID ([manage-bde protectors](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/manage-bde-protectors)).

The first release adds no protector that needs a PIN or a startup key. With either one, "BitLocker can lock the normal startup process until the user supplies a personal identification number (PIN) or inserts a removable device that contains a startup key" ([BitLocker overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/)). Every restart after the step would then wait for a person, which zero-touch deployment rules out. MDT and ConfigMgr offer both ([MDT 8456 reference](research/mdt-8456.md#enabling-in-the-full-os), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)). [Not in the first release](#not-in-the-first-release) lists them.

#### Calls

The step makes these calls in order. Each call checks the volume first or is safe to repeat, so a run after a crash ends in the same state:

1. `GetConversionStatus`. A volume that is decrypting, status 3 or 5, fails the step, as in ConfigMgr ([GetConversionStatus](https://learn.microsoft.com/en-us/windows/win32/secprov/getconversionstatus-win32-encryptablevolume), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)). With `full_disk` set to `true`, a volume that is already encrypted in data-only mode fails the step too. `GetConversionStatus` reports that mode in its `EncryptionFlags` value (same Microsoft page).
2. `ProtectKeyWithTPM`, unless `GetKeyProtectors` lists a TPM protector. The agent reads two answers from `GetKeyProtectors` as a volume with no protectors. One is `S_OK` with an empty list. The other is `FVE_E_NOT_ACTIVATED`, "BitLocker is not enabled on the volume. Add a key protector to enable BitLocker." ([GetKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/getkeyprotectors-win32-encryptablevolume)). [TPM failures](#tpm-failures) says what a failed call does.
3. `ProtectKeyWithNumericalPassword`, unless `GetKeyProtectors` lists a numerical password.
4. `Encrypt`. The agent passes the method. Unless `full_disk` is `true`, it also passes flag 1, which asks for data-only encryption. `Encrypt` starts encryption on a decrypted volume. It resumes encryption on a paused or running volume, and it returns 0 on a fully encrypted one. When `method` is not `OsDefault` and the volume is already partly or fully encrypted with another method, it returns `FVE_E_CANNOT_SET_FVEK_ENCRYPTED` ([Encrypt](https://learn.microsoft.com/en-us/windows/win32/secprov/encrypt-win32-encryptablevolume)), and the step fails.
5. `EnableKeyProtectors`. Protectors that the step adds while the volume's protectors are suspended "will remain disabled unless EnableKeyProtectors is called" ([DisableKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/disablekeyprotectors-win32-encryptablevolume)). `EnableKeyProtectors` turns them on, and it returns 0 when they are already on ([EnableKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/enablekeyprotectors-win32-encryptablevolume)).
6. The read-back. The volume must list a TPM protector and a numerical password. `GetConversionStatus` must report 1 or 2, fully encrypted or encryption in progress. `GetSuspendCount` must return `ERROR_NOT_SUPPORTED`, which on the OS volume means that the volume is not suspended ([GetSuspendCount](https://learn.microsoft.com/en-us/windows/win32/secprov/getsuspendcount-win32-encryptablevolume)). Otherwise the step fails.
7. The wait, when `wait_secs` is set. The agent reads the conversion status every 10 seconds, as ConfigMgr does, until it reports 1. `GetProtectionStatus` must then report 1, protection on ([GetProtectionStatus](https://learn.microsoft.com/en-us/windows/win32/secprov/getprotectionstatus-win32-encryptablevolume)). A status other than 1 or 2, or the end of the wait, fails the step. MDT's wait and ConfigMgr's `/wait` have no time limit ([MDT 8456 reference](research/mdt-8456.md#waiting-for-encryption), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).

A step that cannot do its work fails, unless `tpm_failure` says to skip, and the step result says why. MDT's script exits with success when its tools are missing or the OS is too old, and both BitLocker steps in its template continue on error ([MDT 8456 reference](research/mdt-8456.md#mdt-faults-that-deployarr-fixes)).

- When the step fails, the agent leaves the protectors and any encryption that the step started in place (design decision). A later run of the step goes on from there, while a rollback would be one more change that a crash could interrupt. ConfigMgr's `BitLocker::Rollback` deletes the protectors that its step added and can decrypt the volume ([ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).
- "If the computer turns off while encryption is in progress, encryption will automatically resume when the computer restarts" ([Win32_EncryptableVolume](https://learn.microsoft.com/en-us/windows/win32/secprov/win32-encryptablevolume)). After a crash, the step runs again from call 1, and its wait starts again with the full `wait_secs` (design decision). The agent then needs no wait state in the state file.

#### TPM failures

- `tpm_failure` decides only when the PC has no usable TPM. The agent counts `TBS_E_SERVICE_NOT_RUNNING` as that case, because the page describes it as "No compatible TPM is found on this computer" ([ProtectKeyWithTPM](https://learn.microsoft.com/en-us/windows/win32/secprov/protectkeywithtpm-win32-encryptablevolume)). The M7 check records the code that a PC without a TPM returns. If that code differs, it joins this case (design decision).
- TPM 2.0 "is not supported in Legacy and Compatibility Support Module (CSM) modes of the BIOS" ([BitLocker overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/)). A BIOS PC with TPM 2.0 therefore has no usable TPM for BitLocker. Nobody has checked which code `ProtectKeyWithTPM` returns there.
- When the call returns `0x80310030`, the step fails, and its result says to remove the boot media. MDT and ConfigMgr both read that code as bootable media left in the PC, and both ask a person to remove it ([MDT 8456 reference](research/mdt-8456.md#enabling-in-the-full-os), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)). deployarr's boot ISO goes on USB sticks ([Boot image](#boot-image)), so a stick left in the PC can cause it (inferred).
- Any other failure of the call fails the step, whatever `tpm_failure` says.
- The agent never takes ownership of the TPM. MDT and ConfigMgr take ownership when the TPM has none ([MDT 8456 reference](research/mdt-8456.md#tpm-checks), [ConfigMgr 2509 reference](research/configmgr-2509.md#enable-bitlocker)).
- Windows provisions the TPM itself. "Windows takes ownership of the TPM as part of the provisioning process on each boot", and from Windows 10 1607 on it does not keep the owner password ([Change the TPM owner password](https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/change-the-tpm-owner-password)).
- Nobody has checked that Windows has provisioned the TPM by the time the step runs. The M7 check records whether `ProtectKeyWithTPM` succeeds on the UEFI VM and on the real PC.

#### Used space and full disk

- Used space only is the default, as in ConfigMgr. Microsoft's page for ConfigMgr's Enable BitLocker step says "This default behavior is recommended, as it's faster and more efficient" ([Task sequence steps](https://learn.microsoft.com/en-us/intune/configmgr/osd/understand/task-sequence-steps)).
- MDT's full-OS step encrypts the whole of a volume that WinPE did not pre-provision ([MDT 8456 reference](research/mdt-8456.md#enabling-in-the-full-os)).
- With used space only, "The sections of the volume containing used space will be encrypted but the free space will not" ([manage-bde on](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/manage-bde-on)). Data that a reused disk held before the deployment can stay readable in that free space (inferred). `full_disk` covers that case when the step starts the encryption.
- Flag 1 "only has effect" when encryption starts "from the fully decrypted state, decryption in progress state, or decryption paused state". `Encrypt` also returns 0 on a fully encrypted volume ([Encrypt](https://learn.microsoft.com/en-us/windows/win32/secprov/encrypt-win32-encryptablevolume)). On a volume that is already encrypted in data-only mode, `full_disk` would therefore do nothing. The step fails instead, so the setting never does nothing silently (design decision). The other choice is flag 2, which wipes the free space of an encrypted volume (same page).

#### Device encryption

- Windows can start encryption by itself. When OOBE finishes, "device encryption is initialized on the OS drive and fixed data drives on the computer with a clear key that is the equivalent of standard BitLocker suspended state" ([BitLocker overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/)).
- Starting in Windows 11 24H2, "more devices are eligible for automatic and manual device encryption" (same page). The step can therefore find the volume encrypting or encrypted.
- Calls 4 and 5 handle that state. The clear key matches a suspended state, and protectors added then stay off until call 5 ([DisableKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/disablekeyprotectors-win32-encryptablevolume)). `EnableKeyProtectors` "ensures that the volume's encryption key is not exposed in the clear on the hard disk" ([EnableKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/enablekeyprotectors-win32-encryptablevolume)). That it also ends device encryption's clear key is inferred.
- When device encryption used a method other than `method`, call 4 fails. When it encrypted used space only and `full_disk` is `true`, call 1 fails.
- The M7 check records the volume's state when the step starts.

#### Home and other limits

- Microsoft lists Pro, Enterprise, Pro Education/SE, and Education as the editions that support BitLocker enablement ([BitLocker overview](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/)). Home is not among them.
- When a sequence is saved, the server rejects an enabled `EnableBitLocker` step if the sequence counts as Home by the rule in [Directory join](#directory-join). The error names the image's edition ID.
- The rule reads every `ApplyOsImage` step, because conditions pick the image only at run time. A sequence that names a Home image in any `ApplyOsImage` step therefore cannot hold an enabled `EnableBitLocker` step, even behind a condition.
- A disabled `EnableBitLocker` step does not count, because the compiler drops it (design decision).
- Neither a variable nor an add-on can add the step, so the server needs no second check after lookup (design decision). `BuiltinAction::allowed_from_addon` is false for both BitLocker actions, so an add-on step cannot expand to one. See [What runs on the PC](#what-runs-on-the-pc).
- Nobody has checked whether `sysprep /oobe` runs while BitLocker protects the OS volume. The `CustomerOobe` end state runs it last ([End state](#end-state)). The M7 check records it. If Sysprep refuses, the server must reject `EnableBitLocker` for `CustomerOobe`, and this section changes.
- The compiler rejects a `Restart` into WinPE after an `EnableBitLocker` step. See [Compile the tree into a flat program](#compile-the-tree-into-a-flat-program).
- An importer that turns MDT's Enable BitLocker step into `EnableBitLocker` must respect that rule. MDT's `Client.xml` restarts into WinPE with `smsboot.exe /target:WinPE` in its Capture Image group, after its Enable BitLocker step (`Templates/Client.xml:398-414`, `Templates/Client.xml:460`, `Templates/Client.xml:480-487`, relative to `Source/MDT.8456` in the [MDT 8456 sources](research/mdt-8456.md#sources)).

### Suspend BitLocker

`SuspendBitLocker` turns the volume's key protectors off for a number of restarts. It never decrypts. ConfigMgr calls the same step Disable BitLocker, and that step never decrypts either ([ConfigMgr 2509 reference](research/configmgr-2509.md#disable-bitlocker)). deployarr names the action for what it does.

- When a startup component that the TPM protector's profile measures changes while protection is on, "the TPM will not release the encryption key to unlock the disk volume and the computer will enter into recovery mode" ([ProtectKeyWithTPM](https://learn.microsoft.com/en-us/windows/win32/secprov/protectkeywithtpm-win32-encryptablevolume)).
- The same page says that the default profile "secures the encryption key against changes to the Core Root of Trust of Measurement (CRTM), BIOS, and Platform Extensions (PCR 0)". By that page, a firmware update is such a change. A `SuspendBitLocker` step before it keeps the PC out of recovery.
- The one setting, `restarts`, is the number of restarts after which protection resumes. It holds 1 to 15, and the default is 1. ConfigMgr's step takes "a value between 1 (default) and 15" ([Task sequence steps](https://learn.microsoft.com/en-us/intune/configmgr/osd/understand/task-sequence-steps)).
- `manage-bde` also takes 0, which suspends protection indefinitely ([manage-bde protectors](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/manage-bde-protectors)). `DisableKeyProtectors` "exposes the volume's encryption key in the clear on the hard disk" ([DisableKeyProtectors](https://learn.microsoft.com/en-us/windows/win32/secprov/disablekeyprotectors-win32-encryptablevolume)). A count of 0 lasts until something calls `EnableKeyProtectors`, so a deployment with no later step that does would ship the PC with its key in the clear. deployarr leaves 0 out (design decision).
- When the volume has no key protector, the step changes nothing, and its result says why.
- Otherwise the agent calls `DisableKeyProtectors` with the count. It reads the count back with `GetSuspendCount`, which gives 0 to 15 for a suspended OS volume ([GetSuspendCount](https://learn.microsoft.com/en-us/windows/win32/secprov/getsuspendcount-win32-encryptablevolume)). The step fails unless it reads back `restarts`.
- A run after a crash calls `DisableKeyProtectors` again. That this sets the count again on a volume that is already suspended is inferred. If it does not, the read-back fails the step.

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

## Windows Update

The `InstallUpdates` action installs software updates from Windows Update, or from a WSUS server that the step names. MDT runs `ZTIWindowsUpdate.wsf` for this ([MDT 8456 reference](research/mdt-8456.md#windows-update)), and ConfigMgr has its Install Software Updates step ([ConfigMgr 2509 reference](research/configmgr-2509.md#install-software-updates)). The owner chose how the step treats a failed update and the pass limit, where it gets updates, and where it sits in the built-in template ([#154](https://github.com/Tsukidy/deployarr/issues/154#issuecomment-6956567901)).

- The action runs only in the full OS, after the [handoff to the agent service](#handoff-from-winpe-to-the-new-os). ConfigMgr's step refuses WinPE ([ConfigMgr 2509 reference](research/configmgr-2509.md#flow)). MDT's script reboots out of WinPE instead of searching there ([MDT 8456 reference](research/mdt-8456.md#where-it-runs)).
- The agent drives the Windows Update Agent (WUA) API itself, as MDT's script does. ConfigMgr's step drives the ConfigMgr client instead, and deployarr has no such client ([ConfigMgr 2509 reference](research/configmgr-2509.md#update-source-and-filters)).
- Microsoft lists `Wuapi.dll` as the DLL behind WUA's interfaces, such as `IUpdateSearcher` ([IUpdateSearcher::get_ServerSelection](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdatesearcher-get_serverselection)). Base WinPE has no `wuapi.dll` ([list file](research/winpe-10.1.26100.9457.md#the-list-file)).
- The agent does not import `wuapi.dll`. It creates WUA's update session object, MDT's `Microsoft.Update.Session`, at run time with `CoCreateInstance`. `ole32.dll` exports `CoCreateInstance` ([CoCreateInstance](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-cocreateinstance)).
- WUA's methods take `BSTR` and `VARIANT` values, such as the search criteria ([IUpdateSearcher::Search](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdatesearcher-search)). The agent builds and frees them with `oleaut32.dll` functions, such as `SysAllocString` ([SysAllocString](https://learn.microsoft.com/en-us/windows/win32/api/oleauto/nf-oleauto-sysallocstring)) and `VariantClear` ([VariantClear](https://learn.microsoft.com/en-us/windows/win32/api/oleauto/nf-oleauto-variantclear)).
- `ole32.dll` and `oleaut32.dll` are both on base WinPE's list. The [import check](#agent) therefore passes. COM loads `wuapi.dll` only when the agent creates the object in the full OS (inferred).
- Steps in the full OS run as LocalSystem in session 0 (see [Handoff from WinPE to the new OS](#handoff-from-winpe-to-the-new-os)). Nobody has tested whether WUA searches, downloads, and installs for a caller in that context. The M7 check tests it.
- The built-in Windows client template holds one `InstallUpdates` step, enabled, after its `InstallApplications` step. The step does not continue on error, so a failed search fails the deployment (design decision). MDT's templates hold two such steps, one before Install Applications and one after. They ship both disabled, and both continue on error ([MDT 8456 reference](research/mdt-8456.md#where-it-runs)). A task sequence can turn the step off, for example for PCs that cannot reach their update source.

### Update settings

| Field | Default | Effect |
| --- | --- | --- |
| `source` | `WindowsUpdate` | Where the step gets updates. `Wsus` names a WSUS server by its URL |
| `missing` | `Warn` | What the step does when updates are still missing after `max_passes` passes. See [Success and failure](#success-and-failure) |
| `max_passes` | 7 | How many passes may install updates. MDT's script also allows 7 ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)) |
| `exclude` | empty | KB article numbers and update IDs that the step never installs, like MDT's `WUMU_ExcludeKB` and `WUMU_ExcludeID` ([MDT 8456 reference](research/mdt-8456.md#settings)) |
| `timeouts` | 3,600 seconds to search, 7,200 to download, and 7,200 to install | How long one phase of one pass may take. The search default is ConfigMgr's wait for a scan ([ConfigMgr 2509 reference](research/configmgr-2509.md#waits)). The download and install limits are long, because they only have to stop a phase that hangs, and a slow link or a large update should not fail the step (design decision) |

### Update source

- With `WindowsUpdate`, the searcher's `ServerSelection` is `ssWindowsUpdate`, which searches "the Windows Update service over the Internet" ([ServerSelection](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-uamg/07e2bfa4-6795-4189-b007-cc50b476181a)). It searches Windows Update even when a policy on the PC, such as one that a domain applies, names a WSUS server (inferred from the same page). MDT's script searches without setting `ServerSelection` (`ZTIWindowsUpdate.wsf:263`), so it searches the default server, which follows a WSUS policy on the PC (inferred). deployarr takes `ssWindowsUpdate` so that the source is the one the step names (design decision).
- With `WindowsUpdate`, the step changes no policy value and never registers the Microsoft Update service, so the PC's update settings stay as Windows ships them. MDT registers Microsoft Update when no WSUS server is set ([MDT 8456 reference](research/mdt-8456.md#update-source)).
- With `Wsus`, the `ServerSelection` is `ssManagedServer`, which searches "only the managed server" (same page). The step writes the URL to `WUServer` and `WUStatusServer` under `HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate`, and 1 to `UseWUServer` in that key's `AU` subkey. These are the values MDT writes. The step then restarts the `wuauserv` service so that WUA reads them, as MDT does ([MDT 8456 reference](research/mdt-8456.md#update-source)).
- MDT also sets `AU\NoAutoUpdate` to 1, which turns Automatic Updates off for its step ([MDT 8456 reference](research/mdt-8456.md#update-source)). The step leaves it, so it changes only the values that choose the server (design decision). The trade-off is that Automatic Updates stays on and can scan or install while the step runs.
- With `Wsus`, before its first write, the step saves each value's old data, or the fact that the value did not exist, in the run state's `progress`. Each run of the step writes the values again, but it saves the old data only once.
- With `Wsus`, before the step records its final result, whatever that result is, it restores each value and restarts `wuauserv`. It writes the saved data back, or deletes a value that did not exist before. MDT leaves `WUServer`, `WUStatusServer`, and `UseWUServer` in the deployed OS ([MDT 8456 reference](research/mdt-8456.md#update-source)).

### Update passes

The step works in passes, and the run state's `progress` holds a pass count. Before each pass, the step compares the count with `max_passes`. While the count is lower, a pass does the following:

1. It adds one to the pass count and saves the run state.
2. It searches for `IsInstalled = 0 and IsHidden = 0 and Type = 'Software'`. `Type = 'Software'` leaves out driver updates ([IUpdateSearcher::Search](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdatesearcher-search)), because drivers come from the driver library. See [Injection and verification](#injection-and-verification).
3. It skips each update that `exclude` names, and each update whose `InstallationBehavior.CanRequestUserInput` is true ([CanRequestUserInput](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iinstallationbehavior-get_canrequestuserinput)), because no one can answer a prompt. MDT skips the same updates ([MDT 8456 reference](research/mdt-8456.md#search-and-filters)).
4. If no update is left, the step ends with success.
5. It accepts the license terms of each update that is left. MDT's script accepts them for every update its search finds, the skipped ones included ([MDT 8456 reference](research/mdt-8456.md#search-and-filters)). The step then downloads the updates as one batch and installs them as one batch.
6. It reads each update's `ResultCode`, `HResult`, and `RebootRequired` ([IUpdateInstallationResult](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nn-wuapi-iupdateinstallationresult)). An update failed when its result code is not `orcSucceeded`, which is 2 ([OperationResultCode](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/ne-wuapi-operationresultcode)). An update that failed to download counts as failed (design decision). MDT's script only logs a failed download ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)). An error from the whole download or install call counts as a failure of every update in the batch, with that call's HRESULT (design decision).
7. If an update failed or needs a reboot, the step asks for a reboot and runs again after it. See [Reboots and resume](#reboots-and-resume). Otherwise the next pass starts at once, because an update that just installed can make more updates apply. MDT's script also searches again at once when nothing failed and nothing needs a reboot ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)).

When the count reaches `max_passes`, the step starts no more passes. It runs one last search with the same criteria and skips, and installs nothing. Each update that this search finds and does not skip is a missing update (design decision). MDT's script fails at its pass limit without that search ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)).

- The search, the download, and the install each start with WUA's asynchronous call, `BeginSearch`, `BeginDownload`, or `BeginInstall`. MDT's script uses the last two the same way ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)). When a phase passes its time limit, the agent calls the job's `RequestAbort` ([ISearchJob](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nn-wuapi-isearchjob), [IDownloadJob](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nn-wuapi-idownloadjob), [IInstallationJob](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nn-wuapi-iinstallationjob)) and fails the step with a timeout error. ConfigMgr's step waits up to an hour for a scan and has no limit on an install job ([ConfigMgr 2509 reference](research/configmgr-2509.md#waits)).
- The agent log records each update that a search finds, whether the step installs or skips it, the reason for a skip, and its result. MDT's log has one `INSTALL` or `SKIP` line per update, with its ID, title, KB numbers, and size, and gives no reason ([MDT 8456 reference](research/mdt-8456.md#search-and-filters)). ConfigMgr's step logs little and names five client logs to read instead ([ConfigMgr 2509 reference](research/configmgr-2509.md#logs)).

### Success and failure

| How the step ends | Result |
| --- | --- |
| A search succeeds and finds nothing to install | Success |
| The last search after `max_passes` passes finds missing updates | Set by `missing`. `Fail` fails the step. `Warn` succeeds with a warning. `Ignore` succeeds |
| A search fails | Failure |
| A phase passes its time limit | Failure |
| The agent cannot create WUA's objects, or cannot write or restore a policy value | Failure |

- A failed update never ends the step by itself. The step reboots and searches again until the pass limit, as MDT's script does. ConfigMgr's step fails at once with the failure's HRESULT ([ConfigMgr 2509 reference](research/configmgr-2509.md#results-reboots-and-exit-codes)).
- Only a search that succeeds and finds nothing means that the PC is up to date. MDT counts the search error `0x8024402C` as success, so a PC with no network looks up to date ([MDT 8456 reference](research/mdt-8456.md#search-and-filters)). A PC that cannot reach its update source therefore fails the step.
- The step result lists every update that a search found, with its update ID, title, and KB numbers. For each pass, it gives the update's result code and HRESULT, or the reason the step skipped it. It lists each failure, including one that a later pass fixed. It lists the missing updates under every `missing` value.
- Under `Warn`, the PC finishes, and the deployment shows the warning and the list of missing updates.
- A failed step goes through its `on_error`, like any other step. MDT's templates continue on error for both of their Windows Update steps ([MDT 8456 reference](research/mdt-8456.md#where-it-runs)).

### Reboots and resume

- When an update failed or waits for a reboot, the action returns `Reboot { rerun: true }`. The agent saves the run state, reboots, and runs the same step again. See [State on disk and resume](#state-on-disk-and-resume). MDT's script and ConfigMgr's step ask their engines for the same reboot and retry ([MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop), [ConfigMgr 2509 reference](research/configmgr-2509.md#results-reboots-and-exit-codes)).
- The action keeps the pass count, the update results so far, and the saved policy values in the run state's `progress` field until the step records its final result. A run after a reboot or a crash reads it. See [State on disk and resume](#state-on-disk-and-resume). MDT keeps its pass count in a variable that its engine drops on a retry, and the count survives only through a fallback file (inferred in [MDT 8456 reference](research/mdt-8456.md#download-install-and-the-reboot-loop)).
- A crash or a power loss during the step leaves `in_flight` set, and the action runs again. It reads its saved progress and starts a new pass from a new search, which finds only the updates still missing. It therefore needs no WUA job ID (inferred). ConfigMgr's step saves its job ID to reconnect to the job instead ([ConfigMgr 2509 reference](research/configmgr-2509.md#flow)).
- The pass count goes up at the start of each pass, so an interrupted pass still counts. A PC that crashes in every pass still reaches `max_passes`.
- The step restores the policy values that it saved in `progress`, not the values in the registry. A run after a crash therefore restores the values from before the step, not the ones the step wrote.

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
- For zero touch, deployarr sends `pxeboot.n12` on BIOS, and option 250 sub-option 11 = 2 for `wdsmgfw.efi` on UEFI. ConfigMgr sends these for a required deployment ([ConfigMgr 2509 reference](research/configmgr-2509.md#two-stage-boot-chain)).
  - Microsoft documents that `pxeboot.n12` "Immediately begins PXE boot (doesn't require pressing F12 on the client)" ([PXE boot in Configuration Manager](https://learn.microsoft.com/en-us/troubleshoot/mem/configmgr/os-deployment/understand-pxe-boot)). In the single-stage lab run, `pxeboot.n12` booted the `bios` VM to WinPE with no key pressed ([Single-stage boot chain in the PXE lab](research/pxe-lab/single-stage-chain.md#result)).
  - Sub-option 11 = 2 makes `wdsmgfw.efi` skip its key prompt (inferred). `sccmpxe.exe` sends that value only for a required deployment ([ConfigMgr 2509 reference](research/configmgr-2509.md#two-stage-boot-chain)). Its code only picks the sub-option, and the meaning of sub-option 11 comes from WDS header names ([ConfigMgr 2509 reference](research/configmgr-2509.md#deployment-decision)). Microsoft documents that `wdsmgfw.efi` "Handles prompting the user to press a key to continue PXE boot" ([PXE boot in Configuration Manager](https://learn.microsoft.com/en-us/troubleshoot/mem/configmgr/os-deployment/understand-pxe-boot)). It also documents that "Required deployments use PXE without any user intervention" ([Use PXE for OSD over the network](https://learn.microsoft.com/en-us/intune/configmgr/osd/deploy-use/use-pxe-to-deploy-windows-over-the-network)).
- In WinPE, the agent reads the DHCP reply that booted it from `HKLM\SYSTEM\ControlSet001\Control\PXE\BootServerReply` and logs which server sent it. The boot image's config still names the server. DHCP and TFTP carry no secrets. ConfigMgr hides a password in that reply under keys that are hard-coded in its binaries, so anyone who captures the reply can recover it.

### DHCP

- deployarr works only as a proxyDHCP server, because on one subnet it hears every PXE broadcast itself. The existing DHCP server keeps handing out addresses, and deployarr answers only PXE clients. That server needs no options 66 and 67. An admin chooses interfaces by MAC address. deployarr binds UDP 4011 on each chosen interface address, and it ties each of its UDP 67 sockets to one chosen interface, so it knows which address to put in its replies. The bullets on port 67 sockets below say how it binds port 67. A setting skips port 67, so deployarr can run on a host whose own DHCP server serves a chosen interface.
- deployarr answers a DISCOVER or a REQUEST that carries option 93 (client architecture), with or without option 97 (machine GUID). It ignores INFORM. On port 67 it answers a REQUEST only when the REQUEST carries option 250, which a WDS network boot program sends in its second request. It does not answer the REQUEST with which a PC accepts the DHCP server's lease. A firmware's boot file REQUEST must therefore reach port 4011. ConfigMgr differs on both points. It drops a request without option 97. On port 67 it answers any PXE REQUEST, the lease REQUEST included ([ConfigMgr 2509 reference](research/configmgr-2509.md#requests-it-answers)).
- The port 67 rule keeps PXE working when deployarr and the DHCP server answer from one address. deployarr does not support sharing port 67 with that DHCP server, as a bullet below says. The rule stays as [#55](https://github.com/Tsukidy/deployarr/issues/55) decided it. An ACK to the lease REQUEST from that address carries the same option 54 as the DHCP server's ACK, with `yiaddr` 0.0.0.0. The review of the PXE prototype for [#24](https://github.com/Tsukidy/deployarr/issues/24) tested this case, with the prototype and dnsmasq 2.91 on one address. [#55](https://github.com/Tsukidy/deployarr/issues/55) records the results. With a scripted client, the prototype's ACK arrived first in 20 of 20 trials. OVMF 2024.02 took the prototype's ACK as the DHCP server's. It sent DHCPDECLINE and stopped with `PXE-E07`. With the lease REQUEST unanswered, OVMF sent its PXE REQUEST to port 4011. It then downloaded the network boot program. In the same review, the iPXE ROM of a SeaBIOS guest also sent its PXE REQUEST to port 4011 ([#56](https://github.com/Tsukidy/deployarr/issues/56)). A firmware that sends its boot file REQUEST only to port 67 gets no boot file. No firmware tested so far does that. If a lab run finds one, the owner reopens [#55](https://github.com/Tsukidy/deployarr/issues/55).
- deployarr answers a request without option 97 because the option 97 condition adds no wipe safety. The PXE reply only decides whether a PC boots WinPE. [Wipe safety](#wipe-safety) still decides whether deployarr erases the PC's disk. It recognizes a finished PC by its MAC address, which every request carries. Some boot ROMs send no option 97. With ipxe-qemu 1.21.1 on QEMU 8.2.2, a SeaBIOS guest sends none on the default `pc` and `q35` machine types, even when it has an SMBIOS UUID ([#56](https://github.com/Tsukidy/deployarr/issues/56)). OVMF sends option 97. A reply to a request without option 97 carries none. RFC 4578 section 2.3 says a PXE-compliant client or server must send option 97 in every DHCP and PXE packet. Neither that request nor the reply complies. The rule changes nothing for compliant firmware, which sends option 97.
- The OFFER sets `siaddr` to deployarr's address and carries options 53, 54, 97 (the request's type byte and GUID, when its option 97 parses), 60 (`PXEClient`), and 82 (echoed when present). It carries no boot file and no option 43. Only the ACK to a REQUEST carries the boot file, in the BOOTP `file` field, with option 250 or option 252.
- deployarr parses option 97 where the request enters, as a type byte and a 16-byte GUID. Every reply to that request carries those 17 bytes as its option 97. RFC 4578 section 2.3 defines this layout, and its only type, 0, has a length of 17. A request whose option 97 has any other length counts as a request without option 97, so the rules above for such a request apply. Its reply carries no option 97, and [Wipe safety](#wipe-safety) checks its MAC address in `chaddr` alone. `sccmpxe.exe` differs. It always sends 17 bytes, copied from the start of the request's option 97 value, whatever length the request gave. A shorter option 97 gets back the bytes that follow it in the request, then zeros past the request's end. A longer one gets back only its first 17 bytes ([ConfigMgr 2509 reference](research/configmgr-2509.md#reply-contents)).
- The design sets three reply header fields. `op` is 2, `siaddr` is deployarr's address, and an ACK's `file` holds the boot file. The other header fields are not decided yet. `sccmpxe.exe` copies the other header bytes from the request, such as `xid`, `flags`, `giaddr`, and `chaddr`, with two exceptions. It writes a NUL into the last byte of `sname` and of `file`. A reply can also carry the two bytes of `secs` swapped ([ConfigMgr 2509 reference](research/configmgr-2509.md#reply-contents)).
- deployarr pads every reply, OFFER and ACK alike, to at least 300 bytes. It fills the bytes after the end option (255) with zeros. RFC 2132 section 3.2 says the octets after the end option should be filled with pad options, and section 3.1 gives the pad option as the single octet 0. RFC 1542 section 2.1 calls 300 octets the minimal BOOTP header. It says a receiver should check for that size, and it must silently discard a message that fails the check. `sccmpxe.exe` pads nothing, so its OFFER without option 82 is 280 bytes ([ConfigMgr 2509 reference](research/configmgr-2509.md#reply-contents)).
- Replies go to the request's source address and port. deployarr broadcasts only when the source is `0.0.0.0`, and it answers a relayed request by unicast to the relay. On routed networks, a DHCP relay (IP helper) forwards PXE clients to deployarr.
- The first release supports PXE on one subnet, and the lab tests only that. The relay handling above stays in the design, so routed subnets can come later without a redesign. Before they do, the design must choose where a relayed reply goes. deployarr sends it to the request's source address, as ConfigMgr does. RFC 2131 section 4.1 sends it to port 67 at the relay address in `giaddr`. The two differ when a relay sends from another interface (inferred).
- deployarr offers the [boot chain](#boot-chain) to every request that the rules above accept, except one from a PC whose deployment succeeded. [Wipe safety](#wipe-safety) names the keys that recognize that PC. A finished PC gets no reply on UEFI, so the firmware moves on to the next boot device. On BIOS it gets `abortpxe.com`.
- The reply does not depend on an order, because the service cannot see one. A PXE request carries no baseboard serial and no order number. With the SQL order lookup add-on, the order table matches a PC by its baseboard serial, which the agent reads in WinPE, or by an order number that a technician types in WinPE. The [lookup chain](#lookup-chain) decides what a PC runs once it boots WinPE. [Wipe safety](#wipe-safety) still holds, so deployarr erases that PC's disk only after an identity match, a typed order number, or a picked task sequence.
- ConfigMgr looks a PC up through its management point by the option 97 GUID and the hardware address before it replies. It answers an unknown PC only when unknown computer support is on, and then with the deployments for unknown computers ([ConfigMgr 2509 reference](research/configmgr-2509.md#deployment-decision)). deployarr answers an unknown PC as ConfigMgr does with that support on and a required task sequence deployed to unknown computers. A finished PC gets what ConfigMgr sends a PC with no deployment.
- deployarr does not share UDP 67 on an interface that a DHCP server on the same host serves. On such a host, the admin turns on the setting that skips port 67, and that DHCP server sends option 60 `PXEClient`. The PXE REQUEST then reaches deployarr on port 4011. Microsoft gives the same steps for ConfigMgr's PXE responder on a DHCP server. They set `DoNotListenOnDhcpPort` to 1 and DHCP option 60 to `PXEClient`, then restart the `SCCMPXE` and DHCP services ([Use PXE for OSD over the network](https://learn.microsoft.com/en-us/intune/configmgr/osd/deploy-use/use-pxe-to-deploy-windows-over-the-network)). dnsmasq 2.91 sends the option with `--dhcp-option-force=60,PXEClient` ([triage on #106](https://github.com/Tsukidy/deployarr/issues/106#issuecomment-6855356614)). No lab run has yet tested whether firmware sends its PXE REQUEST to port 4011 when only the DHCP server's OFFER carries option 60. The [M4 check](roadmap.md#m4-pxe) tests it on both VM types.
- deployarr does not share the port, because a shared port can take the DHCP server's traffic. In the [triage on #106](https://github.com/Tsukidy/deployarr/issues/106#issuecomment-6855356614), on Linux 6.18 with dnsmasq 2.91, a test socket that shared an interface with dnsmasq took every unicast lease renewal on that interface when dnsmasq started first. dnsmasq got none. On Windows, Microsoft says that once a second socket binds a port with `SO_REUSEADDR`, "the behavior for all sockets bound to that port is indeterminate" ([Using SO_REUSEADDR and SO_EXCLUSIVEADDRUSE](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)).
- On Linux, deployarr binds two port 67 sockets for each chosen interface. The measurements below come from the [triage on #106](https://github.com/Tsukidy/deployarr/issues/106#issuecomment-6855356614).
  - One binds the wildcard address `0.0.0.0`, and `SO_BINDTODEVICE` limits it to the interface. It receives the broadcasts, such as a DISCOVER. A socket bound to the interface address receives no broadcast DISCOVER. In the triage runs, a lone socket bound to the interface address got 0 of 10 broadcast DISCOVERs, and a lone wildcard socket got all 10.
  - The other binds the interface address. It receives the unicasts to that address. A wildcard socket limited to the interface can lose them. In two triage runs, such a socket got 0 of 20 unicasts to its own address. A dnsmasq that served only two other interfaces had bound the wildcard address with `SO_REUSEPORT` after it. A socket bound to the interface address got every unicast to that address in every start order.
  - In the triage runs, test sockets set up like these two and a `bind-dynamic` dnsmasq that served one or two other interfaces each got all of their traffic, in either start order.
- On Linux, both port 67 sockets set `SO_REUSEADDR`, and neither sets `SO_REUSEPORT`.
  - With `SO_REUSEADDR`, a DHCP server that also sets it and serves only other interfaces of the host starts in either order and keeps all of its traffic. Without it, such a server fails to start when deployarr starts first. In the triage runs, a `bind-dynamic` dnsmasq on one or two other interfaces failed to start while a test socket without `SO_REUSEADDR` held port 67.
  - With `SO_REUSEPORT`, a socket set up like the DHCP server's socket joins its group and takes a share of its renewals. In the triage runs, a test socket with `SO_REUSEPORT` joined the group of a socket set up like a `bind-dynamic` dnsmasq socket with no device. Renewals from varied source ports split between the two sockets by hash.
- A DHCP server that holds port 67 on every interface without `SO_REUSEADDR`, such as dnsmasq in its default mode, leaves deployarr no port 67. deployarr's bind fails. When deployarr starts first, that server fails to start instead. The triage runs saw both with dnsmasq 2.91. The test socket's bind failed with `EADDRINUSE`, and dnsmasq failed with `failed to bind DHCP server socket: Address already in use`.
- On Windows, deployarr binds one port 67 socket on each chosen interface address, as `sccmpxe.exe` does ([ConfigMgr 2509 reference](research/configmgr-2509.md#sockets)). Like `sccmpxe.exe`, it sets no `SO_REUSEADDR`. It sets `SO_EXCLUSIVEADDRUSE`, because Microsoft's socket security guidance says "All server applications must set SO_EXCLUSIVEADDRUSE for a strong level of socket security" ([Using SO_REUSEADDR and SO_EXCLUSIVEADDRUSE](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)). `sccmpxe.exe` answers broadcast DISCOVERs through sockets bound this way, so on Windows such a socket receives them (inferred).
- deployarr cannot run beside WDS on the same host. WDS uses UDP 67, 69, and 4011 ([OSD infrastructure requirements](https://learn.microsoft.com/en-us/intune/configmgr/osd/plan-design/infrastructure-requirements-for-operating-system-deployment)), and deployarr needs 69 and 4011 even with the setting that skips port 67. Beside the Windows DHCP Server service, deployarr runs only with the setting that skips port 67, and that service sends option 60 `PXEClient`, as above. The installer checks for both services. deployarr's bind alone may not detect the DHCP Server service. In Microsoft's tables for Windows Server 2003 and later, a wildcard bind with no options succeeds after a specific-address bind with `SO_EXCLUSIVEADDRUSE`, under one user account or two ([Using SO_REUSEADDR and SO_EXCLUSIVEADDRUSE](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)).
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
2. What do `wdsmgfw.efi` and `wdsnbp.com` send in their second request (destination port, option 250 sub-options), and which files does the boot manager request over TFTP? Does each program always send option 250 sub-option 1 in its second request? `sccmpxe.exe` drops a second request without it ([ConfigMgr 2509 reference](research/configmgr-2509.md#two-stage-boot-chain)). deployarr decides how to treat such a request after the lab answers.
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
- **The server's own work.** Add-on calls run on their own `tokio` runtime, with threads apart from PXE, TFTP, and the agent API. The epoch callback yields through `UpdateDeadline::YieldCustom` with `tokio::task::yield_now`, and traps at the deadline. A run put 8 looping calls beside a 50 ms heartbeat on runtimes of 4 workers ([Add-on design attacks, item 8](research/addon-design-attacks-wasmtime-50.0.1.md#item-8-add-on-calls-can-starve-the-servers-runtime)):
  - On a shared runtime with no yield, the heartbeat stalled for 5964 ms.
  - On a shared runtime with `wasmtime`'s plain yield, it stalled for 4757 ms.
  - On a separate runtime with the plain yield, the heartbeat stayed on time, but no call ended at its own timeout.
  - On a separate runtime with `yield_now`, the heartbeat's longest gap was 54 ms.
- **Concurrency.** Each add-on has a limit on calls in flight, 8 by default (design decision).

Zed uses epochs only to yield, and Mattermost sets no timeout on any hook but one ([Zed extensions 1.28.1 reference](research/zed-extensions-1.28.1.md#what-changes-in-deployarr) item 9, [Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#limits)). A failed call is an error for its caller. It never disables the add-on, because input from a PC must not be able to switch an add-on off. In Mattermost, a member crashed a plugin a few times, and it stayed disabled ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#known-failures-and-lessons)).

### SQL connections

The SQL drivers live in the server, as [Data sources](#data-sources) describes. They build natively for Windows and Linux. In a component, `tiberius` needed WASI libc headers and a patched `rustls-native-certs`, and no SQL Server session ever ran ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#sql-server)). A `sqlx` build in a component needed `--cfg tokio_unstable` ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#tokio-on-wasm32-wasip2)). Spin also keeps its database drivers in the host and gives the guest an interface ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).

`deployarr:sql` has one function, `query(connection, query, params)`. There is no `execute` and no connection string.

- `connection` names a connection that the admin configured. Spin's guest builds the address, password included, and deployarr's guest never does ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).
- `query` names one of the add-on's own `sql_text` settings by its settings path, such as `rules/0/statement`. The admin writes the statement on the settings page. At save, the host checks that it holds one statement and only the parameters that the field declares. The host reads the text from the saved settings itself, so a compromised add-on can run only the admin's statements (design decision).
- The host rewrites named parameters, binds every value, and caps rows and time. A guest that names a path that is not one of its `sql_text` settings gets `no-such-query`.
- One engine-neutral interface serves every engine. Spin has one package per engine, and its MySQL and PostgreSQL types differ ([Spin 4.3.0 reference](research/spin-4.3.0.md#what-changes-in-deployarr)).

A statement belongs to the admin across upgrades. A settings migration returns a document that the host stores. In a model of the first design, version 2's migration put its own statement where the admin's was, and the next lookup returned payroll rows ([Add-on design attacks, item 3](research/addon-design-attacks-wasmtime-50.0.1.md#item-3-an-upgrade-can-replace-the-admins-sql)). The host therefore compares every `sql_text` value in a migration's result with the saved document, by settings path. A value passes in three cases:

- It is the same text at the same path.
- It is absent.
- It is empty, at a path that the old document lacked.

Any other result blocks the upgrade, and the review dialog names the statement. An empty statement makes its rule wait for the admin.

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
- **Compiling is guarded.** A hostile component of 485,757 bytes took 28.7 seconds and 4153 MiB to compile, and a larger one aborted the process under a 6 GB address-space limit, `ulimit -v 6000000` ([Add-on design attacks, item 12](research/addon-design-attacks-wasmtime-50.0.1.md#item-12-compiling-a-hostile-component-exhausts-the-server)). The host compiles only after the trust checks pass. It refuses a component over a cap on its size, on one function body's size, or on one function's locals. One thread compiles one component at a time. The prototype's components compiled in 0.48 to 1.22 seconds each ([Wasmtime 50.0.1 SQL add-on reference](research/wasmtime-50.0.1.md#sizes-and-times)). The M-A spike sets the cap values.
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
- **Install and upgrade check before they commit.** The host does these before it commits:
  - It prepares the new version beside the old one.
  - It instantiates the new version once.
  - It compares `describe` with the manifest.
  - It checks the settings migration.

  `stage` runs these code checks for a signed package, so the dialog can show the result. `apply` runs them again for every package, because settings can change between the two calls. A failure abandons the operation with its reason, and the old version stays active. The old package stays until the next successful upgrade. Grafana and Mattermost delete the old version first and have no rollback ([Grafana plugins 13.3.1 reference](research/grafana-plugins-13.3.1.md#failure-and-rollback), [Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#failure-and-rollback)).
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
| Directory join kinds | Yes | A `[[join_kind]]` names a profile form and, for each stage of the `ApplyJoinProfile` join slot, one of the add-on's step kinds. It adds no export. |
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
- For an identity resolver, `matched_on` must name a kind, and the server must have sent a fact of that kind. `UsableIdentity` holds the facts that the server may send. Its only constructor trims each value, drops known placeholders, and returns none when nothing is left. Its list is wider than the examples in [Identity facts](#identity-facts) (design decision), so that more shared firmware defaults never match an order. It holds common unset serials, a serial of only zeros, and a UUID or MAC address of only `0` or only `F` digits. SMBIOS defines an all-`00h` UUID as not present and an all-`FFh` UUID as not yet set ([SMBIOS 3.7.0](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.7.0.pdf), section 7.2.1).
- For a resolver with a prompt, `matched_on` must be none, and the server must have sent the answers to the prompt that the resolver's own add-on declares. A prompt ID is unique only within its add-on, so `PromptAnswers` names the prompt with its add-on. The authority is the technician's typed answer.
- `PromptAnswers` holds at least one answer that is not blank (design decision), because the server never calls a prompt resolver without a typed answer. Its `Debug` prints the field keys and never the answers, because a prompt field can be secret.
- Any other combination is an error, and the chain moves on.
- The authority's variants hold types with private fields, so no other code can build one. `WipeAuthority` also holds the authorities for a pre-staged record and a manual pick, each with its own constructor. See [Wipe safety](#wipe-safety).
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
	// Every action of `BuiltinAction` in Core data shapes. `RunCommand` and `RunPowerShell`
	// hold `files: StepFiles`, which holds content, never a secret.
	ServerCall { send: Vec<VarName>, limits: StepLimits },
	AddProvisioningPackage { package: SecretRef, limits: StepLimits },
}
```

`TaskSequence`, `Node`, `Group`, and `Step` from [Core data shapes](#core-data-shapes) take the action type as a parameter.

Three more names in `deployarr-core` belong to this model:

- `AddOnRef` is an add-on ID plus a kind ID. `Resolver::AddOn` and `JoinProfile::AddOn` each hold one. An `AddOnId` is `<publisher>.<name>`. Each part holds a lower-case ASCII letter and then lower-case ASCII letters, digits, or `-`. A `KindId` follows the same rule. The lengths, 1 to 32 characters for each part of an `AddOnId` and 1 to 63 for a `KindId`, are defaults until the owner decides the grammar, which is [one-way door](#one-way-doors) 3.
- `Catalog` is the set of step kind, join kind, resolver, and importer descriptors, from installed add-ons and from remembered ones. The UI and `validate` read it.
- `deferred` lists every enabled add-on step and `ApplyJoinProfile` join slot in a sequence. The server must expand each one before it compiles. It skips a step inside a disabled group, because the compiler drops that step too.

How a stored step behaves:

- **An add-on step is plain data.** In the [JSON format](#json-format) it is `{"add_on": {"addon": "deployarr.autopilot", "kind": "register", "config_version": 1, "config": {"group_tag": "%OrderGroupTag%"}}}`. A join slot is `{"apply_join_profile": {"stage": "full_os"}}`, or `offline_image` for the other stage.
- **The config keeps its bytes.** The step keeps the JSON text that was saved, a `Box<RawValue>`. Parsed into a `serde_json::Value`, a config came back with its keys reordered and a 23-digit integer turned into a float ([Add-on design attacks, item 6](research/addon-design-attacks-wasmtime-50.0.1.md#item-6-stored-steps-lose-bytes-and-fields)).
- **`Action` reads one key and dispatches.** `Action` has a hand-written `Deserialize`. It reads the object's one key and passes the value on without buffering it:
  - `add_on` goes to `AddOnStep`.
  - `apply_join_profile` goes to the `ApplyJoinProfile` join slot.
  - Any other key goes to `BuiltinAction` through its enum access.
- **`Action` never uses `#[serde(untagged)]`.** A derived `Action` would need it on `Builtin`, so that a built-in action keeps its own key in JSON. An untagged variant makes serde buffer the whole input first, as an internally tagged enum does. A `Box<RawValue>` field did not parse inside an internally tagged enum (item 6), and an untagged variant buffers the same way, so the add-on step's config would fail. The first design's untagged fallback also hid the real parse error behind one generic message ([Add-on design attacks, finding 23](research/addon-design-attacks-wasmtime-50.0.1.md#finding-23-unknown-fields-on-stored-steps-vanish)).
- **M0 tests the parser.** `crates/deployarr-core/tests/addon.rs` shows that the hand-written `Deserialize` keeps the config's bytes, with its keys out of order and the integer `12345678901234567890123`. A bad field inside an add-on step or a built-in action fails with its full path, such as `root.children[0].step.action.add_on.config_version`. The tests of the JSON format parse every sequence through it too.
- **Unknown fields are errors.** The envelope refuses unknown fields, as every built-in action does. A step from a newer schema is then an error, not a silent loss ([Add-on design attacks, item 6](research/addon-design-attacks-wasmtime-50.0.1.md#item-6-stored-steps-lose-bytes-and-fields)).
- **Saving runs no add-on code.** `validate` checks the config against the kind's schema at its config version. An unknown kind is a warning, not an error. Only planning a deployment runs add-on code.
- **A removed add-on's step still shows.** The server keeps a descriptor, the label and the schema, for every step kind and config version that was ever installed, and never deletes it. The editor therefore shows a removed add-on's step as a read-only form with a banner ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#what-changes-in-deployarr) item 1). The admin can move, disable, or delete the step. The compiler drops a disabled step, so the sequence deploys again.
- **A missing add-on stops a deployment before the disk changes.** When an add-on is disabled, removed, incompatible, or revoked, planning fails with `AddOnUnavailable`. The deployment stops at the lookup screen with the reason. The task sequence list marks such sequences, and the disable dialog lists them first.
- **Upgrades migrate config in memory.** After an upgrade, a stored step keeps its `config_version`. The host migrates the config in memory when it plans a deployment and when the editor opens the step. A saved version is never rewritten, so history stays true. A config older than the add-on accepts, or newer than it knows, is a plan error. MDT never upgraded stored steps ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#known-failures-and-lessons)).
- **An export says what it needs.** An export carries the descriptors of the add-on steps it uses and a derived list of the add-ons it needs. Another server can show the steps and say what to install.
- **A running deployment is not affected.** Its program was lowered and frozen when it started.

Planning runs in two parts before the compiler:

1. `AddOnHost::lower` asks each add-on to expand its steps and the `ApplyJoinProfile` join slots.
2. `lower` in `deployarr-core` puts each expansion in place of its step.

`compile` then takes the `LoweredSequence`, so a program that names an add-on does not type-check.

`lower` keeps the step's `NodeMeta` on a group that holds the expansion. The step's condition and `continue_on_error` therefore still apply. Each child of the expansion is a `Step<Action>`, so that `lower` can see an add-on step inside it. `lower` rejects these:

- a missing expansion
- an action outside the six that an add-on may return
- a child with another `run_in`
- a nested add-on step or join slot
- an expansion for a node that `deferred` does not list (design decision), because it is a fault in the host that ignoring it would hide
- an add-on step or join slot with `run_in` `WinPe` whose condition holds an `InstalledMsi` or `Wmi` condition at any depth, disabled or not. The compiler checks this rule on steps, and the group that replaces the step keeps its condition. `validate` reports the same step as an error, so a save catches it.

An add-on step or join slot that `deferred` skips needs no expansion. `lower` turns it into an empty group with the step's `NodeMeta`, and the compiler drops that group.

The IDs of the new nodes derive from the step's ID, so the same input gives the same program (design decision). The ID of child `i` is the version 5 UUID whose namespace is the step's ID and whose name is `i` in decimal ASCII digits. Each child keeps the rest of the `NodeMeta` that its add-on gave it, such as its own condition, `continue_on_error`, and `enabled` (design decision), so that an add-on can make one of its steps conditional or optional. The step's own condition, `continue_on_error`, and `enabled` stay on the group.

### What runs on the PC

On the PC, only the agent and the commands of the task sequence run. The server expands an add-on step into built-in steps, as MDT's stored step resolves to a run of a program from a content package ([MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#on-the-pc)). The agent stays one static executable for base WinPE and never loads add-on code ([Mattermost plugins 12.1.0 reference](research/mattermost-plugins-12.1.0.md#what-changes-in-deployarr) item 13, [MDT 8450 ConfigMgr integration reference](research/mdt-8450-configmgr-integration.md#what-changes-in-deployarr) item 9). It gains one generic ability and two built-in actions, and it learns nothing about add-ons:

- **Step files.** `RunCommand` and `RunPowerShell` can list files that the agent puts in a private step folder before the step and deletes after it. A step file is content, such as a script from the add-on's assets, and never a secret ([Add-on design attacks, item 9](research/addon-design-attacks-wasmtime-50.0.1.md#item-9-an-add-on-can-read-its-own-secret-file)).
- **`AddProvisioningPackage`.** The agent writes a secret package to a private folder, runs DISM with a command line that it builds itself, and deletes the file. The action can run in WinPE or in the full OS, so it honors every `run_in`. In WinPE the agent adds the package to the applied image with `/Image`. In the full OS it adds the package to the running Windows with `/Online` ([DISM provisioning package options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-provisioning-package-command-line-options)). The Entra ID join add-on uses the WinPE form, and the full-OS form is its fallback. See [Entra ID join add-on](#entra-id-join-add-on).
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
- `AddOnHost` has 20 methods, and the host runs five HTTP auth schemes. The [design](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832653114) lists both as a risk. Each should shrink to what the planned add-ons use before level 1 freezes.
- The attack runs ran on Linux only ([Add-on design attacks, Sources](research/addon-design-attacks-wasmtime-50.0.1.md#sources)), and no SQL Server ran ([Add-on design attacks, Not rerun](research/addon-design-attacks-wasmtime-50.0.1.md#not-rerun)). The M-A check repeats the runtime runs on Windows, and the M5 check runs SQL Server.

### One-way doors

These choices are hard or impossible to change after level 1 freezes in M9. The owner decides each one. Where the design proposes a default, the entry names it. The design session chose this design from four candidate designs. Its [decision log](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832689907) records the base it picked and what it took from the others, in the rows from 2026-11-12 at 14:53:47 and 14:53:48 UTC.

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

- Deployments: live progress, step timeline, logs, variable trace, missing drivers, and missing updates.
- Task sequences: a tree editor with a step palette, a typed form for each action, a condition builder, version history, and templates. Step kinds from add-ons appear in the palette under the add-on's name, with forms drawn from their schemas. Import opens a deployarr export, or another format through an importer add-on such as the MDT importer.
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

`deployarr-addons` hides these from the server:

- the add-on runtime and the bindings for every API level
- the package envelope and trust
- the operation journal, grants, and limits
- the store of each add-on, connections, both egress paths, and the job queue

Every module except `lib.rs` is private, so `deployarr-server` never sees a `wasmtime`, WIT, or package type.

An add-on author sees one trait with default bodies and the `deployarr-core` types. go-plugin's research asks for default bodies, so a new host function never breaks add-on code at compile time ([HashiCorp go-plugin 1.8.0 reference](research/go-plugin-1.8.0.md#what-changes-in-deployarr) item 9). The SDK and the in-tree add-ons live in this repository and build in CI against the current API, because Jellyfin's plugin template lagged a major release behind its server ([Jellyfin plugins 12.4 reference](research/jellyfin-plugins-12.4.md#what-changes-in-deployarr) item 16).

## Not in the first release

These are planned for later and kept out of the first release on purpose:

- BitLocker protectors that need a PIN or a startup key (see [Enable BitLocker](#enable-bitlocker))
- user state migration and in-place upgrades
- multicast
- PXE across routed subnets through a DHCP relay
- IPv6 PXE
- ARM64
- Linux deployment
- vendor driver catalogs, which arrive as an add-on with API level 2 (see [Extension points](#extension-points))
- a desktop program for the server

The [Roadmap](roadmap.md) lists them.
