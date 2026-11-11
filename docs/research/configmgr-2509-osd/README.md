# ConfigMgr 2509 OS deployment components

What ConfigMgr 2509's installer ships for operating system deployment (OSD),
and what that means for Windows 11 deployments. The question going in was
whether ConfigMgr carries updated MDT components.

## Bottom line

- **ConfigMgr 2509 does not ship MDT components.** None of the 8,782 files on
  the media or inside its nested MSI and CAB packages is an MDT script or
  assembly. MDT is still a separate add-on. The console only mentions it in two
  error messages about task sequences that use "an extension, such as the
  Microsoft Deployment Toolkit (MDT), which is not present".
- What ConfigMgr has instead is its own native task sequence engine: 49 x64
  C++ binaries at version 5.00.9141.1002, linked 2025-09-26, plus 5 Visual C++
  runtime DLLs. Most of MDT's `ZTI*` scripts have a native counterpart here
  (see the mapping below).
- For Windows 11, the native steps help, but their defaults don't enforce
  Windows 11 requirements:
  - **Check Readiness** can require UEFI and TPM 2.0 (enabled and activated).
    Both checks are off by default. The default thresholds are 512 MB RAM,
    800 MHz CPU and 25,000 MB free disk.
  - It has **no Secure Boot check and no CPU model or core-count check**.
  - Secure Boot state is still available to conditions through the
    `_TSSecureBoot` variable.
  - **Set Dynamic Variables** gathers make, model, serial number, UUID, asset
    tag, MAC addresses, IP addresses and gateways. It has no chassis type or
    VM detection like MDT's `ZTIGather`.

## Source

| Item | Value |
| --- | --- |
| Link given | `https://go.microsoft.com/fwlink/?linkid=2350307&clcid=0x409&culture=en-us&country=us` |
| Redirects to | `https://download.microsoft.com/download/50e53030-0fcb-4076-8808-676c79a0ed9f/ConfigMgr_2509.exe` |
| Size | 1,254,017,320 bytes (Last-Modified 2026-01-16) |
| SHA-256 | `4cb0380e2b1c43f1e0b2da266beddd1b975dd72537f8d8113c702eadf1e6e313` |
| Format | 7-Zip self-extractor, 4,102 files (`AUTORUN`, `LanguagePack`, `SMSSETUP`) |
| OSD build | 5.00.9141.1002, every binary in `SMSSETUP/OSD/bin/x64` |
| Signer | Microsoft Corporation, issued by Microsoft Code Signing PCA 2011 (decoded, not chain-verified) |

## Finding 1: no MDT components

Method:

1. Unpacked the installer, then every non-empty MSI and CAB outside
   `LanguagePack` (41 packages, 4,680 more files).
2. Searched file names for `ZTI*`, `LTI*`, `*BDD*` and `*.wsf`. There were no
   matches. The only `.vbs` on the media is
   `TOOLS/PortConfiguration/Portswitch.vbs`, which has nothing to do with MDT.
3. Searched file contents, in ASCII and UTF-16LE, for `Microsoft.BDD`,
   `ZTIUtility`, `ZTIGather`, `DeploymentShare` and `MDTIntegration`. There
   were no matches.
4. Searched for `Deployment Toolkit`. It appears only in error strings:
   - `AdminUI.UIResources.dll` (console, also in CMPivot) and
     `DatabaseResources.dll` (reporting): "The Task Sequence XML is invalid.
     The task sequence may contain steps from an extension, such as the
     Microsoft Deployment Toolkit (MDT), which is not present at the
     destination."
   - `Microsoft.ConfigurationManagement.MigrationLibrary.dll` and
     `MigrationManager.dll`: warnings for task sequences migrated from 2007
     and 2012 that contain non-standard actions.

Bounds: the `LanguagePack` CABs weren't unpacked. Only their compressed bytes
were searched, so their contents are effectively unexamined. Content that ConfigMgr downloads later, such as Windows ADK, WinPE,
or in-console updates, is not on this media and wasn't examined.

## Native counterparts to MDT scripts

The step → executable mapping comes from `SMSSETUP/BIN/X64/_TaskSequenceProvider.mof`.
The full catalog, with every property, default and variable, is in
[`task-sequence-steps.md`](task-sequence-steps.md). The MDT column comes from
MDT 8456 and is not on this media.

