# Architecture

deployarr installs Windows on bare-metal PCs from one central server with no technician input. It copies the parts of Microsoft Deployment Toolkit (MDT) that work and fixes the faults listed in [MDT 8456 reference](research/mdt-8456.md) and [MDT 8450 binaries reference](research/mdt-8450-binaries.md). This page explains the design and why each choice was made. For the build order, see [Roadmap](roadmap.md).

## Requirements

These come from the product owner and drive every decision below.

- Zero-touch deployment, Windows first.
- Order lookup in an SQL database. The order's OS identifier selects the task sequence. A manual list of task sequences is the fallback.
- An order is tied to a PC in three ways, and all three must work: the order table holds the motherboard serial number, a technician scans or types the order number, or someone pre-stages the PC in the web UI.
- The order database engine is not fixed. Support Microsoft SQL Server, PostgreSQL, and MySQL or MariaDB.
- Task sequences that work like MDT's: ordered groups and steps, conditions, variables, scripts, commands, and reboots.
- Every device has a working driver when the deployment finishes.
- Configurable applications, with a simple UI to manage drivers and software.
- Install from an ISO or a WIM.
- A WinPE-based preinstall environment.
- PXE boot for UEFI with Secure Boot on and for legacy BIOS.
- One central deployment server that runs on Windows or Linux.

## Components

```
                 deployment server (Windows or Linux)
 +---------------------------------------------------------------------+
 |  deployarr-server (one binary)                                      |
 |                                                                     |
 |  PXE: proxyDHCP (UDP 67, 4011), TFTP (UDP 69)                       |
 |  HTTPS API for agents and the web UI, content downloads             |
 |  rules engine -> order data sources (SQL Server, PostgreSQL, MySQL) |
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

1. The PC PXE-boots. The proxyDHCP service reads the client architecture (DHCP option 93) and offers the Microsoft-signed boot manager for that firmware type.
2. The boot manager loads WinPE from the server over TFTP. WinPE starts `deployarr-agent` from `winpeshl.ini`.
3. The agent initializes WinPE (`wpeinit`), loads the boot image's network and storage drivers with `drvload`, and connects to the server over HTTPS.
4. The agent gathers facts and sends them to the server. Facts include the SMBIOS data (system, baseboard, and chassis serials, UUID, make, model, and SKU), MAC addresses, firmware type, disks, and the hardware IDs of every device.
5. The server runs the lookup chain (see [Order lookup](#order-lookup)). The result is a task sequence and a set of variables, or a request for the technician to type an order number or pick from the list.
6. The server pins the task sequence version and sends the compiled program and variables to the agent.
7. The agent runs the WinPE steps. A typical run partitions the disk, downloads and applies the OS image, injects drivers, writes the unattend file, runs `bcdboot`, and installs itself as a service in the offline image.
8. The PC reboots into Windows Setup. When Setup finishes, the agent service resumes the sequence at the next step.
9. The agent installs applications, runs scripts, reboots as steps request, and checks that every device has a driver.
10. The agent uploads its logs, removes itself, and runs the finish action (shut down, restart, or reseal).
11. The server marks the deployment complete. Later PXE boots of that PC fall through to the local disk.

## Core data shapes

The task sequence model lives in `deployarr-core`, a pure crate with no I/O. The server, the agent, and the web UI all use it. These sketches show the shape, not final code.

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
}

pub struct ExitCodes {
	pub success: Vec<i32>,    // never empty. Default [0]
	pub reboot: Vec<i32>,     // success, then a reboot. Default [3010, 1641]
}

pub struct StepLimits {
	pub max_runtime: Option<Duration>,
	pub rerun_if_interrupted: bool,   // default false
}

pub enum Condition {
	All(Vec<Condition>),
	Any(Vec<Condition>),
	Not(Box<Condition>),
	Var { name: VarName, op: CompareOp, value: Template },
	Registry { key: Template, value: Option<String>, op: CompareOp, data: Option<RegData> },
	File { path: Template, version: Option<(CompareOp, FileVersion)>, modified: Option<(CompareOp, DateTime)> },
	Folder { path: Template, modified: Option<(CompareOp, DateTime)> },
	InstalledMsi(MsiProduct),   // ProductCode or UpgradeCode. Full OS only
	Wmi { namespace: String, query: Template, ignore_failure: bool },   // full OS only
}

pub enum CompareOp { Exists, NotExists, Equals, NotEquals, Less, LessEqual, Greater, GreaterEqual, Like }
```

