# MDT 8450 binaries reference

This page records what the compiled parts of Microsoft Deployment Toolkit (MDT) do. These parts are the task sequence engine, the deployment helper tools, and the .NET code behind Deployment Workbench. The scripts and templates are covered in [MDT 8456 reference](mdt-8456.md). Use this page when you build the deployarr feature that replaces one of these components.

## Sources

- The installer is `MicrosoftDeploymentToolkit_x64.msi`, product version 6.3.8450.1000, from `https://download.microsoft.com/download/0/7/b/07b3b260-f008-4b1c-8eaa-74bcc5c7d646/MicrosoftDeploymentToolkit_x64.msi`. SHA-256 is `e73823db601b06e9c222ae816bbddd491ecb823a1f8cce0f73eec9de51c1690e`. That link still worked on 2026-11-11, after the 8456 link stopped working.
- `msiextract` from msitools unpacked the installer.
- ILSpy 8.2.0.7535 (`ilspycmd -p`) decompiled the .NET assemblies to C#, including `Microsoft.BDD.TaskSequencePSHost40.exe`. Citations such as `PSSnapIn/ImportDriver.cs:125` point into that output. The full path is `Microsoft.BDD.PSSnapIn/Microsoft.BDD.PSSnapIn/ImportDriver.cs`. Another ILSpy version may number lines differently.
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

The analysis of `TsCore.dll`, `TsManager.exe`, and `TsmBootstrap.exe` is still running. This section will hold its verified results.

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
| MDT conditions include file version, file and folder dates, installed MSI products, and OS version | deployarr's `Condition` gets the same types and the full operator set |
| Install Application overloads an empty GUID | deployarr's `AppSelection` has explicit variants for one application and for a list |
| Every editor step gets `runIn="WinPEandFullOS"` | Each deployarr action type fixes where it can run |
| Monitoring has no history, reopens finished records, and has no authentication | deployarr keeps per-step events, makes Succeeded and Failed final, and authenticates the agent API |
| Events block steps, and PowerShell steps send no heartbeat | The agent queues events, sends them in the background with retry, and sends a heartbeat on a timer |
| The PowerShell host re-parses arguments as source and skips password masking | `RunPowerShell` passes arguments as an array and redacts secrets on every captured stream |
