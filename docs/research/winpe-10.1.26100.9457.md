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

For five more mechanisms, the architecture names no function. This page checked a usual function for each. The choice is this page's, not the architecture's:

| Mechanism | DLL | Functions |
| --- | --- | --- |
| `IOCTL_STORAGE_QUERY_PROPERTY` | `kernel32.dll` | `DeviceIoControl` |
| SetupAPI device enumeration | `setupapi.dll` | `SetupDiGetClassDevsW`, `SetupDiEnumDeviceInfo`, `SetupDiGetDeviceRegistryPropertyW` |
| Enabling `SeSystemEnvironmentPrivilege` | `advapi32.dll` | `OpenProcessToken`, `LookupPrivilegeValueW`, `AdjustTokenPrivileges` |
| Reading `UEFISecureBootEnabled` | `advapi32.dll` | `RegGetValueW` |
| Loading `tbs.dll` at run time | `kernel32.dll` | `LoadLibraryExW` |

A short reader of each DLL's export table checked these names. The rerun script does not repeat it. An export shows that a function exists, not that it works in WinPE. The M3 lab tests that.

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
