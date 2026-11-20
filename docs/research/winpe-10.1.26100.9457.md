# WinPE add-on 10.1.26100.9457 reference

This page records which DLLs and API sets base WinPE x64 provides. The source is the Windows PE add-on for the Windows ADK 10.1.26100.9457. `deployarr-agent` runs in base WinPE, so a module it imports must match a name in [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt) by the rule in [The list file](#the-list-file). The CI check reads that file. Use this page when you add an import to the agent. The page also records which letters `PathMatchSpecW` matches in any case in this image, in [PathMatchSpecW letter case](#pathmatchspecw-letter-case), and what it returns for an empty pattern, in [PathMatchSpecW with an empty pattern](#pathmatchspecw-with-an-empty-pattern).

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

## PathMatchSpecW letter case

MDT's Variable condition matches `Like` with `PathMatchSpecW`. It passes the variable's value as `pszFile` and the expanded Value as `pszSpec` (`TsCore!0x1801aa078`, see [Conditions](mdt-8450-binaries.md#conditions) in the MDT 8450 binaries reference). This section records which letters `PathMatchSpecW` in this image matches in any case. It covers letter case only, not the wildcard rules. For an empty pattern, see [PathMatchSpecW with an empty pattern](#pathmatchspecw-with-an-empty-pattern).

The image checked is the `winpe.wim` in [Sources](#sources), version 10.0.26100.1. Other Windows builds were not checked, and neither was a full Windows install.

### Where the function lives

- `shlwapi.dll` exports `PathMatchSpecW` with code at `shlwapi!0x18000d4a0`. That code is a single jump through the import slot `shlwapi!0x180037330`, which imports `PathMatchSpecW` from `api-ms-win-core-shlwapi-legacy-l1-1-0.dll`.
- The API set schema names `kernelbase.dll` as the default host of `api-ms-win-core-shlwapi-legacy-l1-1`. `tools/re/winpe/reproduce.sh` writes this row to `apisets.tsv`.
- `KernelBase.dll` exports `PathMatchSpecW` with code at `KernelBase!0x1800d4dd0`. It copies the pattern, cuts the copy at each `;`, and passes each part with the text and a flags value of 0 to `KernelBase!0x18001b890` (`KernelBase!0x1800d4e8b`, `KernelBase!0x1800d4eaa`).
- Microsoft's [`PathMatchSpecW`](https://learn.microsoft.com/en-us/windows/win32/api/shlwapi/nf-shlwapi-pathmatchspecw) page says nothing about letter case.

### How the matcher compares one character

- `KernelBase!0x18001b890` treats `?` and `*` in the pattern as wildcards (`KernelBase!0x18001b930`, `KernelBase!0x18001b939`). For any other pattern unit, it maps the pattern unit and the text unit through `LCMapStringEx`, one call each (`KernelBase!0x18001b97a`, `KernelBase!0x18001b9d1`). It compares the two results and fails the match when they differ (`KernelBase!0x18001b9de` to `KernelBase!0x18001b9e1`). The text and the pattern are UTF-16, so each step takes one UTF-16 unit.
- Each call passes a null locale name, flags `0x200`, one unit as the source, and the same buffer, one unit long, as the destination (`KernelBase!0x18001b942` to `KernelBase!0x18001b975`, `KernelBase!0x18001b998` to `KernelBase!0x18001b9cc`). That `0x200` is `LCMAP_UPPERCASE` and a null name is `LOCALE_NAME_USER_DEFAULT` is inferred from the values in Wine's [`include/winnls.h`](https://github.com/wine-mirror/wine/blob/master/include/winnls.h), because this page does not analyze the Windows SDK header.
- Microsoft's [`LCMapStringEx`](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-lcmapstringex) page says that `LCMAP_LINGUISTIC_CASING` means "Use linguistic rules for casing, instead of file system rules (default)". The matcher does not set it.
- So a pattern unit other than `?` and `*` matches a text unit when `LCMapStringEx` upper-cases both to the same unit. One more step can then require an exact match. See [A step for double-byte code pages](#a-step-for-double-byte-code-pages).

### How LCMapStringEx upper-cases one unit

With these arguments, `LCMapStringEx` (`KernelBase!0x18001a080`) can take a short path that maps each unit in a loop (`KernelBase!0x18001a48e` to `KernelBase!0x18001a4b4`, `KernelBase!0x18001b706` to `KernelBase!0x18001b746`):

- `a` to `z` become `A` to `Z`.
- Every other unit below U+00C0 stays as it is.
- A unit from U+00C0 up goes through a three-level table `t`, as `unit + t[t[t[unit >> 8] + ((unit >> 4) & 0xf)] + (unit & 0xf)]`, with 16-bit wraparound. `t` is the pointer at `KernelBase!0x1803548f0`. When that pointer is null, the unit stays as it is.

`LCMapStringEx` takes this path only when all of these hold:

- Apart from `0x800` and `0x40000000`, the flags are `0x100` or `0x200` alone (`KernelBase!0x18001a10d`, `KernelBase!0x18001a167`, `KernelBase!0x18001a19b` to `KernelBase!0x18001a1a7`). The call passes no version information and no sort handle (`KernelBase!0x18001a175`, `KernelBase!0x18001a17e`).
- The path reads the locale record at `KernelBase!0x180353228` (`KernelBase!0x18001a194`). With a null locale name, it skips a lookup by name (`KernelBase!0x18001a256` to `KernelBase!0x18001a25d`). A check at `KernelBase!0x18001a1bb` to `KernelBase!0x18001a1ca` XORs the record's field at `+4` with the 32-bit value at `KernelBase!0x18035322c`. On this path that value is the same field, so the check always passes.
- The record's fields at `+0x10` and `+0x18` are null (`KernelBase!0x18001a1d6` to `KernelBase!0x18001a1de`, `KernelBase!0x18001a24f`). The code reads these two fields only when the record's first 32-bit value is at least `0x20` (`KernelBase!0x18001a1cc` to `KernelBase!0x18001a1d0`). Otherwise it reads the two 64-bit values at `KernelBase!0x180267248` instead, and both are zero in the file (`KernelBase!0x18001ae30` to `KernelBase!0x18001ae37`). The record's first 32-bit value is `0x20` in the file.
- The source holds no unit from U+D800 to U+DFFF, unless the top byte of the record's field at `+4` is 1 (`KernelBase!0x18001a263` to `KernelBase!0x18001a2b4`).

Otherwise `LCMapStringEx` calls `KernelBase!0x180017ff0` and then a function that the returned object points to, with flag `0x01000000` flipped (`KernelBase!0x18001a1e0` to `KernelBase!0x18001a243`). Wine's `winnls.h` names that flag `LCMAP_LINGUISTIC_CASING`. This page does not record that path, or which user locales take it. So the result for a lone surrogate unit, and for a user locale whose record fails the checks, is not recorded.

### Where the case table comes from

- `KernelBase!0x1800fa638` sets the pointers at `KernelBase!0x1803548f0` and `KernelBase!0x1803548f8` to null (`KernelBase!0x1800fa657`). When the byte at `KernelBase!0x180355a18` is zero and the 16-bit field at offset `0x350` of the PEB is not zero, it calls `NtGetNlsSectionPtr` with section type 14 (`KernelBase!0x1800fa649` to `KernelBase!0x1800fa687`). On success it stores the section's address plus 4 at `KernelBase!0x1803548f0`. It stores the address plus 4 plus twice the section's second 16-bit word at `KernelBase!0x1803548f8` (`KernelBase!0x1800fa697` to `KernelBase!0x1800fa6b3`).
- Wine's [`include/winternl.h`](https://github.com/wine-mirror/wine/blob/master/include/winternl.h) names the 64-bit PEB's field at `0x350` `UseCaseMapping`. This page does not record what sets that field or the byte at `KernelBase!0x180355a18`. `KernelBase!0x1800fa49c` calls `KernelBase!0x1800fa638` (`KernelBase!0x1800fa4cc`). This page does not trace when that runs.
- `ntoskrnl.exe` maps NLS section type 14 to the file `\SystemRoot\System32\l_intl.nls` (`ntoskrnl!0x1408ea70a` to `ntoskrnl!0x1408ea71b`, `ntoskrnl!0x1408ea85d`). It maps the same type to the section name `\NLS\NlsSectionCASE` (`ntoskrnl!0x1408ec2b9` to `ntoskrnl!0x1408ec2d0`). That `NtGetNlsSectionPtr` reaches these two functions, and that the section starts at the file's first byte, is inferred. This page did not trace the path from the system call, and the layout below fits that reading.
- `Windows\System32\l_intl.nls` starts with the 16-bit words 1 and `0x9f1`. Read with the offsets above, the table at byte 4 maps `x` to `X`, `ä` to `Ä`, and `σ` to `Σ`. The table at byte `4 + 2 * 0x9f1` maps `X`, `Ä`, and `Σ` to `x`, `ä`, and `σ`.

### What it matches

A short Python reader applied the loop to every unit from U+0000 to U+FFFF, with the first table in `l_intl.nls`. This measurement assumes that `LCMapStringEx` takes the short path and that the table pointer is set.

- The loop changes 973 units. 26 of them are `a` to `z`, and 947 are other units. The table maps `a` to `z` to `A` to `Z` and changes no other unit below U+00C0, so the loop gives the table's result for every unit. The reader also compared the loop with the table for every unit, and they agree.
- Each upper case form maps to itself, and no two units map to the same other unit. So each changed unit matches its own upper case form in any case, which gives 973 pairs.
- With the table pointer null, the loop changes only `a` to `z`. Then only `A` to `Z` match `a` to `z` in any case (`KernelBase!0x18001a493` to `KernelBase!0x18001a4a3`, `KernelBase!0x18001b717` to `KernelBase!0x18001b71a`).

| Pair | Upper case of each unit | Match |
| --- | --- | --- |
| `X` (U+0058) with `x` (U+0078) | U+0058, U+0058 | Yes |
| `Ä` (U+00C4) with `ä` (U+00E4) | U+00C4, U+00C4 | Yes |
| `Σ` (U+03A3) with `σ` (U+03C3) | U+03A3, U+03A3 | Yes |
| `Σ` (U+03A3) with `ς` (U+03C2) | U+03A3, U+03C2 | No |
| `Ÿ` (U+0178) with `ÿ` (U+00FF) | U+0178, U+0178 | Yes |
| `ẞ` (U+1E9E) with `ß` (U+00DF) | U+1E9E, U+00DF | No |
| `I` (U+0049) with `ı` (U+0131) | U+0049, U+0131 | No |
| `İ` (U+0130) with `i` (U+0069) | U+0130, U+0049 | No |
| `K` (U+212A, Kelvin sign) with `k` (U+006B) | U+212A, U+004B | No |

A [Rust](https://www.rust-lang.org/) 1.99.1 program read the same map and compared it with a rule that matches two characters when they are equal or when `char::to_lowercase` gives the same text for both. Over every character from U+0000 to U+FFFF except U+D800 to U+DFFF:

- 973 pairs match under both rules.
- 215 pairs match only under the `to_lowercase` rule. Examples are `K` and `k` with the Kelvin sign U+212A, `ω` with the ohm sign U+2126, `ß` with `ẞ`, the title case letters such as `ǅ` (U+01C5) with `ǆ` (U+01C6), Georgian Mkhedruli with Mtavruli, such as U+10D0 with U+1C90, and Cherokee capitals with Cherokee small letters, such as U+13A0 with U+AB70.
- No pair matches only under the `l_intl.nls` map.
- 947 pairs match under the `l_intl.nls` map but not when only `A` to `Z` match `a` to `z`.

### A step for double-byte code pages

After two units match, `KernelBase!0x18001b890` moves to the next unit when the 32-bit value at `KernelBase!0x18035321c` is `0xfde9` (`KernelBase!0x18001b9e7`). Otherwise it reads the pointer at `KernelBase!0x180354930`. When that pointer is null, it calls `KernelBase!0x180017dfc` with 2 and moves on (`KernelBase!0x18001ba70` to `KernelBase!0x18001ba82`). `KernelBase!0x180017dfc` jumps through the import slot at `KernelBase!0x180260a10`, which holds `ntdll!RtlSetLastWin32Error`. That this call sets the last error to 2 is inferred from that import's name. Otherwise it reads a table pointer at offset `0x30` of that structure. A null table moves on too. With a table, it looks up the low byte of the pattern unit as written. A zero entry moves on. A non-zero entry requires the text unit to equal the pattern unit exactly, and the next text unit to equal the next pattern unit exactly, and then moves past both (`KernelBase!0x18001b9f3` to `KernelBase!0x18001ba3a`). In that case the two units match only in the same letter case.

That `0xfde9` is code page 65001 (UTF-8), and that the table marks the lead bytes of a double-byte ANSI code page, is inferred from the shape of the code. This page does not record what the structure and the table hold, which code page base WinPE uses, or which code pages give a non-zero entry.

### Files and tools

The files come from the `winpe.wim` in [Sources](#sources). `KernelBase.dll` is in the table in [Exports with code and forwarders](#exports-with-code-and-forwarders).

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `Windows\System32\shlwapi.dll` | 389,784 | `be1ed2d419ef9e0306b354be8900b4992a63fb60e395821b517174393632eb6b` |
| `Windows\System32\l_intl.nls` | 9,926 | `f8e9574c1c2ee28c669e903f5faf86ac24476f692076340b23881b4d0c82112b` |
| `Windows\System32\ntoskrnl.exe` | 12,543,280 | `1cc3b103b25333ee2b79363bb898832a93576befd2e3a29817bd34cf7a3a102b` |

7-Zip 23.01 took the files from the WIM. `rz-bin` 0.7.4 read the export and import tables. `rizin` 0.7.4 and GNU `objdump` 2.42 disassembled the code. `tools/re/winpe/reproduce.sh` writes the same map as the Python reader, unit for unit, with `tools/re/winpe/case_map.py`. It does not repeat the disassembly, and the Python reader and the Rust program are not in the repository.

## PathMatchSpecW with an empty pattern

MDT's Variable condition passes the variable's value to `PathMatchSpecW` as `pszFile` and the expanded Value as `pszSpec`. It stores the return value as the result (`TsCore!0x1801aa078`, `TsCore!0x1801aa07e`, see [Conditions](mdt-8450-binaries.md#conditions) in the MDT 8450 binaries reference). MDT checks for an empty Value only before the six comparisons, so Like reaches this call with an empty pattern. This section records what `PathMatchSpecW` in this image returns when the pattern is empty as a whole, and when one part between `;` characters is empty. It covers no other wildcard rule.

The image checked is the `winpe.wim` in [Sources](#sources), version 10.0.26100.1. Other Windows builds were not checked, and neither was a full Windows install.

### Null pointers and an empty pattern

- `PathMatchSpecW` (`KernelBase!0x1800d4dd0`) takes `pszFile` in `rcx` and `pszSpec` in `rdx`. It sets `ebx` to 0 at the start (`KernelBase!0x1800d4de9`) and returns `ebx` (`KernelBase!0x1800d4ed2`). `SHLWAPI.dll`'s `PathMatchSpecW` reaches this code, as [Where the function lives](#where-the-function-lives) says.
- A null `pszSpec` (`KernelBase!0x1800d4df1`) or a null `pszFile` (`KernelBase!0x1800d4dfa`) jumps to the return at `KernelBase!0x1800d4ecd` with `ebx` still 0. So a null pointer gives FALSE.
- Next, `KernelBase!0x1800d4e03` compares the first unit of `pszSpec` with `bx`, which is 0. When the pattern is empty, it jumps to `KernelBase!0x1800d4eeb`, which sets `ebx` to 1 and jumps to the return. So an empty `pszSpec` gives TRUE for any `pszFile` that is not null, the empty text included. The function does not read the text on this path.
- This check comes before the copy of the pattern and the cut at each `;` (`KernelBase!0x1800d4e0c` onward). So it covers only a pattern that is empty as a whole.

### An empty part after the cut

- After the cut, `PathMatchSpecW` passes each part with the text to the matcher `KernelBase!0x18001b890` (`KernelBase!0x1800d4eaa`). It stops at the first part that matches, and returns TRUE when a part matched (`KernelBase!0x1800d4eb1` to `KernelBase!0x1800d4ebe`, `KernelBase!0x1800d4ec8` to `KernelBase!0x1800d4eca`). A part that the cut leaves empty, as in the pattern `;` or `a;`, goes to the matcher like any other part.
- The matcher reads the first unit of the text and the first unit of the pattern. When both are 0, it returns TRUE (`KernelBase!0x18001b8cf` to `KernelBase!0x18001b8d7`, `KernelBase!0x18001ba4f`). When the text unit is not 0 and the pattern unit is 0, it returns FALSE (`KernelBase!0x18001b92b` to `KernelBase!0x18001b92e`, `KernelBase!0x18001b912`). So an empty part matches only empty text.

| `pszFile` | `pszSpec` | Result |
| --- | --- | --- |
| `abc` | empty | TRUE |
| empty | empty | TRUE |
| `abc` | `;` | FALSE |
| empty | `;` | TRUE |
| `abc` or empty | null | FALSE |

So `abc` does not match the pattern `;`, but it does match an empty pattern.

### What MDT passes

- MDT chooses `rdx` (`pszSpec`) as the address `rsp+0x1b0`, or as the pointer stored there when the 64-bit value at `rsp+0x1c8` is at least 8 (`TsCore!0x1801aa04c` to `TsCore!0x1801aa05d`). It chooses `rcx` (`pszFile`) the same way from `rsp+0x1d0` and `rsp+0x1e8` (`TsCore!0x1801aa03e`, `TsCore!0x1801aa066` to `TsCore!0x1801aa06f`).
- Neither pointer can be null. This is inferred from the shape of the code. The shape matches an MSVC `std::wstring`, which keeps up to 7 units and a terminator in an 8-unit buffer inside the object, and its capacity at offset `0x18`. With a capacity of at least 8, the object holds a pointer to a heap buffer instead. The function's cleanup frees the pointer at `rsp+0x1b0` only when the value at `rsp+0x1c8` is at least 8 (`TsCore!0x1801aa222` to `TsCore!0x1801aa235`). This page does not trace where the heap buffers come from.
- So on this image, MDT's Variable Like against a Value that expands to empty text is TRUE for every variable value. An unset variable is included, because the reference says that an unset variable and an empty one look the same ([Conditions](mdt-8450-binaries.md#conditions)). This result is inferred, because it rests on the pointers that cannot be null.

### Files and tools for the empty pattern

`KernelBase.dll` comes from the `winpe.wim` in [Sources](#sources), and it is in the table in [Exports with code and forwarders](#exports-with-code-and-forwarders). `TsCore.dll` is the x64 copy at `Microsoft Deployment Toolkit/Templates/Distribution/Tools/x64/TsCore.dll` in the MDT 8450 installer that [Sources](mdt-8450-binaries.md#sources) in the MDT 8450 binaries reference names. `msiextract` from msitools 0.103 unpacked it. It is 2,863,008 bytes, and its SHA-256 is `d0665be2469d52b2078c0153b82322da60fe98f0b5ef9912d01423901fda6bf6`.

7-Zip 23.01 took `KernelBase.dll` from the WIM. GNU `objdump` 2.42 disassembled the code of both files. `rz-bin` 0.7.4 read the import table of `TsCore.dll`, whose slot `TsCore!0x1802b5a08` imports `PathMatchSpecW` from `SHLWAPI.dll`. `tools/re/winpe/reproduce.sh` does not repeat the disassembly.

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
8. [Conditions](../architecture.md#conditions) says that the MDT 8450 binaries reference does not record which letters `PathMatchSpecW` matches in any case. Replace that with the result of [PathMatchSpecW letter case](#pathmatchspecw-letter-case) for this image, and cite MDT's call at `TsCore!0x1801aa078`. Say that the result rests on the short path of `LCMapStringEx` with the case table loaded, and that other Windows builds were not checked.
9. [#402](https://github.com/Tsukidy/deployarr/issues/402) asks the owner to choose the case rule of `Like`. Give the owner what each choice means on this image, from [What it matches](#what-it-matches). The counts hold only when `LCMapStringEx` takes its short path with the `l_intl.nls` case table loaded. With the table pointer null, `PathMatchSpecW` matches only `A` to `Z` with `a` to `z`. This page does not record whether base WinPE loads the table.
   - Keep folding by `char::to_lowercase`. `Like` then matches every pair that `PathMatchSpecW` matches here, and 215 more from U+0000 to U+FFFF, such as `k` with the Kelvin sign U+212A. `Like` compares `char` values, not UTF-16 units (`crates/deployarr-core/src/evaluate.rs`, `like` and `eq_ignore_case`). So `char::to_lowercase` also matches letters above U+FFFF, such as U+10400 with U+10428. This page does not record what `PathMatchSpecW` does with them. Conditions names that departure.
   - Match only `A` to `Z` with `a` to `z`, as the rule from `Equals` to `GreaterEqual` does. With the table loaded, `Like` then misses 947 pairs that `PathMatchSpecW` matches here, such as `Ä` with `ä` and `Σ` with `σ`, and Conditions names that departure. With the table pointer null, this choice matches what `PathMatchSpecW` matches.
   - Follow this image's case table. `deployarr-core` then carries the 973 changed units as data, and must still choose a rule for characters above U+FFFF and for the double-byte step, whose results this page does not record.
10. [Conditions](../architecture.md#conditions), in the MDT importer rules that the owner chose on [#340](https://github.com/Tsukidy/deployarr/issues/340#issuecomment-7015289761), says "The reference does not record what `PathMatchSpecW` does with an empty pattern, so MDT's result is not recorded." Replace that sentence with the result from [PathMatchSpecW with an empty pattern](#pathmatchspecw-with-an-empty-pattern). On this image, MDT's Variable Like against a Value that expands to empty text is TRUE for every variable value, an unset variable included. Cite that section and MDT's call at `TsCore!0x1801aa078`. Say that the result is inferred because it rests on pointers that cannot be null, and that other Windows builds were not checked.
11. In `deployarr-core`, a `Var` `Like` whose Value substitutes to empty text when the condition runs is true only for empty text. The `Var` evaluator's `Like` arm calls `like` with the substituted Value, and `like` returns true for an empty pattern only when the text is empty (`crates/deployarr-core/src/evaluate.rs`, `like`). Its unit test `like_backtracks_over_stars` asserts `!like("a", "")`. MDT gives TRUE for any text. [Conditions](../architecture.md#conditions) says only "`Like` against empty text is not an error" and names no result. Choose one of these:
   - Follow MDT. A `Var` `Like` whose Value substitutes to empty text then matches any text, the empty text included.
   - Keep the rule. Name it in Conditions as a departure from MDT, and add a `difference_` test for it in `crates/deployarr-core/tests/conditions.rs`.
12. The MDT importer rule from [#340](https://github.com/Tsukidy/deployarr/issues/340#issuecomment-7015289761) for a Variable Like with an empty `Value` sets `enabled` to false on the node, and its note says that MDT's result is not recorded. Four texts in [Conditions](../architecture.md#conditions) rest on that and become stale: the rule text "No known deployarr test gives MDT's result", the note text "For a Variable Like, the note also says that MDT's result for an empty Like pattern is not recorded", the reason "MDT's result for a Variable Like with an empty pattern is not recorded, so these rules name no stand-in for it", and the departure "For a Variable Like, MDT may run the step or group on some PCs, because its result is not recorded". Whether the rule itself changes is the owner's call. For example, MDT's result now matches a condition that is always true, such as a `Var` `Like` with the pattern `*`, which `like` makes true for any scalar or unset variable (`crates/deployarr-core/src/evaluate.rs`, `var` and `like`).
13. The [Conditions](mdt-8450-binaries.md#conditions) table row in `docs/research/mdt-8450-binaries.md` that starts "MDT reports a Variable comparison against an empty Value as an error" ends with "For Like, the note also says that MDT's result for an empty pattern is not recorded." Change that row to follow whatever the owner decides in item 12.
