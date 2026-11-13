# ConfigMgr 2509 OSD reference

This page records what Configuration Manager (ConfigMgr) 2509 ships for operating system deployment (OSD), how its steps handle Windows 11, and how its PXE responder boots WinPE without WDS. MDT runs an older build of the same task sequence engine, which [MDT 8450 binaries reference](mdt-8450-binaries.md) covers. Use this page when you design a deployarr step that ConfigMgr also has.

## Sources

- The installer is `ConfigMgr_2509.exe` from `https://go.microsoft.com/fwlink/?linkid=2350307&clcid=0x409&culture=en-us&country=us`, which redirects to `https://download.microsoft.com/download/50e53030-0fcb-4076-8808-676c79a0ed9f/ConfigMgr_2509.exe`. It is 1,254,017,320 bytes, and its SHA-256 is `4cb0380e2b1c43f1e0b2da266beddd1b975dd72537f8d8113c702eadf1e6e313`. The server reported a last-modified date of 2026-01-16.
- The installer is a 7-Zip self-extractor with 4,102 files. Its top level holds the `AUTORUN`, `LanguagePack`, and `SMSSETUP` folders and the file `splash.hta`. `7z` unpacked it, and `msiextract` or `7z` unpacked the 41 non-empty MSI and CAB packages inside it, which hold 4,680 more files.
- Paths such as `OSD/bin/x64/tscore.dll` are relative to the media's `SMSSETUP` folder.
- REA `decompile-all` with Ghidra 12.1.4 exported functions of the native binaries to pseudocode. Citations such as `tscore!0x18016ad90` name a function address in the 2509 build. They never refer to the 8540 build that the MDT pages cite as `TsCore!`.
- The console assemblies are .NET. `tools/re/configmgr-osd/ildump.py` disassembled their CIL with dnfile and dncil. Citations such as `NewTaskSequenceWizard!Utils.GetDefaultUefiPartitionDiskObject` name the method.
- `BIN/X64/_TaskSequenceProvider.mof` is the server's WMI schema for every step. Citations such as `_TaskSequenceProvider.mof:1557-1628` give its line numbers.
- `OSD/bin/x64/OsdPrestartCheck.exe` carries an Authenticode signature from Microsoft Corporation, issued by Microsoft Code Signing PCA 2011. REA decoded the certificate table but did not verify the chain.
- Every result is static. Nothing ran on Windows.
- One agent wrote each claim and re-opened its citation before writing it. Claims marked "inferred" come from names, strings, or partial code.
- MDT citations come from `Source/MDT.8456` in [keithga/DeployShared](https://github.com/keithga/DeployShared), as on the [MDT 8456 reference](mdt-8456.md#sources). Script citations such as `ZTIValidate.wsf:16` point into its `scripts` folder, and template citations such as `Templates/ClientUpgrade.xml:37-43` are relative to `Source/MDT.8456`. None of these files is on the ConfigMgr media.
- `tools/re/configmgr-osd/reproduce.sh` repeats the download, the MDT search, the binary inventory, and the step catalog. See [ConfigMgr 2509 in the agent guide](../../AGENTS.md#configmgr-2509). Whether a later ConfigMgr release changes any of these findings is open. Rerun the script against the next installer to find out.
- [configmgr-2509-osd](configmgr-2509-osd/README.md) holds the generated [step catalog](configmgr-2509-osd/task-sequence-steps.md) and [binary inventory](configmgr-2509-osd/osd-x64-inventory.tsv), and says how to rebuild them.

### Evidence IDs

The first analysis recorded these REA evidence IDs for its claims.

| Claims | REA evidence | Binary and SHA-256 |
| --- | --- | --- |
| [Check Readiness](#check-readiness) | `decompile-all` with Ghidra 12.1.4, `ev_71434dd82c8f377b30d81acd0a471bcdf134da967d6723edafde8cc67b3d16b9` | `OSD/bin/x64/OsdPrestartCheck.exe`, `63358aa79a989e49ce60ac7adb45ff1e94167081a4bff4a4f297007ddf853810` |
| [Upgrade Operating System](#upgrade-operating-system) | `decompile-all`, `ev_a121464baf757a40b99affbc65d2db1bcdd4306e4df8ab54c16e3ff42a3e775f` | `OSD/bin/x64/OSDUpgradeOS.exe`, `0d1e99c549c8cc5ba6920567cf6b454f3aa49850e4b7b7f5d46641d026c944d8` |
| [Secure Boot state](#secure-boot-state) | `decompile-all --name SecureBoot`, `ev_d4c47613550e14cb5a32dd51aa7593457deed98bdcaed662758eb85ace985391` | `OSD/bin/x64/tscore.dll`, `3d33992cf04eeb09e7df4e80d78a54f891d38b2268cabc879703be559b4655de` |
| [Default disk layouts](#default-disk-layouts) | `inspect_managed_artifact`, `ev_8b98724c891861233d85ef2fb9c3ecd6a7ca0d8e28af2f2563fdb1df98c498b8`, and `inspect_managed_members`, `ev_46c2dd1a16580bba7ae49eb5a91514b0c6aebea3d214b416b0aba90c8adbf160`. `tools/re/configmgr-osd/ildump.py` read the CIL constants | `AdminUI.NewTaskSequenceWizard.dll` from `BIN/I386/AdminConsole.msi`, `fef1dfd95404ec421b040a3b1a6ac11486d5d1178269fe462d90574a44847844` |
| The signature in [Sources](#sources) | `inspect_pe_certificates`, `ev_c1b22eacd94945b947ce67bf114165ad408436f4c4933ee7c8b9687a25376001` | |
| [MDT components](#mdt-components), the step catalog, and the binary inventory | `tools/re/configmgr-osd/reproduce.sh` | |

## MDT components

ConfigMgr 2509 ships no component that MDT authors. No file name on the media or in its packages matches `ZTI*`, `LTI*`, `*BDD*`, or `*.wsf`. No file contains `Microsoft.BDD`, `ZTIUtility`, `ZTIGather`, `DeploymentShare`, or `MDTIntegration` in ASCII or UTF-16. The only VBScript file is `TOOLS/PortConfiguration/Portswitch.vbs`, which configures ports.

The text "Microsoft Deployment Toolkit" appears only in error messages. The console and reporting resources warn that a task sequence "may contain steps from an extension, such as the Microsoft Deployment Toolkit (MDT), which is not present at the destination" (`AdminUI.UIResources.dll`, `DatabaseResources.dll`). The console and CMPivot packages ship identical copies of `AdminUI.UIResources.dll` (`BIN/I386/AdminConsole.msi`, `TOOLS/CMPivot/CMPivot.msi`). The migration libraries warn that a task sequence from ConfigMgr 2007 or 2012 holds actions that are not standard, and ask whether an extension such as MDT was installed (`BIN/X64/Microsoft.ConfigurationManagement.MigrationLibrary.dll`, `BIN/X64/Microsoft.ConfigurationManagement.MigrationManager.dll`).

The search has two gaps. The `LanguagePack` CABs stay packed, so only their compressed bytes were searched. Content that ConfigMgr downloads later, such as the Windows ADK, WinPE, and in-console updates, is not on the media.

### The engine MDT runs is ConfigMgr code

MDT 8450 ships the task sequence engine at file version 5.0.8540.1000 ([MDT 8450 binaries reference](mdt-8450-binaries.md#sources)). ConfigMgr 2509 ships the same three binaries, by name and description, at 5.00.9141.1002, linked on 2025-09-26.

| Binary | SHA-256 |
| --- | --- |
| `OSD/bin/x64/tscore.dll` | `3d33992cf04eeb09e7df4e80d78a54f891d38b2268cabc879703be559b4655de` |
| `OSD/bin/x64/tsmanager.exe` | `eb30ac3109d61fdda5e9609f72b54c68ee706309c5967fff44075a056b30a2d2` |
| `OSD/bin/x64/tsmbootstrap.exe` | `d2e2a5e313d29e822ae18452d1b347b662f9fb7b93476711f9fcb1eecce691b0` |

`OSD/bin/x64` holds 49 ConfigMgr binaries at that version and 5 Visual C++ runtime DLLs at 14.28.29914.0. All are native x64 code. `OSD/bin` has only `I386` and `x64` folders, with no ARM64 folder.

Ten binaries in `OSD/bin/x64` still name `CCM::Utility::IsARM64`:

- `ccmcore.dll` exports it as a decorated name. `OSDUpgradeOS.exe` and `OsdBitLocker.exe` import it from `ccmcore.dll` (import and export tables, read with pefile).
- `ccmcore.dll` and seven other binaries hold both the name as text and the log line "Running on ARM64". The seven are `tsmanager.exe`, `tsmbootstrap.exe`, `tsbootshell.exe`, `tsprogressui.exe`, `osdgina.dll`, `OsdSetupHook.exe`, and `OsdBitLocker_wtg.exe`.
- These eight include the progress UI and the GINA replacement `osdgina.dll`. The text therefore comes from CCM utility code that they share (inferred).
- `OSDUpgradeOS.exe` uses it to check for an ARM64 OS and upgrade package ([Upgrade Operating System](#upgrade-operating-system)). This analysis did not trace whether `tsmanager.exe` calls it.

## Steps and their executables

Each step class names the executable it runs in its `CommandLine` qualifier (`_TaskSequenceProvider.mof`). The [step catalog](configmgr-2509-osd/task-sequence-steps.md) lists every class with its full command line. 36 classes carry a command line, and they name 26 executables. Some executables serve several steps. `OSDApplyOS.exe` applies both operating system and data images, and `smsswd.exe` runs both command lines and packages. None of the command lines runs `cscript.exe`.

Eight `OSD/bin/x64` binaries name `CCM::Utility::UseCscript` and log "Adding silent switch to wscript.exe command line". They are the eight that log "Running on ARM64" ([The engine MDT runs is ConfigMgr code](#the-engine-mdt-runs-is-configmgr-code)), including `tsmanager.exe`, the progress UI `tsprogressui.exe`, and `osdgina.dll`. These strings therefore belong to shared CCM utility code (inferred). This analysis did not trace whether `tsmanager.exe` applies the switch to a step's command line.

### Native counterparts to MDT scripts

Each MDT 8456 script below has a native step in ConfigMgr 2509 with a similar purpose. Each pair matches the script's `Purpose:` comment, on lines 14 to 18 of the script, to a step class. The notes below the table give more evidence for some pairs. The pairing is inferred.

| MDT script | Step class (`SMS_TaskSequence_` prefix) | Executable |
| --- | --- | --- |
| `ZTIGather.wsf` with `CustomSettings.ini` rules | `SetDynamicVariablesAction` | `OSDSetDynamicVariables.exe` |
| `ZTIValidate.wsf` | `PrestartCheckAction` | `OSDPrestartCheck.exe` |
| `ZTIDiskpart.wsf` | `PartitionDiskAction` | `osddiskpart.exe` |
| `LTIApply.wsf` | `ApplyOperatingSystemAction`, `ApplyDataImageAction` | `OSDApplyOS.exe` |
| `LTIApply.wsf` in MDT's upgrade step | `UpgradeOperatingSystemAction` | `OSDUpgradeOS.exe` |
| `ZTIDrivers.wsf` | `AutoApplyAction`, `ApplyDriverPackageAction` | `osddriverclient.exe` |
| `ZTIBde.wsf` | `EnableBitLockerAction`, `OfflineEnableBitLockerAction` | `OSDBitLocker.exe`, `OSDOfflineBitlocker.exe` |
| `ZTIDisableBDEProtectors.wsf` | `DisableBitLockerAction` | `OSDBitLocker.exe` |
| `ZTIUserState.wsf` | `CaptureUserStateAction`, `RestoreUserStateAction` | `osdmigrateuserstate.exe` |
| `ZTIDomainJoin.wsf` | `JoinDomainWorkgroupAction` | `osdjoin.exe` |
| `ZTIPowerShell.wsf` | `RunPowerShellScriptAction` | `OSDRunPowerShellScript.exe` |
| `ZTIWindowsUpdate.wsf` | `InstallUpdateAction` | `TSInstallSWUpdate.exe` |
| `ZTIApplications.wsf` | `InstallApplicationAction`, `InstallSoftwareAction` | `smsappinstall.exe`, `smsswd.exe` |
| `ZTINICConfig.wsf` | `CaptureNetworkSettingsAction`, `ApplyNetworkSettingsAction` | `osdnetsettings.exe` |
| `ZTIConfigure.wsf` | `ApplyWindowsSettingsAction`, `ApplyNetworkSettingsAction` | `osdwinsettings.exe`, `osdnetsettings.exe` |
| `ZTISetVariable.wsf` | `SetVariableAction` | `tsenv.exe` |

- MDT's in-place upgrade template runs `LTIApply.wsf` in a `BDD_UpgradeOS` step with `IsOSUpgrade` set to 1 (`Templates/ClientUpgrade.xml:37-43`).
- MDT's client template runs `ZTIBde.wsf` in a step of type `SMS_TaskSequence_EnableBitLockerAction` (`Templates/Client.xml:398-414`). It runs `ZTIDisableBDEProtectors.wsf` in its "Disable BDE Protectors" step (`Templates/Client.xml:85-90`). That script suspends BitLocker by calling `DisableKeyProtectors` (`ZTIDisableBDEProtectors.wsf:96-98`).
- This analysis did not decompile `OSDBitLocker.exe`. How it handles Windows 11 details, such as the default encryption method, is an open question.
- `ZTINICConfig.wsf` saves the settings of adapters without DHCP in the State Capture phase of a refresh and restores them in State Restore (`ZTINICConfig.wsf:102-153`). It reads `OSDMigrateAdapterSettings` and writes `OSDAdapterCount` (`ZTINICConfig.wsf:109-110`, `ZTINICConfig.wsf:138`). The [step catalog](configmgr-2509-osd/task-sequence-steps.md) maps these two variables to `CaptureNetworkSettingsAction` and `ApplyNetworkSettingsAction`. MDT's client template runs the script in its "Capture Network Settings" step (`Templates/Client.xml:74-76`).
- `ZTIDomainJoin.wsf` only verifies and recovers a domain join in the State Restore phase (`ZTIDomainJoin.wsf:15`). The join itself goes through the unattend file, where `ZTIConfigure.wsf` writes `JoinDomain` ([MDT 8456 reference](mdt-8456.md#unattend)). `ZTIDomainJoin.wsf` therefore covers only part of what `JoinDomainWorkgroupAction` does (inferred).
- `ZTIConfigure.wsf` writes settings into the answer files (`ZTIConfigure.wsf:16`). Its mappings include the computer name, product key, admin password, time zone, and locales, which `ApplyWindowsSettingsAction` also takes. They also include the domain, OU, and workgroup, which `ApplyNetworkSettingsAction` takes (`ZTIConfigure.xml:103-216`, [step catalog](configmgr-2509-osd/task-sequence-steps.md)).
- Every ConfigMgr executable in the table is native code ([binary inventory](configmgr-2509-osd/osd-x64-inventory.tsv)), so none of these steps needs `cscript.exe` or VBScript.

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
- The TPM checks call `TS::BitLocker::Tpm::InitializeEx`, which returns a version string. The step parses it with `swscanf_s("%d.%d")` and fails a major version below 2. In that case it logs "TPM 2.0 or higher is not enabled. Found version of TPM is %d.%d which is less than required 2.0" (`OsdPrestartCheck.exe` strings). It then reads `Tpm::GetInitialState`. Bit 0 means enabled and bit 1 means activated. `tscore.dll` builds its TPM class on WMI `Win32_Tpm` (inferred from its strings).
- No check covers Secure Boot, processor model, or core count.
- Two helpers are not yet understood. This analysis did not trace which `Win32_Tpm` property feeds the version string from `InitializeEx`. `tscore.dll` names `SpecVersion` in its strings, so that property is likely. This analysis read only the call site and strings of `GetProcessorSpeed`, so how it measures the speed is open.

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

- Each layout's step gets a condition with four expressions (`Utils.GetDefaultPartitionDiskConditionObject`, IL offsets `0x83a0` to `0x84ff`). Three are the same for both layouts. They are `_SMSTSClientCache` `notExists`, `_SMSTSMediaType` `notEquals` `OEMMedia`, and `_OSDMigrateUseHardlinks` `notEquals` `true`. The fourth is `_SMSTSBootUEFI` `equals` `true` for UEFI and `_SMSTSBootUEFI` `notEquals` `true` for BIOS.
- The method takes a `forBios` argument. The BIOS layout sets it to true through a one-argument overload (`0x87bb`). The UEFI layout passes false (`0x8add`).
- The Windows partition gets 100% instead of 99% when the wizard leaves out the Recovery partition. One branch picks the constant and skips the Recovery block (inferred from the IL; the flag's source was not traced).
- The Recovery partition is about 1% of the disk. That is about 640 MB on a 64 GB disk and about 10 GB on a 1 TB disk.
- Microsoft sizes the Recovery partition differently. Its minimum is the size of `\Windows\System32\Recovery\winre.wim` in the image, plus any WinRE customizations, plus 250 MB for servicing. Microsoft also sets the EFI partition at 200 MB or more, or 300 MB or more on 4K native disks, and the MSR at 16 MB ([UEFI/GPT-based hard drive partitions](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/configure-uefigpt-based-hard-drive-partitions?view=windows-11)).
- This analysis did not decompile `osddiskpart.exe`. How it handles Windows 11 details is an open question.

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

Any other Setup switch, such as one for BitLocker, can reach Setup only through `OSDSetupAdditionalUpgradeOptions` (inferred from this list).

Other behavior:

- The step copies `SetupComplete.cmd` and `SetupRollback.cmd` from its templates into `%WINDIR%\SMSTSPostUpgrade`. The variable `SetupCompletePause` delays them (`OSDUpgradeOS.exe` strings).
- It saves Setup's exit code in `_SMSTSOSUpgradeActionReturnCode`. In scan-only mode it passes on 0 or `0xC1900210`, which Setup returns when it finds no compatibility issues, and fails on any other code (`OSDUpgradeOS!0x140003e7c`).
- It checks for an ARM64 OS and an ARM64 upgrade package through `CCM::Utility::IsARM64` (`OSDUpgradeOS.exe` strings).
- Its properties carry no `VariableName` qualifier (`_TaskSequenceProvider.mof:1684-1737`). The binary reads them as `OSDUpgrade` plus the property name, such as `OSDUpgradeScanOnly` and `OSDUpgradePreserveSettings` (inferred from strings).

## Set Dynamic Variables

- `OSDSetDynamicVariables.exe` calls `TS::Utility::GatherDataForDynamicVariables`, then reads rules from the variable `OSDSetDynamicVariablesRules`. The rules are XML. Each `Rules/Rule` has an optional `condition` and a list of `Variables/Variable` elements with a `Name` attribute. A condition uses the step condition evaluator, including make and model expressions. For a hidden variable the step also sets `<name>_HiddenValueFlag` to `true`, which keeps the value out of logs (`OSDSetDynamicVariables.exe` strings, the effect inferred).
- The gathered variables are `_SMSTSMake`, `_SMSTSModel`, `_SMSTSSerialNumber`, `_SMSTSUUID`, `_SMSTSAssetTag`, `_SMSTSMacAddresses`, `_SMSTSIPAddresses`, and `_SMSTSDefaultGateways`. `tscore.dll` reads WMI `Win32_ComputerSystemProduct`, `Win32_BIOS`, `Win32_SystemEnclosure`, and `Win32_NetworkAdapterConfiguration`, and calls `CcmGetIPAddressInformation` (`tscore.dll` strings). Which class feeds which variable is inferred.
- `tscore.dll` contains no `ChassisTypes` string. ConfigMgr gathers no chassis type and no virtual machine flag. MDT's gather sets `IsLaptop`, `IsDesktop`, and `IsVM` ([MDT 8456 reference](mdt-8456.md#gathered-facts)).
- MDT's gather also sets `IsServer`, `IsOnBattery`, `Memory`, and `ProcessorSpeed` (`ZTIGather.wsf:631-639`). Set Dynamic Variables gathers none of these values (inferred from the gathered variables above).
- MDT's rules can query a database or a web service ([MDT 8456 reference](mdt-8456.md#rules-and-database-lookup)). The rules of Set Dynamic Variables have no such lookup (inferred from the rule format above).

## PXE responder

ConfigMgr 2509 has two PXE paths. `BIN/X64/smspxe.dll` is a provider that runs inside WDS. `BIN/X64/sccmpxe.exe` is a service with its own DHCP and TFTP code, so it needs no WDS. This section describes `sccmpxe.exe` unless it names `smspxe.dll`. In WinPE, `OSD/bin/x64/TsPxe.dll` and `OSD/bin/x64/smstftp.exe` pick up where the boot ends.

| Binary | Version | SHA-256 |
| --- | --- | --- |
| `BIN/X64/sccmpxe.exe` | 5.0.9141.1002 | `8588a8d5da07a4c5270089605d32b4efc6b0def61cd5c2d159e987c6ebc0e841` |
| `BIN/X64/smspxe.dll` | 5.0.9141.1002 | `9f500652ffa47592310582b693a9507907a6a3137a48ae4db9d2bcdfb03d3db2` |
| `OSD/bin/x64/TsPxe.dll` | 5.0.9141.1002 | `a4fc95b588a19555aa5160645b279342dabe67a5f89acc2049819cb747820808` |
| `OSD/bin/x64/smstftp.exe` | 5.0.9141.1002 | `f88cc9848279bad4c0639ff3aba58acdcb389d308c1eb4bbbad611940db13d4a` |

One agent wrote each claim about a binary in this section from Ghidra pseudocode, and a second agent re-opened each citation and tried to refute it. The claims that change deployarr's design were checked a third time by hand.

### Sockets

- The service binds, for each address on an Ethernet adapter that is up (at most 32 addresses), UDP 67 for DHCP (547 for DHCPv6), UDP 4011, and UDP 69 for TFTP. It sets no `SO_REUSEADDR`, so a port that another process holds fails with "Another process (DHCP server or PXE server) is already using the port" (`sccmpxe!0x140023f50`, `sccmpxe!0x1400239e4`, `sccmpxe!0x14001f3b0`).
- The DWORD `DoNotListenOnDhcpPort` under `HKLM\Software\Microsoft\SMS\DP` skips port 67, so a DHCP server can run on the same host. `BindPolicy` and `BindExcept` limit the adapters by MAC address (`sccmpxe!0x140023d30`).

### Requests it answers

- It handles a request only when it is a DISCOVER or a REQUEST that carries option 93 (client architecture) and option 97 (machine GUID). It never checks the client's option 60, and it discards INFORM (`sccmpxe!0x1400207e8`, `sccmpxe!0x140022b38`).
- It never reads option 54, so on port 67 it answers any PXE REQUEST, including the one that accepts another server's address lease (`sccmpxe!0x1400201cc`).
- It replies to the request's source address and port. It broadcasts only when the source is `0.0.0.0` on the port 67 socket, and it answers a relayed request by unicast to the relay (`sccmpxe!0x140023150`).
- The DWORD `ResponseDelay` (default 0) makes it ignore a DISCOVER until the client's `secs` field reaches that value (`sccmpxe!0x140021af4`).

### Reply contents

- A reply is the request edited in place. The service copies each request into a zeroed buffer of its own, writes the reply over it, and sends that buffer (`sccmpxe!0x1400235e0`, `sccmpxe!0x140020f38`, `sccmpxe!0x140023150`).
- The reply sets `op` to 2 and `siaddr` to the server's address. Only the ACK to a REQUEST carries the boot file. The service copies its path, with a NUL, into the BOOTP `file` field (`sccmpxe!0x140020f38`).
- The code that logs each packet writes a NUL into the last byte of `sname` and the last byte of `file`. It runs on the request and again on the reply (`sccmpxe!0x140022b38`, `sccmpxe!0x1400235e0`, `sccmpxe!0x140023150`).
- On an IPv4 socket, the service swaps the two bytes of `secs` on receive when the second is larger than the first. A reply can therefore carry them in the other order (`sccmpxe!0x1400235e0`).
- Every other header byte keeps the request's value. This covers `xid`, `flags`, `ciaddr`, `yiaddr`, `giaddr`, `chaddr`, the cookie, and `sname` and `file` apart from their last bytes. In an ACK, `file` keeps the request's bytes only after the boot file's NUL (`sccmpxe!0x1400235e0`, `sccmpxe!0x140020f38`, `sccmpxe!0x140022b38`).
- The options start right after the cookie, in this order: 53, 54, 97, 60 `PXEClient`, and 82 echoed when the request has it. A first-stage ACK then adds option 250. A second-stage ACK adds option 243, then option 252, and no option 250. One `0xFF` ends the list (`sccmpxe!0x140020f38`).
- Option 53 is 2 in an OFFER and 5 in an ACK (`sccmpxe!0x140020f38`).
- An OFFER carries no option 43, 66, or 67 (`sccmpxe!0x140020f38`).
- Option 97 always goes out with length 17. The service copies 17 bytes from the start of the request's option 97 value. The parser uses that option's length only to step over it, and the code that copies the value ignores it. A shorter option 97 gets back the bytes that follow it in the request, then zeros past the request's end. A longer one gets back only its first 17 bytes (`sccmpxe!0x1400201cc`, `sccmpxe!0x1400235e0`, `sccmpxe!0x14002089c`, `sccmpxe!0x140020f38`).
- The reply ends right after the `0xFF`, and nothing pads it (`sccmpxe!0x140020f38`, `sccmpxe!0x140023150`). An OFFER without option 82 is therefore 280 bytes. That is 240 for the header and cookie, 3 for option 53, 6 for option 54, 19 for option 97, 11 for option 60, and 1 for the `0xFF`.
- [RFC 1542](https://www.rfc-editor.org/rfc/rfc1542) section 2.1 calls 300 octets the minimal BOOTP header. It says a receiver should check for that size, and it must silently discard a message that fails the check.
- The service echoes the whole of option 82, so an option 82 of n bytes adds 2 + n bytes. An OFFER with option 82 reaches 300 bytes only when n is 18 or more (`sccmpxe!0x1400201cc`, `sccmpxe!0x140020f38`).
- A first-stage ACK is 312 bytes without option 82. With the short option 250 that [Two-stage boot chain](#two-stage-boot-chain) lists for an abort or for a required deployment on UEFI, it is 285 bytes (`sccmpxe!0x140020e14`, `sccmpxe!0x140020f38`).

### Two-stage boot chain

Over IPv4, the service never gives the firmware the boot manager directly. The firmware first gets WDS's network boot program. That program sends a second request carrying option 250, and the reply to it holds the boot manager and the BCD path (`sccmpxe!0x1400214c4`, `sccmpxe!0x14002089c`).

| Option 93 | Firmware | First ACK | Second ACK |
| --- | --- | --- | --- |
| 7 or 9 | UEFI x64 | `smsboot\<pkg>\x64\wdsmgfw.efi` and option 250 | `smsboot\<pkg>\x64\bootmgfw.efi` and options 243 and 252 |
| 11 | UEFI ARM64 | `arm64\wdsmgfw.efi` and option 250 | `arm64\bootmgfw.efi` and options 243 and 252 |
| 6 | UEFI x86 | `x86\wdsmgfw.efi` and option 250 | Never `x86\bootmgfw.efi`. The second ACK follows sub-option 1, as the notes below say |
| 0 | BIOS | `x86\wdsnbp.com` and option 250 | `pxeboot.com`, or `pxeboot.n12` for a required deployment, and options 243 and 252 |

- `<pkg>` is the boot image's package ID.
- In the second request, option 250 sub-option 1 stands in for option 93 and picks the second ACK. 7 or 9 gives UEFI x64, and 11 gives UEFI ARM64. 2, 3, or 6 gives BIOS x64, and any other value gives BIOS x86 (`sccmpxe!0x14002089c`, `sccmpxe!0x1400214c4`).
- No value of sub-option 1 selects `x86\bootmgfw.efi`, so no second ACK can name it. This material does not show which value the x86 `wdsmgfw.efi` sends.
- A BIOS client always sends option 93 = 0, whatever its CPU. An x64 boot image therefore also installs `x86\wdsnbp.com`.
- Option 250 is code `0xFA`, a length byte, and a run of sub-options, each written as code, length, value. It holds no `0xFF` of its own (`sccmpxe!0x140020134`, `sccmpxe!0x140020e14`, `sccmpxe!0x140020f38`).
- The first-stage option 250 normally holds sub-option 2 = 1, sub-option 5, sub-option 3 = 20, sub-option 4 = 186, sub-option 6 = `SCCM PXE`, and sub-option 11 = 1, in that order. Sub-options 3 and 4 are 2 bytes each, big-endian. Sub-option 6 is 8 bytes, with no NUL. These are the 32 bytes, where `o0` to `o3` are sub-option 5 (`sccmpxe!0x140020e14`):

  ```text
  FA 1E  02 01 01  05 04 o0 o1 o2 o3  03 02 00 14  04 02 00 BA  06 08 53 43 43 4D 20 50 58 45  0B 01 01
  ```

- Sub-option 5 is the first 4 bytes of the request's option 5 value, unchanged. The parser checks only that option 5 fits in the request, not that it is 4 bytes long. It always reads 4 bytes, so a shorter option 5 gives the bytes that follow it in the request, then zeros past the request's end. The service sets the value to 0 before it parses each request, so a request without option 5 gets `00 00 00 00` (`sccmpxe!0x1400201cc`, `sccmpxe!0x1400235e0`, `sccmpxe!0x140020e14`).
- A required deployment on UEFI gets only sub-option 11 = 2, which is `FA 03 0B 01 02`. A required deployment on BIOS gets the normal option, with sub-option 11 = 1 (`sccmpxe!0x140020e14`).
- An abort in the first stage gets only sub-option 2 = 5, which is `FA 03 02 01 05` (`sccmpxe!0x140020e14`). [Deployment decision](#deployment-decision) says which clients receive it.
- The meanings come from WDS header names and are inferred. Sub-option 2 is the next action, 3 and 4 are the poll interval and retry count, and 11 is the key-press prompt.
- Any option 250 in a request marks it as the second request. The parser walks the sub-options and checks each length against what is left of option 250. It reads only sub-option 1, as 2 bytes big-endian, whatever length that sub-option gives (`sccmpxe!0x1400201cc`).
- The parser stops when 2 or fewer bytes of option 250 remain, so it never reads a sub-option that starts there.
- The service drops a second request when the parser has not read sub-option 1, and it logs `(continuation) does not have a WDS architecture entry`. It also drops the request when a sub-option runs past the end of option 250 before the parser reaches sub-option 1, and it logs `Invalid WDS packet structure.` (`sccmpxe!0x1400201cc`, `sccmpxe!0x1400207e8`, `sccmpxe!0x140020b18`, `sccmpxe!0x140021af4`, `sccmpxe!0x140023150`).
- Option 252 holds the path of a BCD that the server built for the boot image, relative to the TFTP root and with no terminating NUL, such as `SMSTemp\SMS00001-00000-00000.bcd`. The two numbers are the configured TFTP block size and window size. The server reuses a BCD for 600 seconds (`sccmpxe!0x140015eb4`).
- Option 243 carries the WinPE handoff. It holds sub-option 2, an encrypted key, then sub-option 1, the path of a variables file, then sub-option 3, the server's name. Sub-options 1 and 3 carry no NUL (`sccmpxe!0x140020d8c`).
- Which packet the boot manager reads option 252 from is not in this material, because the client boot programs are not in it.

### Deployment decision

- The service looks the PC up through the management point by the option 97 GUID (first three fields little-endian) and the hardware address. It drops a match on an identifier that it knows is shared by several PCs. An unknown PC uses the deployments for unknown computers, or gets no reply when that support is off (`sccmpxe!0x140021af4`, `sccmpxe!0x14000c6c8`).
- With no deployment, a UEFI client gets no reply at all ("Ignoring UEFI boot."), so the firmware moves on to the next boot device. The service checks a UEFI client's deployments at its DISCOVER and again at its REQUEST, so that client gets no OFFER either (`sccmpxe!0x140021af4`, `sccmpxe!0x140010088`, `sccmpxe!0x140023150`).
- The service checks a BIOS client's deployments only at its REQUEST. A BIOS client with no deployment can therefore still get an OFFER, and then an ACK that names `abortpxe.com` (`sccmpxe!0x140021af4`, `sccmpxe!0x140010088`, `sccmpxe!0x1400214c4`, `sccmpxe!0x140020f38`, `sccmpxe!0x140023150`).
- In the first stage, that ACK carries the abort option 250, so only BIOS clients receive it. In the second stage, it carries option 243 with empty sub-options 2 and 1, and an empty option 252, instead (`sccmpxe!0x140020e14`, `sccmpxe!0x140020f38`, `sccmpxe!0x140020d8c`, `sccmpxe!0x1400235e0`, `sccmpxe!0x1400214c4`, `sccmpxe!0x140021af4`, `sccmpxe!0x140023150`).
- The required-deployment choices in [Two-stage boot chain](#two-stage-boot-chain) suppress the key-press prompt, and any other deployment asks for a key press. The file name and the WDS header names suggest this (inferred), because the code only picks the file and the sub-option (`sccmpxe!0x140010088`, `sccmpxe!0x1400214c4`, `sccmpxe!0x140020e14`).
- After the second-stage reply for a required deployment, the service sends a "last PXE advertisement" state message, and the management point then reports that deployment as already run. A deployment counts as used when the PXE reply goes out, not when the install finishes (`sccmpxe!0x140010088`, `sccmpxe!0x140023150`).

### BCD

The service builds each BCD through the BCD WMI provider (`root\wmi`, `BcdStore`), so it can only run on Windows (`sccmpxe!0x140015eb4`). The store holds four objects.

| Object | Elements |
| --- | --- |
| `{bootmgr}`, type 0x10200002 | description `Windows BootManager`, path `0` for IPv4 clients, fontpath `\SMSBoot\<pkg>\Fonts`, timeout 30, displayorder = the loader |
| `{ramdiskoptions}` `{ae5534e0-a924-466c-b836-758539a3ee3a}` | ramdisksdidevice = boot, ramdisksdipath `\SMSBoot\<pkg>\boot.sdi`, and ramdisktftpblocksize and ramdisktftpwindowsize only when the DWORDs `RamdiskTFTPBlockSize` and `RamdiskTFTPWindowSize` are above 0. Both default to 0 |
| OS loader, type 0x10200003, random GUID | device and osdevice `ramdisk=[boot]\<pkg>.WIM,{ramdiskoptions}`, systemroot `\Windows`, detecthal, winpe. No winload path, nx, or locale |
| `{dbgsettings}` | serial debugging at 115200 baud on port 1, which nothing inherits |

In WDS mode, `smspxe.dll` builds the same objects but sets `default` instead of `displayorder` and uses the image's name and real path (`smspxe!0x18003bf04`).

### TFTP server

- The server is read-only and serves files relative to its own folder. It turns `/` into `\`, strips leading `\`, and rejects `..` and `:`. A name of 12 or more characters that matches `*.wim` maps to a boot image by the 8 characters before `.wim` (`sccmpxe!0x14001dcf0`).
- It negotiates `blksize` (at most 32768, lowered to `RamdiskTFTPBlockSize` when that is set), `tsize`, and `windowsize`. It has no variable-window extension and ignores the transfer mode. It echoes the requested window size in the OACK and only then limits it to 32 blocks (`sccmpxe!0x14001dcf0`, `sccmpxe!0x14001cd10`).
- It sends a whole window and accepts only the ACK for the window's last block. Any other ACK resends the whole window. The timeout is 5 seconds, with 5 retries. Each transfer gets its own thread and socket, and a retransmitted request starts a second transfer (`sccmpxe!0x14001cd10`).
- File sizes and byte counts are 32-bit, so a file of 2 GiB or more cannot be served (inferred from the C runtime's `_fstat64i32`).
- The only ERROR it sends is code 0, `Failed to open file.`. It does not log misses for `EFI\Microsoft\Boot\*`, `smsboot\*\x*\BCD`, and `hiberfil.sys`, which are probably the boot manager's probes (inferred).

### WinPE handoff

- In WinPE, `TsPxe.dll` reads the raw DHCP reply from the binary value `BootServerReply` under `HKLM\SYSTEM\ControlSet001\Control\PXE`. A missing value means the PC did not PXE-boot. The server's address is the reply's `siaddr` (`TsPxe!0x180004290`). This material does not show which component writes the value.
- `TsPxe.dll` downloads the variables file named in option 243 with `smstftp.exe get <server> <file> <local file>`. That client sends no TFTP options and moves 512-byte blocks one at a time. It makes up to 7 attempts, 15 seconds apart, after it adds a firewall rule for itself (`TsPxe!0x180004290`, `TsPxe!0x180005970`, `smstftp!0x140001118`).
- The variables file is AES-256-encrypted XML. It holds the management points, the site code, trust roots, and the distribution point's certificate with its private key. It holds no deployment ID, because WinPE asks the management point for the deployment later.
- Without a PXE password, the server makes a random 10-byte password for the file and sends it in option 243 sub-option 2. It encrypts that password with keys that are hard-coded, and identical, in `sccmpxe.exe` and `TsPxe.dll`, so anyone who captures the reply can decrypt the file (`sccmpxe!0x14001fcb8`, `sccmpxe!0x14001fe7c`). Every PXE client shares the same certificate.

### IPv6

The service answers DHCPv6 SOLICIT and REQUEST messages that ask for option 59. The reply carries option 59 with a `tftp://[<server>]/smsboot%5C<pkg>%5C<arch>%5C<file>;mode=octet` URL, and Microsoft vendor options (enterprise 311) that include the BCD path in sub-option 50014. It does not unwrap relayed DHCPv6 messages, and it has no UEFI HTTP boot (`sccmpxe!0x1400204d4`, `sccmpxe!0x140021084`).

### What is not there

- Nothing handles boot managers signed with the Windows UEFI CA 2023. No `EFI_EX`, `_EX.efi`, `UEFI CA`, or `2023` text appears in `sccmpxe.exe`, `smspxe.dll`, `TsPxe.dll`, or the 2509 `tscore.dll`. The service serves whatever `bootmgfw.efi` and `wdsmgfw.efi` the boot image holds, and never checks a client's Secure Boot state.
- `smspxe.dll` copies the boot files from each image's `Windows\Boot\PXE` (`pxeboot.com`, `pxeboot.n12`, `bootmgr.exe`, `wdsnbp.com`, `abortpxe.com`, `wdsmgfw.efi`), `Windows\Boot\EFI\bootmgfw.efi`, and `Windows\Boot\Fonts` (`smspxe!0x18003735c`). The component that fills `sccmpxe.exe`'s `smsboot` folder is not in this material.

## What changes in deployarr

| Finding | deployarr decision |
| --- | --- |
| Over IPv4, ConfigMgr never gives the firmware `bootmgfw.efi` directly. It sends WDS's `wdsmgfw.efi` or `wdsnbp.com` with option 250, and answers that program's second request with the boot manager and option 252 | The PXE prototype first tries `bootmgfw.efi` with option 252 in a single reply. If that fails, deployarr implements the two-stage exchange that ConfigMgr uses. See [PXE](../architecture.md#pxe) |
| The server chooses the BCD and names it in option 252 as a path relative to the TFTP root | deployarr sends option 252 with the path of its fixed BCD for the client's firmware |
| The OFFER carries no boot file, and only the ACK to a REQUEST carries it | deployarr's proxyDHCP replies the same way |
| Nothing pads a reply, so an OFFER without option 82 is 280 bytes, under the 300-byte BOOTP minimum | deployarr pads every reply to the 300-byte BOOTP minimum. The PXE prototype pads its replies, and the lab tests those |
| Option 97 always goes out with length 17, whatever length the request's option 97 has | deployarr parses option 97 where the request enters, as a type byte and a 16-byte GUID, and sends back those 17 bytes. A request whose option 97 has another length counts as a request without option 97. #56 decides how deployarr treats those |
| A second request without option 250 sub-option 1 gets no reply | No decision now. The lab capture in #28 shows whether both network boot programs always send sub-option 1 |
| A reply echoes the request's header, apart from `op`, `siaddr`, the boot file, the last bytes of `sname` and `file`, and a `secs` whose bytes it may swap | No decision now |
| The BCD loader has no winload path, and the TFTP size elements are absent by default | The prototype tests a template without them |
| The TFTP server echoes an unclamped window size, resends whole windows, uses 32-bit sizes, starts a second transfer for a retransmitted request, and answers a miss with error code 0 | deployarr clamps the window size before the OACK, resends from the first block not acknowledged (RFC 7440), uses 64-bit sizes, keeps one session per client address, port, and file, and answers a miss with error code 1 at once |
| With no deployment, a UEFI client gets no reply and a BIOS client gets `abortpxe.com` | deployarr does the same for a PC with no order and for a finished one |
| A required deployment suppresses the key-press prompt (inferred) | deployarr's zero-touch default sends `pxeboot.n12` on BIOS and, in the two-stage chain, option 250 sub-option 11 = 2 on UEFI |
| ConfigMgr counts a deployment as used when the PXE reply goes out | No change. deployarr stops offering PXE when the deployment succeeds, so a PC whose first attempt failed can try again |
| The WinPE handoff hides a password under keys hard-coded in both binaries, and every client shares one certificate | DHCP and TFTP carry no secret in deployarr. The agent authenticates to the server over HTTPS |
| WinPE finds the server through `BootServerReply` | No change now. The boot image already names the server. `BootServerReply` is how one boot image could serve several servers later |
| Nothing handles boot managers signed with the 2023 CA | The prototype must boot 2023-signed boot programs, both stages if the chain needs two, and the importer reports which CA signed each boot file |
| deployarr applies `install.wim` and never runs `setup.exe`, so nothing checks Windows 11 hardware. ConfigMgr's Check Readiness fills that gap with defaults below the Windows 11 minimums and no Secure Boot check | The agent runs a readiness check in WinPE before any disk step. Its defaults are Microsoft's Windows 11 minimums. See [Readiness check](../architecture.md#readiness-check) |
| Check Readiness records each result and each required value in variables | Each readiness check records its result, the required value, and the value found, so a later step or the UI can show why a PC failed |
| ConfigMgr reads TPM state through WMI `Win32_Tpm`, and the deployarr agent does not use WMI | The agent reads the TPM version through TBS. See [Agent](../architecture.md#agent) |
| `GetSecureBootState` keeps a third value, `NA`, for a missing registry value | The agent's Secure Boot fact has three values: on, off, and unknown. Unknown never counts as off |
| Windows 11 requires Secure Boot capable firmware, not Secure Boot on | The Secure Boot check warns by default instead of failing |
| The wizard sizes Recovery as about 1% of the disk | The server sizes Recovery from the image's `winre.wim` plus 250 MB, as Microsoft documents. See [Disk layout and boot files](../architecture.md#disk-layout-and-boot-files) |
| The 2509 `IsUEFI` enables the privilege first, but reports BIOS when it cannot enable it | No change. The agent already calls `GetFirmwareType` and uses the firmware variable check only as a fallback |
| ConfigMgr builds an in-place upgrade from `setup.exe /auto` with `/postoobe`, `/postrollback`, and `/compat ScanOnly` | No change now, because in-place upgrades are not in the first release. This page is the reference when they are |
