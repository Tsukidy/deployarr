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

### M1. Engine and agent in a running Windows

Scope:
- `deployarr-engine`: the state file, journal, resume logic, and reboot outcomes.
- `deployarr-agent` in service mode, running a task sequence from a local file.
- The actions `RunCommand`, `RunPowerShell`, `SetVariable`, and `Restart`.
- Variable, registry, file, and folder conditions.
- CMTrace logs.

Check:
- A test crashes the engine at every step boundary of a 20-step sequence. Each resumed run ends in the same state as a run with no crash.
- In a Windows 11 VM, a sequence with three reboots finishes, and its log shows each step exactly once.

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
- Agent WinPE mode: `wpeinit`, `drvload`, network, the console screen, and fact gathering (SMBIOS, adapters, firmware, disks, device IDs).
- Disk layouts for UEFI and BIOS, image download and apply, `bcdboot`, WinRE, and the unattend file.
- The offline service install, the handoff to the new OS, the finish action, and cleanup.

Check:
- A UEFI VM with Secure Boot on and a BIOS VM both boot the deployarr ISO. Each finishes with Windows 11 in its configured end state, with no input, using a pre-staged record.
- One real PC does the same.

### M4. PXE

Scope:
- Prototype first. Serve `bootmgfw.efi`, a BCD template, `boot.sdi`, and `boot.wim` to an OVMF Secure Boot VM from a minimal TFTP server. Serve `pxeboot.n12` to a SeaBIOS VM. Try a 2011-signed and a 2023-signed boot manager. Record which DHCP options each one needs.
- Decide whether to continue with this design or switch to the fallbacks in [Architecture](architecture.md#risks-to-settle-with-a-prototype-first).
- Then build the proxyDHCP service, the TFTP server with block size and window size, boot file choice by client architecture, the stop-after-success rule, and the PXE settings page.

Check:
- Both VM types PXE-boot WinPE from deployarr while a separate DHCP server hands out addresses.
- The time to transfer `boot.wim` is measured and recorded with the image size and network speed.

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
- Admin login, per-PC local admin passwords, secrets encryption, and the audit log.
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
- ARM64
- Linux deployment
- vendor driver catalogs from Dell, HP, and Lenovo
- writing deployment status back to the order database

## Open questions

These do not block M0 to M2. Each one needs an answer before the milestone named in it.

1. What state should a finished PC be in? One option is ready to use, with OOBE skipped and a local admin account. The other is ready for the customer. The PC deploys in audit mode and then runs `sysprep /oobe /shutdown`, so the customer sees OOBE on first boot. Needed before M3, because it changes the unattend templates and the finish action.
2. Should PCs join a domain, Entra ID, or Autopilot, or stay in a workgroup? Needed before M8.
3. How are PCs activated? Options are a firmware key, KMS or MAK, or OEM key injection. Needed before M3.
4. How many PCs deploy at once, and across how many subnets? Needed before M4, to size TFTP and decide between proxyDHCP and DHCP options.
5. Should finished deployments write status back to the order database? Needed before M5, if yes.
6. How do admins sign in: local accounts, Active Directory, or OIDC? Needed before M9.
