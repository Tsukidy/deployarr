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
- A Variable condition compares as integers when both sides are digits with an optional leading `-`. Otherwise it compares strings without regard to case. Like uses `PathMatchSpecW` wildcards. Exists means "not empty", so an unset variable and an empty one look the same. Comparing against an empty Value is an error (`TsCore!0x1801a96c0`). The engine checks for an empty Value after it expands the Value, and only for the six operators from Equals to GreaterEqual. Like against an empty Value is not an error (`TsCore!0x1801a96c0`). Empty text passes the integer test (`TsCore!0x1801de96c`), so an empty or unset variable compared with an integer Value compares as an integer. `CCM::Utility::String::toInt64` then reads it as 0 (inferred: that function is in `CcmCore.dll`, which nobody has decompiled). A lone `-` fails the integer test, so it compares as text (`TsCore!0x1801de96c`). AnyVersion and ThisVersion skip the empty Value check too. The engine compares the two sides as for the six operators and passes the result to `TsCore!0x1801ab054`, which returns true for any operator other than Exists to GreaterEqual. A Variable test with AnyVersion or ThisVersion is therefore true whenever the evaluation reaches the comparison (`TsCore!0x1801a96c0`, `TsCore!0x1801ab054`).
- The File, Folder, Registry, and Variable evaluators `%VAR%`-expand every parameter when the condition runs, operators included. The WMI evaluator expands its namespace and its query but not its ignore-query-failure flag. Each of these five evaluators calls `TsCore!0x1801de158` on each parameter it expands. That function calls `TS::Environment::SharedEnvironmentClass::expandEnvironmentStrings` (`TsCore!0x1801de1e1`), which calls the expander that [Variables](#variables) describes (`TsCore!0x180130e08`, `TsCore!0x18012cc00`). The failure text after each call to `TsCore!0x1801de158` names the parameter, such as `ExpandTSEnvironmentString(sOriginalPath, sExpandedPath)`.
  - The File evaluator (`TsCore!0x1801a2494`) requires exactly five parameters (`TsCore!0x1801a263a`) and expands all five: the path (`TsCore!0x1801a2947`), the version operator (`TsCore!0x1801a29d6`), the version (`TsCore!0x1801a2a65`), the date time operator (`TsCore!0x1801a2af4`), and the date time (`TsCore!0x1801a2b83`).
  - The Folder evaluator (`TsCore!0x1801a3970`) requires exactly three parameters (`TsCore!0x1801a3abd`) and expands all three: the path (`TsCore!0x1801a3cdc`), the date time operator (`TsCore!0x1801a3d6b`), and the date time (`TsCore!0x1801a3dfa`).
  - The Registry evaluator (`TsCore!0x1801a6bec`) requires exactly five parameters (`TsCore!0x1801a6d89`) and expands all five: the operator (`TsCore!0x1801a70ac`), the key (`TsCore!0x1801a7142`), the value name (`TsCore!0x1801a71d8`), the type (`TsCore!0x1801a726e`), and the data (`TsCore!0x1801a7304`).
  - The Variable evaluator (`TsCore!0x1801a96c0`) requires exactly three parameters (`TsCore!0x1801a97e0`) and expands all three: the operator (`TsCore!0x1801a99ff`), the variable name (`TsCore!0x1801a9ac0`), and the value (`TsCore!0x1801a9b81`).
  - The WMI evaluator (`TsCore!0x1801aa380`) requires exactly three parameters: the namespace, the query, and the ignore-query-failure flag (`TsCore!0x1801aa4ac`). It expands the namespace (`TsCore!0x1801aa655`) and the query (`TsCore!0x1801aa6e4`). It copies the flag's raw text into a string of its own (`TsCore!0x1801aa792` to `TsCore!0x1801aa7a2`). It compares that string with `true` through `CCM::Utility::String::compare_no_case` (`TsCore!0x1801aa7a8` to `TsCore!0x1801aa7b7`). It sets the flag when the result is zero (`TsCore!0x1801aa7c2`). The string has no other use but its constructor and its cleanup, so no call to `TsCore!0x1801de158` touches it (`TsCore!0x1801aa42d` to `TsCore!0x1801aa435`, `TsCore!0x1801aaeb6` to `TsCore!0x1801aaee4`). That `compare_no_case` ignores case is inferred from its name, because the function is in `CcmCore.dll`. That the caller does not expand the parameters before the evaluator runs is inferred, because the evaluator expands the namespace and the query itself.
  - Nobody has checked whether the Software, OS, Computer, MakeModel, and Location evaluators expand their parameters.
- An empty AND is true, and so is an empty OR. A NOT with several children means "none of them is true" (`TsCore!0x1801a0dc0`, `TsCore!0x1801a55e8`, `TsCore!0x1801a53e4`). Each of the three reads its children in order and stops at the first child that settles it. AND stops at the first false child, and OR and NOT stop at the first true child. Each also stops at the first child that fails to evaluate and returns that child's error, so the children after it never run (`TsCore!0x1801a0dc0`, `TsCore!0x1801a55e8`, `TsCore!0x1801a53e4`).
- A missing file, folder, registry key, or registry value makes the condition false. File versions are compared as four 16-bit fields, and dates are compared with the last-write time to the second (`TsCore!0x1801a2494`). A File version or time test and a Folder time test take only the six operators from Equals to GreaterEqual. Any other operator is an error, including Exists, Like, AnyVersion, and ThisVersion, and so is an operator the lookup does not know. The engine checks the operator only when the file or folder exists and the operator is not empty (`TsCore!0x1801a2494`, `TsCore!0x1801a3970`). A File condition checks the version before the time, and both must hold. A false version test makes the condition false without reading the time (`TsCore!0x1801a2494`). The comparison reads the year, month, day, hour, minute, and second that `FileTimeToSystemTime` gives and ignores the milliseconds (`TsCore!0x1801df900`, `TsCore!0x1801de2c8`, `TsCore!0x1801deeec`). A version test on a file without a version resource is an error, because `TsCore!0x1801de644` returns the failure of `CcmGetFileVersion` (inferred: that function is in `CcmCore.dll`). `File::Exists` and `Directory::Exists` return only true or false (`TsCore!0x1801a2494`, `TsCore!0x1801a3970`), so MDT cannot tell a missing path from one it cannot read (inferred: both functions are in `CcmCore.dll`). Before it calls `File::Exists` or `Directory::Exists`, the File or Folder evaluator reads the `SMSTSDisableWow64Redirection` variable. When `CCM::Utility::String::compare_no_case` finds it equal to `true` and `CCM::Utility::IsWow64` reports a WOW64 process, the evaluator first calls `CCM::Utility::DisableWow64FsRedirection`. A failure to read the variable is an error, and so is a failed `IsWow64` or `DisableWow64FsRedirection` call. A failed `DisableWow64FsRedirection` call also logs `Failed to disable wow64 file system redirection. Error 0x%08x` (File `TsCore!0x1801a2ffe` to `TsCore!0x1801a31eb`, Folder `TsCore!0x1801a40d7` to `TsCore!0x1801a42c4`). Once `DisableWow64FsRedirection` succeeds, the evaluator calls `CCM::Utility::RevertWow64FsRedirection` before it returns, whether the condition is true, false, or an error. That call comes after the existence test and any version or time read (`TsCore!0x1801a3617`, `TsCore!0x1801a450d`). When the revert fails, the engine logs `Failed to revert wow64 file system redirection. Error 0x%08x`. If the evaluation has succeeded so far, the evaluator returns the revert's error code, so the condition fails to evaluate. If the evaluation has already failed, the evaluator keeps its own error (File `TsCore!0x1801a3620` to `TsCore!0x1801a3651`, Folder `TsCore!0x1801a4516` to `TsCore!0x1801a4547`, `TsCore!0x1801a1ba8`). The code shows only that a zero result from `compare_no_case` and a non-zero value from `IsWow64` lead to the `DisableWow64FsRedirection` call (File `TsCore!0x1801a3083` to `TsCore!0x1801a311f`, Folder `TsCore!0x1801a415c` to `TsCore!0x1801a41f8`). That these results mean text equal to `true` in any case and a WOW64 process, and that the two redirection calls turn WOW64 file system redirection off and back on, is inferred, because `compare_no_case`, `IsWow64`, `DisableWow64FsRedirection`, and `RevertWow64FsRedirection` are in `CcmCore.dll`. It is therefore also inferred that only a 32-bit engine on 64-bit Windows reaches the `DisableWow64FsRedirection` call, so that the x64 engine never turns redirection off. Registry conditions accept only full hive names and only `REG_DWORD`, `REG_SZ`, and `REG_EXPAND_SZ` values (`TsCore!0x1801a6bec`). The key must start with `HKEY_LOCAL_MACHINE\`, `HKEY_CURRENT_USER\`, `HKEY_CURRENT_CONFIG\`, or `HKEY_USERS\`, in any case. Any other key is an error, such as one under `HKEY_CLASSES_ROOT` or one that starts with `HKLM\` (`TsCore!0x1801ab5c0`). The engine opens the key with `KEY_READ`. It reads the `SMSTSDisableWow64Redirection` variable, and when `CCM::Utility::String::compare_no_case` finds it equal to `true` and `CCM::Utility::IsWow64` reports a WOW64 process, it adds `KEY_WOW64_64KEY`. A failure to read the variable is an error (`TsCore!0x1801ab5c0`). That this flag reaches only a 32-bit engine on 64-bit Windows, so that the x64 engine always reads the 64-bit view, is inferred, because `IsWow64` is in `CcmCore.dll`. The type limit covers every operator except Exists and NotExists, so a comparison, Like, AnyVersion, or ThisVersion test on a value of another type, such as `REG_BINARY`, is an error (`TsCore!0x1801a7c48`, `TsCore!0x1801a7c89`). Exists and NotExists work on a value of any type (`TsCore!0x1801a7c48`). A Registry condition also has a `Type` parameter. When it and the value name are both set, the engine maps the expanded `Type` text to a type with `TsCore!0x1801ab134` and compares it with the value's real type. That function knows `REG_SZ`, `REG_EXPAND_SZ`, `REG_BINARY`, `REG_DWORD`, `REG_MULTI_SZ`, `REG_QWORD`, and `REG_NONE`, compared with `CCM::Utility::String::compare_no_case`, and maps any other text to `REG_NONE` (`TsCore!0x1801ab134`). On a mismatch the engine logs `Registry value %s\%s of type %s does not exist` and treats the value as missing, for every operator. Exists is then false, NotExists is true, and every other operator is false (`TsCore!0x1801a6bec`, `TsCore!0x1801a7c48`). `Type` counts as set when the expanded `Type` is not empty after the engine's `trimLeft` and `trimRight` calls. The third parameter is `Type`. The arity error names the parameters `key, value, type, data and operator` (`TsCore!0x1800775b0`). The engine copies the third parameter into a string (`TsCore!0x1801a6e68` to `TsCore!0x1801a6e78`) and logs that string as `Type = %s` (`TsCore!0x1801a6f8d` to `TsCore!0x1801a6fd0`). It expands that string into a string of its own (`TsCore!0x1801a7255` to `TsCore!0x1801a726e`). The failure text names the new string `sExpandedType` (`TsCore!0x1801a72a8`). A failed expansion is an error. The engine logs `Failed to evaluate a REG expression. Error 0x(%lx)` and returns the expander's code (`TsCore!0x1801a7273`, `TsCore!0x1801a72e6`, `TsCore!0x1801a8454`, `TsCore!0x1801a87cb`). The `ExpandTSEnvironmentString(sOriginalType, sExpandedType)` text goes to `CCM::Logging::Log` only when `CCM::Logging::DebugLoggingInRetail` returns true (`TsCore!0x1801a7281` to `TsCore!0x1801a72c5`). What `DebugLoggingInRetail` checks, and that `Log` writes the text, is inferred, because both functions are in `CcmCore.dll`. The engine then calls `CCM::Utility::String::trimLeft` and `CCM::Utility::String::trimRight` on the expanded string (`TsCore!0x1801a73ab` to `TsCore!0x1801a73c1`). The two calls remove white space at the start and end of the text (inferred: both functions are in `CcmCore.dll`). After it reads the value's type, the engine skips the type check when the string's length is zero. Otherwise it passes the string's text to `TsCore!0x1801ab134` (`TsCore!0x1801a7a2e` to `TsCore!0x1801a7a4d`). It is therefore also inferred that a `Type` of only white space applies no type check. The engine expands a `REG_EXPAND_SZ` value with `ExpandEnvironmentStringsW` before it compares it (`TsCore!0x1801a6bec`).
- A registry comparison with an empty value name, or with empty data, is an error. The check covers only the six operators from Equals to GreaterEqual. Exists and NotExists with an empty value name test the key. Like with an empty value name is an error when the key exists, because the engine reads no value type and so fails the type check. With a missing key it is false (`TsCore!0x1801a6bec`). AnyVersion and ThisVersion with an empty value name take the same path as Like. The lookup gives them the numbers 10 and 11 (`TsCore!0x1801ab0b0`), which are outside the six operators that the check covers (`TsCore!0x1801a760d`) and are not Exists or NotExists (`TsCore!0x1801a7c48`). With an empty value name the engine reads no value, so the value type keeps the 0 that it starts with (`TsCore!0x1801a6cd7`, `TsCore!0x1801a78d3`). On a key that exists, an AnyVersion test and a ThisVersion test are each an error. A value type of 0 is not `REG_DWORD`, `REG_SZ`, or `REG_EXPAND_SZ`, so the type check fails the condition with `0x80004005` (`TsCore!0x1801a7c89` to `TsCore!0x1801a7ca8`, `TsCore!0x1801a8454`). On a missing key, each is false. When the key open fails with `0x80070002`, the engine clears its exists flag, sets the result to false, and returns success (`TsCore!0x1801a77ac`, `TsCore!0x1801a7806`, `TsCore!0x1801a7c5d`). That a missing key gives `0x80070002` is inferred from the log text `Registry key %s does not exist` on that path (`TsCore!0x1801a77f2`). `TsCore!0x1801ab5c0` opens the key with `CCM::Utility::RegKey::Open` (`TsCore!0x1801ab9ae`), which is in `CcmCore.dll`.
- A registry comparison is numeric for a `REG_DWORD` value and text for a `REG_SZ` or `REG_EXPAND_SZ` value, whatever the data looks like. For a `REG_DWORD` value the engine reads the data with `CCM::Utility::String::toULong` and compares the two as unsigned 32-bit numbers. For text it calls `CCM::Utility::String::compare_no_case` (`TsCore!0x1801a6bec`).
- A registry Like is true for every `REG_DWORD`, `REG_SZ`, or `REG_EXPAND_SZ` value that exists, whatever the pattern. The registry code has no Like branch. It passes Like to `TsCore!0x1801ab054`, which turns an operator and a comparison result into true or false and returns true for Like whatever the result (`TsCore!0x1801a6bec`, `TsCore!0x1801ab054`). AnyVersion and ThisVersion take the same path to `TsCore!0x1801ab054`, which returns true for them as it does on a Variable condition. A registry test with either one is therefore true for every `REG_DWORD`, `REG_SZ`, or `REG_EXPAND_SZ` value that exists (`TsCore!0x1801a6bec`, `TsCore!0x1801ab054`).
- A WMI condition is true when the query returns at least one row. The engine calls `CCM::Utility::String::toUpper` on the expanded query before it runs it (`TsCore!0x1801aa380`). That this upper-cases the query is inferred from the name, because the function is in `CcmCore.dll`. Right after `toUpper`, the engine calls `CCM::Utility::String::trimLeft` and then `CCM::Utility::String::trimRight` on the same expanded query (`TsCore!0x1801aa770`, `TsCore!0x1801aa77e`, `TsCore!0x1801aa78c`). The engine `%VAR%`-expands the namespace too. It calls `TsCore!0x1801de158` on the namespace (`TsCore!0x1801aa655`). The failure text for that call is `ExpandTSEnvironmentString(sOriginalNamespace, sExpandedNamespace)` (`TsCore!0x1801aa68f`). `TsCore!0x1801de158` calls `TS::Environment::SharedEnvironmentClass::expandEnvironmentStrings` (`TsCore!0x1801de1e1`), which calls the expander that [Variables](#variables) describes (`TsCore!0x180130e08`, `TsCore!0x18012cc00`). Just before `toUpper`, the engine calls `trimLeft` on the expanded namespace (`TsCore!0x1801aa762`). The namespace gets no `toUpper` or `trimRight` call (`TsCore!0x1801aa380`). `CCM::Utility::Wmi::Namespace::Open` receives the namespace string after its `trimLeft` call (`TsCore!0x1801aa941`), and `CCM::Utility::Wmi::Namespace::Query` receives the query string after all three calls (`TsCore!0x1801aab3b`). That `trimLeft` removes white space from the start of the text and `trimRight` from its end is inferred from the names, because both functions are in `CcmCore.dll`. A failed query is an error unless the expression has `ignoreQueryFailure="true"` (`TsCore!0x1801aa380`). The flag covers only a failed query and a failure to read its first row, which then count as false. A namespace that cannot be opened is always an error, and so is a failed `CCM::Utility::Wmi::Namespace::SetBlanket` call on it (`TsCore!0x1801aa380`).
- A Software condition checks an MSI ProductCode or UpgradeCode with `MsiGetProductInfoW` and `MsiEnumRelatedProductsW`. It compares no versions (`TsCore!0x1801a8800`).
- MakeModel, Location, and Computer conditions use substring tests against the gathered facts. A gateway condition of `10.0.0.1` therefore also matches `10.0.0.10` (`TsCore!0x1801a4c28`, `TsCore!0x1801a4784`, `TsCore!0x1801a0fc4`).
- A condition that fails to evaluate is not false. It fails the step the same way a failed command does, with `_SMSTSLastActionRetCode` set to the error code (`TsManager!0x140033c10`).
- A group condition that fails to evaluate fails the group. The group's `Execute` checks the disabled flag first and the condition second. On an error it logs `Error 0x%x in evaluating the condition for the group (%s)` and returns a failed outcome, not an engine error. It sets neither `_SMSTSLastActionRetCode` nor `_SMSTSLastActionSucceeded`. It returns before it pushes the group on the stack of open groups (`TsManager!0x140033664`). The main loop then calls `EvaluateFailure` on the same group (`TsManager!0x14002f6f4`). A group with `continueOnError` jumps past its EndGroup. A group without it pops a group from the stack and passes the failure to it. With an empty stack, the run stops (`TsManager!0x140032bb8`). The failed group was never pushed, so the group it pops is its parent. The parser sets the group's continue flag only when `continueOnError` is exactly `true` (`TsCore!0x18019d2b8`, `TsManager!0x14002a3d0`, `TsManager!0x140029158`). When the run stops, the main loop's failure message names the failed group and passes `_SMSTSLastActionRetCode` as its code. The group's path does not set that variable, so the code comes from an earlier step (`TsManager!0x14002f6f4`). That the log text shows this code is inferred, because `CcmCore` formats it.

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
- The condition editor writes these types and operators. A `.resx` citation in the table names a file at the top of the `ilspycmd -p` output folder, beside `Microsoft.BDD.Workbench.csproj`, not in the `Microsoft.BDD.Workbench/` folder that holds the `.cs` files.

| Condition | Parameters | Operators |
| --- | --- | --- |
| Variable | Variable, Operator, Value (`Workbench/TaskSequenceVariable.cs:111-119`) | exists, equals, notEquals, greater, greaterEqual, less, lessEqual (`tsCondition.Items` to `tsCondition.Items6` in `Workbench/Microsoft.BDD.Workbench.TaskSequenceVariable.resx`, `Workbench/TaskSequenceVariable.cs:35, 118, 171-180`, `Workbench/TaskSequenceUtility.cs:43-96`) |
| Registry | KeyPath, Value, Type (`REG_SZ`, `REG_EXPAND_SZ`, or `REG_DWORD`), Operator, Data (`Workbench/TaskSequenceRegistryCondition.cs:72-75, 86-90`) | exists, notExists, equals, notEquals, greater, greaterEqual, less, lessEqual (`Workbench/TaskSequenceRegistryCondition.cs:63-71, 128-192`) |
| WMI | Namespace, Query (`Workbench/TaskSequenceWMICondition.cs:55-61`) | true if the query returns rows ([Conditions](#conditions)) |
| File | Path, optional Version with VersionOperator, optional DateTime with DateTimeOperator (`Workbench/TaskSequenceFilePropertiesCondition.cs:253-345`) | equals, notEquals, greater, greaterEqual, less, lessEqual (`Workbench/TaskSequenceFilePropertiesCondition.cs:61-76, 79-104`) |
| Folder | Path, optional DateTime with DateTimeOperator (`Workbench/TaskSequenceFolderPropertiesCondition.cs:182-232`) | the same set (`Workbench/TaskSequenceFolderPropertiesCondition.cs:52-59, 107-132`) |
| Installed software | ProductCode, UpgradeCode, ProductName, Version, Operator (`Workbench/TaskSequenceInstalledSoftwareCondition.cs:171-202`) | ThisVersion, AnyVersion (`Workbench/TaskSequenceInstalledSoftwareCondition.cs:176-185`) |
| OS version | Architecture, Version, Operator (`Workbench/TaskSequenceOSCondition.cs:109-117`). The Architecture and Version choices come from the fixed list in `Bin\OSList.xml`. Version holds the ID that the file gives the chosen operating system (`Workbench/TaskSequenceUtility.cs:12-41, 109-130`, `Workbench/TaskSequenceOSCondition.cs:30-33, 115-116, 119-126`) | equals, notEquals, greater, greaterEqual, less, lessEqual (`tsCondition.Items` to `tsCondition.Items5` in `Workbench/Microsoft.BDD.Workbench.TaskSequenceOSCondition.resx`, `Workbench/TaskSequenceOSCondition.cs:65, 114, 181-189`, `Workbench/TaskSequenceUtility.cs:43-96`) |
| If statement | `<operator type="and\|or\|not">` (`Workbench/TaskSequenceEditor.cs:976-979`, `Workbench/TaskSequenceIf.cs:37-53`) | all, any, none (`radioAnd.Text`, `radioOr.Text`, and `radioNot.Text` in `Workbench/Microsoft.BDD.Workbench.TaskSequenceIf.resx`, `Workbench/TaskSequenceIf.cs:37-53`) |

- The Variable condition dialog maps operators only from lower-case names. Opening a template Variable condition stored as `Equals` or `notExists` and clicking OK saves it as `exists` with an empty value (`Workbench/TaskSequenceVariable.cs:35-49`).
- The Registry condition dialog maps operators with two methods of its own, `FindEquivalentCMBValue` and `FindEquivalentCMBText`. It writes "not exists" as `notExists` and reads `notExists` back as "not exists" (`Workbench/TaskSequenceRegistryCondition.cs:128-192`).
- The Registry dialog offers only three value types, `REG_SZ`, `REG_EXPAND_SZ`, and `REG_DWORD` (`Workbench/TaskSequenceRegistryCondition.cs:72-75`).
- For exists and not exists, the Registry dialog turns off the type box. It does so when it opens a condition with either operator and when the user picks either one (`Workbench/TaskSequenceRegistryCondition.cs:108-114, 291-298`).
- When the user clicks OK with exists or not exists, the Registry dialog selects `REG_SZ` in the type box and writes it to the condition's `Type`. It writes `Type` only into a condition that already has a `Type` variable (`Workbench/TaskSequenceRegistryCondition.cs:226-236, 258-261`).
- The editor's "Registry Setting" menu item creates each new Registry condition with `Type` set to `REG_SZ` and then opens the Registry dialog on it (`Workbench/TaskSequenceEditor.cs:1148-1185`). The dialog writes `REG_SZ` to `Type` for exists and not exists, so every exists or not exists test made with this menu item carries `Type` `REG_SZ` (`Workbench/TaskSequenceEditor.cs:1172-1177`, `Workbench/TaskSequenceRegistryCondition.cs:226-236, 258-261`).
- When `Type` is set, MDT's engine treats a named value whose type differs from it as missing (`TsCore!0x1801a6bec`, `TsCore!0x1801a7c48`, [Conditions](#conditions)). An exists test made with the menu item is therefore false for a named value whose type is not `REG_SZ`, such as a `REG_DWORD` or `REG_EXPAND_SZ` value, and a not exists test is true for one (inferred from the editor code above and the engine rule in [Conditions](#conditions)).
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
| MDT conditions include file version, file and folder dates, installed MSI products, and a WMI failure flag | deployarr's `Condition` gets these types. `Var` and `Registry` tests take all nine `Test` operators, and file version and date comparisons take only the six equality and order operators of `CompareOp` (#62). See [Core data shapes](../architecture.md#core-data-shapes). Make, model, location, and computer checks become variable conditions on gathered facts, with exact matching instead of MDT's substring tests |
| MDT's editor writes an OS version condition with `Architecture`, `Version`, and `Operator` ([Task sequence editor model](#task-sequence-editor-model)) | deployarr's `Condition` has no OS version type, and `docs/architecture.md` has no rule for this yet ([#291](https://github.com/Tsukidy/deployarr/issues/291)). See [Core data shapes](../architecture.md#core-data-shapes) |
| MDT's integer test passes empty text, so an empty variable compares with an integer as an integer, read as 0 (inferred) | An integer has at least one digit, so empty text compares as text |
| MDT reports a Variable comparison against an empty Value as an error when the condition runs, and accepts Like against an empty Value | A `Var` test compares against a `NonEmptyTemplate`, so the parser rejects an empty Value for every operator, `Like` included, and the sequence fails when it is saved. A Value that expands to empty text follows MDT's rule when the condition runs. See [Parameter types](../architecture.md#parameter-types) |
| MDT's registry comparisons fail on every value type but `REG_DWORD`, `REG_SZ`, and `REG_EXPAND_SZ` | A `Registry` condition reads `REG_DWORD`, `REG_QWORD`, `REG_SZ`, and `REG_EXPAND_SZ` values, so it also compares a `REG_QWORD` value. See [Parameter types](../architecture.md#parameter-types) |
| MDT compares a `REG_DWORD` value as a number and a `REG_SZ` or `REG_EXPAND_SZ` value as text, whatever the data looks like | A `Registry` comparison applies the `Var` rules to the text of both sides, with a number as its decimal text. See [Conditions](../architecture.md#conditions) |
| A registry Like in MDT is true for every `REG_DWORD`, `REG_SZ`, or `REG_EXPAND_SZ` value that exists, whatever the pattern | A `Registry` `Like` test matches the pattern against the value's text, as a `Var` `Like` test does |
| MDT calls `trimLeft` and `trimRight` on the expanded `Type` and applies no type check when the result is empty, so a `Type` of only white space applies none (inferred) | The MDT importer trims white space from `Type` before it matches the type names. A `Type` that is empty after the trim leaves the value type out, with no note. The owner chose this rule on [#277](https://github.com/Tsukidy/deployarr/issues/277#issuecomment-6989845415). See [Conditions](../architecture.md#conditions) |
| An MDT Registry condition with `Type` set treats a value of another type as missing | `RegistryCondition` has an optional value type, one of the seven types that MDT's mapping knows. A value of another type counts as missing for every test, as in MDT. The MDT importer copies `Type` into the value type, maps unknown text to `REG_NONE` with a note, and drops `Type` on a test of a key. See [Conditions](../architecture.md#conditions) ([#277](https://github.com/Tsukidy/deployarr/issues/277)) |
| MDT accepts AnyVersion and ThisVersion on Variable and Registry conditions. A Variable test with either one is true whenever the evaluation reaches the comparison, and a Registry test with either one is true for every `REG_DWORD`, `REG_SZ`, or `REG_EXPAND_SZ` value that exists | `Test` has neither operator. An MDT importer replaces a Registry test with either one by `Exists` on the same key, value name, and value type. It replaces a Variable test with either one on variable `x` by `Any([Var{x, Exists}, Var{x, NotExists}])`, which is true on every run. It names each replaced test in a note that holds no values. See [Conditions](../architecture.md#conditions) ([#278](https://github.com/Tsukidy/deployarr/issues/278)) |
| MDT upper-cases the expanded WMI query before it runs it (inferred) | The agent runs a `Wmi` condition's query exactly as substitution leaves it, with no change of case, and an MDT importer copies the query text unchanged ([#279](https://github.com/Tsukidy/deployarr/issues/279)). See [Conditions](../architecture.md#conditions) |
| MDT's evaluator reports a registry comparison with an empty value name or empty data as an error when the condition runs | The parser rejects a `Registry` test other than `Exists` or `NotExists` that names no value, and rejects an empty value name and empty data, so the sequence fails when it is saved. See [Parameter types](../architecture.md#parameter-types) |
| An MDT Registry test with AnyVersion or ThisVersion and no value name fails to evaluate on a key that exists, and is false on a missing key (inferred) | The MDT importer turns such a test into `Exists` on the key, as the owner chose on [#278](https://github.com/Tsukidy/deployarr/issues/278#issuecomment-6988082376). `Exists` is true on a key that exists, where MDT fails to evaluate. On a missing key both are false ([#320](https://github.com/Tsukidy/deployarr/issues/320)) |
| MDT's file and folder existence checks (`File::Exists`, `Directory::Exists`) return only true or false, so they cannot report a path they cannot read (inferred) | A file or folder that the agent cannot read makes the condition fail to evaluate, not false |
| MDT trims the expanded WMI query at both ends, after it upper-cases it and before it runs it (inferred) | The agent runs a `Wmi` condition's substituted query exactly as written, with no case change and no trimming. The owner chose this rule on [#279](https://github.com/Tsukidy/deployarr/issues/279#issuecomment-6988083229) |
| MDT trims the start of the expanded WMI namespace before it opens it (inferred) | The agent opens a `Wmi` condition's namespace exactly as written, with no trimming. An MDT importer removes white space from the start of the namespace and nothing from its end, so that an imported namespace without `%` that is not empty after the trim is the string that MDT passes to `CCM::Utility::Wmi::Namespace::Open` when it reaches that call. This match is inferred, because it depends on which characters `CCM::Utility::String::trimLeft` removes. The owner chose this rule on [#300](https://github.com/Tsukidy/deployarr/issues/300#issuecomment-6992982890). The row that starts "MDT expands the WMI namespace" covers a namespace that holds `%`. See [Conditions](../architecture.md#conditions) |
| MDT expands the WMI namespace when the condition runs, before it opens it | The agent passes a `Wmi` condition's namespace to the probe as written and does not substitute it, because the namespace is a plain `String`, not a `Template`. An MDT importer copies the namespace with no change but the trim of white space at its start ([#300](https://github.com/Tsukidy/deployarr/issues/300#issuecomment-6992982890)). When the namespace holds `%`, the importer adds a note that names the condition. The owner chose the substitution rule and the note on [#301](https://github.com/Tsukidy/deployarr/issues/301#issuecomment-6992982144). See [Conditions](../architecture.md#conditions) |
| A condition that fails to evaluate fails the step in MDT. A group's condition error fails the group | deployarr's evaluator returns an error. A step's error goes through the step's `on_error`. A group's error follows the group's own `continue_on_error` first and then its parent's target, as [#150](https://github.com/Tsukidy/deployarr/issues/150) decides |
| MDT writes its state file in place and ignores a failed save | The agent writes a temporary file, flushes it, and renames it. A failed save stops the run before the next step or reboot |
| MDT saves nothing before a step, so an interrupted step runs again | The agent marks a step in flight before it starts. A typed action converges when run again, and a command fails unless its step allows a re-run |
| A retry with a reboot loses the step's variable changes | The agent saves state before every reboot |
| MDT marks a pending reboot with a volatile registry key | The agent does the same and refuses to resume while the marker exists |
| An empty `successCodeList` fails every exit code | The parser rejects an empty success list (#62). See [JSON format](../architecture.md#json-format) |
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