| MDT script | ConfigMgr 2509 step | Executable |
| --- | --- | --- |
| `ZTIGather.wsf` + `CustomSettings.ini` rules | Set Dynamic Variables | `OSDSetDynamicVariables.exe` |
| `ZTIValidate.wsf` | Check Readiness | `OSDPrestartCheck.exe` |
| `ZTIDiskpart.wsf` | Format and Partition Disk | `osddiskpart.exe` |
| `LTIApply.wsf` | Apply Operating System / Apply Data Image | `OSDApplyOS.exe` |
| `ZTIDrivers.wsf` | Auto Apply Drivers / Apply Driver Package | `osddriverclient.exe` |
| `ZTIBde.wsf` | Enable / Pre-provision / Disable BitLocker | `OSDBitLocker.exe`, `OSDOfflineBitlocker.exe` |
| `ZTIUserState.wsf` | Capture / Restore User State | `osdmigrateuserstate.exe` |
| `ZTIDomainJoin.wsf` | Join Domain or Workgroup | `osdjoin.exe` |
| `ZTIPowerShell.wsf` | Run PowerShell Script | `OSDRunPowerShellScript.exe` |
| `ZTIWindowsUpdate.wsf` | Install Software Updates | `TSInstallSWUpdate.exe` |
| `ZTIApplications.wsf` | Install Application / Install Package | `smsappinstall.exe`, `smsswd.exe` |
| `ZTIConfigure.wsf` | Apply Windows / Network Settings | `osdwinsettings.exe`, `osdnetsettings.exe` |
| `ZTISetVariable.wsf` | Set Task Sequence Variable | `tsenv.exe` |
| (in-place upgrade) | Upgrade Operating System | `OSDUpgradeOS.exe` |

All of these are native code. None needs `cscript.exe` or VBScript.

## Finding 2: Check Readiness (`OSDPrestartCheck.exe`)

Defaults from the MOF:

| Check | Variable | Default |
| --- | --- | --- |
| Memory | `OSDCheckMemory` / `OSDMemory` | on, 512 MB |
| Processor speed | `OSDCheckProcessorSpeed` / `OSDProcessorSpeed` | on, 800 MHz |
| Free disk space | `OSDCheckFreeDiskSpace` / `OSDFreeDiskSpace` | on, 25,000 MB |
| OS type | `OSDCheckOSType` / `OSDOSType` | on, `CLIENT` |
| Architecture | `OSDCheckOSArchitecture` / `OSDOSArchitecture` | off, `64` |
| AC power, network, wired network | `OSDCheckPowerState`, `OSDCheckNetworkConnected`, `OSDCheckNetworkWired` | off |
| UEFI | `OSDCheckDeviceUEFI` | off |
| TPM 2.0 enabled / activated | `OSDCheckDeviceTpmEnabled` / `OSDCheckDeviceTpmActivated` | off |
| Min/max OS version, client version, language | `OSDCheckMinOSVersion`, `OSDCheckMaxOSVersion`, `OSDCheckCMClientMinVersion`, `OSDCheckOSLanguageID` | off (language `1033`) |

Behavior, from Ghidra pseudocode of the single large check function
(`0x140001780`):

- Each check reads its `OSDCheck*` variable. If the check is on, the step calls
  a `tscore.dll` helper and writes a result to `_TS_CR<NAME>` (`1` or `0`). It
  also writes the required value to `_TS_CRREQUIRED<NAME>`. Later steps can
  branch on these variables.
- Helpers: `GetTotalPhysicalMemory`, `GetProcessorSpeed`,
  `IsFreeDiskSpaceAvailable` (falls back to `IsFreeDiskSpaceNoPartitionsAvailable`
  for unpartitioned disks), `GetOsType`, `GetClientVersion`, `IsUEFI`,
  `GetBatteryStatus` (status `2` counts as AC), and
  `ConnectedNetworkAdapterIsFound`.
- TPM: `TS::BitLocker::Tpm::InitializeEx` returns a version string, which is
  parsed with `swscanf("%d.%d")`. A major version below 2 fails with "Found
  version of TPM is %d.%d which is less than required 2.0". Then
  `Tpm::GetInitialState` is read: bit 0 (value 1) means enabled, and bit 1
  (value 2) means activated. Strings in `tscore.dll` show the TPM class comes
  from WMI `Win32_Tpm`.
- There is no Secure Boot check, and processor model, generation and core
  count are not checked.

Windows 11 gap: Microsoft's published Windows 11 minimums (4 GB RAM, 64 GB
storage, 1 GHz with 2 or more cores, TPM 2.0, UEFI with Secure Boot) are all
higher than these defaults. A Windows 11 task sequence has to raise the
thresholds, turn on the UEFI and TPM checks, and test `_TSSecureBoot` itself.

