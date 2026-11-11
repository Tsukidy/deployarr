# MDT 8450 binaries reference

This page records what the compiled parts of Microsoft Deployment Toolkit (MDT) do. These parts are the task sequence engine, the deployment helper tools, and the .NET code behind Deployment Workbench. The scripts and templates are covered in [MDT 8456 reference](mdt-8456.md). Use this page when you build the deployarr feature that replaces one of these components.

## Sources

- The installer is `MicrosoftDeploymentToolkit_x64.msi`, product version 6.3.8450.1000, from `https://download.microsoft.com/download/0/7/b/07b3b260-f008-4b1c-8eaa-74bcc5c7d646/MicrosoftDeploymentToolkit_x64.msi`. SHA-256 is `e73823db601b06e9c222ae816bbddd491ecb823a1f8cce0f73eec9de51c1690e`. That link still worked on 2026-11-11, after the 8456 link stopped working.
- `msiextract` from msitools unpacked the installer.
- ILSpy 8.2.0.7535 (`ilspycmd -p`) decompiled the .NET assemblies to C#, including `Microsoft.BDD.TaskSequencePSHost40.exe`. Citations such as `PSSnapIn/ImportDriver.cs:125` point into that output. The full path is `Microsoft.BDD.PSSnapIn/Microsoft.BDD.PSSnapIn/ImportDriver.cs`. Another ILSpy version may number lines differently.
- The engine binaries `TsCore.dll`, `TsManager.exe`, and `TsmBootstrap.exe` carry file version 5.0.8540.1000. That is a ConfigMgr build number, and build 8540 is ConfigMgr current branch 1706 (inferred from Microsoft's build numbering, not from these files). Current ConfigMgr releases ship newer engine builds, so engine behavior may differ there.
- REA `decompile-all` with Ghidra 12.1.4 exported every function of the native engine binaries to pseudocode. There are no symbols, so citations use the function address, such as `TsCore!0x180012345`.
- Every claim on this page was checked twice. One agent wrote it from the code, and a second agent re-opened each citation and tried to refute it. Claims marked "inferred" come from names or partial code rather than from the cited code itself.

### The script copy matches the installer

The 139 script and template text files in the installer are identical to `Source/MDT.8450/scripts` in [keithga/DeployShared](https://github.com/keithga/DeployShared), after line endings are normalized. Only 30 images are missing from that copy. The [MDT 8456 reference](mdt-8456.md) cites the same repository's `Source/MDT.8456` folder.

Between 8450 and 8456, most script changes are only the version string. These scripts changed in substance:

| Script | Changed lines |
| --- | --- |
| `ServerManager.xml` | 617 |
| `ZTIPatches.wsf` | 163 |
| `LiteTouch.wsf` | 116 |
| `ZTIGather.wsf` | 48, mostly chassis type 13 and VM detection |
| `ZTIBde.wsf` | 11 |
| `DeployWiz_LanguagePack.vbs` | 10 |

The 8456 `Client.xml` and `Server.xml` templates add an "Apply Patches" step. The 8450 engine binaries are therefore a sound stand-in for the engine that runs the 8456 scripts.

## Task sequence engine

The engine is three binaries. `TsmBootstrap.exe` starts or resumes a run. `TsManager.exe` runs the steps. `TsCore.dll` holds the shared code: the XML parser, the condition evaluator, the variable store, and logging. MDT runs the engine in what the code calls standalone mode. Citations such as `TsCore!0x18019c384` name a Ghidra function address, and the pseudocode for it is in `TsCore/functions/0x18019c384.c` of the export.

### Start and resume

- MDT starts the engine with `TsmBootstrap.exe /env:SAStart` for a new run or `/env:SAContinue` to resume (`LiteTouch.wsf:835-847`). The other modes (`FullOS`, `Gina`, `WinPE`, `/configpath`) serve ConfigMgr media and PXE (`TsmBootstrap!0x140029e80`).
- SAStart creates the variable store. It loads `variables.dat` and `TS.XML` from its own folder and stores the whole `TS.XML` in the variable `_SMSTSTaskSequence`. It then puts `_SMSTaskSequence` on the fixed volume with the most free space, deleting any existing folder of that name, and sets `_SMSTSStandAloneMode=true`. Last, it runs `TsManager.exe /standalone` as a child process and returns its exit code (`TsmBootstrap!0x14002cc68`, `TsmBootstrap!0x140061668`).
- No minimum free space applies. On a blank disk in WinPE no fixed volume exists, and every state save fails until one does (inferred from `TsCore!0x1801b5b0c` and `TsCore!0x180132fdc`). The engine never keeps its state on the WinPE RAM disk.
- SAContinue resumes from the `\_SMSTaskSequence\TSEnv.dat` with the newest last-access time on any fixed drive (`TsmBootstrap!0x14005b094`). `LiteTouch.wsf` takes the first fixed drive that has one instead, so the two can pick different copies.
- TsmBootstrap holds the mutex `Global\TSMBootStrapMutex`, so only one run can start at a time. TsManager has no lock of its own (`TsmBootstrap!0x1400300c0`).
- TsManager rebuilds its instruction table from `_SMSTSTaskSequence` every time it starts. The sequence is therefore frozen at the first launch.

### Instruction table and failure handling

- Each group compiles to a BeginGroup entry, its children, and an EndGroup entry. The BeginGroup entry stores the index after its EndGroup. Disabled groups and steps stay in the table with a flag, which is set only when `disable` is exactly `true` (`TsCore!0x18019d2b8`, `TsCore!0x18019dacc`).
- The step parser reads `name`, `type`, `description`, `continueOnError`, `successCodeList`, `retryCount`, `disable`, `runIn`, `timeout`, and `<action>`. It never reads `startIn`, so the 46 `startIn` attributes in `Client.xml` have no effect (`TsCore!0x18019dacc`).
- When a step fails and has `continueOnError`, the run continues. Otherwise the engine takes the enclosing group from a runtime stack. A group with `continueOnError` jumps past its EndGroup, and a group without it passes the failure up. With no group left, the run stops (`TsManager!0x140033090`, `TsManager!0x140032bb8`). The stack is saved as `_SMSTSInstructionStackString` and rebuilt on resume, without checking group conditions again (`TsManager!0x14002ab7c`).
- If the engine's own work for a step fails, such as applying the step's variables or launching the command, the whole run stops and `continueOnError` is ignored (`TsManager!0x14002f6f4`).
- A step with `runIn="WinPE"` that the engine reaches in the full OS, or `runIn="FullOS"` in WinPE, fails with error `0x80070032`. It is not skipped. Any other value, including `WinPEandFullOS`, places no limit (`TsManager!0x1400327cc`).
- An exit code must match an entry in `successCodeList` exactly. The engine has no default, so an empty list fails every exit code, including 0 (`TsManager!0x140035ea4`, `TsManager!0x140036040`). The `0 3010` lists come from the templates.
- Steps can have a `timeout` attribute in seconds (empty means no limit) and a `retryCount` attribute (default 0). The MDT templates set neither (`TsManager!0x14002c984`).
- The engine sets no user token, so child processes run in TsManager's own account and environment (`TsManager!0x140033c10`).

### Conditions

- The engine parses conditions when it loads the sequence and evaluates each one when it reaches its group or step. A false group condition jumps past the group. A false step condition skips the step and leaves the `_SMSTSLastAction*` variables unchanged (`TsManager!0x140033664`, `TsManager!0x140033c10`).
- There are ten expression types, matched case-sensitively: Variable, File, Folder, OS, Registry, WMI, Software, Computer, MakeModel, and Location. The `<condition>` element acts as an AND of its children. The operator nodes are `and`, `or`, and `not` (`TsCore!0x18019c384`).
- Operators are matched without regard to case: Exists, NotExists, Equals, NotEquals, Less, LessEqual, Greater, GreaterEqual, Like, AnyVersion, and ThisVersion (`TsCore!0x1801ab0b0`).
- A Variable condition compares as integers when both sides are digits with an optional leading `-`. Otherwise it compares strings without regard to case. Like uses `PathMatchSpecW` wildcards. Exists means "not empty", so an unset variable and an empty one look the same. Comparing against an empty Value is an error (`TsCore!0x1801a96c0`).
- Every parameter, including the variable name and the WMI query, is `%VAR%`-expanded when the condition runs (`TsCore!0x1801a96c0`).
- An empty AND is true, and so is an empty OR. A NOT with several children means "none of them is true" (`TsCore!0x1801a0dc0`, `TsCore!0x1801a55e8`, `TsCore!0x1801a53e4`).
- A missing file, folder, registry key, or registry value makes the condition false. File versions are compared as four 16-bit fields, and dates are compared with the last-write time to the second (`TsCore!0x1801a2494`). Registry conditions accept only full hive names and only `REG_DWORD`, `REG_SZ`, and `REG_EXPAND_SZ` values (`TsCore!0x1801a6bec`).
- A WMI condition is true when the query returns at least one row. A failed query is an error unless the expression has `ignoreQueryFailure="true"` (`TsCore!0x1801aa380`).
- A Software condition checks an MSI ProductCode or UpgradeCode with `MsiGetProductInfoW` and `MsiEnumRelatedProductsW`. It compares no versions (`TsCore!0x1801a8800`).
- MakeModel, Location, and Computer conditions use substring tests against the gathered facts. A gateway condition of `10.0.0.1` therefore also matches `10.0.0.10` (`TsCore!0x1801a4c28`, `TsCore!0x1801a4784`, `TsCore!0x1801a0fc4`).
- A condition that fails to evaluate is not false. It fails the step the same way a failed command does, with `_SMSTSLastActionRetCode` set to the error code (`TsManager!0x140033c10`).

### Variables

- The variable store is two 32 MB named shared-memory sections. One holds global variables, and the other holds the current step's default variables. Each value is encrypted with AES-256 under a random key that is stored in `HKLM\Software\Microsoft\SMS` and readable by Administrators and SYSTEM. Nothing locks the store (`TsCore!0x180137e60`, `TsCore!0x180137180`).
- `Microsoft.SMS.TSEnvironment` is an in-process COM object in `TsCore.dll`. Every script, and `TSEnv.exe "Name=Value"`, maps the sections and changes them directly. Nothing tells TsManager (`TsCore!0x1801f3738`, `TSEnv!0x140002794`).
- A name is 1 to 256 letters, digits, `_`, or `-` after trimming, and lookups ignore case. Scripts cannot set a name that starts with `_`. The COM object's `Clear()` method still erases every variable, the engine's included (`TsCore!0x18012c52c`).
- A lookup reads the global value first and uses the step's default only when the global value is empty. Values in `globalVarList` only fill variables that are still empty. A step's `defaultVarList` is applied before its condition is evaluated and cleared when the step ends (`TsCore!0x18012d248`, `TsManager!0x14002273c`, `TsManager!0x140033c10`).
- `%VAR%` expansion recurses up to 10 levels, and `%%` stands for a literal `%`. An undefined name stays as written, and the result then goes through `ExpandEnvironmentStringsW`, so process environment variables fill the gaps. The expander scans inserted text again, so storing `%%A%%` in a variable named `A` makes expansion loop forever (`TsCore!0x18012cc00`). Scripts always receive expanded values (`TsCore!0x1801f34e4`).
- The engine logs command lines before expansion. It has no list of sensitive names (`TsManager!0x14002c43c`).
- The engine sets `_SMSTSCurrentActionName`, `_SMSTSNextInstructionPointer`, `_SMSTSInstructionTableSize`, `_SMSTSInstructionStackString`, `_SMSTSLastActionSucceeded`, `_SMSTSLastActionRetCode`, `_SMSTSLastActionRetryCount`, `_SMSTSLogPath`, `_SMSTSMDataPath`, `_SMSTSBootUEFI`, `_SMSTSInWinPE`, and `_SMSTSStandAloneMode`. A skipped step leaves the last-action variables unchanged.

### State file

- Only global variables are saved, to `%_SMSTSMDataPath%\TSEnv.dat`. The file is XML encrypted with AES-128 under a key derived from the SMBIOS UUID and a constant GUID, so anyone holding both the disk and the machine can read it (`TsCore!0x1801fc884`).
- The engine writes the file in place with `CreateFileW(..., CREATE_ALWAYS)`. There is no temporary file, flush, or rename (`TsCore!0x1801fd080`).
- The engine saves after each completed step, with the pointer already past that step. Nothing is written before a step starts, and skipped steps are not saved. After a power loss, the interrupted step runs again (`TsManager!0x14002f6f4`, `TsManager!0x140033c10`).
- If a save fails, the engine logs a warning and moves on to the next step (`TsManager!0x14002f6f4`).

### Reboots

- A step requests a reboot by setting `SMSTSRebootRequested` to any non-empty value. Setting `SMSTSRetryRequested` as well makes the same step run again after the reboot. A retry request without a reboot request is a fatal error (`TsManager!0x14002d174`).
- Exit code 3010 never causes a reboot.
- Before a reboot, the engine creates the volatile registry key `HKLM\Software\Microsoft\SMS\Task Sequence\PendingReboot`, which Windows removes at restart. The engine refuses to resume while the key exists (`TsManager!0x14002f544`, `TsManager!0x1400215ac`).
- In standalone mode, the engine does not reboot. It logs `Skipped RebootSystem()` and exits with `0x80070BC2` (`TsManager!0x140023876`), and `LiteTouch.wsf` does the reboot. `Smsboot.exe /target:WINPE` or `/target:HD` only sets `SMSTSRebootRequested` (`Smsboot!0x140003850`).
- For a retry, the engine moves the pointer back without saving. Any variables the retrying step changed are lost unless a script saved them itself (`TsManager!0x14002f6f4`).
- The handling of an outside shutdown during a step (exit code 1115 and the `retryCount` attribute) runs only when TsManager is the ConfigMgr service, never in MDT (`TsManager!0x1400215ac`).
- ConfigMgr's own code for staging WinPE on disk, which copies `boot.wim` and creates a RAM disk BCD entry through WMI, is in `TsCore.dll` but MDT never reaches it (`TsCore!0x1800c1228`, `TsCore!0x1800d2010`).

### Logging and power

- `smsts.log` uses CMTrace lines. Each file holds 2 MiB by default, with 2 older files kept. `%WINDIR%\SMSTS.INI` can change `LogMaxSize`, `LogMaxHistory`, `LogLevel`, and `DebugLogging`. In WinPE the log is written to `%TEMP%\SMSTSLog` and copied to `%_SMSTSMDataPath%\Logs` after each step that succeeds. Each child process's output is captured into `smsts.log` (`TsCore!0x18016d438`, `TsCore!0x18016d1d8`).
- TsManager holds a power request for the whole run, so the PC does not sleep (`TsManager!0x140020404`).

### Firmware and disks

- `TsCore.dll` detects UEFI in two ways. `IsUEFI` calls `GetFirmwareEnvironmentVariableA` without enabling `SeSystemEnvironmentPrivilege`, and its result becomes `_SMSTSBootUEFI` (`TsCore!0x1801c4ecc`). `Firmware::IsEfi` calls `GetFirmwareType` (`TsCore!0x18013baf0`). The first is the version that the 8456 hotfix fixed in `Microsoft.BDD.Utility.dll`, so `_SMSTSBootUEFI` can be wrong on BIOS PCs (inferred).
- A disk is USB when `IOCTL_STORAGE_QUERY_PROPERTY` reports bus type 7 (`TsCore!0x1800f7a70`).
- On MBR disks the active system partition must be NTFS. On UEFI the EFI partition must be FAT32 (`TsCore!0x1800d43dc`).
- `TsCore.dll` runs `diskpart.exe /s <script>` and runs it once more after 30 seconds if it exits with code 4 (`TsCore!0x1800f842c`).
- `TsCore.dll` uses WIMGAPI only to extract boot files and to capture images. MDT applies the OS with DISM in `LTIApply.wsf` (`TsCore!0x1800cc3a8`).
- ConfigMgr's content download uses WinHTTP with `Range` resume, 3 attempts 15 seconds apart, and one SHA-256 hash per package. A hash mismatch deletes the whole package. The destination is wiped before each download, so a partial download never survives a restart (`TsCore!0x18014c25c`, `TsCore!0x180121668`). MDT never uses this code.

## Deployment helper tools

### BDDRun.exe

- `bddrun.exe /bootstrap`, started by `winpeshl.ini`, runs only `wpeinit.exe` and waits for it. `LiteTouch.wsf` runs inside `wpeinit` as the `RunSynchronous` command in `X:\Unattend.xml` (inferred from WinPE behavior). BDDRun registers F8 as a global hotkey that opens `cmd.exe`. If `wpeinit` fails, BDDRun opens a `cmd.exe` and waits for it. Before it exits, it waits for every F8 console to close, which holds off the WinPE reboot (`BDDRun!0x140006f7c`, `BDDRun!0x1400077d4`).
- `bddrun.exe <command>` runs the command with `CreateProcessW` and waits only for that process. If the launch fails, BDDRun returns the Windows error code as if it were the command's exit code (`BDDRun!0x140007630`).
- `bddrun.exe /runas <command>` reads `SMSTSRunCommandLineUserName` and `SMSTSRunCommandLineUserPassword` and runs the command with `CreateProcessWithLogonW` (`BDDRun!0x140006f7c`).

### Microsoft.BDD.PnpEnum.exe

PnpEnum calls `SetupDiGetClassDevsW(NULL, NULL, NULL, DIGCF_PRESENT | DIGCF_ALLCLASSES)` and writes each present device's hardware IDs and compatible IDs to standard output in this form (`PnpEnum!0x1400064fc`):

```xml
<?xml version="1.0" encoding="utf-8"?>
<devices>
  <device>
    <HardwareID><![CDATA[PCI\VEN_8086&DEV_15F3]]></HardwareID>
    <CompatibleID><![CDATA[PCI\CC_0200]]></CompatibleID>
  </device>
</devices>
```

It reads no device class or problem code. It always exits with 0, even when enumeration fails.

### Microsoft.BDD.Utility.dll

- The type library declares 11 methods: `IsAdmin`, `Is64Bit`, `IsKnownToDNS`, `IsHypervisorRunning`, `SupportsNX`, `SupportsVT`, `Supports64Bit`, `KeepAlive`, `IsUEFI`, `GetErrorMessage`, and `HiddenPartitionsToDrives`. The object also contains `WaitForShell` code, but the type library leaves it out, so the call at `LiteTouch.wsf:822` fails silently.
- The CPU methods read CPUID (`Utility!0x18000c588`):

| Method | CPUID bit |
| --- | --- |
| `SupportsVT` | leaf 1, ECX bit 5 (VMX) on Intel; leaf 0x80000001, ECX bit 2 (SVM) otherwise |
| `IsHypervisorRunning` | leaf 1, ECX bit 31 |
| `SupportsNX` | leaf 0x80000001, EDX bit 20 |
| `Is64Bit`, `Supports64Bit` | leaf 0x80000001, EDX bit 29 |

- `KeepAlive` calls `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`.
- `HiddenPartitionsToDrives` reads each disk's partition layout with `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` and matches partitions to volumes with `IOCTL_STORAGE_GET_DEVICE_NUMBER` (`Utility!0x18000d2a8`).
- The 8456 hotfix changes only `IsUEFI`. The 8450 build calls `GetFirmwareEnvironmentVariableW` without enabling `SeSystemEnvironmentPrivilege`, and the 8456 build enables the privilege first. A likely reason is that newer WinPE versions return `ERROR_PRIVILEGE_NOT_HELD` on BIOS PCs before they return `ERROR_INVALID_FUNCTION`, so the 8450 build reported BIOS PCs as UEFI (inferred).

## Driver import

`Import-MDTDriver` is the `ImportDriver` cmdlet in `Microsoft.BDD.PSSnapIn`.

- It accepts only folders. It walks each folder depth first, treats `*.inf` files as drivers, and expands `*.cab` files with `expand.exe "<cab>" -F:* "%TEMP%\<cab name>.extract"`. It does not handle EXE, ZIP, or `txtsetup.oem` files (`PSSnapIn/ImportDriver.cs:125-212, 379-404`).
- It parses INF files with SetupAPI through P/Invoke (`SetupOpenInfFileW`, `SetupFindFirstLine`, `SetupGetStringField`), so import works only on Windows. The class name comes from the server's own registry through `SetupDiClassNameFromGuid` (`PSSnapIn/InfInfo.cs:118-158, 448-476`).
- An INF is imported only if `[Version]` has both `ClassGuid` and `Provider` (`PSSnapIn/ImportDriver.cs:280`). The verifying skeptic found exceptions. A sibling INF from an already-imported folder skips the `ClassGuid` check, so the result depends on file order.
- A malformed `ClassGuid` throws inside the constructor, and nothing catches it, so the whole import stops (inferred, `PSSnapIn/InfInfo.cs:121-125`).

### Architecture and OS version

MDT derives a driver's architecture with substring tests on each `[Manufacturer]` decoration (`PSSnapIn/InfInfo.cs:186-206`):

| Decoration contains | Platform |
| --- | --- |
| `NTx86` | x86 |
| `NTamd64` | x64 |
| `NTia64` | ia64 |
| any other `NT`, such as `NTarm64` | x86 |
| nothing, an undecorated entry | x86 |

- An ARM64-only INF is catalogued as x86. An INF with an undecorated model section is marked x86 even when every other decoration is `NTamd64`.
- A later check reads the PE header of each binary that the INF names and removes a claimed platform that no binary has. It records the correction as a zero-byte marker file, `<inf>_override.x86` or `<inf>_override.x64` (`PSSnapIn/Verifier.cs:137-156`).
- The OS version is the raw text after the first dot of each decoration. For example, `NTamd64.10.0...16299` is stored as `10.0...16299`. It is never parsed (`PSSnapIn/InfInfo.cs:217-220`). This is why `ZTIDrivers.wsf` never reads `OSVersion`, as the [MDT 8456 reference](mdt-8456.md#drivers) notes.
- MDT collects every hardware ID and compatible ID from every model section into one flat list. Each ID loses its kind, its architecture, and its OS version. De-duplication is case-sensitive (`PSSnapIn/InfInfo.cs:237-253`). That loss is why MDT's driver matching cannot rank packages or filter by OS.

### Storage and duplicates

- MDT keys each driver package by a SHA-256 over the content of the INF's whole folder tree. All INF files in one folder share that key. The hash covers file contents only, so renaming a file does not change it (`PSSnapIn/Utility.cs:193-230`).
- Import copies the INF's whole parent folder to `Out-of-box Drivers\<Class>\<inf name>_<DriverVer version>_<hash>`. It then still walks the subfolders it already copied, so a vendor tree with INF files at several levels is stored more than once (`PSSnapIn/ImportDriver.cs:57, 286, 399-404`).
- Each INF becomes one `Drivers.xml` item with a random GUID. `Manufacturer` holds the `[Version]` Provider, not the `[Manufacturer]` section (`PSSnapIn/Verifier.cs:123-171`).
- Import calls `SetupVerifyInfFile` and stores the result as `WHQLSigned`, but no deployment code reads it. WinPE injection passes `/ForceUnsigned` anyway (`PSSnapIn/InfInfo.cs:363-379`, `PSSnapIn/PEManager.cs:149`).

### Selection profiles

MDT resolves a driver selection profile with two different algorithms.

- On the server, the profile is an ordered list of `<Include path>` and `<Exclude path>` rules. An include adds that folder and every folder below it. An exclude removes every collected folder whose path starts with the given text. The test is case-insensitive and does not check for a path separator (`PSSnapIn/Utility.cs:384-439`).
- At deployment time, `ZTIConfigFile.vbs` walks up from each folder, and the nearest include or exclude decides (`ZTIConfigFile.vbs:211-226, 495-505`). `DriverGroup` matches with `InStr`, so a group named "Dell" also matches a folder such as "HP\Dell Dock" (inferred).
- The stock Inject Drivers step uses the profile "All Drivers", and `DriverGroup` is only consulted when no profile rule matches. So `DriverGroup001` does nothing unless the profile is narrowed (`Templates/Client.xml:275-280`).
- Boot image generation counts drivers in disabled folders, but deployment does not (`PSSnapIn/UpdateDeploymentPoint.cs:371`).

## Boot image build

`Update-MDTDeploymentShare` is the `UpdateDeploymentPoint` cmdlet. Its helper `PEManager` builds `LiteTouchPE_<platform>.wim`.

- MDT finds the Windows ADK only through `HKLM\Software\Microsoft\Windows Kits\Installed Roots\KitsRoot10`. It reads `winpe.wim` from `Windows Preinstallation Environment\<amd64|x86>\en-us` and optional components from `WinPE_OCs` (`PSSnapIn/DeploymentTools.cs:39-115`).
- With `UseBootWim=True`, the default, MDT prefers `sources\boot.wim` from an imported OS whose build equals the ADK's `winpe.wim` build exactly, and otherwise uses the ADK image. It always uses index 1 (`PSSnapIn/UpdateDeploymentPoint.cs:320-341`).
- MDT mounts, unmounts, exports, and marks the boot image through WIMGAPI. It does all servicing through separate `dism.exe` calls (`PSSnapIn/PEManager.cs:80-227`):

```
dism.exe /image:"<mount>" /set-targetpath:X:\
dism.exe /image:"<mount>" /Set-ScratchSpace:<n>
dism.exe /image:"<mount>" /Add-Package /PackagePath:"<WinPE_OCs\component.cab>"
dism.exe /image:"<mount>" /Add-Driver /driver:"<inf>" /ForceUnsigned
```

- Optional components are added in this order: `winpe-hta`, `winpe-scripting`, `winpe-wmi`, `winpe-securestartup`, `winpe-fmapi`, then the `FeaturePacks` setting (default `winpe-mdac`). Each component's `en-us` language CAB follows it (`PSSnapIn/PEManager.cs:90-121`). The dependency table in `Bin/FeatureNames.xml` is incomplete. Choosing `winpe-powershell` alone adds it without `winpe-netfx`.
- The WinPE driver set is every enabled driver for the platform whose class is `NET`, `SCSIADAPTER`, or `HDC` (on by default), or `DISPLAY` or `SYSTEM` (off by default). There is no PnP or OS check (`PSSnapIn/UpdateDeploymentPoint.cs:408-449`).
- With the default profile "All Drivers and Packages", every enabled package in the share is also added to WinPE, including OS updates (`PSSnapIn/UpdateDeploymentPoint.cs:460-466`).
- No failure stops the build. A failed DISM call returns false, the caller still logs "Injected driver", and the saved profile records the item as present. Later incremental builds never retry it (`PSSnapIn/UpdateDeploymentPoint.cs:636-653, 784-836`).
- MDT patches the existing boot image when the recipe changes. It rebuilds from the base image only when forced, when the base changed, or when a referenced item is gone (`PSSnapIn/UpdateDeploymentPoint.cs:494-607`).

### ISO and BCD

- The ISO is built with `oscdimg -u2 -udfver102 -m -o -h -w4 -yo"<BootOrder.txt>" -bootdata:2#p0,e,b"<etfsboot.com>"#pEF,e,b"<efisys.bin>" "<source>" "<iso>"`. MDT uses `efisys.bin`, and no code references `efisys_noprompt.bin` (`PSSnapIn/PEManager.cs:295-306`). The ADK's `efisys.bin` waits for a key press before it boots from the disc (inferred from ADK behavior, not from this code).
- The share update never edits a BCD store and never adds images to WDS. Its only WDS call creates a multicast namespace with `wdsutil /new-namespace` (`PSSnapIn/UpdateDeploymentPoint.cs:889-907`). Admins import the boot image into WDS by hand.
- Offline media (`Update-MDTMedia`) edits the media's BCD stores with `bcdedit`. It creates an OS loader entry with these elements (`PSSnapIn/GenerateMDTMedia.cs:541-676`):

```
device    ramdisk=[boot]\Deploy\Boot\LiteTouchPE_x64.wim,{7619dcc8-fafe-11d9-b411-000476eba25f}
osdevice  ramdisk=[boot]\Deploy\Boot\LiteTouchPE_x64.wim,{7619dcc8-fafe-11d9-b411-000476eba25f}
path      \windows\system32\boot\winload.efi   (winload.exe in the BIOS store)
systemroot \windows
detecthal yes
winpe     yes
```

- At deployment time, `ZTIBCDUtility.vbs` creates `{ramdiskoptions}` with `ramdisksdidevice partition=<drive>` and `ramdisksdipath \Boot\boot.sdi`, plus an OS loader entry with the same elements, to stage WinPE on a local disk (`ZTIBCDUtility.vbs:93-153`).

## OS and application import

- MDT reads image metadata through WIMGAPI (`WIMGetImageCount`, `WIMGetImageInformation`), not DISM. It stores the edition ID in a field named `Flags`, the build as `MAJOR.MINOR.BUILD.SPBUILD`, and the size in MB. An ARM64 image becomes `Unknown` (`Core/BDDWimImage.cs:27-348`, `PSSnapIn/Verifier.cs:219-257`).
- A full-source import requires `Sources\Install.wim` and copies the whole ISO tree. MDT has no import path for `.esd` files (`PSSnapIn/ImportOperatingSystem.cs:227-327`).
- Import writes to the imported files. It rewrites `lang.ini`, always with `en-us = 3`, and names unnamed images "Image N" inside the WIM (`PSSnapIn/Verifier.cs:282-346`).
- Only the first `*.wim` in a folder is registered (`PSSnapIn/Verifier.cs:179-211`).
- An application has a GUID, `hide` and `enable` flags, `Name`, `ShortName`, `Version`, `Publisher`, `Language`, `Source`, `CommandLine`, `WorkingDirectory`, `SupportedPlatform` entries, `UninstallKey`, ordered `Dependency` GUIDs, and `Reboot` (`Templates/Applications.xsd:7-29`). A bundle is an application with no command line. `ZTIApplications.wsf` installs its dependencies and skips the empty command (`ZTIApplications.wsf:437-438`).
- The dependency editor rejects only self-references, so indirect cycles are allowed (`Workbench/ApplicationDependencies.cs:127-147`).

## Task sequence editor model

- The editor's step palette is the data file `Bin\Actions.xml`. It defines groups and 24 step types (`Bin/Actions.xml`).
- A step property becomes a `<defaultVarList>` variable. The exceptions are SuccessCodes, SupportedEnvironment, CommandLine, and WorkingDirectory, which become `successCodeList` (default `0 3010`), `runIn`, `<action>`, and `startIn` (`Workbench/TaskSequenceEditor.cs:1701-1770`).
- The editor gives each node a GUID in memory and strips it before saving. Saved steps have only a name, and names need not be unique (`Workbench/TaskSequenceEditor.cs:339-352`).
- Every step added in the editor gets `runIn="WinPEandFullOS"`, whatever it does (`Workbench/TaskSequenceEditor.cs:1713-1722`).
- Run Command Line can run as another user. The editor stores that user's name and password in plain text as `SMSTSRunCommandLineUserName` and `SMSTSRunCommandLineUserPassword` (`Workbench/TaskSequenceEditor.cs:317-331`).
- Install Application has two modes, chosen by whether `ApplicationGUID` is empty. A GUID installs one application. An empty GUID installs the `MandatoryApplications` and `Applications` lists (`Actions/ActionInstallApplication.cs:45-111`).
- The condition editor writes these types and operators (`Workbench/TaskSequenceUtility.cs:43-96`):

| Condition | Parameters | Operators |
| --- | --- | --- |
| Variable | Variable, Operator, Value | exists, equals, notEquals, greater, greaterEqual, less, lessEqual |
| Registry | KeyPath, Value, Type (REG_SZ and others), Operator, Data | the same set |
| WMI | Namespace, Query | true if the query returns rows |
| File | Path, optional Version with VersionOperator, optional DateTime with DateTimeOperator | equals, notEquals, greater, greaterEqual, less, lessEqual |
| Folder | Path, optional DateTime with DateTimeOperator | the same set |
| Installed software | ProductCode, UpgradeCode, ProductName, Version, Operator | ThisVersion, AnyVersion |
| OS version | Version, Architecture | from a fixed list in `Bin\OSList.xml` |
| If statement | `<operator type="and\|or\|not">` | all, any, none |

- The editor maps operators only from lower-case names. Opening a template condition stored as `Equals` or `notExists` and clicking OK saves it as `exists` with an empty value (`Workbench/TaskSequenceVariable.cs:35-49`).
- File and folder dates are stored in UTC as `yyyyMMddHHmmss.000000+***` (`Workbench/TaskSequenceFilePropertiesCondition.cs:142-146, 310`).
- The editor checks only required fields per step. It does not check structure, references, or `runIn` (`Workbench/TaskSequenceEditor.cs:265-293`).

## Monitoring and the PowerShell host

### Monitoring service

- `MDT_Monitor` is a Windows service with two WCF hosts. Port 9800 serves `GET /MDTMonitorEvent/PostEvent` and `GetSettings` with query strings. Port 9801 serves an OData feed, `/MDTMonitorData/` (`MonitorService/MonitorWindowsService.cs:44-57`).
- Neither port requires authentication, and the OData feed grants all rights on every entity set (`MonitorService/MonitorDataService.cs:7-12`).
- `ZTIUtility.vbs` sends each event as an unencoded query string. It carries `uniqueID`, `computerName`, `messageID`, `severity`, `stepName`, `currentStep`, `totalSteps`, an `id` list of the UUID and every MAC address, `message`, and DaRT and VM details (`ZTIUtility.vbs:278-285, 399-416`).
- The service matches a computer by the GUID it issued, then by any one UUID or MAC address. It keeps one current-state row per computer and no event history (`MonitorService/MonitorEventService.cs:27-118`).
- Status is 1 Running, 2 Failed, 3 Completed, or 4 Unresponsive. Event 41014 sets Failed, 41015 sets Completed, and any other event sets Running again, even after Completed. An hourly job marks a computer Unresponsive after four hours without an event and deletes records after three days (`MonitorService/Maintenance.cs:16-101`).
- Heartbeat events come only from some script helpers. A PowerShell step sends no events at all, so a PowerShell step longer than four hours shows as Unresponsive (`ZTIPSUtility.vbs:102-108`).
- Each event is a blocking HTTP call with 60-second timeouts and certificate errors ignored. Failures are silently dropped (`ZTIUtility.vbs:268`, `ZTIDataAccess.vbs:855-860`).

### PowerShell steps

- A Run PowerShell Script step runs `ZTIPowerShell.wsf`. That script starts MDT's own PowerShell host, `Microsoft.BDD.TaskSequencePSHost40.exe` (`ZTIPSUtility.vbs:30-108`).
- The host sets the execution policy to Bypass for its process and imports the task sequence module. It then rebuilds the script's arguments into one string, losing their original quoting, and runs that string as PowerShell source. The step's exit code is `$LASTEXITCODE` (`TaskSequencePSHost40/ScriptProcessor.cs:30-128`).
- The module exposes two PowerShell drives. `TSEnv:` reads and writes single variables. `TSEnvList:` reads and writes list variables named `Base001` to `Base999`. If the engine's COM object is missing, writes go to an in-memory dictionary and are silently lost (`TaskSequenceModule/TSEnvironment.cs:81-119`).
- `Write-Progress` goes to the progress dialog through COM. Host output goes to `BDD.LOG` without the password masking that the scripts apply. `Read-Host` and `Get-Credential` throw (`TaskSequencePSHost40/TaskSequencePSHostUserInterface.cs:53-108`).

## What changes in deployarr

| Finding | deployarr decision |
| --- | --- |
| MDT flattens every INF ID into one list and loses its kind, architecture, and OS build | `deployarr-inf` stores each ID with its kind (hardware or compatible), architecture, and minimum OS build, taken from the decoration grammar |
| MDT's INF parsing needs SetupAPI and the server's class registry | `deployarr-inf` implements `%strings%` substitution and line rules itself and ships a table of standard class GUIDs |
| One bad INF stops an MDT import | An INF that fails to parse is recorded as rejected with a reason, and the batch continues |
| Nested INF folders are stored more than once | The package is the INF folder tree, and import removes nested duplicates |
| WinPE optional components need DISM servicing | A Linux server cannot add WinPE PowerShell. See [Boot image](../architecture.md#boot-image) |
| MDT keeps patching a serviced boot image and records failed items as present | Rebuild the boot image from the untouched base every time, keyed by a hash of the recipe, and fail the build when any step fails |
| `efisys.bin` waits for a key press | The deployarr boot ISO uses the no-prompt EFI boot image |
| Saved MDT steps have no ID | deployarr's stable `NodeId` is new, and an MDT importer must create IDs |
| MDT conditions include file version, file and folder dates, installed MSI products, and a WMI failure flag | deployarr's `Condition` gets these types and the full operator set. Make, model, location, and computer checks become variable conditions on gathered facts, with exact matching instead of MDT's substring tests |
| A condition that fails to evaluate fails the step in MDT | deployarr's evaluator returns an error, and the error goes through the step's `on_error` |
| MDT writes its state file in place and ignores a failed save | The agent writes a temporary file, flushes it, and renames it. A failed save stops the run before the next step or reboot |
| MDT saves nothing before a step, so an interrupted step runs again | The agent marks a step in flight before it starts. A typed action converges when run again, and a command fails unless its step allows a re-run |
| A retry with a reboot loses the step's variable changes | The agent saves state before every reboot |
| MDT marks a pending reboot with a volatile registry key | The agent does the same and refuses to resume while the marker exists |
| An empty `successCodeList` fails every exit code | The compiler rejects an empty success list |
| Exit code 3010 never reboots in MDT | deployarr reboots on 3010 and 1641 on purpose and documents the difference |
| A `runIn` mismatch fails at run time | A step placed where its action cannot run is a compile error |
| The engine ignores `startIn` | deployarr honors each command's working directory |
| `%VAR%` expansion recurses, scans inserted text again, and can loop forever | `Template` substitutes once, from left to right |
| Scripts reach variables through shared memory with no access control, and `Clear()` erases engine state | Steps reach variables through an agent channel that keeps names starting with `_` read-only and has no bulk clear |
| The engine resumes from the state file with the newest access time on any drive | The agent keeps state at a fixed path and checks the deployment ID in it |
| PnpEnum cannot tell a failed enumeration from a PC with no devices | The agent's device scan returns an error when enumeration fails |
| `IsUEFI` in 8450 and in `TsCore.dll` misreports BIOS PCs | The agent calls `GetFirmwareType` |
| Install Application overloads an empty GUID | deployarr's `AppSelection` has explicit variants for one application and for a list |
| Every editor step gets `runIn="WinPEandFullOS"` | Each deployarr action type fixes where it can run |
| Monitoring has no history, reopens finished records, and has no authentication | deployarr keeps per-step events, makes Succeeded and Failed final, and authenticates the agent API |
| Events block steps, and PowerShell steps send no heartbeat | The agent queues events, sends them in the background with retry, and sends a heartbeat on a timer |
| The PowerShell host re-parses arguments as source and skips password masking | `RunPowerShell` passes arguments as an array and redacts secrets on every captured stream |
