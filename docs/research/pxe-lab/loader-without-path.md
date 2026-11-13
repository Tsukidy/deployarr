# BCD loader with no path element in the PXE lab

This page records whether WinPE boots over PXE from a BCD store whose RAM disk loader has no `path` element, as in ConfigMgr's store. It answers risk 3 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first), for [#29](https://github.com/Tsukidy/deployarr/issues/29). The runs are from 2026-11-13, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

| VM | Store in option 252 | Loader `path` | End state |
| --- | --- | --- | --- |
| `uefi`, OVMF with Secure Boot on | `boot\uefi-nopath.bcd` | None | WinPE 10.0.26100.9457 at a command prompt. WinPE reports `UEFISecureBootEnabled` = 1 and `SystemBootDevice` = `ramdisk(0)`. |
| `bios`, SeaBIOS with QEMU's iPXE ROM | `boot\bios-nopath.bcd` | None | WinPE 10.0.26100.9457 at a command prompt, with an address from dnsmasq. WinPE reports `SystemBootDevice` = `ramdisk(0)`. |

- A loader with no `path` element boots WinPE on both firmware types. The boot managers load the store, the font, `boot.sdi`, and `boot.wim`, then start Windows from the RAM disk.
- No boot requests `winload.efi` or `winload.exe` over TFTP. The boot managers read the loader from inside the RAM disk.
- Each run makes the same TFTP requests, with the same options, as the run with the default store in [#27](https://github.com/Tsukidy/deployarr/issues/27). Only the store's file name differs. See [Control](#control).
- Neither VM waited for a key press. No key went to either VM before WinPE showed its command prompt.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts | `tools/lab/` from [#127](https://github.com/Tsukidy/deployarr/pull/127), branch `lab-120-ubuntu-2604` at `786c13b` |
| PXE prototype | `tools/pxe-prototype/`, the same on that branch as on `main` at `ef0bf5e` |
| Network | `br-lab` at `10.41.0.1/24`. dnsmasq hands out addresses on it, and the prototype answers PXE on ports 67 and 4011. |
| Chain | `--chain single`. The first ACK names the boot manager and carries option 252. |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| Stores | `--bcd-uefi 'boot\uefi-nopath.bcd' --bcd-bios 'boot\bios-nopath.bcd'` |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

The single stage boots both VMs in #27 ([#134](https://github.com/Tsukidy/deployarr/pull/134)). The two-stage chain also boots both in [#28](https://github.com/Tsukidy/deployarr/issues/28) ([#137](https://github.com/Tsukidy/deployarr/pull/137)). The runs use the single stage for three reasons:

- It has fewer steps, and the BCD is the only thing under test.
- The #27 runs used the same chain and root with the default stores, so they serve as the control.
- The prototype README gives `--chain single` for this experiment.

Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new. The prototype's `started` line shows both store paths:

```text
2026-11-13T11:22:37.554+00:00 started chain=single server_ip=10.41.0.1 interface=br-lab root=/home/user/deployarr-lab/tftp/2011 dhcp_bind=0.0.0.0 dhcp_ports=67,4011 tftp=10.41.0.1:69 bcd_uefi=boot\uefi-nopath.bcd bcd_bios=boot\bios-nopath.bcd max_blksize=32768 max_windowsize=32 timeout=5.000000 retries=5 block_wrap_first=0 block_log=false answer_without_guid=false
```

### BCD stores

All four stores come from the `bcd-stores` artifact of [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137), whose zip has the SHA-256 `5c5eff6591370703d027b613ec2259d79411e84fdd44301317932619c1ae8c97`. The hashes match `~/deployarr-lab/tftp/manifest.txt` and the files on disk.

| Store | Used by | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `boot/uefi-nopath.bcd` | This page, `uefi` | 12288 | `8288306e35e34234ebb2f4936a7b9e051b4d77b3625a50ea97c6a64042ad3ffb` |
| `boot/bios-nopath.bcd` | This page, `bios` | 12288 | `cd2e9d9f3135f4013dccd78264adb1210cc71d2fdb82f0aadc5d7ab720645bc8` |
| `boot/uefi.bcd` | The #27 control, `uefi` | 12288 | `c0164082153aa4eeb826b68d9e3acb4ad815fbf64b8dc1b8a51ef0f02d010224` |
| `boot/bios.bcd` | The #27 control, `bios` | 12288 | `5682186b6906955f5f5e610aee7dc9eb66e3d5314d31141c5ff9b1a0750e7b0d` |

The BCD workflow, `.github/workflows/bcd.yml`, runs `tools/bcd/New-BcdStores.ps1` with `-NoLoaderPath` for the `no-path` stores. That switch skips the `path` of line 94. `tools/lab/media.sh` copies them to `boot/<firmware>-nopath.bcd` (line 78). The stores were read offline from copies, with `hivexml` and `hivexsh` from `libhivex-bin` 1.3.24. Element `0x12000002` is `BcdLibraryString_ApplicationPath`, the "Path to a boot environment application" ([Microsoft's `BcdLibraryElementTypes`](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/bcd/bcdlibraryelementtypes)). `hivexsh` lists these keys under each loader's `Elements` key:

| Store | Loader object | Element keys |
| --- | --- | --- |
| `uefi.bcd` | `{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e01}` | `11000001`, `12000002`, `12000004`, `21000001`, `22000002`, `26000010`, `26000022` |
| `uefi-nopath.bcd` | `{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e01}` | `11000001`, `12000004`, `21000001`, `22000002`, `26000010`, `26000022` |
| `bios.bcd` | `{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e02}` | `11000001`, `12000002`, `12000004`, `21000001`, `22000002`, `26000010`, `26000022` |
| `bios-nopath.bcd` | `{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e02}` | `11000001`, `12000004`, `21000001`, `22000002`, `26000010`, `26000022` |

- In `uefi.bcd`, element `12000002` holds `\windows\system32\boot\winload.efi`. In `bios.bcd` it holds `\windows\system32\boot\winload.exe`.
- A dump of every object and element value differs between each default store and its `-nopath` store only in that element.
- `uefi-nopath.bcd` and `bios-nopath.bcd` differ only in the loader's GUID, which also appears in `{bootmgr}`'s `default` and `displayorder`.
- Every store has three objects. `{bootmgr}`, `{9dea862c-5cdd-4e70-acc1-f32b344d4795}`, has the type `0x10100002`. The loader has the type `0x10200003`. `{ramdiskoptions}`, `{ae5534e0-a924-466c-b836-758539a3ee3a}`, has the type `0x30000000`. [#61](https://github.com/Tsukidy/deployarr/issues/61) asks whether the `{bootmgr}` type matters. Both `-nopath` stores boot with `0x10100002`.
- Device elements `11000001` and `21000001` name `\boot\x64\boot.wim` with `{ramdiskoptions}`. `{ramdiskoptions}` names `\boot\boot.sdi`, and `{bootmgr}` names the font path `\boot\fonts`.

## Versions

| Component | Version |
| --- | --- |
| `qemu-system-x86` | 1:10.2.1+ds-1ubuntu3.3 |
| `ovmf` | 2025.11-3ubuntu7.3 |
| `seabios` | 1.17.0-1ubuntu1 |
| `ipxe-qemu` | 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2 |
| `swtpm` | 0.10.1+ds-1ubuntu3 |
| `dnsmasq-base` | 2.92-1ubuntu0.5 |
| `wireshark-common` and `tshark` | 4.6.4-1 |
| `osslsigncode` | 2.13-1 |
| `wimtools` | 1.14.5-1 |
| `libhivex-bin` | 1.3.24-2.1build2 |
| Python | 3.14.4 |

| Firmware file | SHA-256 |
| --- | --- |
| `/usr/share/OVMF/OVMF_CODE_4M.ms.fd` | `33c413a04e9edf512c504e9b0accbcec4d009d406bca761cc0b8eacc93da60b6` |
| `/usr/share/OVMF/OVMF_VARS_4M.ms.fd` | `d2e3a79d28c1c93286048a3e876570882dab4538d6a842ab2e0e53265cbcc983` |
| `/usr/share/seabios/bios-256k.bin` | `e26615f9ad430328f49ca105e570b2dc4490a08a34ea73d27cae8b809a30ee06` |

The boot files come from index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`. The ISO has the SHA-256 `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650`. Index 1 is Microsoft Windows PE (amd64), build 26100.9457. The issuer is the `Issuer :` line under `Signer's certificate:` in the output of `osslsigncode verify`. The version is the file version in each file's version resource.

| File in the TFTP root | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `boot/x64/bootmgfw.efi` | 3087400 | `4bb32c300df0815d8b6c54a0d3793ae1d78fcdc07f8e2b9f831289b2eb109092` | Microsoft Windows Production PCA 2011 | 10.0.28000.367 |
| `boot/x64/pxeboot.n12` | 25238 | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Not a PE file | None |
| `boot/x64/bootmgr.exe` | 814544 | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |
| `boot/boot.sdi` | 3170304 | `cd2c00ce027687ce4a8bdc967f26a8ab82f651c9becd703658ba282ec49702bd` | Not a PE file | None |
| `boot/fonts/wgl4_boot.ttf` | 50096 | `23872d2eba60bbb3bd2585e925c4b48769fb6bb0cfa7f4a6ba4e8f03bcaf2c35` | Not a PE file | None |
| `boot/x64/boot.wim` | 572691590 | `c0a18c72a7f84c36120e5681bb325e2c892e919886671e553a6129899a09097a` | Not a PE file | WinPE 10.0.26100.9457 |

`wimdir` lists the OS loaders at two paths in that `boot.wim`. `wimextract` copied them to a scratch folder outside the repository. The two copies of each loader are the same file:

| Paths in `boot.wim` | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `Windows\System32\winload.efi` and `Windows\System32\Boot\winload.efi` | 3358408 | `1c1a978448b8e0fc2284c13d6e55fde8bf9827dca432cdc8a9be66c2e729adda` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |
| `Windows\System32\winload.exe` and `Windows\System32\Boot\winload.exe` | 1875568 | `fb5f4ff18eb78198934d380ecb0210d1dd75e755dec3bc2423422133ca0549de` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |

## The `uefi` VM

### DHCP

Frames 1 to 7 of [`loader-without-path-uefi-dhcp.txt`](captures/loader-without-path-uefi-dhcp.txt):

1. The DISCOVER carries option 93 = 7, option 97 = `4c414200-0000-4000-8000-000000000011`, and option 60 `PXEClient:Arch:00007:UNDI:003000`.
2. dnsmasq offers `10.41.0.176`, and the prototype sends its OFFER with `siaddr` `10.41.0.1`, option 60 `PXEClient`, and no boot file.
3. The firmware takes the lease from dnsmasq on port 67. The prototype logs that REQUEST as `dhcp-ignored` with `reason=lease-request`.
4. The firmware sends its boot file REQUEST from `10.41.0.176:4011` to port 4011.
5. The prototype's ACK in frame 7 goes to `10.41.0.176:4011`. It has `siaddr` `10.41.0.1`, the boot file `boot\x64\bootmgfw.efi`, and options 53, 54, 97, 60, and 252 in that order. Option 252 is `boot\uefi-nopath.bcd`, 20 bytes with no NUL.

The prototype log shows the ACK:

```text
2026-11-13T11:22:45.349+00:00 dhcp-sent port=4011 destination=10.41.0.176:4011 type=ACK xid=cdd48d2e firmware=uefi-x64 step=only file=boot\x64\bootmgfw.efi option_252=boot\uefi-nopath.bcd
```

### TFTP requests

The table lists every RRQ in order, from the prototype log. The capture holds the same 32 RRQs in the same order. Times are UTC. "Size probe" means that the server sent an OACK with `tsize`, and the client then ended the transfer with an ERROR, as the outcome column quotes.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 11:22:46.348 | `boot\x64\bootmgfw.efi` | `tsize=0,blksize=1468,windowsize=4` | Size probe, then client ERROR 8 `User aborted the transfer` |
| 2 | 11:22:46.358 | `boot\x64\bootmgfw.efi` | `blksize=1468,windowsize=4` | Complete, 3087400 bytes in 0.100 seconds |
| 3 | 11:22:46.599 | `\boot\x64\BCD` | `tsize=0` | ERROR 1 File not found |
| 4 | 11:22:46.599 | `\boot\uefi-nopath.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 5 | 11:22:46.599 | `\boot\uefi-nopath.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.002 seconds |
| 6 | 11:22:46.602 | `\EFI\Microsoft\Boot\Policies\SbcpFlightToken.p7b` | `tsize=0` | ERROR 1 File not found |
| 7 | 11:22:46.660 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 8 to 21 | 11:22:46.675 to 11:22:46.678 | `\EFI\Microsoft\Boot\SiPolicy.p7b`, `SkuSiPolicy.p7b`, `WinSiPolicy.p7b`, `ATPSiPolicy.p7b`, then those four and `VbsSiPolicy.p7b` twice more | `tsize=0` | ERROR 1 File not found, 14 times |
| 22 | 11:22:46.698 | `\boot\x64\bootmgfw.efi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 23 | 11:22:46.699 | `\boot\x64\bootmgfw.efi` | `tsize=0,blksize=1456` | Complete, 3087400 bytes in 0.259 seconds |
| 24 | 11:22:46.963 | `\EFI\Microsoft\Boot\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 25 | 11:22:46.963 | `\boot\x64\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 26 | 11:22:46.964 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 27 | 11:22:46.965 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 28 | 11:22:46.965 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 29 | 11:22:46.966 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.005 seconds |
| 30 | 11:22:46.980 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.314 seconds |
| 31 | 11:22:47.302 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 51.273 seconds |
| 32 | 11:23:38.717 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQs 1 and 2 come from OVMF, and RRQs 3 to 32 from `bootmgfw.efi`.
- `bootmgfw.efi` asks for `\boot\x64\BCD` first, then for the option 252 path with a leading `\`.
- The transfer of `boot.wim` sent 393,333 blocks with window size 1 and no retransmits.
- No request names `winload.efi`. RRQ 32 is the last TFTP request of the boot.

### End state

The screen during RRQ 31 shows `Loading files...`, a progress bar, and `IP: 10.41.0.1, File: \boot\x64\boot.wim`. A screenshot at 11:23:42.309 shows the firmware logo with a spinning progress ring. A screenshot at 11:23:46.395 shows a window titled `Administrator: X:\windows\system32\cmd.exe` after `wpeinit`. So the window appeared at most 67.7 seconds after `vm.sh` started at 11:22:38.734. A screenshot's time is the modification time of its file, and the screenshots came about every 2 seconds. Commands typed into that window through QEMU's `sendkey` gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `reg query hklm\system\currentcontrolset\control\secureboot\state` printed `UEFISecureBootEnabled REG_DWORD 0x1`.
- `reg query hklm\system\currentcontrolset\control /v SystemBootDevice` printed `ramdisk(0)`.
- `reg query hklm\system\currentcontrolset\control /v SystemStartOptions` printed ` MININT  FVEBOOT=2715648  NOVGA RDIMAGEOFFSET=8192 RDIMAGELENGTH=3161088 RDPATH=NET(0)\BOOT\X64\BOOT.WIM`.
- `reg query hklm\system\controlset001\control\pxe /v BootServerReply` printed the ACK of frame 7, with xid `CDD48D2E`, the boot file `boot\x64\bootmgfw.efi`, and option 252 = `boot\uefi-nopath.bcd`.
- `bcdedit /enum all` printed `The boot configuration data store could not be opened.` and `The requested system device cannot be found.`, and `reg query hklm` listed no `BCD00000000` key. So WinPE does not show the store it booted from.
- `ipconfig` printed `Windows IP Configuration` and no adapter, as in #27.

## The `bios` VM

### DHCP

Frames 1 to 7 of [`loader-without-path-bios-dhcp.txt`](captures/loader-without-path-bios-dhcp.txt) follow the same pattern as on the `uefi` VM:

- The DISCOVER carries option 93 = 0, option 97 = `4c414200-0000-4000-8000-000000000021`, and option 60 `PXEClient:Arch:00000:UNDI:002001`.
- The ROM takes the lease `10.41.0.192` from dnsmasq on port 67, then sends its boot file REQUEST from `10.41.0.192:68` to port 4011.
- The prototype's ACK in frame 7 goes to `10.41.0.192:68`. It has the boot file `boot\x64\pxeboot.n12` and option 252 = `boot\bios-nopath.bcd`, 20 bytes.

```text
2026-11-13T11:26:20.099+00:00 dhcp-sent port=4011 destination=10.41.0.192:68 type=ACK xid=fdd0c436 firmware=bios step=only file=boot\x64\pxeboot.n12 option_252=boot\bios-nopath.bcd
```

Frames 38 to 41 are WinPE's own DISCOVER, OFFER, REQUEST, and ACK, with option 60 `MSFT 5.0` and no option 93. The prototype logs them as `dhcp-ignored` with `reason=no-option-93`, and dnsmasq gives WinPE `10.41.0.192` again.

### TFTP requests

The capture holds the same 13 RRQs in the same order.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 11:26:23.239 | `boot\x64\pxeboot.n12` | `blksize=1432,tsize=0` | Complete, 25238 bytes in 0.004 seconds |
| 2 | 11:26:23.265 | `boot\x64\bootmgr.exe` | `blksize=512,tsize=0` | Complete, 814544 bytes in 0.168 seconds |
| 3 | 11:26:23.573 | `\boot\bios-nopath.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 4 | 11:26:23.574 | `\boot\bios-nopath.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.003 seconds |
| 5 | 11:26:23.577 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 6 | 11:26:23.591 | `\hiberfil.sys` | `tsize=0` | ERROR 1 File not found |
| 7 | 11:26:23.591 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 8 | 11:26:23.592 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 9 | 11:26:23.593 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 10 | 11:26:23.593 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.010 seconds |
| 11 | 11:26:23.604 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.588 seconds |
| 12 | 11:26:24.194 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 90.395 seconds |
| 13 | 11:27:54.846 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQ 1 comes from the iPXE ROM, RRQ 2 from `pxeboot.n12`, and RRQs 3 to 13 from `bootmgr.exe`.
- `bootmgr.exe` asks for the option 252 path first.
- The transfer of `boot.wim` sent 393,333 blocks with window size 1 and no retransmits.
- No request names `winload.exe`. RRQ 13 is the last TFTP request of the boot.

### End state

The screen during RRQ 12 shows `Loading files...` and `IP: 10.41.0.1, File: \boot\x64\boot.wim`. A screenshot at 11:28:02.100 shows the Windows logo with a spinning progress ring. A screenshot at 11:28:06.190 shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`. So the window appeared at most 110.3 seconds after `vm.sh` started at 11:26:15.861. After `wpeinit`, these commands gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `ipconfig` printed `Ethernet adapter Ethernet` with the IPv4 address `10.41.0.192` and the default gateway `10.41.0.1`.
- `reg query hklm\system\currentcontrolset\control /v SystemBootDevice` printed `ramdisk(0)`.
- `reg query hklm\system\currentcontrolset\control /v SystemStartOptions` printed ` MININT  FVEBOOT=2187264 RDIMAGEOFFSET=8192 RDIMAGELENGTH=3161088 RDPATH=NET(0)\BOOT\X64\BOOT.WIM`.
- `reg query hklm\system\controlset001\control\pxe /v BootServerReply` printed a reply with xid `FDD0C436`, the boot file `boot\x64\pxeboot.n12`, and option 252 = `boot\bios-nopath.bcd`. As in #27, its options come in ascending code order, and zeros follow option 252 with no End option.
- `bcdedit /enum all` printed the same two lines as on the `uefi` VM.

## Control

The control is the pair of #27 runs on the same lab host, about 45 minutes earlier. They used the same chain, root, prototype, lab scripts, and VM settings, with the default stores `boot\uefi.bcd` and `boot\bios.bcd`. Their raw logs are in `~/deployarr-lab/runs/27/` on the lab host, and [#134](https://github.com/Tsukidy/deployarr/pull/134) records them. The boot files, `boot.sdi`, the font, and `boot.wim` have the same SHA-256 on disk now as in #27.

| Measure | `uefi`, default store | `uefi`, no `path` | `bios`, default store | `bios`, no `path` |
| --- | --- | --- | --- | --- |
| RRQs | 32 | 32 | 13 | 13 |
| `boot.wim` transfer | 52.145 seconds | 51.273 seconds | 89.254 seconds | 90.395 seconds |
| `boot.wim` blocks sent, retransmits | 393,333, 0 | 393,333, 0 | 393,333, 0 | 393,333, 0 |
| Packets in the full capture | 398,285 | 398,285 | 397,223 | 397,223 |
| TFTP ACKs in the full capture | 398,208 | 398,208 | 397,170 | 397,170 |
| WinPE `ver` | 10.0.26100.9457 | 10.0.26100.9457 | 10.0.26100.9457 | 10.0.26100.9457 |

A `diff` of the file and options of every RRQ, from the #27 log against this page's log, prints only the two lines for the store on each VM:

```text
4,5c4,5
< \boot\uefi.bcd "tsize=0"
< \boot\uefi.bcd "tsize=0,blksize=1456"
---
> \boot\uefi-nopath.bcd "tsize=0"
> \boot\uefi-nopath.bcd "tsize=0,blksize=1456"
```

```text
3,4c3,4
< \boot\bios.bcd "tsize=0"
< \boot\bios.bcd "tsize=0,blksize=1456"
---
> \boot\bios-nopath.bcd "tsize=0"
> \boot\bios-nopath.bcd "tsize=0,blksize=1456"
```

The #27 `uefi` window appeared at most 83.2 seconds after start. Its screenshots came less often, so that bound is looser than the 67.7 seconds here. The #27 `bios` window appeared at most 110.5 seconds after start.

## Captures

| File | What it holds |
| --- | --- |
| [`loader-without-path-uefi.pcapng`](captures/loader-without-path-uefi.pcapng) | The `uefi` run, reduced |
| [`loader-without-path-uefi-dhcp.txt`](captures/loader-without-path-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`loader-without-path-bios.pcapng`](captures/loader-without-path-bios.pcapng) | The `bios` run, reduced |
| [`loader-without-path-bios-dhcp.txt`](captures/loader-without-path-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |

The capture filter is `udp and udp[8:2] != 3`. It keeps only UDP, and it drops TFTP DATA packets and IP fragments. Each full capture is over 30 MB, because the boot managers ACK every block of `boot.wim`. The committed files are reduced copies that drop only the TFTP ACKs, with `tshark -r - -Y 'not tftp.opcode == 4' -w -`. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 398,285 packets, 31,867,452 bytes, SHA-256 `a49b52d91cba158ece3de58218d581b99befbccdc6d13625bd1cc972d95404f2` | 398,208 | 77 packets, 10,704 bytes |
| `bios` | 397,223 packets, 31,783,260 bytes, SHA-256 `45dba969267c225e125e524f384ec67c72b825939bd021d7102afbbc3d605604` | 397,170 | 53 packets, 9,552 bytes |

The `dumpcap` logs, `loader-without-path-<vm>-dumpcap.log` next to the full captures, report no dropped packets for either run. The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/29/`: the prototype logs, the `uefi` serial log, the screenshots, the offline store dump `bcd-elements.txt`, and `versions.txt`.

## Commands

Each run started the prototype, then the capture, then the VM. That is the order that "Run an experiment" gives in the lab guide of [#127](https://github.com/Tsukidy/deployarr/pull/127). The lab scripts ran from a checkout of that branch, `lab-120-ubuntu-2604`, here `$LAB`. `<vm>` is `uefi` or `bios`. The prototype and `dumpcap` run in the background, and `dumpcap` writes its log, with the dropped-packet count, to a file next to the capture.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain single --no-block-log --bcd-uefi 'boot\uefi-nopath.bcd' --bcd-bios 'boot\bios-nopath.bcd' --log-file ~/deployarr-lab/runs/29/loader-without-path-<vm>-prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/loader-without-path-<vm>.pcapng 2> ~/deployarr-lab/captures/loader-without-path-<vm>-dumpcap.log &
timeout 1800 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The screenshots came from the HMP command `screendump <file>.png -f png` on the monitor socket. The WinPE commands went in one key at a time with the HMP command `sendkey`. After the last packet, `sudo pkill -INT -x dumpcap` stopped the capture, and the HMP command `quit` stopped the VM.

These commands read the stores offline, from copies in a scratch folder:

```bash
hivexml uefi-nopath.bcd > uefi-nopath.xml
printf 'cd \\Objects\\{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e01}\\Elements\nls\n' | hivexsh uefi-nopath.bcd
printf 'cd \\Objects\\{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e01}\\Elements\\12000002\nlsval\n' | hivexsh uefi.bcd
```

These commands list and copy the OS loaders in `boot.wim`, into a scratch folder:

```bash
wimdir ~/deployarr-lab/tftp/2011/boot/x64/boot.wim 1 | grep -iE '^/Windows/System32/(boot/)?winload\.(efi|exe)$'
wimextract ~/deployarr-lab/tftp/2011/boot/x64/boot.wim 1 /Windows/System32/Boot/winload.efi /Windows/System32/Boot/winload.exe /Windows/System32/winload.efi /Windows/System32/winload.exe --dest-dir=. --preserve-dir-structure
```

This command lists the loader paths that a boot manager holds as UTF-16 strings:

```bash
strings -el ~/deployarr-lab/tftp/2011/boot/x64/bootmgfw.efi | grep -i winload
```

These commands make the committed files:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/loader-without-path-<vm>.pcapng > docs/research/pxe-lab/captures/loader-without-path-<vm>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/loader-without-path-<vm>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/loader-without-path-<vm>.pcapng > docs/research/pxe-lab/captures/loader-without-path-<vm>-dhcp.txt
```

## What is inferred

- The boot managers load the OS loader from the RAM disk (inferred). No TFTP request names it, and the store's `device` is the RAM disk. `SystemBootDevice` = `ramdisk(0)` and `RDPATH=NET(0)\BOOT\X64\BOOT.WIM` in WinPE show that Windows started from that RAM disk.
- With no `path`, the boot managers use a built-in default path (inferred). `bootmgfw.efi` holds the UTF-16 strings `\Windows\System32\winload.efi` and `\Windows\System32\boot\winload.efi`. `bootmgr.exe` holds the same two paths with `.exe`. The runs cannot tell which path the boot managers opened, because this `boot.wim` has the same file at both.
- With no `path`, the UEFI and BIOS stores differ only in the loader's GUID, so one store could serve both firmware types (inferred). No run served one store to both.
- WinPE on the `uefi` VM has no adapter because it has no driver for the virtio-net card (inferred), as #27 says.

## Not covered

- The `uefi2023` VM. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs the 2023-signed boot files there.
- The two-stage chain with the `-nopath` stores. The BCD requests after the boot manager starts are the same in both chains in #28, so the result probably carries over (inferred).
- A boot image that has the OS loader at only one of the two paths.
- Physical PCs and other firmware.

## What changes in deployarr

- deployarr can leave the loader's `path` out of both BCD templates, as ConfigMgr does. The [BCD](../../architecture.md#bcd) section then drops the `path \windows\system32\boot\winload.efi` element and its question. The result holds on the `uefi` VM with Secure Boot on and on the `bios` VM. #32 still has to show it on `uefi2023`.
- Without `path`, the two templates differ only in the loader's GUID. #33 can choose between two templates and one template for both firmware types. One template would need a lab run that serves it to both VMs first.
- The importer can check that the boot image holds the OS loaders at `Windows\System32\winload.efi` and `Windows\System32\winload.exe`, because a loader with no `path` depends on the boot manager's default path (inferred).
- The `{bootmgr}` type `0x10100002` from `bcdedit /create {bootmgr}` does not stop either boot manager. #61 can use that.
- The store's file name is free. Both boot managers requested `boot\uefi-nopath.bcd` and `boot\bios-nopath.bcd` exactly as option 252 named them.
