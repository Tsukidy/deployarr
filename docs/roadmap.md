# Roadmap

This page lists the build order for deployarr. Each milestone ends in a check that someone can run and watch pass. A milestone starts only when the one before it passes its check, except where a milestone says otherwise. For the design behind each item, see [Architecture](architecture.md).

## Test lab

From milestone 1 on, every check that needs a PC or a VM runs in a lab that `xtask lab` drives. M-A's checks, and other checks that need only containers, run in CI. The lab is a separate network, so a mistake cannot erase a real PC.

- The lab host is a Linux machine with KVM. Hyper-V is not a lab host, for two reasons. The [M3 check](#m3-bare-metal-deployment-from-boot-media) needs a BIOS VM that finishes with Windows 11. Microsoft's page [Should I create a generation 1 or 2 virtual machine in Hyper-V?](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/plan/should-i-create-a-generation-1-or-2-virtual-machine-in-hyper-v) shows that a BIOS VM on Hyper-V is a generation 1 VM. The page marks Windows 11 as unsupported on generation 1. The [M6 check](#m6-drivers) needs virtio disk, network, and balloon devices. The page's device table lists no virtio device.
- UEFI test VMs use QEMU with OVMF firmware that has Secure Boot on and Microsoft's keys enrolled. Legacy BIOS test VMs use SeaBIOS.
- Every test VM runs with `-cpu host` under KVM, so the guest sees the lab host's processor. A VM started without `-cpu` runs QEMU's default model, `qemu64`, which lacks SSE4.1 and PrefetchW. On QEMU 8.2.2, QMP `query-cpu-model-expansion` reports `sse4.1` and `3dnowprefetch` (PrefetchW) as false for `qemu64`.
- The lab host's CPU must support PAE, NX, SSE4.1, CMPXCHG16B, LAHF/SAHF, and PrefetchW. Section 3.5 of Microsoft's [Minimum Hardware Requirements for Windows 11](https://download.microsoft.com/download/7/8/8/788bf5ab-0751-4928-a22c-dffdc23c27f2/Minimum%20Hardware%20Requirements%20for%20Windows%2011.pdf) requires them. Section 5 holds virtual machines to the same minimum, so a VM on `qemu64` falls short of it.
- The VMs sit on an isolated bridge with a plain DHCP server. deployarr provides only PXE.
- The OS under test is the multi-edition Windows 11 ISO from Microsoft's [download page](https://www.microsoft.com/software-download/windows11), with Home and Pro, the editions the owner deploys. Lab VMs install Windows with no product key and stay unactivated. Tests of the `ActivateWindows` step then install Microsoft's KMS client setup key for Pro, which cannot activate without a KMS host (see [Activation](architecture.md#activation)). The real PC from milestone 3 on tests the key from its sticker.
- virtio disk and network devices test the driver path, because Windows 11 has no inbox drivers for them. They must come from the deployarr driver library and the WinPE driver set.
- Each UEFI test VM has a virtual TPM 2.0, QEMU's `tpm-crb` device backed by `swtpm` in TPM 2.0 mode. [`tools/lab/vm.sh`](../tools/lab/vm.sh) starts the backend with `swtpm socket --tpm2`. `build_dsdt` in QEMU 8.2.2's [`hw/i386/acpi-build.c`](https://github.com/qemu/qemu/blob/v8.2.2/hw/i386/acpi-build.c#L1794-L1810), lines 1794 to 1810, declares the device with the ACPI hardware ID (`_HID`) `MSFT0101` and no compatible ID (`_CID`). OVMF installs the ACPI tables that QEMU builds, in `InstallAcpiTables` in [`OvmfPkg/AcpiPlatformDxe/AcpiPlatform.c`](https://github.com/tianocore/edk2/blob/edk2-stable202402/OvmfPkg/AcpiPlatformDxe/AcpiPlatform.c#L44-L46) at tag `edk2-stable202402`. The [triage of #72](https://github.com/Tsukidy/deployarr/issues/72#issuecomment-6826925253) read the same IDs from a Linux guest on OVMF. No lab VM therefore shows the agent a TPM that lists `MSFT0101` only as a compatible ID.
- Each test VM that must finish a deployment has at least 4 GiB of memory, a disk of at least 64 GiB, and at least two processor cores. These meet the default minimums of the [readiness check](architecture.md#readiness-check) whether GB there means 10^9 or 2^30 bytes. [`tools/lab/vm.sh`](../tools/lab/vm.sh) starts every VM with `-m 4096`, a 64 GiB disk image, and `-smp 2`. QEMU 8.2.2's own defaults fall short of the minimums. Its [`hw/core/machine.c`](https://github.com/qemu/qemu/blob/v8.2.2/hw/core/machine.c#L969) sets 128 MiB of memory at line 969 and one CPU at [line 1088](https://github.com/qemu/qemu/blob/v8.2.2/hw/core/machine.c#L1088).
- The UEFI VMs use the `q35` machine type and the BIOS VM uses `pc`. On both, in QEMU 8.2.2, `-smp 2` gives one socket with two cores. The two names are aliases of the 8.2 types, `pc-q35-8.2` at line 389 of [`hw/i386/pc_q35.c`](https://github.com/qemu/qemu/blob/v8.2.2/hw/i386/pc_q35.c#L389) and `pc-i440fx-8.2` at line 551 of [`hw/i386/pc_piix.c`](https://github.com/qemu/qemu/blob/v8.2.2/hw/i386/pc_piix.c#L551). Neither type sets `prefer_sockets`, so lines 161 to 173 of [`hw/core/machine-smp.c`](https://github.com/qemu/qemu/blob/v8.2.2/hw/core/machine-smp.c#L161-L173) fill in cores before sockets.
- `-smbios type=2,serial=...` gives each VM a known baseboard serial for order lookup tests.
- From milestone 3 on, each milestone except M-A also deploys at least one real PC of the kind you build. M-A deploys no PC, because it needs only the server.
- From milestone 8 on, a Windows Server evaluation VM is the lab's domain controller, for the domain join check on the UEFI VM, which runs Pro.

## Milestones

The add-on system adds one milestone, M-A, and scope and checks to M0, M1, M3, M5, M8, and M9. [Add-ons](architecture.md#add-ons) holds the design. A check that ends with an item number, such as (item 6), uses the attack from that item of [Add-on design attacks on wasmtime 50.0.1](research/addon-design-attacks-wasmtime-50.0.1.md) as its input.

### M0. Workspace and core model

Scope:
- Cargo workspace with the crates listed in [Crate layout](architecture.md#crate-layout), except `deployarr-addons`, `deployarr-addon-sdk`, `wit/`, and the crates under `addons/`, which M-A adds.
- CI on Linux and Windows. `rustfmt` and `clippy -D warnings` must pass.
- `deployarr-core`: the task sequence model, its JSON format with a schema version, the compiler, the condition evaluator, and variable substitution.
- A CI check that fails if the agent imports a DLL that base WinPE lacks.
- The add-on shapes in `deployarr-core`, from [Task sequences stay loadable](architecture.md#task-sequences-stay-loadable) and the sections around it:
  - `AddOnId`, `KindId`, and `AddOnRef`, with strict parsers.
  - The generic `TaskSequence<A>`, the authored `Action` with an add-on step and the `ApplyJoinProfile` join slot, and `BuiltinAction` with `ServerCall`, `AddProvisioningPackage`, and step files that hold content only. An add-on step's config is raw JSON text, and every action shape refuses unknown fields.
  - `deferred`, `substitute`, and `compile` over the lowered type only.
  - `Resolver::AddOn` in place of the order resolvers, and `JoinProfile::AddOn` in place of the Entra ID and Autopilot profiles.
  - `FormSchema`, `PromptSpec`, and `Catalog`, with a field parser that gives each kind only its own keys.
  - The lifecycle planner `plan(registry, change, facts)`, with its `Change`, `Plan`, and `Refusal` types.
  - `IdentityKind`, `UsableIdentity`, and `WipeAuthority`. The only constructor of `WipeAuthority` for an add-on's match takes the declared input kind, the input the server sent, and a typed `matched_on`.

Check:
- `cargo test --workspace` passes on Linux and Windows.
- A test rebuilds the group, condition, and phase structure of MDT's `Client.xml` in the deployarr model and compiles it. This proves the model can express everything MDT's standard sequence uses.
- Table tests cover the condition rules listed in [MDT 8450 binaries reference](research/mdt-8450-binaries.md#conditions): integer versus string comparison, `Like` wildcards, `Exists` on an empty value, and missing files and registry values. Each place where deployarr differs from MDT on purpose has its own test.
- The compiler rejects an empty success list, a `run_in` the action cannot honor, a WinPE `Restart` before the state moves to disk, and an empty `All`, `Any`, or `Not`.

Check of the add-on shapes:
- A task sequence with a step from an add-on that no catalog knows round-trips through JSON byte for byte, and `validate` returns a warning, not an error. The step's config holds keys out of order and the integer `12345678901234567890123`, and both come back unchanged (item 6).
- A built-in `SetVariable` with an extra field, an add-on step with an extra envelope field, and an unknown action type each fail to parse (item 6).
- A bad field inside an add-on step's envelope, and one inside a built-in action at the top level of a step, each fail with the field's full path, such as `root.children[0].step.action.add_on.config_version` (item 6).
- A `trybuild` test shows that a call of `compile` on an authored sequence does not compile.
- `substitute` rejects a missing expansion, an action outside the six that an add-on may return, a child with another `run_in`, and a nested add-on step. The same input gives the same node IDs.
- An expansion with a `RunCommand` whose step file names a secret fails to parse, because a step file holds content only (item 9).
- Table tests cover every field kind. Each has a valid value and an invalid value. An unknown key such as `allow_write = true`, a key that belongs to another kind, and an unknown kind each fail the parse (item 10).
- Table tests cover every refusal of `plan`: an incompatible level, a revoked key, a key outside its scope, a downgrade without confirmation, an uninstall while a sequence uses the add-on, and a second open operation. A waiting operation has three cases of its own. Force for the same package takes it over, `cancel` abandons it, and any other change is refused with the waiting operation named (item 14).
- `WipeAuthority::from_addon_match` returns an error for an identity resolver's match with no `matched_on`, for a kind the server did not send, for a prompt resolver's match that names a kind, and for input that does not fit the declaration. A `UsableIdentity` built from placeholder serials only is none (item 4).
- The `Client.xml` rebuild test above passes on the generic model.

### M1. Engine and agent in a running Windows

Scope:
- `deployarr-engine`: the state file, journal, resume logic, and reboot outcomes.
- `deployarr-agent` in service mode, running a task sequence from a local file.
- The actions `RunCommand`, `RunPowerShell`, `SetVariable`, and `Restart`.
- Variable, registry, file, folder, installed-MSI, and WMI conditions.
- The single-run mutex, the reboot marker, job objects with `max_runtime_secs`, and the `var get` and `var set` channel for step processes.
- CMTrace logs.
- Step files that hold content only, `ServerCall`, and `AddProvisioningPackage`, in the `Executor` trait and the fake executor. See [What runs on the PC](architecture.md#what-runs-on-the-pc).

Check:
- A test crashes the engine at every step boundary of a 20-step sequence. Each resumed run ends in the same state as a run with no crash.
- A test kills the agent in the middle of a state save. The next start reads either the old state or the new state.
- In a Windows 11 VM, a sequence with three reboots finishes, and its log shows each step exactly once.
- A step that exceeds `max_runtime_secs` fails, and no process from its tree is still running.
- A `ServerCall` that names a variable marked secret fails its step and sends nothing (item 9).
- The crash test at every step boundary includes a `ServerCall` step and a step with files. Each resumed run ends in the same state, and the fake server sees one effective call per node.
- After an `AddProvisioningPackage` step, its private folder is gone, and no log line and no variable holds the package's bytes. A `RunCommand` that follows finds no package in its own step folder, and a `ServerCall` after that sends nothing of the package (item 9).

The PXE prototype from M4 runs in parallel with this milestone, because it is the largest unknown and depends on nothing else.

### M2. Server core

Scope:
- `deployarr-server` with SQLite, the content store, the private CA, and the agent and admin APIs.
- Live events over server-sent events.
- A first web UI with a deployment list, a live log view, and a JSON editor for task sequences.
- Windows service and systemd packaging, minimal versions.

Check:
- The M1 VM's agent registers with the server, receives its sequence, and its progress shows live in the UI.
- The same integration test passes with the server on Linux and on Windows.

### M3. Bare-metal deployment from boot media

Scope:
- OS import from an ISO (7-Zip) or a WIM or ESD, with single-edition export through `wimlib-imagex`.
- The boot image builder and the boot ISO.
- Agent WinPE mode: `wpeinit`, `drvload`, network, the console screen, and fact gathering (SMBIOS, adapters, firmware, Secure Boot, TPM, memory, processor cores, disks, device IDs).
- The readiness check, with Microsoft's Windows 11 minimums as defaults.
- Disk layouts for UEFI and BIOS, image download and apply, `bcdboot`, WinRE, and the unattend file.
- The offline service install, the handoff to the new OS, the finish action for both end states, and cleanup.
- The product key prompt and the `ActivateWindows` step.
- The agent renders any `PromptSpec` that the server sends. The product key prompt becomes a field on the same screen. See [What runs on the PC](architecture.md#what-runs-on-the-pc).

Check:
- A UEFI VM with Secure Boot on and a BIOS VM both boot the deployarr ISO. Each finishes with Windows 11 in its configured end state, with no input, using a pre-staged record. The UEFI VM installs Pro, and the BIOS VM installs Home. Neither run types a key, so their sequence has no `ActivateWindows` step.
- One real PC does the same with the edition its sticker names, and its product key typed at the prompt. `slmgr /dli` on that PC shows the last five characters of that key.
- On the UEFI VM, a sequence with an `ActivateWindows` step and `online` off takes Microsoft's KMS client setup key for Pro, and `slmgr /dli` shows `T83GX`. The same key with one character changed fails the step, and the deployment shows the error. The lab records the error code.
- The UEFI VM runs once with each end state. With `ReadyToUse`, it boots to the sign-in screen with no OOBE. With `CustomerOobe`, it shuts down, shows OOBE on its next boot, and has no account that deployarr created. Neither leaves a cached answer file in `%WINDIR%\Panther`.
- The BIOS VM's task sequence turns off the UEFI and TPM readiness checks. A VM below a readiness minimum stops before its disk changes, and its log names the failed check and the value found.
- On the UEFI VM and the real PC, the agent reads the TPM version and the Secure Boot state in WinPE. It reports the TPM version that the PC has, and its log names the device ID that matched and whether it is a hardware ID or a compatible ID. If base WinPE lacks the Secure Boot registry value, the gap is recorded in [Agent](architecture.md#agent).
- A VM shows a prompt from a `PromptSpec` that the server sends, such as one that a test add-on declares. It accepts typed input and scanner-style input, and it applies the declared pattern.

### M4. PXE

Scope:
- Prototype first. Answer the six questions in [Risks to settle with a prototype first](architecture.md#risks-to-settle-with-a-prototype-first) on an OVMF Secure Boot VM and a SeaBIOS VM, using a minimal proxyDHCP and TFTP server and packet captures. Try the single-stage chain (`bootmgfw.efi` and option 252) and ConfigMgr's two-stage chain (`wdsmgfw.efi` or `wdsnbp.com` with option 250, then the boot manager with option 252). Try boot programs signed with the 2011 and 2023 certificates.
- Choose the single-stage or two-stage chain, or switch to the fallbacks in [Architecture](architecture.md#risks-to-settle-with-a-prototype-first), and record the choice and the captures in the architecture.
- Then build the proxyDHCP service, the TFTP server, boot file choice by client architecture, the no-reply rule for PCs whose deployment succeeded, and the PXE settings page.

Check:
- Both VM types PXE-boot WinPE from deployarr with no key press while a separate DHCP server hands out addresses.
- A UEFI VM whose deployment succeeded gets no reply and boots from its disk. A BIOS VM whose deployment succeeded gets `abortpxe.com`.
- A VM with no pre-staged record and no order row PXE-boots WinPE, and its disk is unchanged.
- A TFTP test suite covers option-less requests, window size clamping, a retransmitted request, an ACK in the middle of a window, and a missing file.
- The time to transfer `boot.wim` is measured and recorded with the image size, the network speed, and each block and window size tried.
- As many VMs as the lab host can run, and at least four, PXE-boot at the same time. Each one reaches WinPE. The time each takes to load `boot.wim` is recorded next to the time for one VM alone, to show how load time grows with the number of PCs.

### M-A. Add-on host

M-A starts when M2 passes and runs alongside M3 and M4. It needs only the server. M5 waits for it, because SQL order lookup is an add-on. See [Add-ons](architecture.md#add-ons).

Scope:
- The `deployarr-addons` crate, with `AddOnHost` as its only public item. It holds these:
  - the `wasmtime` runtime on its own `tokio` runtime, with a new instance per call
  - the level 0 bindings and the scope table
  - the resource budget, and the compile queue with its `wasmparser` caps
  - the envelope and trust checks
  - the operation journal and `reconcile`
  - the store of each add-on and the job queue
  - connections and HTTP egress
- `wit/`, with the packages, `wit/levels.toml`, and `wit/PENDING_CHANGES.md`.
- `deployarr-addon-sdk`, and `cargo xtask addon new`, `pack`, and `sign`.
- The Add-ons screen, the review dialog, settings forms rendered from a schema, and the Trust page.
- `deployarr.webhooks` under `addons/`, the first add-on.

Order inside M-A:
1. A runtime spike. It embeds `wasmtime` with these parts:
   - its own `tokio` runtime, with async calls
   - an empty WASI context
   - an epoch callback that yields with `tokio::task::yield_now` and then traps
   - the summing `ResourceLimiter` and the `wasmparser` caps
   - a new store per call
   - one host import behind `permit`, and a "hello" component

   Its checks are marked "spike" below. If any of them but the `Pure` check fails, stop and revisit the runtime choice before you build anything else. If `wasmtime-wasi` cannot supply a fixed clock and seeded randomness, rename the `Pure` scope and drop its promise of the same output.
2. `wit/` and the SDK, with the exports generated from the manifest.
3. The envelope, trust, the compile queue, and `stage` with the `describe` comparison.
4. The pure `plan`, the journal, `apply`, `reconcile`, and the kill tests.
5. The store, the job queue, settings, and the forms.
6. HTTP egress and connections.
7. `deployarr.webhooks`, the hostile test add-on, and the measurements.

Check:
- On Windows and Linux in CI, a test installs `deployarr.webhooks` from a signed package, saves an endpoint, and publishes a deployment event. A local listener receives one signed POST. With the listener down, the job retries and then succeeds.
- Spike. The same `.wasm` file runs on Windows and Linux. An endless loop dies at its deadline, a new store after a trap leaks nothing, an export built against an older minor version of a package links, and an empty WASI context refuses files and sockets. The runs in the research showed these on Linux only.
- Spike. 16 core instances that each grow a memory, and 8 tables of 30,000,000 elements that a start function fills, each stop at the 64 MiB budget (item 1). The 1 GiB allocation case below does not test the budget, because `wasmtime` refuses one large `memory.grow` without it.
- Spike. With 4 workers on the server's runtime, 8 concurrent calls into a looping guest run to their deadlines. Then 2 calls enter a test host function that blocks its thread for 3000 ms, through the same wrapper the drivers use, under a 100 ms deadline. A 50 ms heartbeat task on the server's runtime is never more than 250 ms late, and each call ends within 250 ms of its deadline (item 8).
- Spike. The `wasmparser` caps refuse the 485,757-byte component with 20,000 blocks and 20,000 locals before any compile. The spike records the compile time and memory at each cap and sets the three cap values (item 12).
- Spike. A `Pure` call that reads the wall clock, the monotonic clock, and 32 random bytes returns the same values in two runs (item 11).
- A test kills the server at every phase of install, upgrade, rollback, and uninstall. After each restart the add-on is at the old version or the new one, never between. `reconcile` leaves no stray staged package and no open operation, except one that waits for deployments. The data directory holds no compiled code after any step (item 5).
- An upgrade waits for a running deployment. Force for the same package takes over the waiting operation and commits. In a second run, `cancel` abandons it, and Roll back and Uninstall then proceed (item 14).
- Package tests cover a changed byte, a trusted comment for another ID, a key outside its scope, a revoked key, a revoked hash, a lower bundle version, an entry named `../x`, a link entry, a duplicate entry, and an oversized payload. The server refuses each with its own error. It refuses an unsigned package, which then installs after the setting and the confirmation, and shows as Unsigned.
- The server refuses a manifest with an unknown key, with one case per table and per field kind. One case is `allow_write = true` on a `sql_text` field (item 10).
- `stage` refuses a package that holds the 485,757-byte component from the spike check, before any compile. The server refuses the same component at start, before any compile (item 12).
- The host does not compile an unsigned package before the admin confirms its hash. Two packages staged at once compile one after the other (item 12).
- `stage` refuses a component whose `describe` answer names a kind that the manifest lacks, or lacks one that the manifest names. `apply` refuses the same component in an unsigned package, after the confirmation, and the old version stays active. A call for a kind that the component does not implement returns `unknown-kind` (item 15).
- Version 2 of a test add-on has a settings migration that replaces a `sql_text` value. Two more variants add one and move one. The host refuses each, and the dialog names the statement. A migration that drops a statement, or leaves a new one empty, installs, and the dialog lists it (item 3).
- Sandbox tests with a hostile test add-on: a socket connect, a file open, a call to an undeclared connection, a call to another add-on's store key, a 1 GiB allocation, and an endless loop each fail or trap inside the limits. The server keeps answering the agent API during each.
- A test add-on calls every host interface from every export, and each result matches the scope table. `steps.expand` reads settings. `steps.call` writes to `store` and enqueues a job. `steps.expand` is denied `store`, `jobs`, `sql`, and `http` (item 7).
- HTTP egress gets these paths as segments: `@attacker.example/x`, `\\attacker.example\x`, `//attacker.example/x`, and `%2e%2e`. Each request reaches the listener for the connection's host, with the segment percent-encoded. The host refuses `..`, `.`, and an empty segment. A second listener that stands in for `attacker.example` receives nothing (item 13).
- A test stores a known password in a secret field and in a connection. A test add-on that asks through every host interface never receives it, and a search of the debug log for the password finds nothing.
- Measured and recorded on both systems: the compile time of each component at start, the instantiation time, the memory per call, and the largest function body and local count among the in-tree add-ons, which set the compile caps.
- A test compares the WIT enum `identity-kind` with `IdentityKind` in `deployarr-core`.
- A component built against an older copy of a package, with one function fewer, links against the host's newer copy.

### M5. Order lookup

Order lookup is high priority. M5 starts when M3 and M-A pass, and runs alongside M4. It needs the WinPE agent from M3 and the add-on host from M-A, but not PXE. M6 starts when both M4 and M5 pass.

Order lookup is an optional add-on. A server without it still deploys from pre-staged records and the manual list.

Scope:
- `deployarr:sql` in the add-on host, with `tiberius` for SQL Server and `sqlx` for PostgreSQL, MySQL, and MariaDB. It covers named parameter rewriting, binding, the [write guard](architecture.md#write-guards) of each engine, row and time caps, pools that are rebuilt when a connection changes, and the host's connection test. See [SQL connections](architecture.md#sql-connections).
- The lookup chain with `Resolver::AddOn`, the skip rules, and the trace:
  1. pre-staged record
  2. the add-on's resolver by baseboard serial
  3. the add-on's order number prompt
  4. manual list
- A chain an admin can reorder or trim, checked when it is saved, and a no-match screen for a PC that no resolver matches.
- Placeholder filtering for SMBIOS values.
- `deployarr.sql-order-lookup`, with its named queries, its OS-code map, and its Test lookup action.
- The variable trace in the UI.

Check:
- Integration tests pass against SQL Server, PostgreSQL, MySQL, and MariaDB containers in CI. A cloud session cannot run SQL Server, so this check runs in CI or the lab. The SQL Server test uses `Encrypt=true` and a CA given on the connection form.
- On PostgreSQL, MySQL, and MariaDB, the integration tests check the [write guards](architecture.md#write-guards) with an account that may write. Named queries that run `INSERT`, `TRUNCATE TABLE`, and `DROP TABLE` each fail and leave the data unchanged (item 2). A named query that sets the session to read-write, or one that starts a read-write transaction, does not let the next named query on the same connection write. A named query that holds two statements fails and changes nothing. The known MariaDB gaps are not part of this check. They are a query that starts with `SET STATEMENT` ([#104](https://github.com/Tsukidy/deployarr/issues/104)) and a compound statement ([#118](https://github.com/Tsukidy/deployarr/issues/118)).
- On SQL Server, `TRUNCATE TABLE orders` and `DROP TABLE orders`, saved as named queries, run with an account that may write. The test records whether the rollback restored the table and its rows. If it did not, the rollback leaves the design, and [Write guards](architecture.md#write-guards) says so (item 2).
- A parameter that holds `'; DROP TABLE orders; --` is bound and returns no row.
- The host refuses to save a `sql_text` setting with two statements, or with a parameter that the field does not declare. A guest that names a query path that is not one of its `sql_text` settings gets `no-such-query`.
- A VM whose baseboard serial is in the test order table deploys the order's OS with no input.
- A VM with an unknown serial stops at the order number prompt, deploys after the number is typed, and does not touch its disk before that.
- A VM with only placeholder serials never reaches the add-on's identity resolver.
- A resolver that returns a `matched_on` kind that the server did not send is an error in the trace, and the chain moves on (item 4).
- A resolver that sleeps past its deadline counts as an error, and the chain moves on.
- With the add-on disabled, the chain skips its resolvers, the trace says so, and an unknown VM reaches the no-match screen with its disk untouched.
- With the add-on not installed, a VM with no pre-staged record shows the manual list and deploys the task sequence picked there. The server opens no connection besides its own SQLite file. The base binary holds the drivers and uses none.
- With a chain of only the pre-staged record, an unknown VM stops at the no-match screen, and its disk is untouched.

### M6. Drivers

Scope:
- The INF parser and importer for folders, ZIP files, and CAB files.
- The driver library UI, driver profiles, and matching by hardware ID with architecture and OS build filters.
- The WinPE driver set and injection one package at a time.
- `VerifyDrivers`, with `pnputil` remediation and the missing-driver report.

Check:
- A VM with virtio disk, network, and balloon devices deploys with drivers that come only from the library. `VerifyDrivers` reports no problem devices.
- After the balloon driver is removed from the library, the next deployment's `VerifyDrivers` reports that device with its hardware ID and applies the configured policy.

### M7. Applications

Scope:
- The application library, including the MSI reader, success codes, reboot behavior, detection rules, and dependencies.
- App lists that come from a task sequence, a pre-staged record, or an order column.

Check:
- A lab deployment installs four apps:
  - one app that exits with 3010. The PC reboots, then continues with the next app.
  - a two-app dependency chain
  - one app that detection marks as already installed, which is skipped
- An app that exits with an unexpected code fails its step.

### M8. Task sequence editor and management UI

Scope:
- The tree editor, the step palette, a typed form for each action, and the condition builder.
- Version history with a diff view, and templates.
- The monitoring dashboard.
- Join profiles for a workgroup and a domain, and the join kinds that add-ons declare.
- Step kinds from add-ons in the palette, forms drawn from their schemas, the read-only view of a step whose add-on is unavailable, and plan errors. See [Task sequences stay loadable](architecture.md#task-sequences-stay-loadable).
- Export and import of task sequences, with the descriptors of their add-on steps and the derived list of the add-ons they need.
- `ApplyJoinProfile` join slots in the built-in Windows client template.
- Three add-ons, which prove the step, join kind, and importer extension points before the API freezes in M9:
  - `deployarr.entra-join`, for Entra ID join through bulk enrollment
  - `deployarr.autopilot`, for Autopilot registration
  - `deployarr.mdt-import`, an importer for MDT task sequences (`ts.xml`) that applies the conversions listed in [MDT 8450 binaries reference](research/mdt-8450-binaries.md#what-changes-in-deployarr). The owner can move it after the first release. The importer extension point then leaves API level 1.

Check:
- Build the standard Windows client sequence from scratch in the UI, then deploy with it in the lab. It deploys to the UEFI VM, with Microsoft's KMS client setup key for Pro and `online` off.
- Edit a sequence while a deployment that uses it is running. The running deployment keeps its pinned version.
- The UEFI VM, which runs Pro, joins the lab domain. No file in `%WINDIR%\Panther` holds the join password afterward.
- The BIOS VM, which runs Home, has no pre-staged record, so lookup reaches its order row by baseboard serial. The row sets a `Domain` profile. The deployment stops before the disk changes, and its error names the edition ID and the profile.
- A human checks Entra ID join and Autopilot registration on Pro against a test tenant, because the lab has no tenant. Home supports neither (see [Directory join](architecture.md#directory-join)).
- The lowered program for `deployarr.entra-join` holds one `AddProvisioningPackage` step and no command. Planning refuses a test add-on that expands to a `RunCommand` naming the enrollment package (item 9).
- A step that sets `JoinProfile` from a `Workgroup` profile to a join profile from an add-on fails, and so does one that sets it from an add-on's profile to `Domain`. A step that switches from `Workgroup` to `Domain` passes. See [Directory join](architecture.md#directory-join).
- Disable `deployarr.entra-join`. The sequence that uses it opens, shows the step read-only, exports, and imports on a second server that never had the add-on. A deployment with it stops at the lookup screen with the reason, and its disk is untouched. Disable the step, and the sequence deploys.
- Upgrade `deployarr.autopilot` from a version with step config version 1 to one with version 2 while a deployment runs. The running deployment finishes on its frozen program. The next one deploys with the migrated config. The saved task sequence version is byte for byte the same before and after.
- Roll the upgrade back. The add-on's store matches its snapshot.
- The server refuses to uninstall the add-on, and the refusal lists the sequence, the join profile, and the chain entry that use it.
- The MDT importer turns the 8456 `Client.xml` into a draft that passes `validate`, with a note for each step that it could not convert.

### M9. Hardening and release

Scope:
- Admin login with local accounts, per-PC local admin passwords, secrets encryption, and the audit log.
- Backup and restore. They include add-on packages, add-on stores, grants, and the trust bundle version.
- A Windows installer, a Linux package, a container image, and how-to guides.
- The add-on API freezes at level 1. `wit/levels.toml` moves level 0 to level 1, `wit/PENDING_CHANGES.md` is cleared, and release builds refuse level 0.
- The root key ceremony, the first `addon-trust.json`, and official add-ons signed in the release pipeline. See [Package and signing](architecture.md#package-and-signing).
- Password confirmation on add-on changes, which needs the admin login above.
- The pre-upgrade check in the installers, which names the add-ons that an upgrade would disable.
- The add-on author guide and the manifest and API reference, generated for each level from `wit/` and the `FormSchema` types.

Check:
- On a clean Windows Server and a clean Ubuntu server, install from the packages and deploy one real PC from each.
- A CI job fails when a released WIT file or a released level's row changes.
- An add-on built at the frozen level 1 installs on the release build. The release build refuses a level 0 build and shows both ranges.
- A restore on a clean server brings back every add-on, enabled as before, with its data.
- The installer, run against a server with an add-on that the new version cannot serve, names the add-on and stops for confirmation.

## After the first release

None of these are scheduled yet:

- a Windows Update step
- BitLocker
- an auto-logon session for installers that fail as LocalSystem
- iPXE and wimboot over HTTP for BIOS or Secure Boot off
- multicast
- PXE across routed subnets through DHCP relays, after a lab VM boots through a relay
- IPv6 PXE, with DHCPv6 option 59 and the Microsoft vendor options that ConfigMgr sends
- ARM64
- Linux deployment
- vendor driver catalogs from Dell, HP, and Lenovo, as the `deployarr.driver-catalogs` add-on with API level 2 (see below)
- a PowerShell module with MDT's `TSEnv:` and `TSEnvList:` drives, for porting MDT scripts
- a desktop program for the server, built with Tauri, that shows the web UI in its own window

API level 2 is the first proof that the add-on API grows without a break. Its scope is the `deployarr:library` package and the `[[schedule]]` manifest table, then `deployarr.driver-catalogs`. Its check is that every level 1 add-on from the release runs unchanged on the level 2 server, and that the level 1 server refuses the level 2 add-on with "this add-on needs a newer deployarr server". See [Extension points](architecture.md#extension-points).

## Open questions

The add-on design session raised four questions with the owner. Each has a default, and none is needed before M9 freezes the add-on API. [One-way doors](architecture.md#one-way-doors) names the doors that they shape.

- **Root signing keys.** Who holds the root keys that every server embeds, and how many must sign the trust bundle? The default is that the owner holds them offline, and that 2 of 3 sign the trust bundle.
- **Who signs add-ons.** Does the project sign every add-on, or do third parties get publisher entries in the trust bundle? The default is that the project signs the official add-ons, and admins can add their own local keys.
- **Support window.** How long does an old add-on API major version stay supported? The default is 24 months after its replacement ships.
- **SQL Server login.** How does deployarr sign in to SQL Server? The default is that Windows authentication works only when the deployarr server runs on Windows. On Linux, SQL Server uses SQL logins.