## Finding 3: Secure Boot and firmware variables

- `TS::Utility::GetSecureBootState` (`tscore.dll`, decompiled) reads
  `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State\UEFISecureBootEnabled`.
  It returns `"Enabled"` for 1, `"Disabled"` for 0, and `"NA"` when the value is
  missing or unreadable. The `"NA"` default was read from the binary at RVA
  `0x23dea4`.
- `tsmanager.exe` logs `SecureBootState: %s` and stores the result in its
  `EnvironmentVar::SecureBootState` variable. The only matching variable name
  in that binary is `_TSSecureBoot`. Linking the two is inferred from the
  strings, not traced.
- UEFI detection uses `GetFirmwareType` (strings: `TS::Boot::GetSystemFirmwareType`,
  `Firmware::IsEfi`). The console's default disk steps use `_SMSTSBootUEFI` as
  their condition.

## Finding 4: default Windows 11 disk layout (console wizard)

These are the CIL constants in `AdminUI.NewTaskSequenceWizard.dll`, methods
`Utils.GetDefaultUefiPartitionDiskObject` and `GetDefaultBiosPartitionDiskObject`.

| Firmware | Layout |
| --- | --- |
| UEFI (`_SMSTSBootUEFI` true) | GPT, boot disk 0: EFI FAT32 500 MB, MSR 128 MB, Windows NTFS 99%, Recovery NTFS 100% of the remainder |
| BIOS | MBR, disk 0: System Reserved NTFS 350 MB (bootable), Windows NTFS 99%, Recovery NTFS 100% of the remainder |

Windows takes 100% instead of 99% when the recovery partition is left out.
That's inferred from the branch that chooses between the two constants; the
caller's flag wasn't traced. The recovery partition is about 1% of the disk,
so its size scales with disk size.

## Finding 5: Upgrade Operating System (`OSDUpgradeOS.exe`)

`UpgradeWindows::CreateUpgradeCmdLine` (decompiled, `0x1400078a8`) builds:

```text
<source>\SETUP.EXE /ImageIndex <n> /auto <PreserveSettings> /quiet /noreboot /EULA accept
    [/postoobe "<SetupComplete.cmd>" /postrollback "<SetupRollback.cmd>" [/postrollbackcontext system]]
    [/installdrivers "<drivers>"]
    /DynamicUpdate Enable|Disable
    [/pkey <key>]
    [/compat IgnoreWarning]
    [/compat ScanOnly]
    [<OSDSetupAdditionalUpgradeOptions>]
```

- `/postoobe` and `/postrollback` are added only when not in ScanOnly mode.
  `/postrollbackcontext system` is added only when the setup build is above
  17133 (`0x42ED`). Reading that field as the build number is an inference.
- The step installs `SetupComplete.cmd` and `SetupRollback.cmd` from the
  templates into `%WINDIR%\SMSTSPostUpgrade`. `SetupCompletePause` delays them.
- The exit code is saved to `_SMSTSOSUpgradeActionReturnCode`. In ScanOnly mode
  the step passes on `0` or `0xC1900210` (setup found no compatibility issues)
  and fails on any other code (decompiled, `0x140003e7c`).
- When Dynamic Update is on and "ignore dismissible messages" is also on, the
  step logs a warning.
