# Roadmap

This page lists the build order for deployarr. Each milestone ends in a check that someone can run and watch pass. A milestone starts only when the one before it passes its check. For the design behind each item, see [Architecture](architecture.md).

## Test lab

From milestone 1 on, every check runs in a lab that `xtask lab` drives. The lab is a separate network, so a mistake cannot erase a real PC.

- The lab host is a Linux machine with KVM, or a Windows machine with Hyper-V.
- UEFI test VMs use QEMU with OVMF firmware that has Secure Boot on and Microsoft's keys enrolled. Legacy BIOS test VMs use SeaBIOS.
- The VMs sit on an isolated bridge with a plain DHCP server. deployarr provides only PXE.
- The OS under test is the Windows 11 Enterprise evaluation ISO.
- virtio disk and network devices test the driver path, because Windows 11 has no inbox drivers for them. They must come from the deployarr driver library and the WinPE driver set.
- `-smbios type=2,serial=...` gives each VM a known baseboard serial for order lookup tests.
- From milestone 3 on, each milestone also deploys at least one real PC of the kind you build.

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
- The single-run mutex, the reboot marker, job objects with `max_runtime`, and the `var get` and `var set` channel for step processes.
- CMTrace logs.

Check:
- A test crashes the engine at every step boundary of a 20-step sequence. Each resumed run ends in the same state as a run with no crash.
- A test kills the agent in the middle of a state save. The next start reads either the old state or the new state.
- In a Windows 11 VM, a sequence with three reboots finishes, and its log shows each step exactly once.
- A step that exceeds `max_runtime` fails, and no process from its tree is still running.

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
- The offline service install, the handoff to the new OS, the finish action, and cleanup.

Check:
- A UEFI VM with Secure Boot on and a BIOS VM both boot the deployarr ISO. Each finishes with Windows 11 in its configured end state, with no input, using a pre-staged record.
- One real PC does the same.
- The BIOS VM's task sequence turns off the UEFI and TPM readiness checks. A VM below a readiness minimum stops before its disk changes, and its log names the failed check and the value found.
- On the UEFI VM and the real PC, the agent reads the TPM version and the Secure Boot state in WinPE. If base WinPE lacks `tbs.dll` or the Secure Boot registry value, the gap is recorded in [Agent](architecture.md#agent).

### M4. PXE

Scope:
- Prototype first. Answer the six questions in [Risks to settle with a prototype first](architecture.md#risks-to-settle-with-a-prototype-first) on an OVMF Secure Boot VM and a SeaBIOS VM, using a minimal proxyDHCP and TFTP server and packet captures. Try the single-stage chain (`bootmgfw.efi` and option 252) and ConfigMgr's two-stage chain (`wdsmgfw.efi` or `wdsnbp.com` with option 250, then the boot manager with option 252). Try boot programs signed with the 2011 and 2023 certificates.
- Choose the single-stage or two-stage chain, or switch to the fallbacks in [Architecture](architecture.md#risks-to-settle-with-a-prototype-first), and record the choice and the captures in the architecture.
- Then build the proxyDHCP service, the TFTP server, boot file choice by client architecture, the no-reply rule for PCs with no order or a finished deployment, and the PXE settings page.

Check:
- Both VM types PXE-boot WinPE from deployarr with no key press while a separate DHCP server hands out addresses.
- A UEFI VM with no order gets no reply and boots from its disk. A BIOS VM with no order gets `abortpxe.com`.
- A TFTP test suite covers option-less requests, window size clamping, a retransmitted request, an ACK in the middle of a window, and a missing file.
- The time to transfer `boot.wim` is measured and recorded with the image size, the network speed, and each block and window size tried.
- At least four VMs PXE-boot at the same time. The time each takes to load `boot.wim` is recorded next to the time for one VM alone.

### M5. Order lookup

Scope:
- Data sources for SQL Server (`tiberius`), PostgreSQL, and MySQL (`sqlx`).
- Named queries with bound parameters, the OS-code map, and the lookup chain:
  1. pre-staged record
  2. order by baseboard serial
  3. order number prompt
  4. manual list
- Placeholder filtering for SMBIOS values.
- The variable trace in the UI and the lookup test box.

Check:
- Integration tests pass against SQL Server, PostgreSQL, and MySQL containers.
- A VM whose baseboard serial is in the test order table deploys the order's OS with no input.
- A VM with an unknown serial stops at the order number prompt, deploys after the number is typed, and does not touch its disk before that.

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

Check:
- Build the standard Windows client sequence from scratch in the UI, then deploy with it in the lab.
- Edit a sequence while a deployment that uses it is running. The running deployment keeps its pinned version.

### M9. Hardening and release

Scope:
- Admin login with local accounts, per-PC local admin passwords, secrets encryption, and the audit log.
- Backup and restore.
- A Windows installer, a Linux package, a container image, and how-to guides.

Check:
- On a clean Windows Server and a clean Ubuntu server, install from the packages and deploy one real PC from each.

## After the first release

None of these are scheduled yet:

- domain join, Entra ID join, and Autopilot hash upload
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

## Open questions

These do not block M0 to M2. Each one needs an answer before the milestone named in it.

1. What state should a finished PC be in? One option is ready to use, with OOBE skipped and a local admin account. The other is ready for the customer. The PC deploys in audit mode and then runs `sysprep /oobe /shutdown`, so the customer sees OOBE on first boot. Needed before M3, because it changes the unattend templates and the finish action.
2. Should PCs join a domain, Entra ID, or Autopilot, or stay in a workgroup? Needed before M8.
3. How are PCs activated? Options are a firmware key, KMS or MAK, or OEM key injection. Needed before M3.