Each MDT step type that does real work becomes a typed `Action` variant with typed parameters. `RunCommand` and `RunPowerShell` cover everything else. MDT passes every step's settings as untyped variables and command lines, so a typo becomes a runtime failure. With typed actions, the editor and the server reject a bad step when you save it.

The condition types match what MDT's engine evaluates. MDT also has make, model, location, and computer conditions, which compare by substring, so a gateway of `10.0.0.1` also matches `10.0.0.10`. deployarr expresses those as `Var` conditions on gathered facts, which compare exactly.

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
- The agent runs each command in a Windows job object. When `max_runtime` passes, it ends the whole process tree and fails the step. MDT's templates set no time limit, so a hung step waits forever.
- The agent streams each step start, step result, and log line to the server. The server is the record of what happened, and the local journal covers network outages.

The engine is a pure state machine over an `Executor` trait, so it is tested on Linux with a fake executor. The key test crashes the agent at every step boundary and checks that each resumed run ends in the same state as a run with no crash.

### Variables

- A name is 1 to 256 letters, digits, `_`, or `-`, and names are case-insensitive. These are MDT's rules. Names that start with `_` belong to the agent, and steps can read them but not set them.
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
- The setup guide recommends a read-only database account.

### Wipe safety

A zero-touch server can erase any PC that network-boots by mistake. deployarr erases a disk only after one of these:

- a positive identity match, from a pre-staged record or an order row
- a technician entering an order number
- a technician picking a task sequence

An unknown PC stops at the prompt and does not touch its disk. After a deployment succeeds, the server stops offering PXE boot to that PC's MAC address and SMBIOS UUID (DHCP option 97), until an admin re-arms it. This stops a PC that boots from the network first from reinstalling in a loop.

## Agent

- The agent is one static Rust executable built for `x86_64-pc-windows-msvc` with the static C runtime.
- It uses Win32 APIs, not WMI, so base WinPE needs no optional components:
  - SMBIOS: `GetSystemFirmwareTable`
  - network adapters: `GetAdaptersAddresses`
  - firmware type: `GetFirmwareType`. The fallback is MDT's `GetFirmwareEnvironmentVariableW` check, and only with `SeSystemEnvironmentPrivilege` enabled first. Without the privilege, newer WinPE versions make that check report BIOS PCs as UEFI, which is the bug the MDT 8456 hotfix fixed.
  - disks and bus type: `IOCTL_STORAGE_QUERY_PROPERTY`. Bus type 7 is USB.
  - device hardware IDs: SetupAPI with `DIGCF_PRESENT | DIGCF_ALLCLASSES`, which is what MDT's `Microsoft.BDD.PnpEnum.exe` does. Unlike PnpEnum, the agent reports an error when enumeration fails, so a failed scan never looks like a PC with no devices.
  - CPU features: the `cpuid` instruction, for the same virtualization, NX, and 64-bit facts that MDT's `Microsoft.BDD.Utility.dll` reports.
