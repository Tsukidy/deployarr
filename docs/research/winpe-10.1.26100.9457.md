# WinPE add-on 10.1.26100.9457 reference

This page records which DLLs and API sets base WinPE x64 provides. The source is the Windows PE add-on for the Windows ADK 10.1.26100.9457. `deployarr-agent` runs in base WinPE, so a module it imports must match a name in [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt) by the rule in [The list file](#the-list-file). The CI check reads that file. Use this page when you add an import to the agent.

## Sources

- The setup program is `adkwinpesetup.exe` from `https://go.microsoft.com/fwlink/?linkid=2289981`. The link redirects to `https://download.microsoft.com/download/a4a79e7a-f085-41c4-aebf-2538fd000790/adkwinpeaddons/adkwinpesetup.exe`. The file is 1,951,784 bytes, and its SHA-256 is `d4de67acc83de253ccc941dd0590808d83726bdd8ea768a374a7beaff7949d6a`. The server reported a last-modified date of 2026-09-22.
- Microsoft's ADK download page, `https://learn.microsoft.com/en-us/windows-hardware/get-started/adk-install`, lists the link as the add-on for ADK 10.1.26100.9457 (September 2026). The page says this ADK supports Windows 11 26H2, 25H2, and 24H2, all earlier supported versions of Windows 10 and 11, Windows Server 2025, and Windows Server 2022.
- The setup program is a WiX Burn bundle. `7z` unpacked its manifest, whose `Registration` version is `10.1.26100.9457`. The manifest's `package_WindowsPEwimsDesktopEditions_x86_en_us` package holds `Windows PE wims (DesktopEditions)-x86_en-us.msi` and two CABs.
- The manifest gives no download URLs. Each payload downloads from the setup program's folder plus its `FilePath`, with backslashes turned into slashes and the result percent-encoded. An example is `Installers/a6296c5a6eec9ca02b431ed0461b952d.cab`. The SHA-1 of each download matched the manifest's `Hash`.
- The MSI's `File` table has two `winpe.wim` rows. The row installed under `Windows Preinstallation Environment\amd64\en-us` is in `a6296c5a6eec9ca02b431ed0461b952d.cab`, which is 338,097,556 bytes with SHA-1 `054936166468ba70637f181a5626cfb8ec62b4af`.
- The x64 `winpe.wim` is 340,134,390 bytes, the size the `File` table gives. Its SHA-256 is `fbcbdb1c6651ab3a69384e9d4f95f2c02321318603849453b252e21e827c8197`. Every finding on this page comes from this file, except the `WinPE-SecureStartup` package in [tbs.dll](#tbsdll).
- The WIM's XML describes one image, `Microsoft Windows PE (amd64)`, with edition `WindowsPE`, version 10.0.26100.1, and branch `ge_release`. The XML gives a creation date of 2024-04-01.
- All 3,772 component manifests under `Windows\WinSxS\Manifests` have version 10.0.26100.1. The September 2026 add-on therefore ships the 24H2 image with no cumulative update applied (inferred from those versions).
- 7-Zip 23.01 read the CABs and the WIM. msitools 0.103 read the MSI tables.
- `tools/re/winpe/reproduce.sh` repeats the download, the checks, and the lists. See [WinPE in `tools/re/README.md`](../../tools/re/README.md#winpe). Two parsers written separately produced the same list file, byte for byte, with SHA-256 `d7bd783a0a3a046bbbe9eacccbdee82ca875f950e1cc1768e257f29a5f25fe66`.

## Modules in System32

- The top level of `Windows\System32` holds 802 files that end in `.dll` and one that ends in `.drv`. The list counts all 803 as modules.
- The `.drv` file is `winspool.drv`. Its PE header marks it as a DLL, and its file description is "Windows Spooler Driver". The API set schema names it as the default host of 3 API sets, so Windows loads it as a user-mode module (inferred from the schema).
- `ncpa.cpl` and `kmddsp.tsp` are DLLs by their PE headers too. Their file descriptions are "Network Connections Control-Panel Stub" and "TAPI Kernel-Mode Service Provider". The list leaves them out, because programs load such files by path, not through an import table (inferred from those roles).
- The list also leaves out the 4 `.sys` files at the top of `System32`, and every subfolder. `System32\downlevel` holds 112 more DLLs.
- The list records files that are present. It does not check that a listed module loads in user mode, or that the module's own imports are present.
- [winpe-10.1.26100.9457-system32-dlls.txt](winpe-10.1.26100.9457-system32-dlls.txt) names each module in the case the image stores it. It holds one name per line, sorted by lower-case name, with LF line ends. `tools/re/winpe/reproduce.sh` writes the same file as `system32-dlls.txt`.

### tbs.dll

- The image has no `tbs.dll` anywhere. It does have the TPM driver, as `Windows\System32\drivers\tpm.sys` and `Windows\INF\tpm.inf`.
- `tbs.dll` ships in the `WinPE-SecureStartup` optional component. The add-on's amd64 `WinPE_OCs\WinPE-SecureStartup.cab` is 897,767 bytes, with SHA-256 `f8ae41b03360411752169f02cf29225e60a2212e5709c7572e5b6901efa03e74`. It holds `tbs.dll` and `tbs.sys` from `amd64_microsoft-windows-tpm-tbs_31bf3856ad364e35_10.0.26100.1_none_ebab7d55e0e0e8ac`.
- The package's `update.mum` names `WinPE-WMI-Package` in its `parent` element. DISM therefore needs `WinPE-WMI` in the image before it adds `WinPE-SecureStartup` (inferred from the `parent` element).
- That CAB comes from `9722214af0ab8aa9dffb6cfdafd937b7.cab` of `Windows PE Optional Packages (DesktopEditions)-x86_en-us.msi`. Its SHA-1, `f6d32f0081386aa9039c8ab4a9623f81b0686d63`, matched the manifest. The rerun script does not fetch the optional components.

Base WinPE's `tpm.inf` binds these hardware IDs to the TPM driver:

| Hardware IDs | Device description | Citation |
| --- | --- | --- |
| `ACPI\PNP0C31`, `*PNP0C31` | Trusted Platform Module 1.2 | `tpm.inf:36-38`, `tpm.inf:209` |
| `ACPI\BCM0101` | Trusted Platform Module 1.2 (with Port IO) | `tpm.inf:40-41`, `tpm.inf:211` |
| `ACPI\BCM0102` | Trusted Platform Module 1.2 | `tpm.inf:40-42`, `tpm.inf:209` |
| `ACPI\FTPM0101`, `*FTPM0101` | Trusted Platform Module 2.0 (Simulator-backed) | `tpm.inf:44-46`, `tpm.inf:214` |
| `ACPI\MSFT0101`, `*MSFT0101`, `{36DEAA79-C5DD-447C-95E6-B3859589291A}\TrEETPM`, `{1F75CF6D-F709-4C0C-8BCB-0CDCA1289DDD}\TrEETPM` | Trusted Platform Module 2.0 | `tpm.inf:48-52`, `tpm.inf:215` |

### Functions the architecture names

The architecture names these functions. `Tbsi_GetDeviceInfo` cannot run, because `tbs.dll` is missing. Each of the others is exported by name:

| DLL | Functions | Named in |
| --- | --- | --- |
| `kernel32.dll` | `GetSystemFirmwareTable`, `GetFirmwareType`, `GetFirmwareEnvironmentVariableW`, `GetPhysicallyInstalledSystemMemory`, `GetLogicalProcessorInformationEx`, `SetThreadExecutionState` | [Agent](../architecture.md#agent) |
| `IPHLPAPI.DLL` | `GetAdaptersAddresses` | [Agent](../architecture.md#agent) |
| `advapi32.dll` | `RegLoadKeyW` | [Handoff from WinPE to the new OS](../architecture.md#handoff-from-winpe-to-the-new-os) |
| `cfgmgr32.dll` | `CM_Get_DevNode_Status` | [Injection and verification](../architecture.md#injection-and-verification) |
| `kernel32.dll` | `LoadLibraryExW` | [TPM version](../architecture.md#tpm-version) |
| `setupapi.dll` | `SetupDiGetDeviceRegistryPropertyW` | [TPM version](../architecture.md#tpm-version) |
| `netapi32.dll` | `DsRoleGetPrimaryDomainInformation`, `DsRoleFreeMemory` | [Directory join](../architecture.md#directory-join) |
| `ole32.dll` | `CoCreateInstance` | [How the agent reaches BitLocker](../architecture.md#how-the-agent-reaches-bitlocker), [Windows Update](../architecture.md#windows-update) |
| `oleaut32.dll` | `SysAllocString`, `VariantClear` | [Windows Update](../architecture.md#windows-update) |

This table lists five more mechanisms. For three of them, the architecture names no function. For the other two, [TPM version](../architecture.md#tpm-version) names one function each, `SetupDiGetDeviceRegistryPropertyW` for SetupAPI device enumeration and `LoadLibraryExW` for loading `tbs.dll`. The first table lists both. This page checked a usual function for each mechanism. Except for those two, the choice of function is this page's, not the architecture's:

| Mechanism | DLL | Functions |
| --- | --- | --- |
| `IOCTL_STORAGE_QUERY_PROPERTY` | `kernel32.dll` | `DeviceIoControl` |
| SetupAPI device enumeration | `setupapi.dll` | `SetupDiGetClassDevsW`, `SetupDiEnumDeviceInfo`, `SetupDiGetDeviceRegistryPropertyW` |
| Enabling `SeSystemEnvironmentPrivilege` | `advapi32.dll` | `OpenProcessToken`, `LookupPrivilegeValueW`, `AdjustTokenPrivileges` |
| Reading `UEFISecureBootEnabled` | `advapi32.dll` | `RegGetValueW` |
| Loading `tbs.dll` at run time | `kernel32.dll` | `LoadLibraryExW` |

A short reader of each DLL's export table checked these names. The rerun script does not repeat it. An export shows that a function exists, not that it works in WinPE. The M3 lab tests that.

### Exports with code and forwarders

An entry in a DLL's export address table either holds the address of the function's code or is a forwarder. A forwarder points to a string that names another DLL and a function in it, so the function is "simultaneously imported and exported" ([PE Format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format#export-address-table)). `cargo xtask winpe-imports` reads only the module names in the agent's own import tables, so it does not check a forwarder's target module.

A second read of the export tables checked seven functions from the first table in [Functions the architecture names](#functions-the-architecture-names). Code means that the entry holds an address in the DLL's `.text` section:

| DLL | Function | Export |
| --- | --- | --- |
| `netapi32.dll` | `DsRoleGetPrimaryDomainInformation` | Forwarder to `DSROLE.DsRoleGetPrimaryDomainInformation` |
| `netapi32.dll` | `DsRoleFreeMemory` | Forwarder to `DSROLE.DsRoleFreeMemory` |
| `ole32.dll` | `CoCreateInstance` | Forwarder to `api-ms-win-core-com-l1-1-0.CoCreateInstance` |
| `oleaut32.dll` | `SysAllocString` | Code |
| `oleaut32.dll` | `VariantClear` | Code |
| `kernel32.dll` | `LoadLibraryExW` | Code |
| `setupapi.dll` | `SetupDiGetDeviceRegistryPropertyW` | Code |

- Each forwarder string names its target module without `.dll`. With `.dll` added, both targets pass the rule in [The list file](#the-list-file). The list holds `dsrole.dll`. `api-ms-win-core-com-l1-1-0.dll` matches the list entry `api-ms-win-core-com-l1-1-3.dll` by that rule.
- The API set schema names `combase.dll` as the default host of `api-ms-win-core-com-l1-1-3`. The entry maps no importing module to another host. The list holds `combase.dll`.
- The target modules export these functions with code. `dsrole.dll` exports `DsRoleGetPrimaryDomainInformation` and `DsRoleFreeMemory`, and `combase.dll` exports `CoCreateInstance`.
- Two of these exports with code are a single jump through the DLL's own import table. `kernel32.dll`'s `LoadLibraryExW` jumps to the `LoadLibraryExW` that it imports from `api-ms-win-core-libraryloader-l1-2-0.dll`. `dsrole.dll`'s `DsRoleFreeMemory` jumps to the `LocalFree` that it imports from `api-ms-win-core-heap-l2-1-0.dll`. Both API sets resolve, and the schema names `kernelbase.dll` as the default host of both. `kernelbase.dll` exports `LoadLibraryExW` with code.
- `llvm-readobj` 18.1.3 read the export tables with `--coff-exports`. A short reader written with the Python standard library read them again and found the section of each address. Both gave the same ordinals, the same addresses for the exports with code, and the same forwarder strings. `rz-bin` 0.7.4 with `-E` gave the same ordinals, and the same addresses for the exports with code. GNU `objdump` 2.42 disassembled the two single-jump exports, and `llvm-readobj --coff-imports` gave the import behind each jump. The rerun script does not repeat these reads.

The DLLs come from `Windows\System32` in the `winpe.wim` that [Sources](#sources) describes:

| DLL | Bytes | SHA-256 |
| --- | --- | --- |
| `netapi32.dll` | 117,160 | `854ca03638be71ce504e0e5040cdcb16569dbcaf1a76a4a0e8a60536f898fa39` |
| `dsrole.dll` | 55,072 | `336517fd56e2b4e4ee524a8d4d1121173224fe7f142af18739e23c10f3453551` |
| `ole32.dll` | 1,674,832 | `fda2ee5887922cb9ca97a146ad708c8b56cedfcfc25c0d019ec7c625a136573d` |
| `combase.dll` | 3,654,152 | `506a2e3ee3ac3f5b094b71ba0bb5e9db332e8d5ce071c35984725fe60516bf9c` |
| `oleaut32.dll` | 885,624 | `d5b2274f3cbfa58a5bfcff9d2b22920a64e9df523c9054bc13404fc1149e9eb6` |
| `kernel32.dll` | 827,784 | `e1593f34c26f7ef8e0b8e564c9d3de4bfc78379d6251fa4da150422771822cc9` |
| `KernelBase.dll` | 3,827,720 | `833b9c0f5c79b413992d2e441fffef58a55a0d1df63197d89c52d785538dfaff` |
| `setupapi.dll` | 4,749,136 | `4b039cfd96979696add00ca64f294285b33857d477d199d495141fef83d5f4f8` |

## API sets

- `Windows\System32\apisetschema.dll` holds API set schema version 6 in its `.apiset` section. The schema has 974 entries. 363 names start with `api-` and 611 start with `ext-`.
- 577 entries resolve, because their default host is a module in the list. 316 of them are `api-` sets and 261 are `ext-` sets. The schema stores each of their names in lower case. [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt) names each one with `.dll` added.
- 397 entries do not resolve. 135 have no host. 44 have a kernel-mode `.sys` host, such as `win32kfull.sys`. 218 name one of 153 DLLs that `System32` lacks, such as `comctl32.dll` and `twinapi.appcore.dll`. The image keeps `comctl32.dll` only under `WinSxS`.
- Six entries map to a different host for one importing module. `api-ms-win-core-io-l1-1-1` maps to `kernel32.dll`, except for imports from `kernel32.dll` itself, which map to `kernelbase.dll`. The list follows the default host.
- Each entry stores a full versioned name, such as `api-ms-win-core-synch-l1-2-1`. In all 974 entries, the hashed length ends at the last hyphen, as in `api-ms-win-core-synch-l1-2`. Each of the 974 values in the schema's hash table is the hash of that prefix, and none is the hash of a full name.
- The loader therefore ignores the last number when it matches an import (inferred from the hashed length and the hash table). An import of `api-ms-win-core-synch-l1-2-0.dll` resolves through the entry stored as `api-ms-win-core-synch-l1-2-1`.
- `kernel32.dll` relies on the same rule. It forwards `GetLogicalProcessorInformationEx` to `api-ms-win-core-sysinfo-l1-1-0`, and the schema stores only `api-ms-win-core-sysinfo-l1-1-1`. Microsoft's API set pages do not state the rule.

## The list file

- [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt) holds 1,380 names, one per line, in lower case, sorted, with LF line ends. They are the 803 modules and the 577 resolvable API sets, each API set with `.dll` added.
- To check one import name against the list:
  1. Lower-case the import name.
  2. If it starts with `api-` or `ext-`, cut it at its last hyphen. Accept it when an `api-` or `ext-` list entry, cut the same way, is equal.
  3. Otherwise, accept it only when the list holds the same name.
- `cargo xtask winpe-imports <exe>` applies these steps to the import table and the delay-load import table of an executable. It lists each rejected name with the table it came from. The Windows CI job runs it on the release build of `deployarr-agent`.
- The check covers module names only. A module that resolves can still lack a function. For example, an API set import whose last number is higher than the stored one resolves to a host that may lack the newer functions (inferred). A stricter check rejects such an import. The M3 lab covers functions.
- The list covers the image as Microsoft ships it. A boot image built from another add-on, or serviced with a cumulative update, needs its own page and list.

## What changes in deployarr

1. The [Agent](../architecture.md#agent) section says it is not yet checked whether base WinPE includes `tbs.dll`. It does not. Replace that sentence with this result, and drop the `tbs.dll` half of the M3 check in the [roadmap](../roadmap.md#m3-bare-metal-deployment-from-boot-media).
2. Without `tbs.dll`, `Tbsi_GetDeviceInfo` cannot run in base WinPE, and the TPM version is unknown. The [readiness check](../architecture.md#readiness-check) fails a fact that it cannot read, and its TPM 2.0 check fails by default. In base WinPE, every PC would stop at the readiness check unless its task sequence turns the TPM check off. Choose one of these:
   - Map the TPM device's hardware ID to a TPM version, with the IDs in [tbs.dll](#tbsdll). The agent already collects every device's hardware IDs with SetupAPI. Keep `Tbsi_GetDeviceInfo` for a boot image that has `tbs.dll`. Nobody has checked whether every firmware TPM reports one of these IDs. The M3 check on the UEFI VM and the real PC can confirm it.
   - Add `WinPE-WMI` and `WinPE-SecureStartup` to the boot image. Only DISM can add them, so a Linux server needs a base image serviced on Windows, as the PowerShell components do. See [Boot image](../architecture.md#boot-image).
3. Load `tbs.dll` with `LoadLibraryExW`, not through the import table or the delay-load import table. The import check that #23 builds reads both tables, so an import of `tbs.dll` in either one fails it.
4. Record the matching rule from [The list file](#the-list-file) in the Agent section, next to the CI check.
5. [Directory join](../architecture.md#directory-join) says that base WinPE's list file holds `netapi32.dll`. Add that base WinPE's `netapi32.dll` forwards `DsRoleGetPrimaryDomainInformation` and `DsRoleFreeMemory` to `dsrole.dll`, and that the list file holds `dsrole.dll` too. Cite [Exports with code and forwarders](#exports-with-code-and-forwarders).
6. [How the agent reaches BitLocker](../architecture.md#how-the-agent-reaches-bitlocker) names `CoCreateInstance` and says that the list file holds `ole32.dll`. Add that base WinPE's `ole32.dll` forwards `CoCreateInstance` to the API set `api-ms-win-core-com-l1-1-0`, whose default host is `combase.dll`, and that the API set and `combase.dll` both pass the list file's rule. Cite [Exports with code and forwarders](#exports-with-code-and-forwarders).
7. [Windows Update](../architecture.md#windows-update) says that `ole32.dll` exports `CoCreateInstance`. In base WinPE, `ole32.dll` forwards it to `api-ms-win-core-com-l1-1-0`, whose default host is `combase.dll`. Say so, and cite [Exports with code and forwarders](#exports-with-code-and-forwarders).
