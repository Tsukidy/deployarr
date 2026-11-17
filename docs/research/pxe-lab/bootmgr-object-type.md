# `{bootmgr}` object type in the PXE lab

This page records whether the object type of `{bootmgr}` in a BCD store changes PXE boot, for [#61](https://github.com/Tsukidy/deployarr/issues/61). `tools/bcd/New-BcdStores.ps1` creates `{bootmgr}` with `bcdedit /create {bootmgr}`, which gives the type `0x10100002`. ConfigMgr's PXE store has `0x10200002`, as [BCD](../configmgr-2509.md#bcd) in the ConfigMgr reference records. The new runs are from 2026-11-17, on the lab host.

## Result

| Store | `{bootmgr}` type | VM | Chain | Boot | Record |
| --- | --- | --- | --- | --- | --- |
| `boot/uefi.bcd` | `0x10100002` | `uefi` | Single stage | WinPE | [#27](https://github.com/Tsukidy/deployarr/issues/27), [Single-stage boot chain in the PXE lab](single-stage-chain.md) |
| `boot/uefi.bcd` | `0x10100002` | `uefi` | Two stage | WinPE | [#28](https://github.com/Tsukidy/deployarr/issues/28), [#137](https://github.com/Tsukidy/deployarr/pull/137) |
| `boot/uefi-nopath.bcd` | `0x10100002` | `uefi` | Single stage | WinPE | [#29](https://github.com/Tsukidy/deployarr/issues/29), [#138](https://github.com/Tsukidy/deployarr/pull/138) |
| `uefi-bootmgr-10200002.bcd` | `0x10200002` | `uefi` | Single stage | WinPE | This page |
| `boot/bios.bcd` | `0x10100002` | `bios` | Single stage | WinPE | #27, [Single-stage boot chain in the PXE lab](single-stage-chain.md) |
| `boot/bios.bcd` | `0x10100002` | `bios` | Two stage | WinPE | #28, #137 |
| `boot/bios-nopath.bcd` | `0x10100002` | `bios` | Single stage | WinPE | #29, #138 |
| `bios-bootmgr-10200002.bcd` | `0x10200002` | `bios` | Single stage | WinPE | This page |

- No effect seen. With either type, every store boots WinPE 10.0.26100.9457 on the `uefi` VM, OVMF with Secure Boot on, and on the `bios` VM, SeaBIOS with QEMU's iPXE ROM.
- The `0x10200002` stores are the default stores with one byte changed, the type of `{bootmgr}`. With them, each boot manager makes the same TFTP requests, with the same options, as with the default store in #27. Only the store's name differs. See [Control](#control).
- The earlier lab runs booted only `0x10100002`. This page adds the two `0x10200002` boots, because no earlier run covered that type.

## Object types

Microsoft's [`BcdObject`](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/bcd/bcdobject) page describes the `Type` property. The upper 4 bits are the object type, and 1 is an application. Bits 20 to 23 are the image type, where 0x1 is a firmware image and 0x2 is a boot image. The lowest 20 bits are the application type, and 0x2 is the Windows boot manager. So the two values differ only in the image type:

| Type | Image type | Where |
| --- | --- | --- |
| `0x10100002` | 0x1, firmware | `bcdedit /create {bootmgr}` in `tools/bcd/New-BcdStores.ps1` |
| `0x10200002` | 0x2, boot | ConfigMgr's PXE store, built through the BCD WMI provider |

## Stores

The default stores come from the `bcd-stores` artifact of [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137), whose zip has the SHA-256 `5c5eff6591370703d027b613ec2259d79411e84fdd44301317932619c1ae8c97`. Their hashes match `~/deployarr-lab/tftp/manifest.txt` and the files in `~/deployarr-lab/tftp/2011/boot/`. `hivexget` from `libhivex-bin` 1.3.24 reads the `Type` value under `\Objects\{9dea862c-5cdd-4e70-acc1-f32b344d4795}\Description`. That GUID is `{bootmgr}`.

| Store | Bytes | SHA-256 | `{bootmgr}` type |
| --- | --- | --- | --- |
| `boot/uefi.bcd` | 12288 | `c0164082153aa4eeb826b68d9e3acb4ad815fbf64b8dc1b8a51ef0f02d010224` | `0x10100002` |
| `boot/bios.bcd` | 12288 | `5682186b6906955f5f5e610aee7dc9eb66e3d5314d31141c5ff9b1a0750e7b0d` | `0x10100002` |
| `boot/uefi-nopath.bcd` | 12288 | `8288306e35e34234ebb2f4936a7b9e051b4d77b3625a50ea97c6a64042ad3ffb` | `0x10100002` |
| `boot/bios-nopath.bcd` | 12288 | `cd2e9d9f3135f4013dccd78264adb1210cc71d2fdb82f0aadc5d7ab720645bc8` | `0x10100002` |
| `uefi-bootmgr-10200002.bcd` | 12288 | `2043aa09ff3a8e4ccec2b36449e8862f9ab15c65970e1bbfea582e49e4b783f9` | `0x10200002` |
| `bios-bootmgr-10200002.bcd` | 12288 | `7bec53f44a2e6382fe6aa52a2c570327212d07783e2e7c9f9104c169f780af7e` | `0x10200002` |

Each `0x10200002` store is a copy of `uefi.bcd` or `bios.bcd` with one byte changed:

- The `Type` value is a `REG_DWORD`, so its 4 bytes sit inside its value record. In both stores, that record is at file offset 4900 and its data at offset 4908.
- The script in [Commands](#commands) finds the one value record named `Type` that holds `0x10100002`, and changes byte 4910 from `0x10` to `0x20`. The bytes `02 00 10 10` become `02 00 20 10`.
- `cmp -l` lists that one byte and no other. The hive's base block, with its checksum and sequence numbers, stays as it was.
- `hivexget` reads `270532610`, which is `0x10200002`, from each new store. A `hivexml` dump of each new store differs from its source only in that value.

Windows did not make these stores. `bcdedit` has no known switch that sets this type (inferred), and the BCD WMI provider's `BcdStore.CreateObject` takes the type directly. No run here used a store that Windows wrote with `0x10200002`.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM. The host restarted at about 18:10 UTC, before the runs. |
| Lab scripts and PXE prototype | `tools/lab/` and `tools/pxe-prototype/` from `main` at `d799a1b`. Its `tools/lab/vm.sh` and `tools/pxe-prototype/` are the same as in the #27 to #29 runs. |
| Network | `br-lab` at `10.41.0.1/24`, started with `tools/lab/network.sh up`. dnsmasq hands out addresses on it, and the prototype answers PXE on ports 67 and 4011. |
| Chain | `--chain single`. The first ACK names the boot manager and carries option 252. |
| TFTP root | `~/deployarr-lab/runs/61b/tftp-root`. It holds the two `0x10200002` stores in `boot/`, and links `boot/boot.sdi`, `boot/fonts`, and `boot/x64` to the same paths in `~/deployarr-lab/tftp/2011`, which [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) builds. |
| Stores | `--bcd-uefi 'boot\uefi-bootmgr-10200002.bcd' --bcd-bios 'boot\bios-bootmgr-10200002.bcd'` |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

The runs use the single stage because #27 is their control. Its runs used the same chain, files, prototype, and VM settings with the default stores. Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new.

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
| `libhivex-bin` | 1.3.24-2.1build2 |
| Python | 3.14.4 |

| Firmware file | SHA-256 |
| --- | --- |
| `/usr/share/OVMF/OVMF_CODE_4M.ms.fd` | `33c413a04e9edf512c504e9b0accbcec4d009d406bca761cc0b8eacc93da60b6` |
| `/usr/share/OVMF/OVMF_VARS_4M.ms.fd` | `d2e3a79d28c1c93286048a3e876570882dab4538d6a842ab2e0e53265cbcc983` |
| `/usr/share/seabios/bios-256k.bin` | `e26615f9ad430328f49ca105e570b2dc4490a08a34ea73d27cae8b809a30ee06` |

The boot files come from index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`, whose SHA-256 is `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650`. Index 1 is Microsoft Windows PE (amd64), build 26100.9457. The issuer is the `Issuer :` line under `Signer's certificate:` in the output of `osslsigncode verify`. The version is the file version in each file's version resource. `boot.wim` has no version resource, so its row gives the version and build of index 1 from the image metadata.

| File in `~/deployarr-lab/tftp/2011` | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `boot/x64/bootmgfw.efi` | 3087400 | `4bb32c300df0815d8b6c54a0d3793ae1d78fcdc07f8e2b9f831289b2eb109092` | Microsoft Windows Production PCA 2011 | 10.0.28000.367 |
| `boot/x64/pxeboot.n12` | 25238 | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Not a PE file | None |
| `boot/x64/bootmgr.exe` | 814544 | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |
| `boot/boot.sdi` | 3170304 | `cd2c00ce027687ce4a8bdc967f26a8ab82f651c9becd703658ba282ec49702bd` | Not a PE file | None |
| `boot/fonts/wgl4_boot.ttf` | 50096 | `23872d2eba60bbb3bd2585e925c4b48769fb6bb0cfa7f4a6ba4e8f03bcaf2c35` | Not a PE file | None |
| `boot/x64/boot.wim` | 572691590 | `c0a18c72a7f84c36120e5681bb325e2c892e919886671e553a6129899a09097a` | Not a PE file | WinPE 10.0.26100.9457 |

## The `uefi` VM

### DHCP

[`bootmgr-object-type-uefi-dhcp.txt`](captures/bootmgr-object-type-uefi-dhcp.txt) holds frames 1 to 7. Times are UTC.

| Frame | Time | Packet |
| --- | --- | --- |
| 1 | 19:07:28.132 | OVMF's DISCOVER, xid `a0d97a48`, option 93 = 7 |
| 2 | 19:07:28.132 | The prototype's OFFER, with no boot file |
| 3 | 19:07:31.139 | dnsmasq's OFFER of `10.41.0.176` |
| 4 | 19:07:31.875 | OVMF's lease REQUEST. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. |
| 5 | 19:07:31.876 | dnsmasq's ACK |
| 6 | 19:07:31.876 | OVMF's boot file REQUEST, from `10.41.0.176:4011` to port 4011 |
| 7 | 19:07:31.876 | The prototype's ACK to `10.41.0.176:4011`, with the boot file `boot\x64\bootmgfw.efi` and option 252 = `boot\uefi-bootmgr-10200002.bcd` |

dnsmasq's OFFER came 3.0 seconds after the DISCOVER. In the #27 run, it came with the prototype's OFFER. [What is inferred](#what-is-inferred) gives a likely cause. The boot went on as in #27 from frame 4.

### TFTP requests

The table lists every RRQ in order, from the prototype log. The capture holds the same 32 RRQs in the same order. "Size probe" means that the server sent an OACK with `tsize`, and the client then ended the transfer with an ERROR, as the outcome column quotes.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 19:07:32.876 | `boot\x64\bootmgfw.efi` | `tsize=0,blksize=1468,windowsize=4` | Size probe, then client ERROR 8 `User aborted the transfer` |
| 2 | 19:07:32.881 | `boot\x64\bootmgfw.efi` | `blksize=1468,windowsize=4` | Complete, 3087400 bytes in 0.102 seconds |
| 3 | 19:07:33.126 | `\boot\x64\BCD` | `tsize=0` | ERROR 1 File not found |
| 4 | 19:07:33.126 | `\boot\uefi-bootmgr-10200002.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 5 | 19:07:33.127 | `\boot\uefi-bootmgr-10200002.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.002 seconds |
| 6 | 19:07:33.130 | `\EFI\Microsoft\Boot\Policies\SbcpFlightToken.p7b` | `tsize=0` | ERROR 1 File not found |
| 7 | 19:07:33.183 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 8 to 21 | 19:07:33.199 to 19:07:33.202 | `\EFI\Microsoft\Boot\SiPolicy.p7b`, `SkuSiPolicy.p7b`, `WinSiPolicy.p7b`, `ATPSiPolicy.p7b`, then those four and `VbsSiPolicy.p7b` twice more | `tsize=0` | ERROR 1 File not found, 14 times |
| 22 | 19:07:33.223 | `\boot\x64\bootmgfw.efi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 23 | 19:07:33.223 | `\boot\x64\bootmgfw.efi` | `tsize=0,blksize=1456` | Complete, 3087400 bytes in 0.268 seconds |
| 24 | 19:07:33.496 | `\EFI\Microsoft\Boot\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 25 | 19:07:33.496 | `\boot\x64\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 26 | 19:07:33.498 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 27 | 19:07:33.498 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 28 | 19:07:33.499 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 29 | 19:07:33.500 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.005 seconds |
| 30 | 19:07:33.513 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.317 seconds |
| 31 | 19:07:33.840 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 51.118 seconds |
| 32 | 19:08:25.208 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- The first file that `bootmgfw.efi` loads is the store, after `\boot\x64\BCD` gets File not found (inferred). See [What is inferred](#what-is-inferred). It then loads the font, `boot.sdi`, and `boot.wim` that the store names.
- The transfer of `boot.wim` sent 393,333 blocks with a window of 1 and no retransmits.

### End state

`vm.sh` started at 19:07:23.128. The screenshot written at 19:08:37.57 shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`, so WinPE started at most 74.4 seconds after `vm.sh` did. A second later the window shows a command prompt.

The lab host then stood still from shortly after 19:09:15 until 19:15:05. The owner reported a power event at that time. The journal has no entry in that time, and `systemd-resolved` logs `Clock change detected` at 19:15:05. `uptime -s` moved from 18:10:05 to 18:15:55, so the host's uptime missed about 350 seconds. QEMU, swtpm, the prototype, and `dumpcap` survived it without a restart. Every boot event above came before it. Commands typed into the WinPE window through QEMU's `sendkey` after it gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `reg query hklm\system\currentcontrolset\control\secureboot\state` printed `UEFISecureBootEnabled REG_DWORD 0x1`.
- `reg query hklm\system\currentcontrolset\control /v SystemBootDevice` printed `ramdisk(0)`.
- `reg query hklm\system\currentcontrolset\control /v SystemStartOptions` printed ` MININT  FVEBOOT=2715648  NOVGA RDIMAGEOFFSET=8192 RDIMAGELENGTH=3161088 RDPATH=NET(0)\BOOT\X64\BOOT.WIM`, the same value as in #29.

## The `bios` VM

### DHCP

[`bootmgr-object-type-bios-dhcp.txt`](captures/bootmgr-object-type-bios-dhcp.txt) holds the DHCP packets.

| Frame | Time | Packet |
| --- | --- | --- |
| 1 | 19:17:58.060 | The ROM's DISCOVER, xid `9395630f`, option 93 = 0 |
| 2 | 19:17:58.060 | The prototype's OFFER, with no boot file |
| 3 | 19:17:59.087 | The ROM's second DISCOVER |
| 4 | 19:17:59.088 | The prototype's OFFER again |
| 5 and 6 | 19:18:01.067 | Two OFFERs of `10.41.0.192` from dnsmasq |
| 7 | 19:18:01.070 | The ROM's lease REQUEST |
| 8 | 19:18:01.070 | dnsmasq's ACK |
| 9 | 19:18:01.070 | The ROM's boot file REQUEST, from `10.41.0.192:68` to port 4011 |
| 10 | 19:18:01.071 | The prototype's ACK to `10.41.0.192:68`, with the boot file `boot\x64\pxeboot.n12` and option 252 = `boot\bios-bootmgr-10200002.bcd` |
| 41 to 44 | 19:19:47.955 to 19:19:48.469 | WinPE's own DHCP exchange with dnsmasq. WinPE's DISCOVER and REQUEST carry option 60 `MSFT 5.0` and no option 93. |

As on `uefi`, dnsmasq's first OFFER came 3.0 seconds after the first DISCOVER. The ROM sent its DISCOVER again after 1.0 seconds, and the prototype answered both. The full capture has 3 packets more than #27's, and frames 3, 4, and 6 are the extra DHCP packets.

### TFTP requests

The capture holds the same 13 RRQs in the same order.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 19:18:02.188 | `boot\x64\pxeboot.n12` | `blksize=1432,tsize=0` | Complete, 25238 bytes in 0.004 seconds |
| 2 | 19:18:02.210 | `boot\x64\bootmgr.exe` | `blksize=512,tsize=0` | Complete, 814544 bytes in 0.167 seconds |
| 3 | 19:18:02.523 | `\boot\bios-bootmgr-10200002.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 4 | 19:18:02.524 | `\boot\bios-bootmgr-10200002.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.004 seconds |
| 5 | 19:18:02.528 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 6 | 19:18:02.541 | `\hiberfil.sys` | `tsize=0` | ERROR 1 File not found |
| 7 | 19:18:02.542 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 8 | 19:18:02.542 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 9 | 19:18:02.543 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 10 | 19:18:02.543 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.008 seconds |
| 11 | 19:18:02.553 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.521 seconds |
| 12 | 19:18:03.075 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 91.496 seconds |
| 13 | 19:19:34.853 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- The first file that `bootmgr.exe` loads is the store (inferred). See [What is inferred](#what-is-inferred). It then loads the font, `boot.sdi`, and `boot.wim` that the store names.
- The transfer of `boot.wim` sent 393,333 blocks with a window of 1 and no retransmits.

### End state

`vm.sh` started at 19:17:54.803. The screenshot written at 19:19:46.22 shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`, so WinPE started at most 111.4 seconds after `vm.sh` did. After `wpeinit`, these commands gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `ipconfig` printed `Ethernet adapter Ethernet` with the IPv4 address `10.41.0.192` and the default gateway `10.41.0.1`.
- `reg query hklm\system\currentcontrolset\control /v SystemBootDevice` printed `ramdisk(0)`.
- `reg query hklm\system\currentcontrolset\control /v SystemStartOptions` printed ` MININT  FVEBOOT=2187264 RDIMAGEOFFSET=8192 RDIMAGELENGTH=3161088 RDPATH=NET(0)\BOOT\X64\BOOT.WIM`, the same value as in #29.

## Control

The control is the pair of #27 runs on the same lab host on 2026-11-13, with the default stores `boot\uefi.bcd` and `boot\bios.bcd`. Their raw logs are in `~/deployarr-lab/runs/27/`, and [Single-stage boot chain in the PXE lab](single-stage-chain.md) records them. The boot files, `boot.sdi`, the font, and `boot.wim` have the same SHA-256 on disk now as in #27.

| Measure | `uefi`, `0x10100002` | `uefi`, `0x10200002` | `bios`, `0x10100002` | `bios`, `0x10200002` |
| --- | --- | --- | --- | --- |
| RRQs | 32 | 32 | 13 | 13 |
| `boot.wim` transfer | 52.145 seconds | 51.118 seconds | 89.254 seconds | 91.496 seconds |
| `boot.wim` blocks sent, retransmits | 393,333, 0 | 393,333, 0 | 393,333, 0 | 393,333, 0 |
| Packets in the full capture | 398,285 | 398,285 | 397,223 | 397,226 |
| TFTP ACKs in the full capture | 398,208 | 398,208 | 397,170 | 397,170 |
| WinPE `ver` | 10.0.26100.9457 | 10.0.26100.9457 | 10.0.26100.9457 | 10.0.26100.9457 |

A `diff` of the file and options of every RRQ, from the #27 log against this page's log, prints only the two lines for the store on each VM:

```text
4,5c4,5
< \boot\uefi.bcd "tsize=0"
< \boot\uefi.bcd "tsize=0,blksize=1456"
---
> \boot\uefi-bootmgr-10200002.bcd "tsize=0"
> \boot\uefi-bootmgr-10200002.bcd "tsize=0,blksize=1456"
```

```text
3,4c3,4
< \boot\bios.bcd "tsize=0"
< \boot\bios.bcd "tsize=0,blksize=1456"
---
> \boot\bios-bootmgr-10200002.bcd "tsize=0"
> \boot\bios-bootmgr-10200002.bcd "tsize=0,blksize=1456"
```

## Captures

| File | What it holds |
| --- | --- |
| [`bootmgr-object-type-uefi.pcapng`](captures/bootmgr-object-type-uefi.pcapng) | The `uefi` run, reduced |
| [`bootmgr-object-type-uefi-dhcp.txt`](captures/bootmgr-object-type-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`bootmgr-object-type-bios.pcapng`](captures/bootmgr-object-type-bios.pcapng) | The `bios` run, reduced |
| [`bootmgr-object-type-bios-dhcp.txt`](captures/bootmgr-object-type-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |

The capture filter is `udp and udp[8:2] != 3`. It keeps only UDP, and it drops TFTP DATA packets and IP fragments. Each full capture is over 30 MB, because the boot managers ACK every block of `boot.wim`. The committed files are reduced copies that drop only the TFTP ACKs, with `tshark -r - -Y 'not tftp.opcode == 4' -w -`. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 398,285 packets, 31,867,484 bytes, SHA-256 `0046e4133482d7677c1921fd414481cbb9f2fb2782f96fcde2a80f6993f4a5a6` | 398,208 | 77 packets, 10,736 bytes, SHA-256 `510b294a1936409776535db20f95ea89af40090b1dae20060bd6223a3649692f` |
| `bios` | 397,226 packets, 31,784,528 bytes, SHA-256 `b42ed21b334e48216575ae43958c7d02b7f2038bef8284a5827a63c952c1dccc` | 397,170 | 56 packets, 10,820 bytes, SHA-256 `727f536a1bbc6a2dae78e68d1bd3b5ee0f6dfa91db27fb8f26be8f75f6094ab7` |

The `dumpcap` logs, `bootmgr-object-type-<vm>-dumpcap.log` next to the full captures, report no dropped packets for either run. The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/61b/`: the prototype logs, the screenshots, the stores and their `hivexml` dumps, `notes.txt`, and `versions.txt`. A session that claimed #61 on 2026-11-13 started a `uefi` run with stores made the same way and stopped during it. Its files are in `~/deployarr-lab/runs/61/`, and this page does not use them.

## Commands

These commands read the type and make each `0x10200002` store, from copies in `~/deployarr-lab/runs/61b/stores/`. `patch_bootmgr_type.py` is the script below.

```bash
hivexget uefi.bcd '\Objects\{9dea862c-5cdd-4e70-acc1-f32b344d4795}\Description' Type
python3 -I patch_bootmgr_type.py uefi.bcd uefi-bootmgr-10200002.bcd
cmp -l uefi.bcd uefi-bootmgr-10200002.bcd
diff <(hivexml uefi.bcd | sed 's/></>\n</g') <(hivexml uefi-bootmgr-10200002.bcd | sed 's/></>\n</g')
```

```python
import re, struct, sys

src, dst = sys.argv[1], sys.argv[2]
data = bytearray(open(src, "rb").read())
old = struct.pack("<I", 0x10100002)
new = struct.pack("<I", 0x10200002)
# vk, name length 4, inline data length 4, data, REG_DWORD, flags 1, "Type"
pattern = re.escape(b"vk\x04\x00\x04\x00\x00\x80" + old + b"\x04\x00\x00\x00\x01\x00\x00\x00Type")
hits = [m.start() for m in re.finditer(pattern, bytes(data))]
if len(hits) != 1:
    sys.exit(f"expected one Type value holding 0x10100002, found {len(hits)}")
data[hits[0] + 8:hits[0] + 12] = new
open(dst, "wb").write(data)
```

Each run started the prototype, then the capture, then the VM, as [Run an experiment](../../lab-setup.md#run-an-experiment) says. `$LAB` is the checkout of `main` at `d799a1b`, and `<vm>` is `uefi` or `bios`.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/runs/61b/tftp-root --chain single --no-block-log --bcd-uefi 'boot\uefi-bootmgr-10200002.bcd' --bcd-bios 'boot\bios-bootmgr-10200002.bcd' --log-file ~/deployarr-lab/runs/61b/<vm>-prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/bootmgr-object-type-<vm>.pcapng 2> ~/deployarr-lab/captures/bootmgr-object-type-<vm>-dumpcap.log &
timeout 1800 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The screenshots came from the HMP command `screendump <file>.png -f png` on the monitor socket, about once a second. The WinPE commands went in one key at a time with the HMP command `sendkey`. After the last packet, `sudo pkill -INT -x dumpcap` stopped the capture, the HMP command `quit` stopped the VM, and `sudo pkill -INT -f '^python3 -B .*pxe_prototype.py'` stopped the prototype.

These commands make the committed files:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/bootmgr-object-type-<vm>.pcapng > docs/research/pxe-lab/captures/bootmgr-object-type-<vm>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/bootmgr-object-type-<vm>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/bootmgr-object-type-<vm>.pcapng > docs/research/pxe-lab/captures/bootmgr-object-type-<vm>-dhcp.txt
```

## What is inferred

- The runs do not show whether either boot manager reads the `{bootmgr}` type at all (inferred). They show only that both types boot.
- dnsmasq's 3.0-second delay before its first OFFER on each VM comes from the restart, which left it with no lease for either MAC address (inferred). In the #27 runs, both OFFERs came at once. The delay ends before the boot file REQUEST, so it does not touch the boot managers.
- Which program sent each RRQ is inferred, because neither the capture nor the log names the program. RRQs 3 to 31 on `uefi` come from `bootmgfw.efi`, and RRQs 3 to 12 on `bios` from `bootmgr.exe`. The last RRQ on each VM, for `UnlockToken.pol`, comes from the OS loader that the boot manager starts. Pull request [#138](https://github.com/Tsukidy/deployarr/pull/138) gives the reasons for the same requests in #29.

## Not covered

- The `uefi2023` VM, whose `db` holds only the Windows UEFI CA 2023, and the 2023-signed boot files. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs those.
- The two-stage chain with a `0x10200002` store. `boot/uefi.bcd` and `boot/bios.bcd`, with `0x10100002`, boot in both chains, as #28 records.
- The two-stage chain with the `-nopath` stores. #29 boots them only in the single stage.
- A store with no loader `path` and `0x10200002`.
- A `0x10200002` store that Windows writes, through `BcdStore.CreateObject`, and ConfigMgr's own store.
- Physical PCs and other firmware.

## What changes in deployarr

- Nothing, for the type. `bcdedit /create {bootmgr}` in `tools/bcd/New-BcdStores.ps1` gives `0x10100002`, and that type boots on the `uefi` VM with Secure Boot on and on the `bios` VM, in both chains. The script stays as it is, and [BCD](../../architecture.md#bcd) needs no object type.
- deployarr does not need the BCD WMI provider, or a patch to the store, to match ConfigMgr's `0x10200002`.
- #32 boots the same stores on `uefi2023`. If a store fails there, the type is one difference from ConfigMgr's store to test.
