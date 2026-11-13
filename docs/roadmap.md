# Roadmap

This page lists the build order for deployarr. Each milestone ends in a check that someone can run and watch pass. A milestone starts only when the one before it passes its check, except where a milestone says otherwise. For the design behind each item, see [Architecture](architecture.md).

## Test lab

From milestone 1 on, every check runs in a lab that `xtask lab` drives. The lab is a separate network, so a mistake cannot erase a real PC.

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
- From milestone 3 on, each milestone also deploys at least one real PC of the kind you build.
- From milestone 8 on, a Windows Server evaluation VM is the lab's domain controller, for the domain join check on the UEFI VM, which runs Pro.

## Milestones

### M0. Workspace and core model

Scope:
- Cargo workspace with the crates listed in [Crate layout](architecture.md#crate-layout).
- CI on Linux and Windows. `rustfmt` and `clippy -D warnings` must pass.
- `deployarr-core`: the task sequence model, its JSON format with a schema version, the compiler, the condition evaluator, and variable substitution.
- A CI check that fails if the agent imports a DLL that base WinPE lacks.

Check:
- `cargo test --workspace` passes on Linux and Windows.
- A test rebuilds the group, condition, and phase structure of MDT's `Client.xml` in the deployarr model and compiles it. This proves the model can express everything MDT's standard sequence uses.
- Table tests cover the condition rules listed in [MDT 8450 binaries reference](research/mdt-8450-binaries.md#conditions): integer versus string comparison, `Like` wildcards, `Exists` on an empty value, and missing files and registry values. Each place where deployarr differs from MDT on purpose has its own test.
- The compiler rejects an empty success list, a `run_in` the action cannot honor, a WinPE `Restart` before the state moves to disk, and an empty `All`, `Any`, or `Not`.

### M1. Engine and agent in a running Windows

Scope:
- `deployarr-engine`: the state file, journal, resume logic, and reboot outcomes.
- `deployarr-agent` in service mode, running a task sequence from a local file.
- The actions `RunCommand`, `RunPowerShell`, `SetVariable`, and `Restart`.
- Variable, registry, file, folder, installed-MSI, and WMI conditions.
- The single-run mutex, the reboot marker, job objects with `max_runtime_secs`, and the `var get` and `var set` channel for step processes.
- CMTrace logs.

Check:
- A test crashes the engine at every step boundary of a 20-step sequence. Each resumed run ends in the same state as a run with no crash.
- A test kills the agent in the middle of a state save. The next start reads either the old state or the new state.
- In a Windows 11 VM, a sequence with three reboots finishes, and its log shows each step exactly once.
- A step that exceeds `max_runtime_secs` fails, and no process from its tree is still running.

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

Check:
- A UEFI VM with Secure Boot on and a BIOS VM both boot the deployarr ISO. Each finishes with Windows 11 in its configured end state, with no input, using a pre-staged record. The UEFI VM installs Pro, and the BIOS VM installs Home. Neither run types a key, so their sequence has no `ActivateWindows` step.
- One real PC does the same with the edition its sticker names, and its product key typed at the prompt. `slmgr /dli` on that PC shows the last five characters of that key.
- On the UEFI VM, a sequence with an `ActivateWindows` step and `online` off takes Microsoft's KMS client setup key for Pro, and `slmgr /dli` shows `T83GX`. The same key with one character changed fails the step, and the deployment shows the error. The lab records the error code.
- The UEFI VM runs once with each end state. With `ReadyToUse`, it boots to the sign-in screen with no OOBE. With `CustomerOobe`, it shuts down, shows OOBE on its next boot, and has no account that deployarr created. Neither leaves a cached answer file in `%WINDIR%\Panther`.
- The BIOS VM's task sequence turns off the UEFI and TPM readiness checks. A VM below a readiness minimum stops before its disk changes, and its log names the failed check and the value found.
- On the UEFI VM and the real PC, the agent reads the TPM version and the Secure Boot state in WinPE. It reports the TPM version that the PC has, and its log names the device ID that matched and whether it is a hardware ID or a compatible ID. If base WinPE lacks the Secure Boot registry value, the gap is recorded in [Agent](architecture.md#agent).

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

### M5. Order lookup

Order lookup is high priority. M5 starts as soon as M3 passes its check and runs alongside M4, because it needs the WinPE agent from M3 but not PXE. M6 starts when both M4 and M5 pass.

Order lookup is optional for a deployarr installation. A server with no data source still deploys from pre-staged records and the manual list.

Scope:
- Data sources for SQL Server (`tiberius`), PostgreSQL, and MySQL (`sqlx`). All of them are optional.
- Named queries with bound parameters, the OS-code map, and the lookup chain:
  1. pre-staged record
  2. order by baseboard serial
  3. order number prompt
  4. manual list
- A chain an admin can reorder or trim, checked when it is saved, and a no-match screen for a PC that no resolver matches.
- Placeholder filtering for SMBIOS values.
- The variable trace in the UI and the lookup test box.

Check:
- Integration tests pass against SQL Server, PostgreSQL, and MySQL containers.
- A VM whose baseboard serial is in the test order table deploys the order's OS with no input.
- A VM with an unknown serial stops at the order number prompt, deploys after the number is typed, and does not touch its disk before that.
- With no data source configured, a VM with no pre-staged record shows the manual list and deploys the task sequence picked there. The server opens no connection to an order database.
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
- Join profiles for a workgroup, a domain, Entra ID bulk enrollment, and Autopilot registration.

Check:
- Build the standard Windows client sequence from scratch in the UI, then deploy with it in the lab. It deploys to the UEFI VM, with Microsoft's KMS client setup key for Pro and `online` off.
- Edit a sequence while a deployment that uses it is running. The running deployment keeps its pinned version.
- The UEFI VM, which runs Pro, joins the lab domain. No file in `%WINDIR%\Panther` holds the join password afterward.
- The BIOS VM, which runs Home, has no pre-staged record, so lookup reaches its order row by baseboard serial. The row sets a `Domain` profile. The deployment stops before the disk changes, and its error names the edition ID and the profile.
- A human checks Entra ID join and Autopilot registration on Pro against a test tenant, because the lab has no tenant. Home supports neither (see [Directory join](architecture.md#directory-join)).

### M9. Hardening and release

Scope:
- Admin login with local accounts, per-PC local admin passwords, secrets encryption, and the audit log.
- Backup and restore.
- A Windows installer, a Linux package, a container image, and how-to guides.

Check:
- On a clean Windows Server and a clean Ubuntu server, install from the packages and deploy one real PC from each.

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
- vendor driver catalogs from Dell, HP, and Lenovo
- an importer for MDT task sequences (`ts.xml`) that applies the conversions listed in [MDT 8450 binaries reference](research/mdt-8450-binaries.md#what-changes-in-deployarr)
- a PowerShell module with MDT's `TSEnv:` and `TSEnvList:` drives, for porting MDT scripts
- a desktop program for the server, built with Tauri, that shows the web UI in its own window
