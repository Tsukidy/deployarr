# ConfigMgr 2509 OSD reference

This page records what Configuration Manager (ConfigMgr) 2509 ships for operating system deployment (OSD) and how its steps handle Windows 11. MDT runs an older build of the same task sequence engine, which [MDT 8450 binaries reference](mdt-8450-binaries.md) covers. Use this page when you design a deployarr step that ConfigMgr also has.

## Sources

- The installer is `ConfigMgr_2509.exe` from `https://go.microsoft.com/fwlink/?linkid=2350307&clcid=0x409&culture=en-us&country=us`, which redirects to `https://download.microsoft.com/download/50e53030-0fcb-4076-8808-676c79a0ed9f/ConfigMgr_2509.exe`. It is 1,254,017,320 bytes, and its SHA-256 is `4cb0380e2b1c43f1e0b2da266beddd1b975dd72537f8d8113c702eadf1e6e313`. The server reported a last-modified date of 2026-01-16.
- The installer is a 7-Zip self-extractor with 4,102 files. `7z` unpacked it, and `msiextract` or `7z` unpacked the 41 non-empty MSI and CAB packages inside it, which hold 4,680 more files.
- Paths such as `OSD/bin/x64/tscore.dll` are relative to the media's `SMSSETUP` folder.
- REA `decompile-all` with Ghidra 12.1.4 exported functions of the native binaries to pseudocode. Citations such as `tscore!0x18016ad90` name a function address in the 2509 build. They never refer to the 8540 build that the MDT pages cite as `TsCore!`.
- The console assemblies are .NET. `tools/re/configmgr-osd/ildump.py` disassembled their CIL with dnfile and dncil. Citations such as `NewTaskSequenceWizard!Utils.GetDefaultUefiPartitionDiskObject` name the method.
- `BIN/X64/_TaskSequenceProvider.mof` is the server's WMI schema for every step. Citations such as `_TaskSequenceProvider.mof:1557-1628` give its line numbers.
- `OSD/bin/x64/OsdPrestartCheck.exe` carries an Authenticode signature from Microsoft Corporation, issued by Microsoft Code Signing PCA 2011. REA decoded the certificate table but did not verify the chain.
- One agent wrote each claim and re-opened its citation before writing it. Claims marked "inferred" come from names, strings, or partial code.
- `tools/re/configmgr-osd/reproduce.sh` repeats the download, the MDT search, the binary inventory, and the step catalog. See [ConfigMgr 2509 in the agent guide](../../AGENTS.md#configmgr-2509).

## MDT components

ConfigMgr 2509 ships no component that MDT authors. No file name on the media or in its packages matches `ZTI*`, `LTI*`, `*BDD*`, or `*.wsf`. No file contains `Microsoft.BDD`, `ZTIUtility`, `ZTIGather`, `DeploymentShare`, or `MDTIntegration` in ASCII or UTF-16. The only VBScript file is `TOOLS/PortConfiguration/Portswitch.vbs`, which configures ports.

The text "Microsoft Deployment Toolkit" appears only in error messages. The console and reporting resources warn that a task sequence "may contain steps from an extension, such as the Microsoft Deployment Toolkit (MDT), which is not present at the destination" (`AdminUI.UIResources.dll`, `DatabaseResources.dll`). The migration libraries give a similar warning for task sequences from ConfigMgr 2007 and 2012 (`BIN/X64/Microsoft.ConfigurationManagement.MigrationLibrary.dll`, `MigrationManager.dll`).

The search has two gaps. The `LanguagePack` CABs stay packed, so only their compressed bytes were searched. Content that ConfigMgr downloads later, such as the Windows ADK and WinPE, is not on the media.

### The engine MDT runs is ConfigMgr code

MDT 8450 ships the task sequence engine at file version 5.0.8540.1000 ([MDT 8450 binaries reference](mdt-8450-binaries.md#sources)). ConfigMgr 2509 ships the same three binaries, by name and description, at 5.00.9141.1002, linked on 2025-09-26.

| Binary | SHA-256 |
| --- | --- |
| `OSD/bin/x64/tscore.dll` | `3d33992cf04eeb09e7df4e80d78a54f891d38b2268cabc879703be559b4655de` |
| `OSD/bin/x64/tsmanager.exe` | `eb30ac3109d61fdda5e9609f72b54c68ee706309c5967fff44075a056b30a2d2` |
| `OSD/bin/x64/tsmbootstrap.exe` | `d2e2a5e313d29e822ae18452d1b347b662f9fb7b93476711f9fcb1eecce691b0` |

`OSD/bin/x64` holds 49 ConfigMgr binaries at that version and 5 Visual C++ runtime DLLs at 14.28.29914.0. All are native x64 code. `OSD/bin` has only `I386` and `x64` folders, with no ARM64 folder.

## Steps and their executables

Each step class names the executable it runs in its `CommandLine` qualifier (`_TaskSequenceProvider.mof`). None of them runs `cscript.exe`.

| Step class (`SMS_TaskSequence_` prefix) | Executable |
| --- | --- |
| `SubTasksequence` | `SubTasksequence.exe` |
| `RunCommandLineAction` | `smsswd.exe` |
| `RebootAction` | `smsboot.exe` |
| `InstallUpdateAction` | `TSInstallSWUpdate.exe` |
| `InstallSoftwareAction` | `smsswd.exe` |
| `InstallApplicationAction` | `smsappinstall.exe` |
| `ApplyOperatingSystemAction` | `OSDApplyOS.exe` |
| `ApplyDataImageAction` | `OSDApplyOS.exe` |
| `SetupWindowsAndSMSAction` | `OSDSetupWindows.exe` |
| `CaptureSystemImageAction` | `osdcapturesystemimage.exe` |
| `CaptureNetworkSettingsAction` | `osdnetsettings.exe` |
| `ApplyNetworkSettingsAction` | `osdnetsettings.exe` |
| `InstallDeployToolsAction` | `osdprepareos.exe` |
| `PrepareOSAction` | `osdprepareos.exe` |
| `PrepareSMSClientAction` | `osdpreparesmsclient.exe` |
| `CaptureUserStateAction` | `osdmigrateuserstate.exe` |
| `RestoreUserStateAction` | `osdmigrateuserstate.exe` |
| `RequestStateStoreAction` | `osdsmpclient.exe` |
| `ReleaseStateStoreAction` | `osdsmpclient.exe` |
| `PartitionDiskAction` | `osddiskpart.exe` |
| `ConvertDiskAction` | `osddiskpart.exe` |
| `SetVariableAction` | `tsenv.exe` |
| `ConnectNetworkFolderAction` | `smsnetuse.exe` |
| `JoinDomainWorkgroupAction` | `osdjoin.exe` |
| `CaptureWindowsSettingsAction` | `osdwinsettings.exe` |
| `ApplyWindowsSettingsAction` | `osdwinsettings.exe` |
| `AutoApplyAction` | `osddriverclient.exe` |
| `ApplyDriverPackageAction` | `osddriverclient.exe` |
| `DisableBitLockerAction` | `OSDBitLocker.exe` |
| `EnableBitLockerAction` | `OSDBitLocker.exe` |
| `OfflineEnableBitLockerAction` | `OSDOfflineBitlocker.exe` |
| `PrestartCheckAction` | `OSDPrestartCheck.exe` |
| `RunPowerShellScriptAction` | `OSDRunPowerShellScript.exe` |
| `UpgradeOperatingSystemAction` | `OSDUpgradeOS.exe` |
| `DownloadPackageContentAction` | `OsdDownloadContent.exe` |
| `SetDynamicVariablesAction` | `OSDSetDynamicVariables.exe` |

## Check Readiness

The Check Readiness step runs `OSDPrestartCheck.exe` in WinPE or the full OS. Each check has an on/off variable and, where it applies, a threshold variable (`_TaskSequenceProvider.mof:1557-1628`).

| Check | Variables | Default |
| --- | --- | --- |
| Memory | `OSDCheckMemory`, `OSDMemory` | on, 512 MB |
| Processor speed | `OSDCheckProcessorSpeed`, `OSDProcessorSpeed` | on, 800 MHz |
| Free disk space | `OSDCheckFreeDiskSpace`, `OSDFreeDiskSpace` | on, 25,000 MB |
| OS type | `OSDCheckOSType`, `OSDOSType` | on, `CLIENT` |
| OS architecture | `OSDCheckOSArchitecture`, `OSDOSArchitecture` | off, `64` |
| AC power | `OSDCheckPowerState` | off |
| UEFI | `OSDCheckDeviceUEFI` | off |
| Network, wired network | `OSDCheckNetworkConnected`, `OSDCheckNetworkWired` | off |
| TPM 2.0 enabled | `OSDCheckDeviceTpmEnabled` | off |
| TPM 2.0 activated | `OSDCheckDeviceTpmActivated` | off |
| Minimum and maximum OS version | `OSDCheckMinOSVersion`, `OSDCheckMaxOsVersion` | off |
| Client version | `OSDCheckCMClientMinVersion` | off |
| OS language | `OSDCheckOSLanguageID`, `OSDOSLanguageID` | off, 1033 |

One function runs every check in order (`OsdPrestartCheck!0x140001780`):

- Each enabled check writes `1` or `0` to a `_TS_CR` variable, such as `_TS_CRMEMORY` or `_TS_CRTPMENABLED`. It also writes the required value to a `_TS_CRREQUIRED` variable, such as `_TS_CRREQUIREDMEMORY`. Later steps can test these variables.
- The checks call helpers exported by `tscore.dll`. They are `TS::Utility::GetTotalPhysicalMemory`, `GetProcessorSpeed`, `IsFreeDiskSpaceAvailable`, `GetOsType`, `GetClientVersion`, `IsUEFI`, `GetBatteryStatus`, and `ConnectedNetworkAdapterIsFound`.
- The disk check falls back to `IsFreeDiskSpaceNoPartitionsAvailable`, so an unpartitioned disk with enough space passes.
- The power check passes when the PC is on AC power. A `GetBatteryStatus` result of 2 passes with the log line "Device has a battery but is wired".
- The TPM checks call `TS::BitLocker::Tpm::InitializeEx`, which returns a version string. The step parses it with `swscanf_s("%d.%d")` and fails a major version below 2. It then reads `Tpm::GetInitialState`. Bit 0 means enabled and bit 1 means activated. `tscore.dll` builds its TPM class on WMI `Win32_Tpm` (inferred from its strings).
- No check covers Secure Boot, processor model, or core count.

Microsoft lists these Windows 11 minimums: a compatible 64-bit processor at 1 GHz with two or more cores, 4 GB of memory, 64 GB of storage, UEFI firmware that is Secure Boot capable, and TPM 2.0 ([Windows 11 requirements](https://learn.microsoft.com/en-us/windows/whats-new/windows-11-requirements), dated 2026-07-14). Every Check Readiness default is below these, and the UEFI and TPM checks start off.

## Secure Boot state

- `TS::Utility::GetSecureBootState` reads the DWORD `UEFISecureBootEnabled` under `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State`. It returns `Enabled` for 1 and `Disabled` for 0. When the value is missing or holds anything else, it returns its default, `NA`, which is the wide string at RVA `0x23dea4` (`tscore!0x18016ad90`).
- `tsmanager.exe` logs `SecureBootState: %s` and stores the result in its `SecureBootState` environment variable. The only matching variable name in the binary is `_TSSecureBoot` (inferred from strings).
- Windows 11 requires Secure Boot capable firmware. `UEFISecureBootEnabled` reports whether Secure Boot is on, which is a different fact.

## UEFI detection

`tscore.dll` still detects UEFI in two ways, as the 8540 build does ([MDT 8450 binaries reference](mdt-8450-binaries.md#firmware-and-disks)). The 2509 `IsUEFI` now enables the privilege first.

- `TS::Utility::IsUEFI` enables `SeSystemEnvironmentPrivilege`. If that fails, it returns false, which means BIOS. Otherwise it calls `GetFirmwareEnvironmentVariableA` with an empty name and the GUID `{00000000-0000-0000-0000-000000000000}`. A successful call returns true. A failed call returns false only when `GetLastError` is 1 (`ERROR_INVALID_FUNCTION`), and true for any other error (`tscore!0x18016ad00`, read from the disassembly).
- This matches the fix that the MDT 8456 hotfix made in `Microsoft.BDD.Utility.dll` ([MDT 8456 reference](mdt-8456.md#native-helper-dll)). That page does not say what the hotfix does when it cannot enable the privilege. The 2509 build reports BIOS in that case, even on a UEFI PC.
- Check Readiness calls `IsUEFI` for its UEFI check (`OsdPrestartCheck!0x140001780`).
- `TS::Boot::Firmware::IsEfi` returns true when `TS::Boot::GetSystemFirmwareType` reports type 2, and `Firmware::IsMbr` when it reports type 1 (`tscore!0x1800a2170`, `tscore!0x1800a2270`). Types 1 and 2 match the `FIRMWARE_TYPE` values that `GetFirmwareType` returns (inferred). `GetSystemFirmwareType` resolves its function from the system directory at run time (`tscore!0x1800a1608`).

## Default disk layouts

The console's new task sequence wizard builds these layouts (`NewTaskSequenceWizard!Utils.GetDefaultUefiPartitionDiskObject`, IL offsets `0x87d5` to `0x8ade`, and `Utils.GetDefaultBiosPartitionDiskObject`, `0x850d` to `0x87c0`).

| Firmware | Disk | Partitions in order |
| --- | --- | --- |
| UEFI | disk 0, GPT, `GPTBootDisk` true | EFI, FAT32, 500 MB. MSR, 128 MB. Windows, NTFS, 99%. Recovery, NTFS, 100% of what remains |
| BIOS | disk 0, MBR | System, NTFS, 350 MB, bootable. Windows, NTFS, 99%. Recovery, NTFS, 100% of what remains |

- The Windows partition gets 100% instead of 99% when the wizard leaves out the Recovery partition. One branch picks the constant and skips the Recovery block (inferred from the IL; the flag's source was not traced).
- The Recovery partition is about 1% of the disk. That is about 640 MB on a 64 GB disk and about 10 GB on a 1 TB disk.
- Microsoft sizes the Recovery partition differently. Its minimum is the size of `\Windows\System32\Recovery\winre.wim` in the image, plus any WinRE customizations, plus 250 MB for servicing. Microsoft also sets the EFI partition at 200 MB or more, or 300 MB or more on 4K native disks, and the MSR at 16 MB ([UEFI/GPT-based hard drive partitions](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/configure-uefigpt-based-hard-drive-partitions?view=windows-11)).

## Upgrade Operating System

`UpgradeWindows::CreateUpgradeCmdLine` builds the Windows Setup command line (`OSDUpgradeOS!0x1400078a8`). It adds these arguments in this order:

1. `SETUP.EXE` from the upgrade source, with ` /ImageIndex <n>`.
2. ` /auto <value>`. The value is the step's preserve setting, such as `Upgrade`, `DataOnly`, or `Clean` (inferred from the step's strings).
3. ` /quiet /noreboot /EULA accept`.
4. Outside scan-only mode, ` /postoobe "<SetupComplete.cmd>"` and ` /postrollback "<SetupRollback.cmd>"`. It adds ` /postrollbackcontext system` only when a version field of the setup source is above 17133 (`0x42ED`). Reading that field as the build number is inferred.
5. ` /installdrivers "<path>"` when the step has driver content.
6. ` /DynamicUpdate Enable` or ` /DynamicUpdate Disable`.
7. ` /pkey <key>` when the step has a product key.
8. ` /compat IgnoreWarning` when the step ignores dismissible messages. If Dynamic Update is also on, the step first logs a warning against that combination.
9. ` /compat ScanOnly` in scan-only mode.
10. The value of `OSDSetupAdditionalUpgradeOptions`, if set.

Other behavior:

- The step copies `SetupComplete.cmd` and `SetupRollback.cmd` from its templates into `%WINDIR%\SMSTSPostUpgrade`. The variable `SetupCompletePause` delays them (`OSDUpgradeOS.exe` strings).
- It saves Setup's exit code in `_SMSTSOSUpgradeActionReturnCode`. In scan-only mode it passes on 0 or `0xC1900210`, which Setup returns when it finds no compatibility issues, and fails on any other code (`OSDUpgradeOS!0x140003e7c`).
- It checks for an ARM64 OS and an ARM64 upgrade package through `CCM::Utility::IsARM64` (`OSDUpgradeOS.exe` strings).
- Its properties carry no `VariableName` qualifier (`_TaskSequenceProvider.mof:1684-1737`). The binary reads them as `OSDUpgrade` plus the property name, such as `OSDUpgradeScanOnly` and `OSDUpgradePreserveSettings` (inferred from strings).

## Set Dynamic Variables

- `OSDSetDynamicVariables.exe` calls `TS::Utility::GatherDataForDynamicVariables`, then reads rules from the variable `OSDSetDynamicVariablesRules`. The rules are XML. Each `Rules/Rule` has an optional `condition` and a list of `Variables/Variable` elements with a `Name` attribute. A condition uses the step condition evaluator, including make and model expressions. For a hidden variable the step also sets `<name>_HiddenValueFlag` to `true`, which keeps the value out of logs (`OSDSetDynamicVariables.exe` strings, the effect inferred).
- The gathered variables are `_SMSTSMake`, `_SMSTSModel`, `_SMSTSSerialNumber`, `_SMSTSUUID`, `_SMSTSAssetTag`, `_SMSTSMacAddresses`, `_SMSTSIPAddresses`, and `_SMSTSDefaultGateways`. `tscore.dll` reads WMI `Win32_ComputerSystemProduct`, `Win32_BIOS`, `Win32_SystemEnclosure`, and `Win32_NetworkAdapterConfiguration`, and calls `CcmGetIPAddressInformation` (`tscore.dll` strings). Which class feeds which variable is inferred.
- `tscore.dll` contains no `ChassisTypes` string. ConfigMgr gathers no chassis type and no virtual machine flag. MDT's gather sets `IsLaptop`, `IsDesktop`, and `IsVM` ([MDT 8456 reference](mdt-8456.md#gathered-facts)).

## What changes in deployarr

| Finding | deployarr decision |
| --- | --- |
| deployarr applies `install.wim` and never runs `setup.exe`, so nothing checks Windows 11 hardware. ConfigMgr's Check Readiness fills that gap with defaults below the Windows 11 minimums and no Secure Boot check | The agent runs a readiness check in WinPE before any disk step. Its defaults are Microsoft's Windows 11 minimums. See [Readiness check](../architecture.md#readiness-check) |
| Check Readiness records each result and each required value in variables | Each readiness check records its result, the required value, and the value found, so a later step or the UI can show why a PC failed |
| ConfigMgr reads TPM state through WMI `Win32_Tpm`, and the deployarr agent does not use WMI | The agent reads the TPM version through TBS. See [Agent](../architecture.md#agent) |
| `GetSecureBootState` keeps a third value, `NA`, for a missing registry value | The agent's Secure Boot fact has three values: on, off, and unknown. Unknown never counts as off |
| Windows 11 requires Secure Boot capable firmware, not Secure Boot on | The Secure Boot check warns by default instead of failing |
| The wizard sizes Recovery as about 1% of the disk | The server sizes Recovery from the image's `winre.wim` plus 250 MB, as Microsoft documents. See [Disk layout and boot files](../architecture.md#disk-layout-and-boot-files) |
| The 2509 `IsUEFI` enables the privilege first, but reports BIOS when it cannot enable it | No change. The agent already calls `GetFirmwareType` and uses the firmware variable check only as a fallback |
| ConfigMgr builds an in-place upgrade from `setup.exe /auto` with `/postoobe`, `/postrollback`, and `/compat ScanOnly` | No change now, because in-place upgrades are not in the first release. This page is the reference when they are |
