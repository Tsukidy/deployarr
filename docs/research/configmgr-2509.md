# ConfigMgr 2509 OSD reference

This page records what Configuration Manager (ConfigMgr) 2509 ships for operating system deployment (OSD), how its steps handle Windows 11, and how its PXE responder boots WinPE without WDS. MDT runs an older build of the same task sequence engine, which [MDT 8450 binaries reference](mdt-8450-binaries.md) covers. Use this page when you design a deployarr step that ConfigMgr also has.

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

## PXE responder

ConfigMgr 2509 has two PXE paths. `BIN/X64/smspxe.dll` is a provider that runs inside WDS. `BIN/X64/sccmpxe.exe` is a service with its own DHCP and TFTP code, so it needs no WDS. This section describes `sccmpxe.exe` unless it names `smspxe.dll`. In WinPE, `OSD/bin/x64/TsPxe.dll` and `OSD/bin/x64/smstftp.exe` pick up where the boot ends.

| Binary | Version | SHA-256 |
| --- | --- | --- |
| `BIN/X64/sccmpxe.exe` | 5.0.9141.1002 | `8588a8d5da07a4c5270089605d32b4efc6b0def61cd5c2d159e987c6ebc0e841` |
| `BIN/X64/smspxe.dll` | 5.0.9141.1002 | `9f500652ffa47592310582b693a9507907a6a3137a48ae4db9d2bcdfb03d3db2` |
| `OSD/bin/x64/TsPxe.dll` | 5.0.9141.1002 | `a4fc95b588a19555aa5160645b279342dabe67a5f89acc2049819cb747820808` |
| `OSD/bin/x64/smstftp.exe` | 5.0.9141.1002 | `f88cc9848279bad4c0639ff3aba58acdcb389d308c1eb4bbbad611940db13d4a` |

One agent wrote each claim in this section from Ghidra pseudocode, and a second agent re-opened each citation and tried to refute it. The claims that change deployarr's design were checked a third time by hand.

### Sockets

- The service binds, for each address on an Ethernet adapter that is up (at most 32 addresses), UDP 67 for DHCP (547 for DHCPv6), UDP 4011, and UDP 69 for TFTP. It sets no `SO_REUSEADDR`, so a port that another process holds fails with "Another process (DHCP server or PXE server) is already using the port" (`sccmpxe!0x140023f50`, `sccmpxe!0x1400239e4`, `sccmpxe!0x14001f3b0`).
- The DWORD `DoNotListenOnDhcpPort` under `HKLM\Software\Microsoft\SMS\DP` skips port 67, so a DHCP server can run on the same host. `BindPolicy` and `BindExcept` limit the adapters by MAC address (`sccmpxe!0x140023d30`).

### Requests it answers

- It handles a request only when it is a DISCOVER or a REQUEST that carries option 93 (client architecture) and option 97 (machine GUID). It never checks the client's option 60, and it discards INFORM (`sccmpxe!0x1400207e8`, `sccmpxe!0x140022b38`).
- It never reads option 54, so on port 67 it answers any PXE REQUEST, including the one that accepts another server's address lease (`sccmpxe!0x1400201cc`).
- It replies to the request's source address and port. It broadcasts only when the source is `0.0.0.0` on the port 67 socket, and it answers a relayed request by unicast to the relay (`sccmpxe!0x140023150`).
- The DWORD `ResponseDelay` (default 0) makes it ignore a DISCOVER until the client's `secs` field reaches that value (`sccmpxe!0x140021af4`).

### Reply contents

- An OFFER sets `siaddr` to the server's address and carries option 53, option 54, option 97 echoed, option 60 `PXEClient`, and option 82 echoed when present. It carries no option 43, 66, or 67, and no boot file (`sccmpxe!0x140020f38`).
- Only the ACK to a REQUEST carries the boot file, in the BOOTP `file` field. It also carries option 250 in the first stage, or options 243 and 252 in the second (`sccmpxe!0x140020f38`).

### Two-stage boot chain

