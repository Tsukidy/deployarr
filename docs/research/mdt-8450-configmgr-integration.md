# MDT 8450 ConfigMgr integration reference

This page records how Microsoft Deployment Toolkit (MDT) 8450 adds its own task sequence steps to Configuration Manager (ConfigMgr). It covers the WMI class for each step, the console extension that edits it, the command line the step runs on the PC, how the step's files reach the PC, and what breaks when the extension is gone. MDT's integration is prior art for a third party that adds step types to a deployment engine it does not own. deployarr uses it to design add-ons that contribute steps, settings pages, and server code without breaking stored task sequences.

## Sources

- The installer is `MicrosoftDeploymentToolkit_x64.msi`, product version 6.3.8450.1000, from `https://download.microsoft.com/download/0/7/b/07b3b260-f008-4b1c-8eaa-74bcc5c7d646/MicrosoftDeploymentToolkit_x64.msi`. It is 20,529,152 bytes, and its SHA-256 is `e73823db601b06e9c222ae816bbddd491ecb823a1f8cce0f73eec9de51c1690e`, which matches [`tools/re/README.md`](../../tools/re/README.md#sources). Its summary information gives a creation date of 2017-12-01. It was downloaded on 2026-11-12.
- MDT 8456 is the last release, but its installer link returns HTTP 404 ([MDT 8450 binaries reference](mdt-8450-binaries.md#sources)). 8450 is therefore the pinned version. The 8456 hotfix replaces only `Microsoft.BDD.Utility.dll`, which plays no part in the integration.
- `msiextract` 0.103 unpacked the MSI. Paths such as `SCCM/Microsoft.BDD.CM12Actions.mof` are relative to the extracted `Microsoft Deployment Toolkit` folder. Line numbers count lines after CRLF line endings are stripped.
- `ilspycmd` 8.2.0.7535 decompiled these .NET assemblies from `Bin`. Citations such as `Microsoft.BDD.Wizards!SCCM_ConfigureRemove.CompileActionsMOF` name the type and method.

| Assembly | Target | SHA-256 |
| --- | --- | --- |
| `Microsoft.BDD.CM12Actions.dll` | .NET 4.0, ConfigMgr 2012 and later | `380fefafd7b286a3f921295789261aa167177d93a92cb5b1ff809ad02c8d0885` |
| `Microsoft.BDD.SCCMActions.dll` | .NET 3.5, ConfigMgr 2007 | `70089023dae1713a234ff9769054538c54ff480cf35a6c6a240e7bb4044c7efa` |
| `Microsoft.BDD.CM12Wizards.dll` | .NET 4.0 | `97e3ad00ef09f1c66b3041bcff1f79699d703dfec72ff783c740ac1c56fdde8d` |
| `Microsoft.BDD.Wizards.dll` | .NET 3.5 | `109537ca6ece256b2631925792dd090e0c2edbab2f7f4ec1e5883efc3f7b2e26` |

- The ConfigMgr SDK pages on learn.microsoft.com describe custom actions and console extensions. They show an `updated_at` date of 2026-08-28 and were fetched on 2026-11-12:
  - [About Configuration Manager custom actions](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-actions)
  - [About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files)
  - [How to create a MOF file for a custom action](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-mof-file-for-a-configuration-manager-custom-action)
  - [How to create a custom action control](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-configuration-manager-custom-action-control)
  - [About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)
  - [How to use a custom action control](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-use-a-configuration-manager-custom-action-control)
  - [Console extension deployment](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/console-extension-deployment)
  - [About console extensions](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/about-configuration-manager-console-extension)
- The WMI SDK page [pragma classflags](https://learn.microsoft.com/en-us/windows/win32/wmisdk/pragma-classflags) shows an `updated_at` date of 2023-06-09 and was fetched on 2026-11-12.
- The target framework column comes from each assembly's CLR metadata version, `v2.0.50727` for .NET 3.5 and `v4.0.30319` for .NET 4.0.
- The ConfigMgr and MDT product pages were fetched on 2026-11-12:
  - [MDT release notes](https://learn.microsoft.com/en-us/intune/configmgr/mdt/release-notes) and [MDT known issues](https://learn.microsoft.com/en-us/intune/configmgr/mdt/known-issues), both updated 2026-08-31
  - [Removed and deprecated features](https://learn.microsoft.com/en-us/intune/configmgr/core/plan-design/changes/deprecated/removed-and-deprecated-cmfeatures), updated 2026-10-13
  - [Manage console extensions](https://learn.microsoft.com/en-us/intune/configmgr/core/servers/manage/admin-console-extensions) and [Import console extensions](https://learn.microsoft.com/en-us/intune/configmgr/core/servers/manage/import-admin-console-extensions), both updated 2026-08-31
- The source markdown in `MicrosoftDocs/memdocs` could not be cloned or fetched from raw.githubusercontent.com, so the pages carry no commit hash.
- Claims marked "inferred" come from names, strings, or partial code.

## What an extension has to supply

One MDT step needs four parts in four places. Each part names the others by string.

| Part | What it is | Where it lands | Who installs it |
| --- | --- | --- | --- |
| Step schema | A WMI class that derives from `SMS_TaskSequence_Action` | The site's SMS Provider namespace `root\sms\site_<code>` | `mofcomp.exe`, run by the Configure ConfigMgr Integration wizard |
| Step editor | A .NET WinForms control in `Microsoft.BDD.CM12Actions.dll` | `AdminConsole\Bin` on each console computer | The same wizard, as a file copy |
| Console menu items | Action and form XML | `AdminConsole\XmlStorage\Extensions` on each console computer | The same wizard, as a file copy |
| Step code | VBScript files plus native tools | A ConfigMgr package, the "MDT toolkit package" | The Create MDT Task Sequence wizard, when the admin asks it for a new package |

Sources: `SCCM/Microsoft.BDD.CM12Actions.mof:1-15`, `Microsoft.BDD.Wizards!SCCM_ConfigureRemove.ExtendCM12Console`, `Microsoft.BDD.Wizards!SCCM_ConfigureRemove.CompileActionsMOF`, `Microsoft.BDD.CM12Wizards!SCCM_ImportTaskSequenceTask` (toolkit package creation). The ConfigMgr SDK documents the same split into a MOF class, a control assembly copied next to `AdminUI.TaskSequenceEditor.dll`, and a client application in a package ([About custom actions](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-actions), [About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)).

## Package format and manifest

### The step class

Each MDT step is a MOF class. The class qualifiers are the manifest. `BDD_UsePackage` is typical (`SCCM/Microsoft.BDD.CM12Actions.mof:3-15`):

- `CommandLine("smsswd.exe /run:%1 cscript.exe Scripts\\ZTISCCM.wsf")` gives the command the PC runs. `%1` is filled from the property with `CommandLineArg(1)`.
- `ActionCategory("MDT,1,7")` gives the editor's Add menu category, the order in the category, and the category's order.
- `ActionUI{"Microsoft.BDD.CM12Actions.dll", "Microsoft.BDD.Actions", "ActionBDDPackage", "ActionOptions"}` names the assembly, the namespace, the property page class, and the options page class.
- `ActionName{...}` names the assembly and resource class that hold the step's display name.
- `VariablePrefix("BDD")` prefixes the variable names of the class's properties.
- Each property is a step setting. `[TaskSequencePackage, CommandLineArg(1)] string PackageID` marks a package reference that goes on the command line.
- `SupportedEnvironment` takes `WinPE`, `FullOS`, or `WinPEandFullOS` ([About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)). MDT uses all three (`SCCM/Microsoft.BDD.CM12Actions.mof:14, 202, 215`). It becomes the step's `runIn` attribute (inferred from `SCCM/SCCM_Client.xml:281`, where a `BDD_UsePackage` step carries `runIn="WinPEandFullOS"`, the class default).

The SDK lists the other property qualifiers: `Not_Null`, `ValueMap`, `ValueRange`, `RequiredIfNull`, `VariableName`, `AllowedLen`, and `SuccessCodes` ([About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files)). MDT's 13 step classes use only defaults, `TaskSequencePackage`, and `CommandLineArg` on their properties (`SCCM/Microsoft.BDD.CM12Actions.mof:1-255`). Nested settings use three embedded classes that do not derive from `SMS_TaskSequence_Action`: `BDD_DNSZones`, `BDD_DHCPScopes`, and `BDD_RunbookParameters` (`SCCM/Microsoft.BDD.CM12Actions.mof:111-120, 142-159, 250-255`).

### Version and compatibility fields

- The MOF has no version field and no minimum host version. The class name is the only identity.
- The MOF's first line is `#pragma namespace("\\\\%SMSSERVER%\\root\\sms\\site_%SMSSITECODE%")`. The wizard replaces both tokens with the provider server and site code before it compiles (`SCCM/Microsoft.BDD.CM12Actions.mof:1`, `Microsoft.BDD.Wizards!SCCM_ConfigureRemove.CompileActionsMOF`).
- The wizard picks the MOF by the site's major version. A site with `SMS_Site.Version` major 5 or later gets `Microsoft.BDD.CM12Actions.mof`, and an older site gets `Microsoft.BDD.SCCMActions.mof` (`Microsoft.BDD.Wizards!SCCM_ConfigureRemove.CompileActionsMOF`, `ValidateConfigMgr`).
- The two MOF files differ only in the assembly name inside `ActionUI` and `ActionName`. Every class, property, and command line is the same (`SCCM/Microsoft.BDD.CM12Actions.mof`, `SCCM/Microsoft.BDD.SCCMActions.mof`).
- The editor assembly ships twice, once per console generation. `Microsoft.BDD.SCCMActions.dll` targets .NET 3.5 and `Microsoft.BDD.CM12Actions.dll` targets .NET 4.0. Both decompile to the same 35 source files in the `Microsoft.BDD.Actions` namespace. The code differs where the console API changed, such as `ParentNode.NodeID` in ConfigMgr 2007 and `ParentNode.NodeId` in 2012 (`Microsoft.BDD.SCCMActions!TaskSequenceActionControl.SetDirtyFlag`, `Microsoft.BDD.CM12Actions!TaskSequenceActionControl.SetDirtyFlag`).
- The console XML also ships twice. `Templates/CM12Extensions` adds `MergeGroupMembership` and `ShowOn` elements for the 2012 ribbon, which `Templates/Extensions` lacks (`Templates/CM12Extensions/Actions/b0b9625f-7afe-409e-9b65-5cb973a82a2c/Microsoft.BDD.SCCM_CreateBootImage.xml:1-10`).
- The task sequence template wizard adapts to the site version by deleting steps. On a site whose version string sorts at or below `5.00.7711.0000` it removes every `SMS_TaskSequence_OfflineEnableBitLockerAction` step and the `Format and Partition Disk (UEFI)` step. On a site with major version 4 it removes every `SMS_TaskSequence_InstallApplicationAction` step (`Microsoft.BDD.CM12Wizards!SCCM_ImportTaskSequenceTask`, just before the `LoadFromXml` call).

### Per-architecture files

The step code is not per architecture at the package level. The toolkit package source is a copy of `Templates/Distribution`, which holds `Scripts` and both `Tools/x86` and `Tools/x64` (`Microsoft.BDD.CM12Wizards!SCCM_ImportTaskSequenceTask`, the `DirectoryUtility` copy of `Templates\Distribution\`). `ZTISCCM.wsf` picks the folder at run time and sets `ToolRoot` to `DeployRoot\Tools\<arch>` (`Templates/Distribution/Scripts/ZTISCCM.wsf:133-153`).

## Install, upgrade, and uninstall

### The UX

- Installing the MSI does not touch ConfigMgr. The MSI's `CustomAction` table holds four actions, `WixUIPrintEula`, `LaunchUrl`, `BackupConfiguration`, and `RestoreConfiguration`, and none of them concerns ConfigMgr (`msiinfo export MicrosoftDeploymentToolkit_x64.msi CustomAction`).
- The admin runs a separate Start menu shortcut, Configure ConfigMgr Integration. It starts `Microsoft.BDD.WizLauncher.exe ConfigureSCCM`. That wizard has an options pane, `SCCM_ConfigureRemove`, and a confirmation pane (MSI `Shortcut` table, `Bin/Microsoft.BDD.Wizards.xml:152-155`).
- The pane has an Install or Remove choice, check boxes for the console files and the MOF, and text boxes for the site server and site code. It fills the server and site code from the console's most recently used site in `HKCU\Software\Microsoft\ConfigMgr10\AdminUI\MRU\1` (`Microsoft.BDD.Wizards!SCCM_ConfigureRemove.LoadData`).
- It finds the console folder from `HKLM\Software\Microsoft\ConfigMgr10\Setup\UI Installation Directory`. It treats MDT as installed when `Bin\Microsoft.BDD.CM12Actions.dll` exists there (`Microsoft.BDD.Wizards!SCCM_ConfigureRemove.LoadData`).

### Install steps and their order

`SCCM_ConfigureRemove.ApplyChanges` runs these steps in order:

1. Copy six assemblies into `AdminConsole\Bin` with overwrite: `CM12Actions`, `Workbench`, `ConfigManager`, `CM12Wizards`, `PSSnapIn`, and `Core` (`ExtendCM12Console`).
2. Copy `Templates/CM12Extensions` into `AdminConsole\XmlStorage\Extensions`, recursive, with replace (`ExtendCM12Console`).
3. Connect to `\\<server>\root\sms`, find the provider through `SMS_ProviderLocation`, and read `SMS_Site.Version` (`ValidateConfigMgr`, called from `CompileActionsMOF`).
4. Write the MOF with the tokens replaced to `AdminConsole\Bin\Microsoft.BDD.CM12Actions.mof`, then run `mofcomp.exe` on it (`CompileActionsMOF`).

### Failure handling

- `ExtendCM12Console` wraps the assembly copies in one handler and the XML copy in another. Each handler appends the exception to an output log and adds one to an error count, and the MOF step still runs (`ExtendCM12Console`, `ApplyChanges`).
- `ValidateConfigMgr` logs a connection or query failure, counts it, and skips the MOF step. A nonzero exit code from `mofcomp.exe` adds one to the error count. Any other exception in `CompileActionsMOF` reaches the handler in `ApplyChanges`, which logs it as unhandled and counts it (`ValidateConfigMgr`, `CompileActionsMOF`, `ApplyChanges`).
- The assembly copy fails if the console is running. Its error text says "Make sure the ConfigMgr console is not running" (`ExtendCM12Console`).
- Nothing is staged or rolled back. The six copies run one after another in one handler, so a failure partway leaves the earlier assemblies at the new version and the later ones at the old (`ExtendCM12Console`). A failed `mofcomp` leaves the console files in place with no matching classes (inferred from `ApplyChanges`).
- `ValidateConfigMgr` builds its WQL by pasting the typed site code into the query text, as in `"SELECT * FROM SMS_ProviderLocation WHERE SiteCode = '" + siteCode + "'"`. Its `SMS_Site` query does the same.

### Upgrade

- MDT ships no upgrade path for the integration. After an MDT upgrade, Microsoft says to "Run the Configure ConfigMgr Integration Wizard after the upgrade" ([MDT release notes](https://learn.microsoft.com/en-us/intune/configmgr/mdt/release-notes)).
- Stored task sequences do not upgrade. "No mechanism is provided to upgrade these task sequences." Admins create a new MDT toolkit package for new task sequences and keep the old package for old ones ([MDT release notes](https://learn.microsoft.com/en-us/intune/configmgr/mdt/release-notes)).
- Until the wizard runs again, each console keeps the assemblies from the last run, because the MSI does not touch them (inferred from the `CustomAction` table and `ExtendCM12Console`).

### Uninstall

- Remove deletes five of the six assemblies. It leaves `Microsoft.BDD.PSSnapIn.dll`, which install copied, and it leaves the written `.mof` file (`RemoveCM12ConsoleExtensions`).
- It deletes the four XML files by hard-coded path (`RemoveCM12ConsoleExtensions`).
- It deletes every subclass of `SMS_TaskSequence_Action` whose name starts with `BDD_` (`RemoveActions`). It does not check whether any task sequence still uses those classes.
- The SDK's own removal advice for a custom action's XML is to delete the file, and to remove the folder only when it holds no other XML files ([Console extension deployment](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/console-extension-deployment)).

## Runtime model

### On the PC

- The step runs as a separate process. The engine runs the step's stored command line, which is `smsswd.exe /run:<PackageID> cscript.exe <script>`. `smsswd.exe` resolves the package, gets its files, and starts the program in it ([About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)).
- The stored step carries the resolved command line in `<action>` and the settings in `<defaultVarList>`. For example, the template's Use Toolkit Package step stores `smsswd.exe /run:CAS0001E cscript.exe Scripts\ZTISCCM.wsf` and a hidden `BDDPackageID` variable (`SCCM/SCCM_Client.xml:281-286`).
- The engine reads a step's `type` but runs its `<action>`. It applies `defaultVarList` before the condition and clears it when the step ends ([MDT 8450 binaries reference](mdt-8450-binaries.md#instruction-table-and-failure-handling), [variables](mdt-8450-binaries.md#variables)). The PC therefore needs no MOF class and no editor assembly (inferred).
- Settings reach the program as task sequence variables. The SDK says every property except those with `CommandLineArg` becomes a variable ([About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files)). The program reads them through the `TSEnvironment` COM object. It reports progress through `ProgressUI`, signals success with exit code 0, and asks for a restart by setting `SMSTSRebootRequested` ([About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)).
- The program must run unattended. The SDK says it "shouldn't prompt for user input" (same page).
- MDT's other steps depend on an earlier step. The 12 classes other than `BDD_UsePackage` run a script under `%DeployRoot%\Scripts`, and seven of them pass no package to `smsswd.exe /run:` (`SCCM/Microsoft.BDD.CM12Actions.mof:18, 33, 48, 63, 79, 123, 162, 180, 193, 206, 218, 232`). Of the 13 classes, only Use Toolkit Package runs `ZTISCCM.wsf`, which sets `DeployRoot` (`Templates/Distribution/Scripts/ZTISCCM.wsf:146-179`). A task sequence that leaves it out, or runs these steps before it, has no `DeployRoot` (inferred).
- When `DeployRoot` is a local path or `_SMSTSMDataPath` is set, `ZTISCCM.wsf` copies the whole package to `_SMSTSMDataPath\WDPackage` and points `DeployRoot` there. It skips the copy when the folder already exists (`Templates/Distribution/Scripts/ZTISCCM.wsf:159-179`).
- When `SysWOW64\CCM\TSCore.DLL` exists, `ZTISCCM.wsf` first writes that path as the `win64` entry of type library `{D5E1749D-832D-4587-AFC3-9462187FE2F5}`. Its comment says this repairs a bad registry key in x64 images captured by MDT 2010 Lite Touch, which would otherwise put the script into an infinite loop (`Templates/Distribution/Scripts/ZTISCCM.wsf:4-16`).
- Steps that must run before the network or disk exist cannot use a package. The SDK says to add them to a custom boot image instead ([About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)).

### In the console

- `ActionUI` names the assembly and classes the editor uses, and the assembly must sit in the same folder as `AdminUI.TaskSequenceEditor.dll` ([About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files), [About custom actions](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-actions)). The console therefore loads the editor into its own process (inferred).
- The SDK says to close and reopen the console after the MOF or the assembly changes ([How to use a custom action control](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-use-a-configuration-manager-custom-action-control)).
- No source describes crash isolation, health checks, or resource limits for console extensions or custom actions.

## Host-to-add-on API

### The step contract

- The contract is WMI class inheritance plus qualifiers. A custom step derives from `SMS_TaskSequence_Action`. The class is compiled into the provider namespace with `mofcomp.exe` ([How to create a MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-mof-file-for-a-configuration-manager-custom-action)).
- The SDK's sample MOF uses `#pragma classflags("forceupdate")` (same page). That flag forces a class update when conflicting child classes exist, and the update fails if a child class has instances ([pragma classflags](https://learn.microsoft.com/en-us/windows/win32/wmisdk/pragma-classflags)). MDT's MOF has no `classflags` pragma. Its only pragma is the namespace (`SCCM/Microsoft.BDD.CM12Actions.mof:1`).
- The SDK says to keep the property names and limits "synchronized between the MOF file, custom action control and client application". The same rule lives in three places, and nothing checks that they agree ([About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files)).
- The SDK says not to reuse a category that another action already uses (same page). MDT uses its own category, `MDT` (`SCCM/Microsoft.BDD.CM12Actions.mof:4`).

### The editor contract

- The editor is a .NET class that derives from `SmsOsdEditorPageControl`, with an options page that derives from `TaskSequenceOptionControl` ([How to create a custom action control](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-configuration-manager-custom-action-control)).
- It reads and writes settings through `PropertyManager["<property>"]`, keyed by the MOF property name (same page). MDT wraps it in `TaskSequencePropertyManager`, which fills a null value with an empty string, a zero, or false by the WMI data type, and converts embedded object arrays to dictionaries (`Microsoft.BDD.CM12Actions!TaskSequencePropertyManager`).
- MDT's base control reads `Name` and `Description` and marks the node invalid in the editor's `AllInvalidReferenceNodes` list when validation fails (`Microsoft.BDD.CM12Actions!TaskSequenceActionControl`).
- The assembly compiles against the console's own assemblies, such as `AdminUI.TaskSequenceEditor`, `AdminUI.OsdCommon`, and `Microsoft.ConfigurationManagement.ManagementProvider` (decompiled `Microsoft.BDD.CM12Actions.csproj` references).

### Versioning

- There is no version negotiation. The console loads the named assembly and class, and the build either matches the console's types or does not (inferred).
- The console API broke at least once between ConfigMgr 2007 and 2012. MDT answered with a second assembly and a second MOF, not with a shim (see [Version and compatibility fields](#version-and-compatibility-fields)).
- No source states a deprecation policy for custom actions. The SDK pages still name Visual Studio 2005 and 2010 ([How to create a custom action control](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-configuration-manager-custom-action-control)).

## Isolation and permissions

- The editor runs with the console user's rights inside the console process. It queries the provider directly. For example, `ActionBDDPackage` looks up `SMS_Package.PackageID='<id>'` through the console's `ConnectionManager` (`Microsoft.BDD.CM12Actions!ActionBDDPackage.ActionBDDPackage_Load`).
- Console assemblies share one folder. The SDK warns that placing assemblies "in the ... AdminConsole\bin folder may create conflicts with other console extensions and prevent your extension from loading" ([Console extension deployment](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/console-extension-deployment)).
- On the PC, the step program runs with the engine's rights and can read and write every task sequence variable through `TSEnvironment` ([About custom action client applications](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-client-applications)).
- Secrets are ordinary string properties. `BDD_ConfigureADDS` declares `ADDSPassword` and `SafeModeAdminPassword` as plain `string` with no qualifier (`SCCM/Microsoft.BDD.CM12Actions.mof:98-105`). The editor writes the recovery password text box and the password from its account dialog straight into those properties (`Microsoft.BDD.CM12Actions!ActionConfigureADDS`). Whether the provider encrypts these values at rest is not covered by these sources.

## UI contributions

- Every UI part is shipped code. The step editor is a WinForms control. The two console menu items, Create MDT Task Sequence and Create Boot Image using MDT, are `ShowDialog` actions whose form XML names `WizardDispatch` in `Microsoft.BDD.CM12Actions.dll` (`Templates/CM12Extensions/Actions/*/Microsoft.BDD.SCCM_*.xml`, `Templates/CM12Extensions/Forms/Microsoft.BDD.SCCM_CreateBootImage.xml:1-6`).
- Action XML goes in `XmlStorage\Extensions\Actions\<GUID>`, where the GUID names the console node the action attaches to ([Console extension deployment](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/console-extension-deployment)).
- The only declarative part is the MOF. Its `ActionCategory` and `ActionName` place the step in the Add menu, and its qualifiers state the constraints. The console does not build a form from them. The control has to repeat each constraint itself ([About the custom action MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-action-mof-files)).
- The SDK says an extension cannot hook F1 help, cannot create wizards with the console framework, and must not edit the core console XML ([About custom actions](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/about-configuration-manager-custom-actions), [About console extensions](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/about-configuration-manager-console-extension)).

## Trust

- Each of the eight assemblies in this page's table or in the wizard's copy list carries a PE certificate table of 7,568 bytes that contains the string `Microsoft Corporation` (read from each file's security data directory). This page did not verify the chain.
- The old-style extension mechanism that MDT uses has no documented verification step. The documented install is a file copy into `AdminConsole\bin` and `XmlStorage\Extensions` ([Console extension deployment](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/console-extension-deployment)).
- ConfigMgr 2103 added hierarchy-approved console extensions. An admin imports a `.cab` through the administration service, approves it for the site, and enables notifications. Then console users install it, and the console restarts. Revoking approval uninstalls the extension from each console on its next start ([Manage console extensions](https://learn.microsoft.com/en-us/intune/configmgr/core/servers/manage/admin-console-extensions)).
- Version 2103 imports only signed extensions. Version 2107 adds a hierarchy setting to allow unsigned ones. Version 2111 adds an import wizard and a Require Extension action, and the site writes status messages 54201 to 54208 with the user, extension ID, and extension version ([Import console extensions](https://learn.microsoft.com/en-us/intune/configmgr/core/servers/manage/import-admin-console-extensions), [Manage console extensions](https://learn.microsoft.com/en-us/intune/configmgr/core/servers/manage/admin-console-extensions)).
- A hierarchy setting allows only approved extensions. It is on by default for sites installed from the 2103 baseline. With it on, "old style extensions that aren't approved ... will no longer be able to be used" (same page).
- The MOF class has no trust story. Anyone who can run `mofcomp.exe` against the provider namespace can add a step class (inferred from [How to create a MOF file](https://learn.microsoft.com/en-us/intune/configmgr/develop/osd/how-to-create-a-mof-file-for-a-configuration-manager-custom-action)).

## Known failures and lessons

- **The approval setting blocked MDT.** On a new site at version 2103 or 2107, the integration wizard does not add the MDT extensions. The fix is to turn off the approved-extensions setting ([MDT known issues](https://learn.microsoft.com/en-us/intune/configmgr/mdt/known-issues)). The host tightened its extension model under an installer that only copies files and compiles a MOF, so the wizard has no check that would see the setting (inferred from `SCCM_ConfigureRemove.ApplyChanges`).
- **Removing the extension before its steps corrupts task sequences.** MDT is retired, and Microsoft says customers "should remove all MDT task sequence steps and then remove MDT integration to prevent task sequence corruption and modification failures" ([MDT release notes](https://learn.microsoft.com/en-us/intune/configmgr/mdt/release-notes)). Support ended in "the first release after Oct 10, 2025" ([Removed and deprecated features](https://learn.microsoft.com/en-us/intune/configmgr/core/plan-design/changes/deprecated/removed-and-deprecated-cmfeatures)).
- **Copied task sequences break where the extension is missing.** ConfigMgr 2509's console and reporting resources warn that a task sequence "may contain steps from an extension, such as the Microsoft Deployment Toolkit (MDT), which is not present at the destination" ([ConfigMgr 2509 OSD reference](configmgr-2509.md#mdt-components)).
- **The remove step ignores what still uses the classes.** `RemoveActions` deletes every `BDD_` class without a reference check (`Microsoft.BDD.Wizards!SCCM_ConfigureRemove.RemoveActions`). The PC side keeps working, because each step stores its own `<action>` command line. The server side loses the class it needs to edit or validate the step (inferred from the two facts above).
- **A host API change forced a parallel build.** The 2007 and 2012 consoles needed separate assemblies and MOFs for the same steps (see [Versioning](#versioning)).
- **Stored steps never upgrade.** An MDT upgrade leaves old task sequences on the old toolkit package, and new features need new task sequences ([MDT release notes](https://learn.microsoft.com/en-us/intune/configmgr/mdt/release-notes)).
- **Built-in features replaced extension features.** ConfigMgr later added in-console boot image creation, so Create Boot Image using MDT is no longer needed ([MDT known issues](https://learn.microsoft.com/en-us/intune/configmgr/mdt/known-issues)).

## What changes in deployarr

1. **Keep every add-on step loadable without the add-on.** Store an add-on step as `Action::AddOn { addon: AddOnId, step: StepKind, schema_version: u32, config: serde_json::Value }` or a similar shape in `deployarr-core`, so the server can load, show, export, and diff it when the add-on is disabled or gone. The editor shows such a step read-only with a "needs add-on X" banner. This avoids the MDT failure where removing the extension before its steps corrupts task sequences. It follows [Known failures and lessons](#known-failures-and-lessons).
2. **Refuse to uninstall an add-on that a task sequence version still uses.** The uninstall screen lists each task sequence, version, and step that references the add-on. Disable stays allowed. A step from a disabled add-on fails at compile time with a clear error. This is the order Microsoft now asks MDT users to follow by hand, done by the server. It follows the `RemoveActions` finding.
3. **Track what an add-on owns by add-on ID, not by name prefix.** Record every file, route, setting key, secret, and step kind an add-on registers in a server table keyed by add-on ID and version. Uninstall removes exactly that set. MDT's removal missed `PSSnapIn.dll` and the `.mof` copy and deleted classes by the `BDD_` prefix.
4. **Make install and upgrade one staged, checked swap.** Unpack the package to a staging folder, check the manifest, the signature, and the host API version, and switch the active version only when every check passes. A failure leaves the old version active. MDT's wizard counts errors and keeps going, so it can leave a mix of versions. It follows [Failure handling](#failure-handling).
5. **Install once on the server.** The server serves the web UI, so an add-on's UI ships with the add-on and needs no copy to each admin's computer. ConfigMgr spent three releases, 2103 to 2111, adding central approval for the per-console copy that MDT relied on. It follows [Trust](#trust).
6. **Declare a host API version and a minimum server version in the manifest.** The server refuses an add-on whose API version it does not support, and says why, instead of loading it. MDT had no version field and shipped two builds when the console API changed. It follows [Versioning](#versioning).
7. **Define each step's settings once, as a schema.** One schema in the add-on's manifest drives the web UI form, the server's validation at save, and the typed config the agent receives. MDT kept the same rules in a MOF, a WinForms control, and a script, and the SDK asks authors to keep the three in sync by hand. Prefer a declarative form over shipped UI code. It follows [UI contributions](#ui-contributions).
8. **Mark secret fields in the schema.** A field marked secret is stored with the server key, shown masked, never logged, and sent to the agent only for the step that needs it. MDT's ADDS step stores two passwords as plain string properties. This matches the existing rule in [Server](../architecture.md#server).
9. **Keep the agent closed, and ship PC-side add-on code as content.** The agent must stay one static executable that runs in base WinPE ([Agent](../architecture.md#agent)). Copy the half of MDT's design that worked. Its stored step resolves to a run of a program from a content package, and the settings travel as values. An add-on step compiles into existing agent primitives, such as `RunCommand` with a content reference, at plan time. The agent never loads add-on code into its own process.
10. **Make a step's content dependency explicit.** Each add-on step names the content it needs, and the compiler adds the download. MDT's later steps depend on `DeployRoot`, which only an earlier Use Toolkit Package step sets. A missing or misplaced first step fails at run time on the PC, not at save time (inferred). It follows [On the PC](#on-the-pc).
11. **Record which add-on version a task sequence version was saved with.** When an add-on upgrades and its step schema changes, the add-on supplies a migration from the old `schema_version`, or the server keeps running the old config shape. MDT never upgraded stored steps. It follows [Stored steps never upgrade](#known-failures-and-lessons).
12. **Do not let an add-on change the built-in step set.** ConfigMgr tells extensions not to change core console XML, and it gives them no way to modify or remove steps of its own wizards ([About console extensions](https://learn.microsoft.com/en-us/intune/configmgr/develop/core/servers/console/about-configuration-manager-console-extension)). Add-ons add step kinds and pages. They never remove or rewrite built-in ones. It follows [UI contributions](#ui-contributions).
13. **Bind values in any query an add-on or the installer builds.** MDT's installer pasted the site code into WQL text. This matches the existing code rule in [AGENTS.md](../../AGENTS.md#code-rules).

## Open questions

- These sources do not show how the ConfigMgr 2509 provider or console reacts, step by step, when a stored step's class is missing. A lab test that imports an MDT task sequence into a site without MDT would show whether the import fails, the editor drops the step, or the step is kept unchanged.
- These sources do not show whether the SMS Provider encrypts string properties such as `ADDSPassword` in the stored sequence.
- The new-style console extension `.cab` manifest format is not described on the pages read here.