- `RunPowerShell` steps in WinPE need the WinPE PowerShell component, which only DISM can add. See [Boot image](#boot-image).
- `RunPowerShell` runs `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <script>` and passes each argument separately. MDT's PowerShell host joins the arguments into one string and runs it as PowerShell code, which loses their quoting.
- A CI check lists the agent's DLL imports and fails if one is missing from base WinPE.
- In WinPE the agent is the shell. It shows a full-screen console screen with the current step, a progress bar, and the order-number and task-sequence prompts.
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
4. The service starts at boot. It waits until Windows Setup reports that it has finished, by watching `ImageState` and `SystemSetupInProgress`, and then continues the sequence.

This removes the stored Administrator password and the logon count from the deployment. It also makes the PC deploy the same way whether or not anyone is in front of it.

The trade-off is that steps run as LocalSystem in session 0, with no desktop. A few installers fail without a logged-on user. For those, a later milestone adds a per-step option that runs the step in an auto-logon session with a one-time password. The agent removes that password after the step.

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
- The content store keeps files by SHA-256. The agent checks each download against its hash.
- The agent downloads the image over HTTPS into `W:\Deployarr\cache` on the new Windows partition, then runs `dism /Apply-Image /ImageFile:<wim> /Index:1 /ApplyDir:W:\ /ScratchDir:W:\Deployarr\scratch`. The scratch directory on disk avoids WinPE's small RAM scratch space. Downloads resume after a network drop. The target disk needs room for the image file plus the applied OS. Windows 11 already requires a 64 GB disk, which has room for both.
- A clean install never runs `setup.exe`. MDT 8456 also applies `install.wim` for an OS imported from an ISO. Its `setup.exe` path has no caller.

## Disk layout and boot files

- Two built-in layouts follow Microsoft's documented defaults:
  - UEFI: GPT with EFI (FAT32), MSR, Windows, and Recovery partitions
  - BIOS: MBR with an active System partition (NTFS), Windows, and Recovery partitions. ConfigMgr's engine requires the active partition on an MBR disk to be NTFS.
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
- Outputs are `boot.wim` for PXE, plus an ISO for USB sticks and remote-management virtual media. The ISO is built with `xorriso`, using the El Torito boot files from the imported Windows media. It uses the no-prompt EFI boot image, `efisys_noprompt.bin`, so it starts without a key press. MDT's ISO uses `efisys.bin`, which waits for a key (inferred from ADK behavior).

## PXE

- The UEFI path keeps Secure Boot on. The server sends the Microsoft-signed `bootmgfw.efi`, a BCD store, `boot.sdi`, and `boot.wim` over TFTP. The boot manager loads WinPE into a RAM disk. WDS boots WinPE the same way, and deployarr does it without the WDS service.
- The BIOS path sends `pxeboot.n12` (no F12 prompt) or `pxeboot.com` (F12 prompt), which loads `bootmgr.exe`, the same BCD layout with `winload.exe`, and the same `boot.wim`.
- The boot files come from the imported WinPE image.
- The BCD stores are fixed templates, one per firmware type, with fixed paths such as `\boot\x64\boot.wim`. The server never edits a BCD file. It serves a different boot image at the same path. The templates are made once with `bcdedit`, and the repository keeps the `bcdedit` script that created them.
- The templates use the same RAM disk elements as MDT's offline media and its `ZTIBCDUtility.vbs`. `{ramdiskoptions}` has `ramdisksdidevice boot` and `ramdisksdipath \boot\boot.sdi`. MDT stages WinPE on a local partition, so its script sets `ramdisksdidevice partition=<drive>` instead. The loader entry has `device` and `osdevice` set to `ramdisk=[boot]\boot\x64\boot.wim,{ramdiskoptions}`, `path \windows\system32\boot\winload.efi` (or `winload.exe` for BIOS), `systemroot \windows`, `detecthal yes`, and `winpe yes`.
- DHCP works in two modes:
  - proxyDHCP, the default. The existing DHCP server keeps handing out addresses. deployarr answers only PXE clients, on UDP 67 and 4011, with the boot file for the client architecture from DHCP option 93.
  - DHCP options. For routed networks, the existing DHCP server or relay points PXE clients at deployarr.
- The TFTP server supports block size (RFC 2348), transfer size (RFC 2349), and window size (RFC 7440). WinPE images are 300 MB or larger, and plain TFTP is too slow for that. The BCD template sets the boot manager's `ramdisktftpblocksize` and `ramdisktftpwindowsize`.
- deployarr cannot share UDP 67 or 69 with a Windows DHCP server or WDS on the same host. The installer checks for both.

### Risks to settle with a prototype first

- Whether `bootmgfw.efi` fetches its BCD from a TFTP server that is not WDS, and which DHCP options it needs for that (WDS uses option 252 for the BCD path). The whole PXE design depends on this, so it is the first thing the lab proves.
- Microsoft's 2011 Secure Boot certificates expire during 2026, and Microsoft now signs boot managers with the Windows UEFI CA 2023. Firmware that revokes the old certificate boots only the newer boot manager. The prototype must boot both a 2011-signed and a 2023-signed boot manager, and the importer must report which one an image contains.
- If the signed path fails, the fallbacks are UEFI HTTP boot, or the boot ISO through USB or remote-management virtual media. An iPXE and wimboot path over HTTP is faster, but it works only on BIOS or with Secure Boot off, so it stays optional.

## Server

- The server is one `tokio` binary. It uses `axum` for HTTPS, `rustls` for TLS, `sqlx` with SQLite for its own data, and a content store directory for files. It runs as a Windows service through `windows-service`, or as a systemd unit on Linux.
- The agent API covers sessions, the compiled plan, content manifests, events, logs, and the post-install device report.
- The admin API serves the web UI. Live progress reaches the UI over server-sent events.
- Each deployment gets its own ID from the server. Succeeded and Failed are final states that later events cannot change. MDT's monitoring service keeps one record per computer, sets a finished record back to Running on its next event, and keeps no history.
- The agent queues events and sends them in the background with retries, plus a heartbeat on a timer. A slow or unreachable server never delays a step. MDT sends each event as a blocking call with a 60-second timeout, and its PowerShell steps send no events at all.
- Installation creates a private CA and a server certificate. Each boot image carries the CA certificate.
- Secrets are encrypted at rest with a server key. This covers data source passwords and per-PC local admin passwords. Each deployed PC gets its own random local admin password, which an admin can read in the UI. MDT uses one shared password.
- Admin accounts are local at first, with argon2id password hashes. LDAP or OIDC can come later.

## Web UI

The UI is written in Leptos and compiled to WebAssembly, so it uses the same `deployarr-core` types as the server. The editor validates a task sequence with the same code the server runs when you save. The screens are these:

- Deployments: live progress, step timeline, logs, variable trace, and missing drivers.
- Task sequences: a tree editor with a step palette, a typed form for each action, a condition builder, version history, and templates.
- OS images: import an ISO or WIM, then choose editions.
- Drivers: import packages, browse by hardware ID, manage profiles, and view missing-driver reports.
- Applications: upload files, set the command, the detection rule, and dependencies, and test detection.
- Order lookup: data source connections with a test button, named queries, the OS-code map, and the lookup chain. A test box takes an order number or a serial number and shows the resolved task sequence and its trace.
- Machines: pre-staged PCs, with CSV import.
- Boot images and PXE: build boot images, set the active image, and configure DHCP mode and interfaces.

## Crate layout

```
crates/
	deployarr-core     task sequence model, compiler, conditions, variables, templates,
	                   identity rules, API types. No I/O. Builds for Linux, Windows, and wasm32.
	deployarr-engine   interpreter, state file, journal, and resume logic over an Executor trait
	deployarr-inf      INF parser and hardware ID index
	deployarr-smbios   SMBIOS table parser
	deployarr-pxe      proxyDHCP and TFTP
	deployarr-server   server binary
	deployarr-agent    Windows agent binary
	deployarr-ui       Leptos web UI
xtask/               boot image builds, lab automation, packaging
```

`deployarr-inf` and `deployarr-smbios` are separate crates because they are pure parsers tested against real files on Linux. The agent itself only builds for Windows.

## Not in the first release

These are planned for later and kept out of the first release on purpose:

- domain, Entra ID, or Autopilot join
- Windows Update during deployment
- BitLocker
- user state migration and in-place upgrades
- multicast
- ARM64
- Linux deployment
- vendor driver catalogs
- writing status back to the order database

The [Roadmap](roadmap.md) lists them.
