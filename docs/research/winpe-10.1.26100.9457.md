# WinPE add-on 10.1.26100.9457 reference

This page records which DLLs and API sets base WinPE x64 provides. The source is the Windows PE add-on for the Windows ADK 10.1.26100.9457. `deployarr-agent` runs in base WinPE, so a module it imports must be on this page. The CI check reads the same names from [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt). Use this page when you add an import to the agent.

## Sources

- The setup program is `adkwinpesetup.exe` from `https://go.microsoft.com/fwlink/?linkid=2289981`. The link redirects to `https://download.microsoft.com/download/a4a79e7a-f085-41c4-aebf-2538fd000790/adkwinpeaddons/adkwinpesetup.exe`. The file is 1,951,784 bytes, and its SHA-256 is `d4de67acc83de253ccc941dd0590808d83726bdd8ea768a374a7beaff7949d6a`. The server reported a last-modified date of 2026-09-22. Microsoft's ADK download page, `https://learn.microsoft.com/en-us/windows-hardware/get-started/adk-install`, lists it as the add-on for ADK 10.1.26100.9457 (September 2026), for Windows 11 24H2, 25H2, and 26H2.
- The setup program is a WiX Burn bundle. `7z` unpacked its manifest, whose `Registration` version is `10.1.26100.9457`. The manifest's `package_WindowsPEwimsDesktopEditions_x86_en_us` package holds `Windows PE wims (DesktopEditions)-x86_en-us.msi` and two CABs. The manifest gives no download URLs. Each payload downloads from the setup program's folder plus its `FilePath`, such as `Installers/a6296c5a6eec9ca02b431ed0461b952d.cab`. Each download's SHA-1 matched the manifest's `Hash`.
- The MSI's `File` table has two `winpe.wim` rows. The row installed under `Windows Preinstallation Environment\amd64\en-us` is in `a6296c5a6eec9ca02b431ed0461b952d.cab`, which is 338,097,556 bytes with SHA-1 `054936166468ba70637f181a5626cfb8ec62b4af`.
- The x64 `winpe.wim` is 340,134,390 bytes. Its SHA-256 is `fbcbdb1c6651ab3a69384e9d4f95f2c02321318603849453b252e21e827c8197`. Every finding on this page comes from this file, except the `WinPE-SecureStartup` package in [tbs.dll](#tbsdll).
- The WIM's XML describes one image, `Microsoft Windows PE (amd64)`, with edition `WindowsPE`, version 10.0.26100.1, and branch `ge_release`. The WIM was created on 2024-04-01. The September 2026 add-on therefore ships a 24H2 image with no cumulative update applied (inferred from the build number 26100.1 and the date).
- 7-Zip 23.01 read the CABs and the WIM. msitools 0.103 read the MSI tables.
- `tools/re/winpe/reproduce.sh` repeats the download, the checks, and the lists. See [WinPE in the agent guide](../../AGENTS.md#winpe). Two parsers written separately produced the same list file, byte for byte.

## Modules in System32

- The top level of `Windows\System32` holds 802 files that end in `.dll` and one that ends in `.drv`, `winspool.drv`. The list counts all 803 as modules. `winspool.drv` is the print spooler client, and the API set schema names it as the host of 3 API sets, so Windows loads it as a user-mode module.
- The list leaves out `ncpa.cpl` and `kmddsp.tsp`, which programs load by path rather than import (inferred from their roles as a Control Panel item and a TAPI provider). It also leaves out the 4 kernel-mode `.sys` files at the top of `System32` and every subfolder. `System32\downlevel` holds 112 more DLLs.
- [Module list](#module-list) names each module in the case the image stores it.

### tbs.dll

- The image has no `tbs.dll` anywhere. It does have the TPM driver, as `Windows\System32\drivers\tpm.sys` and `Windows\INF\tpm.inf`.
- `tbs.dll` ships in the `WinPE-SecureStartup` optional component. The add-on's amd64 `WinPE_OCs\WinPE-SecureStartup.cab` is 897,767 bytes, with SHA-256 `f8ae41b03360411752169f02cf29225e60a2212e5709c7572e5b6901efa03e74`. It holds `tbs.dll` and `tbs.sys` from `amd64_microsoft-windows-tpm-tbs_31bf3856ad364e35_10.0.26100.1_none_ebab7d55e0e0e8ac`.
- The package's `update.mum` names `WinPE-WMI-Package` in its `parent` element, so `WinPE-WMI` must be installed before `WinPE-SecureStartup`.
- That CAB comes from `9722214af0ab8aa9dffb6cfdafd937b7.cab` of `Windows PE Optional Packages (DesktopEditions)-x86_en-us.msi`. Its SHA-1, `f6d32f0081386aa9039c8ab4a9623f81b0686d63`, matched the manifest. The rerun script does not fetch the optional components.
- Base WinPE's `tpm.inf` binds `ACPI\MSFT0101` and `*MSFT0101` to "Trusted Platform Module 2.0" (`tpm.inf:48-52`, `tpm.inf:215`). It binds `ACPI\PNP0C31` and `*PNP0C31` to "Trusted Platform Module 1.2" (`tpm.inf:36-38`, `tpm.inf:209`).

### Functions the agent names

Each DLL that the architecture's [Agent](../architecture.md#agent) section relies on is present, and exports each function by name:

| DLL | Exported functions |
| --- | --- |
| `kernel32.dll` | `GetSystemFirmwareTable`, `GetFirmwareType`, `GetFirmwareEnvironmentVariableW`, `GetPhysicallyInstalledSystemMemory`, `GetLogicalProcessorInformationEx`, `SetThreadExecutionState`, `DeviceIoControl`, `LoadLibraryExW` |
| `IPHLPAPI.DLL` | `GetAdaptersAddresses` |
| `setupapi.dll` | `SetupDiGetClassDevsW`, `SetupDiEnumDeviceInfo`, `SetupDiGetDeviceRegistryPropertyW` |
| `cfgmgr32.dll` | `CM_Get_DevNode_Status` |
| `advapi32.dll` | `OpenProcessToken`, `LookupPrivilegeValueW`, `AdjustTokenPrivileges`, `RegGetValueW` |

A short reader of each DLL's export table checked these names. The rerun script does not repeat it. An export shows that a function exists, not that it works in WinPE. The M3 lab tests that.

## API sets

- `Windows\System32\apisetschema.dll` holds API set schema version 6 in its `.apiset` section. The schema has 974 entries. 363 names start with `api-` and 611 start with `ext-`.
- 577 entries resolve, because their default host is a module in the list. 316 of them are `api-` sets and 261 are `ext-` sets. [API set list](#api-set-list) names each one.
- 397 entries do not resolve. 135 have no host. 44 have a kernel-mode `.sys` host, such as `win32kfull.sys`. 218 name one of 153 DLLs that `System32` lacks, such as `comctl32.dll` and `twinapi.appcore.dll`. The image keeps `comctl32.dll` only under `WinSxS`.
- Six entries map to a different host for one importing module. `api-ms-win-core-io-l1-1-1` maps to `kernel32.dll`, except for imports from `kernel32.dll` itself, which map to `kernelbase.dll`. The list follows the default host.
- Each entry stores a full versioned name, such as `api-ms-win-core-synch-l1-2-1`. In all 974 entries, the hashed length covers the name only up to its last hyphen, as in `api-ms-win-core-synch-l1-2`. The loader therefore ignores the last number when it matches an import, so an import of `api-ms-win-core-synch-l1-2-0.dll` resolves through the entry stored as `api-ms-win-core-synch-l1-2-1` (inferred from the hashed length). Microsoft's API set pages do not state this rule.

## The list file

- [winpe-10.1.26100.9457-x64-dlls.txt](winpe-10.1.26100.9457-x64-dlls.txt) holds 1,380 names, one per line, in lower case and sorted. They are the 803 modules and the 577 resolvable API sets, each API set with `.dll` added.
- A check that compares names exactly rejects an API set import whose last number differs from the stored name. For a name that starts with `api-` or `ext-`, compare the part before the last hyphen.
- The list covers the image as Microsoft ships it. A boot image built from another add-on, or serviced with a cumulative update, needs its own page.

## Design changes

1. The [Agent](../architecture.md#agent) section says it is not yet checked whether base WinPE includes `tbs.dll`. It does not. Replace that sentence with this result, and drop the `tbs.dll` half of the M3 check in [roadmap](../roadmap.md#m3-bare-metal-deployment-from-boot-media).
2. Without `tbs.dll`, `Tbsi_GetDeviceInfo` cannot run in base WinPE, and the TPM version is unknown. The [readiness check](../architecture.md#readiness-check) fails a fact that it cannot read, and its TPM 2.0 check fails by default. In base WinPE, every PC would stop at the readiness check unless its task sequence turns the TPM check off. Choose one of these:
   - Read the TPM version from the TPM device's hardware ID, which the agent already collects with SetupAPI. `ACPI\MSFT0101` means TPM 2.0 and `ACPI\PNP0C31` means TPM 1.2, by base WinPE's own `tpm.inf`. Keep `Tbsi_GetDeviceInfo` for a boot image that has `tbs.dll`. Whether every firmware TPM reports one of these IDs is not checked. The M3 check on the UEFI VM and the real PC can confirm it.
   - Add `WinPE-WMI` and `WinPE-SecureStartup` to the boot image. Only DISM can add them, so a Linux server needs a base image serviced on Windows, as the PowerShell components do. See [Boot image](../architecture.md#boot-image).
3. Load `tbs.dll` with `LoadLibraryExW`, not through the import table or the delay-load import table. The CI check reads both tables, so an import of `tbs.dll` in either one fails it.
4. The CI check compares an API set import on the part before its last hyphen, as [API sets](#api-sets) explains.

## Module list

The 803 modules at the top of `Windows\System32`, in stored case:

```text
4545ffe2-0dc4-4df4-9d02-299ef204635e_hvsocket.dll
69fe178f-26e7-43a9-aa7d-2b616b672dde_eventlogservice.dll
6bea57fb-8dfb-4177-9ae8-42e8b3529933_RuntimeDeviceInstall.dll aclui.dll activeds.dll adsldpc.dll
adtschema.dll advapi32.dll advapi32res.dll advpack.dll aepic.dll apisetschema.dll Apx01000.dll
asycfilt.dll atl.dll atmlib.dll authui.dll authz.dll basesrv.dll bcd.dll bcdsrv.dll BCP47Langs.dll
BCP47mrm.dll bcrypt.dll bcryptprimitives.dll BFE.DLL bootstr.dll bootsvc.dll BOOTVID.DLL
BrokerLib.dll browcli.dll browseui.dll C_G18030.DLL c_GSM7.DLL C_IS2022.DLL C_ISCII.DLL cabapi.dll
cabinet.dll capisp.dll cdd.dll certca.dll certcli.dll cfgmgr32.dll cfmifs.dll cfmifsproxy.dll ci.dll
cimfs.dll clusapi.dll cmdext.dll cmifw.dll cnvfat.dll combase.dll comcat.dll comdlg32.dll coml2.dll
comres.dll ConhostV1.dll console.dll ConsoleLogon.dll CoreMessaging.dll CorePrivacySettingsStore.dll
CoreUIComponents.dll CredProv2faHelper.dll CredProvCommonCore.dll CredProvDataModel.dll
credprovhost.dll credprovs.dll credprovslegacy.dll credui.dll crypt32.dll cryptbase.dll
cryptcatsvc.dll cryptdlg.dll cryptdll.dll cryptext.dll cryptnet.dll cryptoss.dll cryptsp.dll
cryptsvc.dll cryptui.dll cryptxml.dll csrsrv.dll CSystemEventsBrokerClient.dll dab.dll dabapi.dll
davhlpr.dll dbgcore.dll dbgeng.dll dbghelp.dll DbgModel.dll dciman32.dll dcntel.dll defragproxy.dll
defragres.dll defragsvc.dll devicengccredprov.dll DeviceUpdateAgent.dll devobj.dll devrtl.dll
dfscli.dll dhcpcmonitor.dll dhcpcore.dll dhcpcore6.dll dhcpcsvc.dll dhcpcsvc6.dll
diagnosticdataquery.dll DiagnosticDataSettings.dll diagtrack.dll difxapi.dll directmanipulation.dll
DismApi.dll dnsapi.dll dnsrslvr.dll dpapi.dll dpapisrv.dll dpx.dll drvsetup.dll drvstore.dll
dsparse.dll dsreg.dll dsrole.dll dssenh.dll dui70.dll duser.dll dwmapi.dll DWrite.dll dxgwdi.dll
eapp3hst.dll eappcfg.dll eappcfgui.dll eappgnui.dll eapphost.dll eappprxy.dll eapprovp.dll
eapputil.dll eapsvc.dll EapTeapAuth.dll EapTeapConfig.dll edgeIso.dll efscore.dll efslsaext.dll
efssvc.dll efsutil.dll esent.dll esevss.dll ETWESEProviderResources.dll EventAggregation.dll
eventcls.dll f3ahvoas.dll Faultrep.dll fbwflib.dll fcon.dll feclient.dll FirewallAPI.dll
FlightSettings.dll fltLib.dll fltmgrres.dll fmifs.dll fms.dll fontsub.dll fsutilext.dll fveapi.dll
fveapibase.dll fvecerts.dll fwbase.dll fwpolicyiomgr.dll FWPUCLNT.DLL FwRemoteSvr.dll gdi32.dll
gdi32full.dll GdiPlus.dll gmsaclient.dll gpapi.dll gpsvc.dll hal.dll HalExtIntcLpioDMA.dll
HalExtIntcPseDMA.dll HalExtPL080.dll hid.dll hidserv.dll httpapi.dll HvSocket.dll icfupgd.dll
icmp.dll iertutil.dll ifmon.dll ifsutil.dll ifsutilx.dll IKEEXT.DLL imagehlp.dll imageres.dll
imagesp1.dll imapi.dll imapi2.dll imapi2fs.dll imm32.dll inetmib1.dll input.dll IPHLPAPI.DLL
IPSECSVC.DLL iscsilog.dll joinproviderol.dll joinutil.dll jsproxy.dll kbd101.dll kbd101a.dll
kbd101b.dll kbd101c.dll kbd103.dll kbd106.dll kbd106n.dll KBDA1.DLL KBDA2.DLL KBDA3.DLL KBDADLM.DLL
KBDAL.DLL KBDARME.DLL kbdarmph.dll kbdarmty.dll KBDARMW.DLL kbdax2.dll KBDAZE.DLL KBDAZEL.DLL
KBDAZST.DLL KBDBASH.DLL KBDBE.DLL KBDBENE.DLL KBDBGPH.DLL KBDBGPH1.DLL KBDBHC.DLL KBDBLR.DLL
KBDBR.DLL KBDBU.DLL KBDBUG.DLL KBDBULG.DLL KBDCA.DLL KBDCAN.DLL KBDCHER.DLL KBDCHERP.DLL KBDCMK.DLL
KBDCR.DLL KBDCZ.DLL KBDCZ1.DLL KBDCZ2.DLL KBDDA.DLL KBDDIV1.DLL KBDDIV2.DLL KBDDV.DLL KBDDZO.DLL
KBDES.DLL KBDEST.DLL KBDFA.DLL kbdfar.dll KBDFC.DLL KBDFI.DLL KBDFI1.DLL KBDFO.DLL KBDFR.DLL
KBDFRNA.DLL KBDFRNB.DLL KBDFTHRK.DLL KBDGAE.DLL KBDGEO.DLL kbdgeoer.dll kbdgeome.dll kbdgeooa.dll
kbdgeoqw.dll KBDGKL.DLL KBDGN.DLL KBDGR.DLL KBDGR1.DLL KBDGRE1.DLL KBDGRE2.DLL KBDGRLND.DLL
KBDGTHC.DLL KBDHAU.DLL KBDHAW.DLL KBDHE.DLL KBDHE220.DLL KBDHE319.DLL KBDHEB.DLL kbdhebl3.dll
kbdhebsi.dll KBDHELA2.DLL KBDHELA3.DLL KBDHEPT.DLL KBDHU.DLL KBDHU1.DLL kbdibm02.dll KBDIBO.DLL
KBDIC.DLL KBDINASA.DLL KBDINBE1.DLL KBDINBE2.DLL KBDINBEN.DLL KBDINDEV.DLL KBDINEN.DLL KBDINGUJ.DLL
KBDINHIN.DLL KBDINKAN.DLL KBDINMAL.DLL KBDINMAR.DLL KBDINORI.DLL KBDINPUN.DLL KBDINTAM.DLL
KBDINTEL.DLL KBDINUK2.DLL KBDIR.DLL KBDIT.DLL KBDIT142.DLL KBDIULAT.DLL KBDJAV.DLL KBDJPN.DLL
KBDKAZ.DLL KBDKHMR.DLL KBDKNI.DLL KBDKOR.DLL KBDKURD.DLL KBDKYR.DLL KBDLA.DLL KBDLAO.DLL
kbdlisub.dll kbdlisus.dll kbdlk41a.dll KBDLT.DLL KBDLT1.DLL KBDLT2.DLL KBDLV.DLL KBDLV1.DLL
KBDLVST.DLL KBDMAC.DLL KBDMACST.DLL KBDMAORI.DLL KBDMLT47.DLL KBDMLT48.DLL KBDMON.DLL KBDMONMO.DLL
KBDMONST.DLL KBDMYAN.DLL KBDNE.DLL kbdnec.dll kbdnec95.dll kbdnecat.dll kbdnecnt.dll KBDNEPR.DLL
kbdnko.dll KBDNO.DLL KBDNO1.DLL KBDNSO.DLL KBDNTL.DLL KBDOGHAM.DLL KBDOLCH.DLL KBDOLDIT.DLL
KBDOSA.DLL KBDOSM.DLL KBDPASH.DLL kbdphags.dll KBDPL.DLL KBDPL1.DLL KBDPO.DLL KBDRO.DLL KBDROPR.DLL
KBDROST.DLL KBDRU.DLL KBDRU1.DLL KBDRUM.DLL KBDSF.DLL KBDSG.DLL KBDSL.DLL KBDSL1.DLL KBDSMSFI.DLL
KBDSMSNO.DLL KBDSN1.DLL KBDSORA.DLL KBDSOREX.DLL KBDSORS1.DLL KBDSORST.DLL KBDSP.DLL KBDSW.DLL
KBDSW09.DLL KBDSYR1.DLL KBDSYR2.DLL KBDTAILE.DLL KBDTAJIK.DLL KBDTAM99.DLL KBDTAT.DLL KBDTH0.DLL
KBDTH1.DLL KBDTH2.DLL KBDTH3.DLL KBDTIFI.DLL KBDTIFI2.DLL KBDTIPRC.DLL KBDTIPRD.DLL KBDTT102.DLL
KBDTUF.DLL KBDTUQ.DLL KBDTURME.DLL KBDTZM.DLL KBDUGHR.DLL KBDUGHR1.DLL KBDUK.DLL KBDUKX.DLL
KBDUR.DLL KBDUR1.DLL KBDURDU.DLL KBDUS.DLL KBDUSA.DLL KBDUSL.DLL KBDUSR.DLL KBDUSX.DLL KBDUZB.DLL
KBDVNTC.DLL KBDWOL.DLL KBDYAK.DLL KBDYBA.DLL KBDYCC.DLL KBDYCL.DLL kd.dll kd_02_10df.dll
kd_02_10ec.dll kd_02_1137.dll kd_02_1414.dll kd_02_14e4.dll kd_02_15ad.dll kd_02_15b3.dll
kd_02_1969.dll kd_02_19a2.dll kd_02_1af4.dll kd_02_8086.dll kd_07_1415.dll kd_0C_8086.dll kdcom.dll
kdnet.dll kdnet_uart16550.dll KdsCli.dll kdstub.dll kdusb.dll kerb3961.dll KerbClientShared.dll
kerberos.dll kernel.appcore.dll kernel32.dll KernelBase.dll KeyCredMgr.dll keyiso.dll ktmw32.dll
linkinfo.dll lmhsvc.dll loadperf.dll localkdcsvc.dll logoncli.dll LogonController.dll lpk.dll
lsaadt.dll lsasrv.dll lsm.dll lz32.dll mapi32.dll mapistub.dll mcupdate_AuthenticAMD.dll
mcupdate_GenuineIntel.dll mf3216.dll mfc42.dll mfc42u.dll microsoft-windows-battery-events.dll
microsoft-windows-hal-events.dll microsoft-windows-kernel-pnp-events.dll
microsoft-windows-kernel-power-events.dll microsoft-windows-kernel-processor-power-events.dll
microsoft-windows-pdc.dll microsoft-windows-sac-events.dll microsoft-windows-sleepstudy-events.dll
microsoft-windows-storage-tiering-events.dll microsoft-windows-system-events.dll mispace.dll mpr.dll
mprapi.dll mprext.dll mprmsg.dll MPSSVC.dll msafd.dll msasn1.dll msaudite.dll mscat32.dll msctf.dll
msdelta.dll msftedit.dll msiltcfg.dll msimg32.dll msIso.dll mskeyprotect.dll msls31.dll msobjs.dll
mspatcha.dll mspatchc.dll msports.dll msprivs.dll mssign32.dll mssip32.dll msv1_0.dll msvcirt.dll
msvcp110_win.dll msvcp60.dll msvcp_win.dll msvcrt.dll mswsock.dll msxml3.dll msxml3r.dll msxml6.dll
msxml6r.dll mtxdm.dll muifontsetup.dll nci.dll ncrypt.dll ncryptprov.dll ncryptsslp.dll ncsi.dll
netapi32.dll netbios.dll netcfgx.dll NetDriverInstall.dll neth.dll netiohlp.dll netjoin.dll
netlogon.dll netmsg.dll netprofmsvc.dll netprovfw.dll NetSetupApi.dll NetSetupEngine.dll
NetSetupShim.dll NetSetupSvc.dll netshell.dll netutils.dll newdev.dll ngclocal.dll ninput.dll
nlaapi.dll nlmproxy.dll nlmsprep.dll normaliz.dll nrpsrv.dll nrtapi.dll nshdnsclient.dll nshwfp.dll
nsi.dll nsisvc.dll ntasn1.dll ntdll.dll ntdsapi.dll ntfsres.dll ntlanman.dll NtlmShared.dll
ntmarta.dll ntprint.dll ntshrui.dll odbc32.dll odbccp32.dll odbcint.dll odbctrac.dll offlinelsa.dll
offlinesam.dll ole32.dll oleacc.dll oleacchooks.dll oleaccrc.dll oleaut32.dll
OneCoreCommonProxyStub.dll OneSettingsClient.dll osuninst.dll pcwum.dll perfdisk.dll perfnet.dll
perfos.dll perfproc.dll plutonfw_AuthenticAMD.dll pnpclean.dll pnpdiag.dll pnppolicy.dll pnpui.dll
polstore.dll powrprof.dll prflbmsg.dll printui.dll profapi.dll profext.dll profsvc.dll propsys.dll
prxyqry.dll psapi.dll PSHED.DLL rasadhlp.dll rasapi32.dll rasauto.dll raschap.dll raschapext.dll
rasctrs.dll rasdiag.dll rasdlg.dll rasman.dll rasmans.dll rasmbmgr.dll rasmontr.dll rasppp.dll
rastapi.dll rastls.dll rastlsext.dll RefsDedupSvc.Proxy.dll regapi.dll resutils.dll riched20.dll
riched32.dll RpcEpMap.dll rpcrt4.dll RpcRtRemote.dll rpcss.dll rsaenh.dll rtutils.dll sacsvr.dll
samcli.dll samlib.dll samsrv.dll scecli.dll scesrv.dll schannel.dll schedcli.dll sdhcinst.dll
sechost.dll secur32.dll security.dll SensApi.dll ServicingCommon.dll setbcdlocale.dll setupapi.dll
setupetw.dll sfc.dll sfc_os.dll SHCore.dll shell32.dll shlwapi.dll shutdownux.dll slc.dll
smbwmiv2.dll SmiEngine.dll smphost.dll snmpapi.dll softpub.dll spfileq.dll spinf.dll sppc.dll
sppnp.dll sprefs.dll srvcli.dll srvsvc.dll sscore.dll sspicli.dll sspisrv.dll SSShim.dll sstpcfg.dll
sstpsvc.dll Storprop.dll streamci.dll svsvc.dll swprv.dll sxs.dll sxshared.dll sxssrv.dll
symcryptk.dll sysclass.dll sysntfy.dll syssetup.dll SystemEventsBrokerServer.dll tapi32.dll
tcpipcfg.dll tdh.dll tdhres.dll TextInputFramework.dll TextShaping.dll TimeBrokerClient.dll
TimeBrokerServer.dll tokenbinding.dll TrustedSignalCredProv.dll TSSessionUX.dll TtlsAuth.dll
TtlsCfg.dll tzres.dll ucrtbase.dll uexfat.dll ufat.dll UIAutomationCore.dll uiomapapi.dll ulib.dll
umpdc.dll umpnpmgr.dll umpo.dll unattend.dll UnionFSApi.dll untfs.dll UpdateCompression.dll
uReFS.dll uReFSv1.dll ureg.dll urlmon.dll user32.dll userenv.dll userinitext.dll usermgr.dll
usermgrcli.dll UserMgrProxy.dll usp10.dll utcapi.dll utcutil.dll utildll.dll uudf.dll UXInit.dll
uxtheme.dll vds_ps.dll vdsbas.dll vdsdyn.dll vdsutil.dll vdsvd.dll verifier.dll version.dll
VhfUm.dll virtdisk.dll vmbuspipe.dll vpnike.dll vpnikeapi.dll vss_ps.dll vssapi.dll vsstrace.dll
w32time.dll w32topl.dll wdigest.dll wdscore.dll webio.dll webservices.dll Websocket.dll wer.dll
werdiagcontroller.dll WerEnc.dll weretw.dll wersvc.dll wevtapi.dll wevtsvc.dll wfapigp.dll
wimgapi.dll win32u.dll winbio.dll winbrand.dll wincorlib.dll wincredui.dll
Windows.Devices.HumanInterfaceDevice.dll Windows.FileExplorer.Common.dll
Windows.Internal.UI.Logon.ProxyStub.dll windows.storage.dll Windows.Storage.OneCore.dll
Windows.System.RemoteDesktop.dll Windows.UI.CredDialogController.dll WindowsCodecs.dll
windowsperformancerecordercontrol.dll winhttp.dll winhttpcom.dll wininet.dll wininitext.dll
winipsec.dll winlogonext.dll winmm.dll winmmbase.dll winnlsres.dll winnsi.dll WinSCard.dll
winsku.dll winspool.drv winsqlite3.dll winsrv.dll winsrvext.dll winsta.dll wintrust.dll WinTypes.dll
wkscli.dll wkssvc.dll wlanutil.dll Wldap32.dll wldp.dll wmi.dll wmiclnt.dll wmiprop.dll wmsgapi.dll
WofUtil.dll wosc.dll wpeutil.dll wpx.dll ws2_32.dll ws2help.dll wshelper.dll wshhyperv.dll
wship6.dll WSHTCPIP.DLL wsock32.dll wtsapi32.dll xmllite.dll
```

## API set list

The 577 API sets whose default host is in the module list, as the schema stores them:

```text
api-ms-win-appmodel-advertisingid-l1-1-0 api-ms-win-appmodel-identity-l1-2-0
api-ms-win-appmodel-runtime-internal-l1-1-11 api-ms-win-appmodel-runtime-l1-1-7
api-ms-win-appmodel-state-l1-1-2 api-ms-win-appmodel-state-l1-2-0 api-ms-win-appmodel-unlock-l1-1-0
api-ms-win-base-bootconfig-l1-1-0 api-ms-win-base-util-l1-1-0 api-ms-win-core-apiquery-l1-1-1
api-ms-win-core-apiquery-l2-1-0 api-ms-win-core-appcompat-l1-1-1 api-ms-win-core-appinit-l1-1-0
api-ms-win-core-atoms-l1-1-0 api-ms-win-core-backgroundtask-l1-1-0 api-ms-win-core-calendar-l1-1-0
api-ms-win-core-com-l1-1-3 api-ms-win-core-com-l2-1-1 api-ms-win-core-com-midlproxystub-l1-1-0
api-ms-win-core-com-private-l1-1-1 api-ms-win-core-com-private-l1-2-0
api-ms-win-core-com-private-l1-3-1 api-ms-win-core-comm-l1-1-2
api-ms-win-core-commandlinetoargv-l1-1-0 api-ms-win-core-console-ansi-l2-1-0
api-ms-win-core-console-internal-l1-1-0 api-ms-win-core-console-l1-1-0
api-ms-win-core-console-l1-2-2 api-ms-win-core-console-l2-1-0 api-ms-win-core-console-l2-2-0
api-ms-win-core-console-l3-1-0 api-ms-win-core-console-l3-2-0 api-ms-win-core-crt-l1-1-0
api-ms-win-core-crt-l2-1-0 api-ms-win-core-datetime-l1-1-2 api-ms-win-core-debug-l1-1-2
api-ms-win-core-debug-minidump-l1-1-0 api-ms-win-core-delayload-l1-1-1
api-ms-win-core-enclave-l1-1-1 api-ms-win-core-errorhandling-l1-1-3
api-ms-win-core-featurestaging-l1-1-1 api-ms-win-core-featuretoggles-l1-1-0
api-ms-win-core-fibers-l1-1-2 api-ms-win-core-fibers-l2-1-1 api-ms-win-core-file-ansi-l1-1-0
api-ms-win-core-file-ansi-l2-1-0 api-ms-win-core-file-fromapp-l1-1-0 api-ms-win-core-file-l1-1-1
api-ms-win-core-file-l1-2-4 api-ms-win-core-file-l2-1-4 api-ms-win-core-firmware-l1-1-0
api-ms-win-core-guard-l1-1-0 api-ms-win-core-handle-l1-1-0 api-ms-win-core-heap-l1-1-0
api-ms-win-core-heap-l1-2-0 api-ms-win-core-heap-l2-1-0 api-ms-win-core-heap-obsolete-l1-1-0
api-ms-win-core-interlocked-l1-1-1 api-ms-win-core-interlocked-l1-2-0 api-ms-win-core-io-l1-1-1
api-ms-win-core-ioring-l1-1-2 api-ms-win-core-job-l1-1-0 api-ms-win-core-job-l2-1-1
api-ms-win-core-kernel32-legacy-ansi-l1-1-0 api-ms-win-core-kernel32-legacy-l1-1-6
api-ms-win-core-kernel32-private-l1-1-2 api-ms-win-core-kernel32-private-l1-2-0
api-ms-win-core-largeinteger-l1-1-0 api-ms-win-core-libraryloader-l1-1-1
api-ms-win-core-libraryloader-l1-2-3 api-ms-win-core-libraryloader-l2-1-0
api-ms-win-core-libraryloader-private-l1-1-0 api-ms-win-core-localization-ansi-l1-1-0
api-ms-win-core-localization-l1-1-0 api-ms-win-core-localization-l1-2-4
api-ms-win-core-localization-l2-1-0 api-ms-win-core-localization-obsolete-l1-1-0
api-ms-win-core-localization-obsolete-l1-2-0 api-ms-win-core-localization-obsolete-l1-3-0
api-ms-win-core-localization-private-l1-1-0 api-ms-win-core-localregistry-l1-1-0
api-ms-win-core-marshal-l1-1-0 api-ms-win-core-memory-l1-1-9 api-ms-win-core-misc-l1-1-0
api-ms-win-core-multipleproviderrouter-l1-1-0 api-ms-win-core-namedpipe-ansi-l1-1-1
api-ms-win-core-namedpipe-l1-1-0 api-ms-win-core-namedpipe-l1-2-2
api-ms-win-core-namespace-ansi-l1-1-0 api-ms-win-core-namespace-l1-1-0
api-ms-win-core-normalization-l1-1-0 api-ms-win-core-path-l1-1-0 api-ms-win-core-pcw-l1-1-0
api-ms-win-core-perfcounters-l1-1-0 api-ms-win-core-perfcounters-l1-2-0
api-ms-win-core-privateprofile-l1-1-1 api-ms-win-core-processenvironment-ansi-l1-1-0
api-ms-win-core-processenvironment-l1-1-1 api-ms-win-core-processenvironment-l1-2-0
api-ms-win-core-processsecurity-l1-1-0 api-ms-win-core-processsnapshot-l1-1-0
api-ms-win-core-processthreads-l1-1-8 api-ms-win-core-processtopology-l1-1-0
api-ms-win-core-processtopology-l1-2-0 api-ms-win-core-processtopology-obsolete-l1-1-1
api-ms-win-core-processtopology-private-l1-1-0 api-ms-win-core-profile-l1-1-0
api-ms-win-core-psapi-ansi-l1-1-0 api-ms-win-core-psapi-l1-1-0 api-ms-win-core-psapi-obsolete-l1-1-0
api-ms-win-core-psapiansi-l1-1-0 api-ms-win-core-psm-key-l1-1-3 api-ms-win-core-quirks-l1-1-1
api-ms-win-core-realtime-l1-1-2 api-ms-win-core-registry-l1-1-2 api-ms-win-core-registry-l2-1-0
api-ms-win-core-registry-l2-2-0 api-ms-win-core-registry-l2-3-0
api-ms-win-core-registry-private-l1-1-0 api-ms-win-core-registryuserspecific-l1-1-0
api-ms-win-core-rtlsupport-l1-1-1 api-ms-win-core-rtlsupport-l1-2-2
api-ms-win-core-shlwapi-legacy-l1-1-0 api-ms-win-core-shlwapi-obsolete-l1-1-0
api-ms-win-core-shlwapi-obsolete-l1-2-0 api-ms-win-core-shutdown-ansi-l1-1-0
api-ms-win-core-shutdown-l1-1-1 api-ms-win-core-sidebyside-ansi-l1-1-0
api-ms-win-core-sidebyside-l1-1-0 api-ms-win-core-state-helpers-l1-1-0 api-ms-win-core-string-l1-1-0
api-ms-win-core-string-l2-1-1 api-ms-win-core-string-obsolete-l1-1-1
api-ms-win-core-stringansi-l1-1-0 api-ms-win-core-stringloader-l1-1-1
api-ms-win-core-synch-ansi-l1-1-0 api-ms-win-core-synch-l1-1-1 api-ms-win-core-synch-l1-2-1
api-ms-win-core-sysinfo-l1-1-1 api-ms-win-core-sysinfo-l1-2-7 api-ms-win-core-sysinfo-l2-1-0
api-ms-win-core-systemtopology-l1-1-2 api-ms-win-core-textinput-client-l1-1-1
api-ms-win-core-textinput-client-l1-2-0 api-ms-win-core-threadpool-l1-1-0
api-ms-win-core-threadpool-l1-2-0 api-ms-win-core-threadpool-legacy-l1-1-0
api-ms-win-core-threadpool-private-l1-1-0 api-ms-win-core-timezone-l1-1-1
api-ms-win-core-timezone-private-l1-1-0 api-ms-win-core-toolhelp-l1-1-1 api-ms-win-core-ums-l1-1-0
api-ms-win-core-url-l1-1-0 api-ms-win-core-util-l1-1-1 api-ms-win-core-version-l1-1-1
api-ms-win-core-version-private-l1-1-0 api-ms-win-core-versionansi-l1-1-1
api-ms-win-core-windowsceip-l1-1-0 api-ms-win-core-windowserrorreporting-l1-1-3
api-ms-win-core-winrt-error-l1-1-1 api-ms-win-core-winrt-errorprivate-l1-1-1
api-ms-win-core-winrt-l1-1-0 api-ms-win-core-winrt-propertysetprivate-l1-1-1
api-ms-win-core-winrt-registration-l1-1-0 api-ms-win-core-winrt-robuffer-l1-1-0
api-ms-win-core-winrt-roparameterizediid-l1-1-0 api-ms-win-core-winrt-string-l1-1-1
api-ms-win-core-wow64-l1-1-3 api-ms-win-core-xstate-l1-1-3 api-ms-win-core-xstate-l2-1-2
api-ms-win-crt-conio-l1-1-0 api-ms-win-crt-convert-l1-1-0 api-ms-win-crt-environment-l1-1-0
api-ms-win-crt-filesystem-l1-1-0 api-ms-win-crt-heap-l1-1-0 api-ms-win-crt-locale-l1-1-0
api-ms-win-crt-math-l1-1-0 api-ms-win-crt-multibyte-l1-1-0 api-ms-win-crt-private-l1-1-0
api-ms-win-crt-process-l1-1-0 api-ms-win-crt-runtime-l1-1-0 api-ms-win-crt-stdio-l1-1-0
api-ms-win-crt-string-l1-1-0 api-ms-win-crt-time-l1-1-0 api-ms-win-crt-utility-l1-1-0
api-ms-win-deprecated-apis-obsolete-l1-1-0 api-ms-win-devices-config-l1-1-2
api-ms-win-devices-query-l1-1-1 api-ms-win-devices-swdevice-l1-1-1
api-ms-win-downlevel-advapi32-l1-1-0 api-ms-win-downlevel-advapi32-l2-1-0
api-ms-win-downlevel-advapi32-l3-1-0 api-ms-win-downlevel-advapi32-l4-1-0
api-ms-win-downlevel-kernel32-l1-1-0 api-ms-win-downlevel-kernel32-l2-1-0
api-ms-win-downlevel-normaliz-l1-1-0 api-ms-win-downlevel-ole32-l1-1-0
api-ms-win-downlevel-shell32-l1-1-0 api-ms-win-downlevel-shlwapi-l1-1-0
api-ms-win-downlevel-shlwapi-l2-1-0 api-ms-win-downlevel-user32-l1-1-0
api-ms-win-downlevel-version-l1-1-0 api-ms-win-dwmapi-l1-1-0 api-ms-win-dx-d3dkmt-l1-1-8
api-ms-win-eventing-classicprovider-l1-1-0 api-ms-win-eventing-consumer-l1-1-2
api-ms-win-eventing-controller-l1-1-0 api-ms-win-eventing-legacy-l1-1-0
api-ms-win-eventing-obsolete-l1-1-0 api-ms-win-eventing-provider-l1-1-0
api-ms-win-eventing-tdh-l1-1-2 api-ms-win-eventlog-legacy-l1-1-0
api-ms-win-gaming-deviceinformation-l1-1-0 api-ms-win-gdi-dpiinfo-l1-1-0
api-ms-win-gdi-internal-uap-l1-1-0 api-ms-win-http-time-l1-1-0 api-ms-win-legacy-shlwapi-l1-1-0
api-ms-win-mm-joystick-l1-1-0 api-ms-win-mm-mci-l1-1-0 api-ms-win-mm-misc-l1-1-1
api-ms-win-mm-misc-l2-1-0 api-ms-win-mm-mme-l1-1-0 api-ms-win-mm-playsound-l1-1-0
api-ms-win-mm-time-l1-1-0 api-ms-win-net-isolation-l1-1-1 api-ms-win-ntuser-ie-message-l1-1-0
api-ms-win-ntuser-ie-window-l1-1-0 api-ms-win-ntuser-ie-wmpointer-l1-1-0
api-ms-win-ntuser-rectangle-l1-1-0 api-ms-win-ntuser-sysparams-l1-1-0
api-ms-win-obsolete-localization-l1-1-0 api-ms-win-obsolete-psapi-l1-1-0
api-ms-win-obsolete-shlwapi-l1-1-0 api-ms-win-ole32-ie-l1-1-0 api-ms-win-oobe-notification-l1-1-0
api-ms-win-perf-legacy-l1-1-0 api-ms-win-power-base-l1-1-0 api-ms-win-power-limitsmanagement-l1-1-0
api-ms-win-power-setting-l1-1-1 api-ms-win-privacy-coreprivacysettingsstore-l1-1-0
api-ms-win-ro-typeresolution-l1-1-1 api-ms-win-rtcore-ntuser-clipboard-l1-1-0
api-ms-win-rtcore-ntuser-draw-l1-1-0 api-ms-win-rtcore-ntuser-powermanagement-l1-1-0
api-ms-win-rtcore-ntuser-private-l1-1-11 api-ms-win-rtcore-ntuser-shell-l1-1-0
api-ms-win-rtcore-ntuser-synch-l1-1-0 api-ms-win-rtcore-ntuser-window-l1-1-0
api-ms-win-rtcore-ntuser-winevent-l1-1-0 api-ms-win-rtcore-ntuser-wmpointer-l1-1-3
api-ms-win-rtcore-ntuser-wmpointer-l1-2-0 api-ms-win-rtcore-ole32-clipboard-l1-1-1
api-ms-win-security-accesshlpr-l1-1-0 api-ms-win-security-activedirectoryclient-l1-1-1
api-ms-win-security-appcontainer-l1-1-0 api-ms-win-security-audit-l1-1-1
api-ms-win-security-base-ansi-l1-1-0 api-ms-win-security-base-l1-1-1 api-ms-win-security-base-l1-2-2
api-ms-win-security-base-private-l1-1-2 api-ms-win-security-capability-l1-1-1
api-ms-win-security-cpwl-l1-1-0 api-ms-win-security-credentials-l1-1-0
api-ms-win-security-credentials-l2-1-1 api-ms-win-security-cryptoapi-l1-1-0
api-ms-win-security-grouppolicy-l1-1-0 api-ms-win-security-isolatedcontainer-l1-1-1
api-ms-win-security-isolationapi-l1-1-0 api-ms-win-security-isolationapi-l1-2-0
api-ms-win-security-isolationpolicy-l1-1-0 api-ms-win-security-isolationpolicy-l1-2-0
api-ms-win-security-logon-l1-1-1 api-ms-win-security-lsalookup-ansi-l2-1-0
api-ms-win-security-lsalookup-l1-1-2 api-ms-win-security-lsalookup-l2-1-1
api-ms-win-security-lsapolicy-l1-1-2 api-ms-win-security-provider-ansi-l1-1-0
api-ms-win-security-provider-l1-1-0 api-ms-win-security-sddl-ansi-l1-1-0
api-ms-win-security-sddl-l1-1-0 api-ms-win-security-sddl-private-l1-1-0
api-ms-win-security-sddlparsecond-l1-1-1 api-ms-win-security-systemfunctions-l1-1-0
api-ms-win-security-trustee-l1-1-2 api-ms-win-service-core-ansi-l1-1-1
api-ms-win-service-core-l1-1-5 api-ms-win-service-legacy-l1-1-0 api-ms-win-service-management-l1-1-0
api-ms-win-service-management-l2-1-0 api-ms-win-service-private-l1-1-5
api-ms-win-service-private-l1-2-2 api-ms-win-service-winsvc-l1-1-0 api-ms-win-service-winsvc-l1-2-0
api-ms-win-shcore-comhelpers-l1-1-0 api-ms-win-shcore-obsolete-l1-1-0 api-ms-win-shcore-path-l1-1-0
api-ms-win-shcore-registry-l1-1-1 api-ms-win-shcore-scaling-l1-1-2 api-ms-win-shcore-stream-l1-1-0
api-ms-win-shcore-stream-winrt-l1-1-0 api-ms-win-shcore-sysinfo-l1-1-0
api-ms-win-shcore-taskpool-l1-1-0 api-ms-win-shcore-thread-l1-1-0
api-ms-win-shcore-unicodeansi-l1-1-0 api-ms-win-shell-associations-l1-1-3
api-ms-win-shell-changenotify-l1-1-1 api-ms-win-shell-dataobject-l1-1-1
api-ms-win-shell-namespace-l1-1-1 api-ms-win-shell-shdirectory-l1-1-0
api-ms-win-shell-shellcom-l1-1-0 api-ms-win-shell-shellfolders-l1-1-1 api-ms-win-shlwapi-ie-l1-1-0
api-ms-win-shlwapi-winrt-storage-l1-1-1 api-ms-win-stateseparation-helpers-l1-1-1
api-ms-win-storage-exports-external-l1-1-2 api-ms-win-storage-exports-internal-l1-1-0
ext-ms-onecore-appdefaults-l1-1-0 ext-ms-onecore-shlwapi-l1-1-0 ext-ms-win-adsi-activeds-l1-1-0
ext-ms-win-advapi32-auth-l1-1-0 ext-ms-win-advapi32-encryptedfile-l1-1-1
ext-ms-win-advapi32-eventlog-ansi-l1-1-0 ext-ms-win-advapi32-eventlog-l1-1-2
ext-ms-win-advapi32-hwprof-l1-1-0 ext-ms-win-advapi32-idletask-l1-1-0 ext-ms-win-advapi32-lsa-l1-1-3
ext-ms-win-advapi32-msi-l1-1-0 ext-ms-win-advapi32-npusername-l1-1-0
ext-ms-win-advapi32-ntmarta-l1-1-0 ext-ms-win-advapi32-registry-l1-1-1
ext-ms-win-advapi32-safer-l1-1-0 ext-ms-win-advapi32-shutdown-l1-1-0
ext-ms-win-appcompat-aepic-l1-1-0 ext-ms-win-appmodel-restrictedappcontainer-internal-l1-1-0
ext-ms-win-appmodel-shellexecute-l1-1-0 ext-ms-win-appmodel-state-ext-l1-2-0
ext-ms-win-authz-claimpolicies-l1-1-0 ext-ms-win-authz-context-l1-1-0 ext-ms-win-authz-remote-l1-1-0
ext-ms-win-base-psapi-l1-1-0 ext-ms-win-biometrics-winbio-core-l1-1-6
ext-ms-win-biometrics-winbio-l1-1-0 ext-ms-win-branding-winbrand-l1-1-2
ext-ms-win-branding-winbrand-l1-2-0 ext-ms-win-cluster-clusapi-l1-1-6
ext-ms-win-cluster-resutils-l1-1-3 ext-ms-win-cmd-util-l1-1-0 ext-ms-win-cng-rng-l1-1-1
ext-ms-win-com-coml2-l1-1-1 ext-ms-win-com-ole32-l1-1-5 ext-ms-win-com-ole32-l1-2-0
ext-ms-win-com-ole32-l1-3-0 ext-ms-win-com-ole32-l1-4-0 ext-ms-win-com-psmregister-l1-1-0
ext-ms-win-com-psmregister-l1-2-2 ext-ms-win-com-psmregister-l1-3-1 ext-ms-win-com-sta-l1-1-0
ext-ms-win-core-iuri-l1-1-0 ext-ms-win-core-marshal-l2-1-0 ext-ms-win-core-symbolicnames-l1-1-0
ext-ms-win-core-winsrv-l1-1-0 ext-ms-win-domainjoin-netjoin-l1-1-0 ext-ms-win-driver-setup-l1-1-0
ext-ms-win-driver-setup-wu-l1-1-1 ext-ms-win-drvinst-desktop-l1-1-0 ext-ms-win-dwmapi-ext-l1-1-2
ext-ms-win-dwmapidxgi-ext-l1-1-1 ext-ms-win-dx-d3dkmt-gdi-l1-1-0 ext-ms-win-eventing-tdh-ext-l1-1-0
ext-ms-win-eventing-tdh-priv-l1-1-0 ext-ms-win-feclient-encryptedfile-l1-1-3
ext-ms-win-firewallapi-webproxy-l1-1-1 ext-ms-win-font-setup-l1-1-0 ext-ms-win-fs-vssapi-l1-1-0
ext-ms-win-fsutilext-ifsutil-l1-1-0 ext-ms-win-fsutilext-ulib-l1-1-0 ext-ms-win-fveapi-query-l1-1-0
ext-ms-win-gdi-clipping-l1-1-0 ext-ms-win-gdi-dc-create-l1-1-2 ext-ms-win-gdi-dc-l1-2-1
ext-ms-win-gdi-devcaps-l1-1-0 ext-ms-win-gdi-draw-l1-1-3 ext-ms-win-gdi-font-l1-1-3
ext-ms-win-gdi-gdiplus-l1-1-0 ext-ms-win-gdi-internal-desktop-l1-1-6
ext-ms-win-gdi-internal-uap-init-l1-1-0 ext-ms-win-gdi-metafile-l1-1-2 ext-ms-win-gdi-path-l1-1-0
ext-ms-win-gdi-print-l1-1-0 ext-ms-win-gdi-private-l1-1-0 ext-ms-win-gdi-render-l1-1-0
ext-ms-win-gdi-rgn-l1-1-0 ext-ms-win-gdi-wcs-l1-1-0 ext-ms-win-gpapi-grouppolicy-l1-1-0
ext-ms-win-gpsvc-grouppolicy-l1-1-0 ext-ms-win-gui-dui70-l1-1-0 ext-ms-win-gui-uxinit-l1-1-1
ext-ms-win-imm-l1-1-2 ext-ms-win-kernel32-appcompat-l1-1-0 ext-ms-win-kernel32-datetime-l1-1-0
ext-ms-win-kernel32-elevation-l1-1-0 ext-ms-win-kernel32-errorhandling-l1-1-0
ext-ms-win-kernel32-file-l1-1-0 ext-ms-win-kernel32-localization-l1-1-0
ext-ms-win-kernel32-package-current-l1-1-0 ext-ms-win-kernel32-package-l1-1-2
ext-ms-win-kernel32-process-l1-1-0 ext-ms-win-kernel32-quirks-l1-1-1
ext-ms-win-kernel32-registry-l1-1-0 ext-ms-win-kernel32-sidebyside-l1-1-0
ext-ms-win-kernel32-transacted-l1-1-0 ext-ms-win-kernel32-updateresource-l1-1-0
ext-ms-win-kernel32-windowserrorreporting-l1-1-1 ext-ms-win-kernelbase-processthread-l1-1-3
ext-ms-win-kernelbase-processthread-l1-2-0 ext-ms-win-mapi-mapi32-l1-1-0 ext-ms-win-mm-io-l1-1-0
ext-ms-win-mpr-multipleproviderrouter-l1-1-0 ext-ms-win-msiltcfg-msi-l1-1-0
ext-ms-win-msimg-draw-l1-1-0 ext-ms-win-net-isoext-l1-1-0 ext-ms-win-net-netbios-l1-1-0
ext-ms-win-net-netshell-l1-1-0 ext-ms-win-netprovision-netprovfw-l1-1-0
ext-ms-win-networking-winipsec-l1-1-0 ext-ms-win-newdev-config-l1-1-2
ext-ms-win-ntdsapi-activedirectoryclient-l1-1-1 ext-ms-win-ntuser-caret-l1-1-0
ext-ms-win-ntuser-chartranslation-l1-1-0 ext-ms-win-ntuser-dc-access-ext-l1-1-0
ext-ms-win-ntuser-dde-l1-1-0 ext-ms-win-ntuser-dialogbox-l1-1-2 ext-ms-win-ntuser-draw-l1-1-2
ext-ms-win-ntuser-gui-l1-1-1 ext-ms-win-ntuser-gui-l1-2-0 ext-ms-win-ntuser-gui-l1-3-1
ext-ms-win-ntuser-keyboard-ansi-l1-1-0 ext-ms-win-ntuser-keyboard-l1-1-1
ext-ms-win-ntuser-keyboard-l1-2-0 ext-ms-win-ntuser-keyboard-l1-3-2 ext-ms-win-ntuser-menu-l1-1-3
ext-ms-win-ntuser-message-l1-1-3 ext-ms-win-ntuser-misc-l1-1-0 ext-ms-win-ntuser-misc-l1-2-0
ext-ms-win-ntuser-misc-l1-3-0 ext-ms-win-ntuser-misc-l1-5-1 ext-ms-win-ntuser-misc-l1-6-0
ext-ms-win-ntuser-misc-l1-7-0 ext-ms-win-ntuser-mit-l1-1-0 ext-ms-win-ntuser-mouse-l1-1-1
ext-ms-win-ntuser-powermanagement-l1-1-0 ext-ms-win-ntuser-private-l1-1-1
ext-ms-win-ntuser-private-l1-2-0 ext-ms-win-ntuser-private-l1-3-3 ext-ms-win-ntuser-private-l1-4-0
ext-ms-win-ntuser-private-l1-5-0 ext-ms-win-ntuser-private-l1-6-3 ext-ms-win-ntuser-rawinput-l1-1-0
ext-ms-win-ntuser-rawinput-l1-2-0 ext-ms-win-ntuser-rectangle-ext-l1-1-0
ext-ms-win-ntuser-rim-l1-1-2 ext-ms-win-ntuser-rim-l1-2-1 ext-ms-win-ntuser-rotationmanager-l1-1-2
ext-ms-win-ntuser-server-l1-1-1 ext-ms-win-ntuser-string-l1-1-0 ext-ms-win-ntuser-synch-l1-1-0
ext-ms-win-ntuser-sysparams-ext-l1-1-1 ext-ms-win-ntuser-touch-hittest-l1-1-0
ext-ms-win-ntuser-uicontext-ext-l1-1-0 ext-ms-win-ntuser-window-l1-1-6
ext-ms-win-ntuser-windowclass-l1-1-2 ext-ms-win-ntuser-windowstation-ansi-l1-1-1
ext-ms-win-ntuser-windowstation-l1-1-2 ext-ms-win-odbc-odbc32-l1-1-0 ext-ms-win-ole32-bindctx-l1-1-0
ext-ms-win-ole32-ie-ext-l1-1-0 ext-ms-win-ole32-oleautomation-l1-1-0 ext-ms-win-oleacc-l1-1-2
ext-ms-win-printer-winspool-core-l1-1-0 ext-ms-win-printer-winspool-l1-1-4
ext-ms-win-printer-winspool-l1-2-0 ext-ms-win-profile-extender-l1-1-0
ext-ms-win-profile-userenv-l1-1-1 ext-ms-win-ras-rasapi32-l1-1-2 ext-ms-win-ras-rasdlg-l1-1-0
ext-ms-win-ras-rasman-l1-1-0 ext-ms-win-ras-tapi32-l1-1-1 ext-ms-win-raschapext-eap-l1-1-0
ext-ms-win-rastlsext-eap-l1-1-0 ext-ms-win-rdr-davhlpr-l1-1-0 ext-ms-win-ro-typeresolution-l1-1-1
ext-ms-win-rpc-firewallportuse-l1-1-0 ext-ms-win-rpc-ssl-l1-1-0 ext-ms-win-rtcore-gdi-devcaps-l1-1-1
ext-ms-win-rtcore-gdi-object-l1-1-0 ext-ms-win-rtcore-gdi-rgn-l1-1-1
ext-ms-win-rtcore-ntuser-cursor-l1-1-1 ext-ms-win-rtcore-ntuser-dc-access-l1-1-1
ext-ms-win-rtcore-ntuser-dpi-l1-1-0 ext-ms-win-rtcore-ntuser-dpi-l1-2-2
ext-ms-win-rtcore-ntuser-iam-l1-1-2 ext-ms-win-rtcore-ntuser-inputintercept-l1-1-0
ext-ms-win-rtcore-ntuser-integration-l1-1-0 ext-ms-win-rtcore-ntuser-keyboard-l1-1-0
ext-ms-win-rtcore-ntuser-message-ansi-l1-1-0 ext-ms-win-rtcore-ntuser-message-l1-1-0
ext-ms-win-rtcore-ntuser-rawinput-l1-1-1 ext-ms-win-rtcore-ntuser-rawinput-l1-2-0
ext-ms-win-rtcore-ntuser-synch-ext-l1-1-0 ext-ms-win-rtcore-ntuser-syscolors-l1-1-0
ext-ms-win-rtcore-ntuser-sysparams-l1-1-0 ext-ms-win-rtcore-ntuser-usersecurity-l1-1-0
ext-ms-win-rtcore-ntuser-window-ansi-l1-1-0 ext-ms-win-rtcore-ntuser-window-ext-l1-1-1
ext-ms-win-rtcore-ntuser-window-l1-1-0 ext-ms-win-rtcore-ntuser-winevent-ext-l1-1-0
ext-ms-win-rtcore-ntuser-wmpointer-l1-1-0 ext-ms-win-rtcore-ole32-dragdrop-l1-1-0
ext-ms-win-rtcore-ole32-misc-l1-1-0 ext-ms-win-samsrv-accountstore-l1-1-1
ext-ms-win-scesrv-server-l1-1-0 ext-ms-win-secur32-translatename-l1-1-0
ext-ms-win-security-catalog-database-l1-1-0 ext-ms-win-security-credui-internal-l1-1-0
ext-ms-win-security-credui-l1-1-1 ext-ms-win-security-cryptui-l1-1-1
ext-ms-win-security-kerberos-l1-1-0 ext-ms-win-security-lsaadt-l1-1-0
ext-ms-win-security-lsaadtpriv-l1-1-0 ext-ms-win-security-lsaauditrpc-l1-1-0
ext-ms-win-security-ngc-local-l1-1-0 ext-ms-win-security-slc-l1-1-0
ext-ms-win-security-winscard-l1-1-1 ext-ms-win-session-userinit-l1-1-0
ext-ms-win-session-usermgr-l1-1-0 ext-ms-win-session-usermgr-l1-2-0
ext-ms-win-session-usertoken-l1-1-0 ext-ms-win-session-wininit-l1-1-1
ext-ms-win-session-wininit-l1-2-0 ext-ms-win-session-winlogon-l1-1-2
ext-ms-win-session-winsta-l1-1-5 ext-ms-win-session-wtsapi32-l1-1-1
ext-ms-win-setupapi-classinstallers-l1-1-2 ext-ms-win-setupapi-inf-l1-1-1
ext-ms-win-setupapi-logging-l1-1-0 ext-ms-win-shell-aclui-l1-1-0 ext-ms-win-shell-comdlg32-l1-1-1
ext-ms-win-shell-directory-l1-1-0 ext-ms-win-shell-exports-internal-l1-1-1
ext-ms-win-shell-fileplaceholder-l1-1-0 ext-ms-win-shell-ntshrui-l1-1-0
ext-ms-win-shell-propsys-l1-1-1 ext-ms-win-shell-shell32-l1-2-3 ext-ms-win-shell-shell32-l1-3-0
ext-ms-win-shell-shell32-l1-4-0 ext-ms-win-shell-shell32-l1-5-0 ext-ms-win-shell-shlwapi-l1-1-2
ext-ms-win-shell-shlwapi-l1-2-1 ext-ms-win-shell32-shellcom-l1-1-0
ext-ms-win-shell32-shellfolders-l1-1-1 ext-ms-win-shell32-shellfolders-l1-2-1
ext-ms-win-smbshare-browserclient-l1-1-0 ext-ms-win-spinf-inf-l1-1-0
ext-ms-win-sxs-oleautomation-l1-1-0 ext-ms-win-tsf-inputsetting-l1-1-0 ext-ms-win-tsf-msctf-l1-1-4
ext-ms-win-uiacore-l1-1-3 ext-ms-win-usp10-l1-1-0 ext-ms-win-uxtheme-themes-l1-1-3
ext-ms-win-wer-reporting-l1-1-3 ext-ms-win-wer-wct-l1-1-0 ext-ms-win-wevtapi-eventlog-l1-1-3
ext-ms-win-winrt-storage-l1-1-0 ext-ms-win-winrt-storage-l1-2-3
ext-ms-win-winrt-storage-win32broker-l1-1-0 ext-ms-win-wlan-scard-l1-1-0 ext-ms-win-wrp-sfc-l1-1-0
```