Over IPv4, the service never gives the firmware the boot manager directly. The firmware first gets WDS's network boot program. That program sends a second request carrying option 250, and the reply to it holds the boot manager and the BCD path (`sccmpxe!0x1400214c4`, `sccmpxe!0x14002089c`).

| Option 93 | Firmware | First ACK | Second ACK |
| --- | --- | --- | --- |
| 7 or 9 | UEFI x64 | `smsboot\<pkg>\x64\wdsmgfw.efi` and option 250 | `smsboot\<pkg>\x64\bootmgfw.efi` and options 243 and 252 |
| 11 | UEFI ARM64 | `arm64\wdsmgfw.efi` and option 250 | `arm64\bootmgfw.efi` and options 243 and 252 |
| 6 | UEFI x86 | `x86\wdsmgfw.efi` and option 250 | `x86\bootmgfw.efi` and options 243 and 252 |
| 0 | BIOS | `x86\wdsnbp.com` and option 250 | `pxeboot.com`, or `pxeboot.n12` for a required deployment, and options 243 and 252 |

- `<pkg>` is the boot image's package ID.
- A BIOS client always sends option 93 = 0, whatever its CPU. The server learns the CPU only from option 250 sub-option 1 in the second request, where 6 selects the `x64` folder. An x64 boot image therefore also installs `x86\wdsnbp.com`.
- The first-stage option 250 normally holds sub-option 2 = 1, sub-option 5 = the 4 bytes of the client's option 5, sub-option 3 = 20, sub-option 4 = 186, sub-option 6 = `SCCM PXE`, and sub-option 11 = 1. A required deployment on UEFI gets only sub-option 11 = 2. An abort gets only sub-option 2 = 5 (`sccmpxe!0x140020e14`). The meanings come from WDS header names (inferred): sub-option 2 is the next action, 3 and 4 are the poll interval and retry count, and 11 is the key-press prompt.
- Option 252 holds the path of a BCD that the server built for the boot image, relative to the TFTP root and with no terminating NUL, such as `SMSTemp\SMS00001-00000-00000.bcd`. The two numbers are the configured TFTP block size and window size. The server reuses a BCD for 600 seconds (`sccmpxe!0x140015eb4`).
- Option 243 carries the WinPE handoff. Sub-option 1 is the path of a variables file, sub-option 2 an encrypted key, and sub-option 3 the server's name.
- Which packet the boot manager reads option 252 from is not in this material, because the client boot programs are not in it.

### Deployment decision

- The service looks the PC up through the management point by the option 97 GUID (first three fields little-endian) and the hardware address. It drops a match on an identifier that it knows is shared by several PCs. An unknown PC uses the deployments for unknown computers, or gets no reply when that support is off (`sccmpxe!0x140021af4`, `sccmpxe!0x14000c6c8`).
- With no deployment, a UEFI client gets no reply at all ("Ignoring UEFI boot."), so the firmware moves on to the next boot device. A BIOS client gets `abortpxe.com` (`sccmpxe!0x140021af4`).
- A required deployment suppresses the key-press prompt: `pxeboot.n12` on BIOS, and option 250 sub-option 11 = 2 on UEFI. Any other deployment asks for a key press (`sccmpxe!0x140010088`).
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
| The BCD loader has no winload path, and the TFTP size elements are absent by default | The prototype tests a template without them |
| The TFTP server echoes an unclamped window size, resends whole windows, uses 32-bit sizes, starts a second transfer for a retransmitted request, and answers a miss with error code 0 | deployarr clamps the window size before the OACK, resends from the first block not acknowledged (RFC 7440), uses 64-bit sizes, keeps one session per client address, port, and file, and answers a miss with error code 1 at once |
| With no deployment, a UEFI client gets no reply and a BIOS client gets `abortpxe.com` | deployarr does the same for a PC with no order and for a finished one |
| A required deployment suppresses the key-press prompt | deployarr's zero-touch default sends `pxeboot.n12` on BIOS and, in the two-stage chain, option 250 sub-option 11 = 2 on UEFI |
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