- It has explicit ARM64 handling ("Architecture of OS upgrade package is
  arm64", `CCM::Utility::IsARM64`).
- Its properties carry no `VariableName` qualifier in the MOF. The binary reads
  them as `OSDUpgrade<Property>`, for example `OSDUpgradeScanOnly` and
  `OSDUpgradePreserveSettings`. A BitLocker or other extra setup switch has to
  go through `OSDSetupAdditionalUpgradeOptions`.

## Finding 6: Set Dynamic Variables (`OSDSetDynamicVariables.exe`)

- It calls `TS::Utility::GatherDataForDynamicVariables` and then evaluates the
  `OSDSetDynamicVariablesRules` XML (`Rules/Rule`, an optional `condition`, and
  `Variables/Variable[@Name]`). Each rule's condition uses the same expression
  evaluator as step conditions, including make-and-model expressions.
  Variables can be hidden through `<name>_HiddenValueFlag`.
- The gathered values come from WMI `Win32_ComputerSystemProduct`, `Win32_BIOS`,
  `Win32_SystemEnclosure` and `Win32_NetworkAdapterConfiguration`, plus
  `CcmGetIPAddressInformation`. They are `_SMSTSMake`, `_SMSTSModel`, `_SMSTSSerialNumber`,
  `_SMSTSUUID`, `_SMSTSAssetTag`, `_SMSTSMacAddresses`, `_SMSTSIPAddresses` and
  `_SMSTSDefaultGateways`.
- Missing compared with MDT's `ZTIGather`: `IsLaptop`, `IsDesktop`, `IsServer`,
  `IsVM`, `IsOnBattery`, memory and processor values, database lookups and web
  service rules. No `ChassisTypes` query appears in `tscore.dll`.

## Other observations

- The media contains OSD binaries only for `I386` and `x64`. There is no ARM64
  OSD folder, although `OSDUpgradeOS.exe`, `tsmanager.exe` and others contain
  ARM64 detection code.
- `tsmanager.exe` contains `CCM::Utility::UseCscript` and logs "Adding silent
  switch to wscript.exe command line", so it rewrites script-host command lines
  it is given. This wasn't decompiled. The engine's own steps don't use
  VBScript.

## What this means for deployarr

These are design inferences, not observations.

1. Don't wait for MDT to show up inside ConfigMgr. The parts worth reusing are
   the native step model and its variable contract: the `OSD*` inputs and the
   `_SMSTS*` / `_TS*` outputs listed in [`task-sequence-steps.md`](task-sequence-steps.md).
2. A Windows 11 readiness check needs more than ConfigMgr provides: Secure
   Boot, CPU support, core count, and Windows 11 thresholds by default.
   ConfigMgr's check order and its `_TS_CR*` result variables are a reasonable
   pattern to copy.
3. A gather step should cover chassis type and VM detection, which MDT has and
   ConfigMgr 2509 lacks.
4. The disk layout should treat recovery size as a deliberate choice rather
   than a fixed 1% of the disk.

## Evidence

| Claim | Evidence |
| --- | --- |
| Check Readiness logic | REA `decompile-all`, Ghidra 12.1.4, `ev_71434dd82c8f377b30d81acd0a471bcdf134da967d6723edafde8cc67b3d16b9`; `OsdPrestartCheck.exe` SHA-256 `63358aa7…853810` |
| Upgrade OS command line | REA `decompile-all`, `ev_a121464baf757a40b99affbc65d2db1bcdd4306e4df8ab54c16e3ff42a3e775f`; `OSDUpgradeOS.exe` SHA-256 `0d1e99c5…c944d8` |
| Secure Boot state | REA `decompile-all --name SecureBoot`, `ev_d4c47613550e14cb5a32dd51aa7593457deed98bdcaed662758eb85ace985391`; `tscore.dll` SHA-256 `3d33992c…4655de` |
| Wizard assembly identity and members | REA `inspect_managed_artifact` `ev_8b98724c891861233d85ef2fb9c3ecd6a7ca0d8e28af2f2563fdb1df98c498b8`, `inspect_managed_members` `ev_46c2dd1a16580bba7ae49eb5a91514b0c6aebea3d214b416b0aba90c8adbf160`; CIL constants read with `tools/re/configmgr-osd/ildump.py` |
| Signer | REA `inspect_pe_certificates` `ev_c1b22eacd94945b947ce67bf114165ad408436f4c4933ee7c8b9687a25376001` |
| Step catalog, inventory, MDT search | `tools/re/configmgr-osd/reproduce.sh` |

Pseudocode is Ghidra output, not source. All results are static: nothing was
run on Windows.

## Open questions

- Which WMI property feeds the TPM version string in
  `Tpm::InitializeEx`. `SpecVersion` is likely but wasn't confirmed.
- How `IsUEFI` and `GetProcessorSpeed` are implemented. Only their call sites
  and strings were read.
- How `osddiskpart.exe` and `OSDBitLocker.exe` handle Windows 11 specifics such
  as encryption method defaults. These binaries weren't decompiled.
- Whether later ConfigMgr releases change any of this. Rerun the script below
  against the next installer.

## Reproduce

```bash
pip install pefile
bash tools/re/configmgr-osd/reproduce.sh /path/to/workdir
```

This downloads the installer, checks the SHA-256, unpacks everything, and
writes the MDT search results, `osd-x64-inventory.tsv` and
`task-sequence-steps.md` to `<workdir>/out`. Pass a new URL and hash as the
second and third arguments for a later release. `ildump.py` also needs
`dnfile` and `dncil`.
