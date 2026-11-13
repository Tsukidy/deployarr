# Two-stage boot chain in the PXE lab

This page records what `wdsmgfw.efi` and `wdsnbp.com` send in their second request, and which files the boot manager then requests over TFTP. It answers risk 2 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first), for [#28](https://github.com/Tsukidy/deployarr/issues/28). The runs are from 2026-11-13, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

| VM | Second request | Option 250 in it | Second ACK | End state |
| --- | --- | --- | --- | --- |
| `uefi`, OVMF with Secure Boot on | From `wdsmgfw.efi`, by unicast from `10.41.0.176:68` to `10.41.0.1:4011` | `0c 01 02 0d 02 08 00 01 02 00 07 0e 01 00 ff` | `boot\x64\bootmgfw.efi`, option 252 = `boot\uefi.bcd` | WinPE 10.0.26100.9457 at a command prompt. WinPE reports `UEFISecureBootEnabled` = 1. |
| `bios`, SeaBIOS with QEMU's iPXE ROM | From `wdsnbp.com`, by unicast from `10.41.0.192:68` to `10.41.0.1:4011` | `0d 02 08 00 0e 01 01 01 02 00 06 05 04 00 00 00 00 ff` | `boot\x64\pxeboot.n12`, option 252 = `boot\bios.bcd` | WinPE 10.0.26100.9457 at a command prompt, with an address from dnsmasq. |

- Both network boot programs send their requests to port 4011 of the server, by unicast. Neither sends anything to port 67. The case in which dnsmasq takes a unicast request to port 67 did not happen.
- Option 250 in each second request is a run of sub-options with a `0xFF` byte at its end. The sub-options decode as follows. The names come from Microsoft's `WdsBp.h` ([Sub-option names](#sub-option-names)).

| Sub-option | Name in `WdsBp.h` | `uefi` value | `bios` value |
| --- | --- | --- | --- |
| 12 | `WDSBP_OPT_PXE_PROMPT_DONE` | 2 (`WDSBP_OPTVAL_PXE_PROMPT_NOPROMPT`) | Not sent |
| 13 | `WDSBP_OPT_NBP_VER` | `0x0800` (`WDSBP_OPTVAL_NBP_VER_8`) | `0x0800` (`WDSBP_OPTVAL_NBP_VER_8`) |
| 14 | `WDSBP_OPT_ACTION_DONE` | 0 | 1 |
| 1 | `WDSBP_OPT_ARCHITECTURE` | 7 | 6 |
| 5 | `WDSBP_OPT_REQUEST_ID` | Not sent | `00 00 00 00` |

- The table lists the sub-options in each program's order. `wdsmgfw.efi` sends 12, 13, 1, and 14. `wdsnbp.com` sends 13, 14, 1, and 5.
- On `bios`, `wdsnbp.com` first sends a request with no option 250. The prototype answers it with the first ACK again. `wdsnbp.com` then shows a screen that asks for an administrator's approval, and 0.70 seconds later it sends the request with option 250.
- Both second requests keep option 93 from the firmware, 7 on `uefi` and 0 on `bios`. Sub-option 1 is 6 on `bios`, where `wdsnbp.com` prints `Architecture: x64`.
- Once the boot manager runs, it requests the same files in the same order as in the single-stage run of [#27](https://github.com/Tsukidy/deployarr/issues/27). It loads the BCD that option 252 names in the second ACK.
- Neither VM waited for a key press. No key went to either VM before WinPE showed its command prompt.
- In WinPE, `BootServerReply` holds the second ACK on both VMs.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts | `tools/lab/` from [#127](https://github.com/Tsukidy/deployarr/pull/127), branch `lab-120-ubuntu-2604` at `786c13b`. Its `tools/lab/` differs from `main` at `ef0bf5e` only in `check.sh` and in the swtpm socket path of `vm.sh`. |
| PXE prototype | `tools/pxe-prototype/`, the same on that branch as on `main` at `ef0bf5e` |
| Network | `br-lab` at `10.41.0.1/24`. dnsmasq hands out addresses on it from `0.0.0.0%br-lab:67`, and the prototype answers PXE on ports 67 and 4011. |
| Chain | `--chain two-stage`. The first ACK names the WDS network boot program and carries option 250. A request with option 250 gets the boot manager and option 252. |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new.

The prototype's first-stage option 250 follows the ConfigMgr recipes in [Two-stage boot chain](../configmgr-2509.md#two-stage-boot-chain), from `BOOT_TABLE` in `tools/pxe-prototype/policy.py` (lines 61 to 68) and the recipes in `tools/pxe-prototype/wds250.py` (lines 38 to 46):

- `uefi` gets `boot\x64\wdsmgfw.efi` and the zero-touch option, `FA 03 0B 01 02`. That is sub-option 11 = 2 only.
- `bios` gets `boot\x64\wdsnbp.com` and the normal option, 30 bytes: sub-option 2 = 1, 5 = `00 00 00 00`, 3 = 20, 4 = 186, 6 = `SCCM PXE`, and 11 = 1. Sub-option 5 is zero because the ROM's request has no option 5.

The prototype chooses the second step when a request has option 250 (`_step` in `policy.py`, lines 153 to 157). It chooses the firmware type from option 93, not from sub-option 1 (`_FIRMWARE_BY_ARCH`, line 71).

### BCD stores

Each boot used the prototype's default store for its firmware type, as #27 did. Both stores come from the `bcd-stores` artifact of [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137), whose zip has the SHA-256 `5c5eff6591370703d027b613ec2259d79411e84fdd44301317932619c1ae8c97`. The hashes match `~/deployarr-lab/tftp/manifest.txt` and the files on disk.

| Store | Boot | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `boot/uefi.bcd` | `uefi` | 12288 | `c0164082153aa4eeb826b68d9e3acb4ad815fbf64b8dc1b8a51ef0f02d010224` |
| `boot/bios.bcd` | `bios` | 12288 | `5682186b6906955f5f5e610aee7dc9eb66e3d5314d31141c5ff9b1a0750e7b0d` |

These are the default stores of `tools/bcd/New-BcdStores.ps1`. They set the loader `path` and leave out `ramdisktftpblocksize` and `ramdisktftpwindowsize`. The runs request the paths that the script writes into the stores:

- `fontpath` `\boot\fonts` (`New-BcdStores.ps1` line 74)
- `ramdisksdipath` `\boot\boot.sdi` (line 78)
- `device` and `osdevice` `ramdisk=[boot]\boot\x64\boot.wim,{ramdiskoptions}` (lines 87 to 89)

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
| Python | 3.14.4 |

| Firmware file | SHA-256 |
| --- | --- |
| `/usr/share/OVMF/OVMF_CODE_4M.ms.fd` | `33c413a04e9edf512c504e9b0accbcec4d009d406bca761cc0b8eacc93da60b6` |
| `/usr/share/OVMF/OVMF_VARS_4M.ms.fd` | `d2e3a79d28c1c93286048a3e876570882dab4538d6a842ab2e0e53265cbcc983` |
| `/usr/share/seabios/bios-256k.bin` | `e26615f9ad430328f49ca105e570b2dc4490a08a34ea73d27cae8b809a30ee06` |

The boot files come from index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`. The ISO has the SHA-256 `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650`. Index 1 is Microsoft Windows PE (amd64), build 26100.9457. The issuer is the `Issuer :` line under `Signer's certificate:` in the output of `osslsigncode verify`, which reads only PE files. The version is the file version in the `VS_FIXEDFILEINFO` that follows each file's `VS_VERSION_INFO` key.

| File in the TFTP root | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `boot/x64/wdsmgfw.efi` | 1171400 | `b3794319f7b6d68f993801d232737e6d73b95340590d23814f651562e534b9fc` | Microsoft Windows Production PCA 2011 | 10.0.26100.30227 |
| `boot/x64/bootmgfw.efi` | 3087400 | `4bb32c300df0815d8b6c54a0d3793ae1d78fcdc07f8e2b9f831289b2eb109092` | Microsoft Windows Production PCA 2011 | 10.0.28000.367 |
| `boot/x64/wdsnbp.com` | 30832 | `2b2fb3a7cfba1ef640bcb5d75050e57c79ff639ec621b0162f837c2c889ca178` | Not a PE file | None |
| `boot/x64/pxeboot.n12` | 25238 | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Not a PE file | None |
| `boot/x64/bootmgr.exe` | 814544 | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |
| `boot/boot.sdi` | 3170304 | `cd2c00ce027687ce4a8bdc967f26a8ab82f651c9becd703658ba282ec49702bd` | Not a PE file | None |
| `boot/fonts/wgl4_boot.ttf` | 50096 | `23872d2eba60bbb3bd2585e925c4b48769fb6bb0cfa7f4a6ba4e8f03bcaf2c35` | Not a PE file | None |
| `boot/x64/boot.wim` | 572691590 | `c0a18c72a7f84c36120e5681bb325e2c892e919886671e553a6129899a09097a` | Not a PE file | WinPE 10.0.26100.9457 |

## The `uefi` VM

### DHCP

The table lists every DHCP packet in [`two-stage-chain-uefi-dhcp.txt`](captures/two-stage-chain-uefi-dhcp.txt). Frame numbers are those of the committed capture. Times are UTC.

| Frame | Time | From | To | Packet |
| --- | --- | --- | --- | --- |
| 1 | 10:58:53.385 | `0.0.0.0:68` | `255.255.255.255:67` | OVMF's DISCOVER, xid `e8cd1566`, option 93 = 7, option 97 = `4c414200-0000-4000-8000-000000000011` |
| 2 | 10:58:53.385 | `10.41.0.1:67` | `255.255.255.255:68` | dnsmasq's OFFER of `10.41.0.176` |
| 3 | 10:58:53.385 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER, with no boot file |
| 4 | 10:58:57.124 | `0.0.0.0:68` | `255.255.255.255:67` | OVMF's lease REQUEST. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. |
| 5 | 10:58:57.125 | `10.41.0.1:67` | `255.255.255.255:68` | dnsmasq's ACK |
| 6 | 10:58:57.125 | `10.41.0.176:4011` | `10.41.0.1:4011` | OVMF's boot file REQUEST, xid `71d011eb` |
| 7 | 10:58:57.126 | `10.41.0.1:4011` | `10.41.0.176:4011` | First ACK, `boot\x64\wdsmgfw.efi`, options 53, 54, 97, 60, and 250 = `0b 01 02` |
| 13 | 10:58:58.315 | `10.41.0.176:68` | `10.41.0.1:4011` | `wdsmgfw.efi`'s second request, xid `40e20100` |
| 14 | 10:58:58.315 | `10.41.0.1:4011` | `10.41.0.176:68` | Second ACK, `boot\x64\bootmgfw.efi`, options 53, 54, 97, 60, and 252 = `boot\uefi.bcd` |

The second request in frame 13 has these fields:

- `op` 1, xid `40e20100`, `secs` 65535, flags `0x0000`, `ciaddr` `10.41.0.176`, `siaddr` `10.41.0.1`, and an empty `sname` and `file`.
- Options in this order: 93 = 7, 97 = `4c414200-0000-4000-8000-000000000011`, 53 = 3 (REQUEST), 60 = `PXEClient`, 55 = 60 and 128 to 135, 250, and 255.
- Option 250, 15 bytes:

```text
0c 01 02  0d 02 08 00  01 02 00 07  0e 01 00  ff
```

The prototype logs the same request, and names only sub-option 1:

```text
2026-11-13T10:58:58.314+00:00 dhcp-received port=4011 source=10.41.0.176:68 type=REQUEST xid=40e20100 mac=52:54:00:41:00:11 arch=0007 guid=000042414c000000408000000000000011 option_250="unknown(12)=02,unknown(13)=0800,architecture(1)=0007,unknown(14)=00,unparsed=ff" options=60,55
2026-11-13T10:58:58.314+00:00 dhcp-sent port=4011 destination=10.41.0.176:68 type=ACK xid=40e20100 firmware=uefi-x64 step=second file=boot\x64\bootmgfw.efi option_252=boot\uefi.bcd
```

The serial log shows the firmware and `wdsmgfw.efi`:

```text
>>Start PXE over IPv4.
  Station IP address is 10.41.0.176

  Server IP address is 10.41.0.1
  NBP filename is boot\x64\wdsmgfw.efi
  NBP filesize is 1171400 Bytes
 Downloading NBP file...

  NBP file downloaded successfully.
BdsDxe: loading Boot0003 "UEFI PXEv4 (MAC:525400410011)" from PciRoot(0x0)/Pci(0x3,0x0)/MAC(525400410011,0x1)/IPv4(0.0.0.0,0x0,DHCP,0.0.0.0,0.0.0.0,0.0.0.0)
BdsDxe: starting Boot0003 "UEFI PXEv4 (MAC:525400410011)" from PciRoot(0x0)/Pci(0x3,0x0)/MAC(525400410011,0x1)/IPv4(0.0.0.0,0x0,DHCP,0.0.0.0,0.0.0.0,0.0.0.0)
Windows Deployment Services (Server IP: 10.41.0.1)
Contacting Server (10.41.0.1): -
ESC=Exit
```

The last three lines come from `wdsmgfw.efi`. The serial log draws them with cursor moves, and the block above shows them as text. `wdsmgfw.efi` showed no key prompt and no approval screen.

### TFTP requests

The table lists every RRQ in order, from the prototype log. The capture holds the same 34 RRQs in the same order. "OACK, then client ERROR" is a size probe. The server sent an OACK with `tsize`, and the client then ended the transfer.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 10:58:58.125 | `boot\x64\wdsmgfw.efi` | `tsize=0,blksize=1468,windowsize=4` | OACK, then client ERROR 8 `User aborted the transfer` |
| 2 | 10:58:58.129 | `boot\x64\wdsmgfw.efi` | `blksize=1468,windowsize=4` | Complete, 1171400 bytes in 0.045 seconds |
| 3 | 10:58:58.316 | `\boot\x64\bootmgfw.efi` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 4 | 10:58:58.317 | `\boot\x64\bootmgfw.efi` | `tsize=0,blksize=1456` | Complete, 3087400 bytes in 0.263 seconds |
| 5 | 10:58:58.713 | `\boot\x64\BCD` | `tsize=0` | ERROR 1 File not found |
| 6 | 10:58:58.714 | `\boot\uefi.bcd` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 7 | 10:58:58.714 | `\boot\uefi.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.002 seconds |
| 8 | 10:58:58.717 | `\EFI\Microsoft\Boot\Policies\SbcpFlightToken.p7b` | `tsize=0` | ERROR 1 File not found |
| 9 | 10:58:58.771 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 10 | 10:58:58.786 | `\EFI\Microsoft\Boot\SiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 11 | 10:58:58.787 | `\EFI\Microsoft\Boot\SkuSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 12 | 10:58:58.788 | `\EFI\Microsoft\Boot\WinSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 13 | 10:58:58.788 | `\EFI\Microsoft\Boot\ATPSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 14 | 10:58:58.788 | `\EFI\Microsoft\Boot\SiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 15 | 10:58:58.789 | `\EFI\Microsoft\Boot\SkuSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 16 | 10:58:58.789 | `\EFI\Microsoft\Boot\WinSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 17 | 10:58:58.789 | `\EFI\Microsoft\Boot\ATPSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 18 | 10:58:58.789 | `\EFI\Microsoft\Boot\VbsSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 19 | 10:58:58.789 | `\EFI\Microsoft\Boot\SiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 20 | 10:58:58.789 | `\EFI\Microsoft\Boot\SkuSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 21 | 10:58:58.790 | `\EFI\Microsoft\Boot\WinSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 22 | 10:58:58.790 | `\EFI\Microsoft\Boot\ATPSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 23 | 10:58:58.790 | `\EFI\Microsoft\Boot\VbsSiPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 24 | 10:58:58.817 | `\boot\x64\bootmgfw.efi` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 25 | 10:58:58.818 | `\boot\x64\bootmgfw.efi` | `tsize=0,blksize=1456` | Complete, 3087400 bytes in 0.279 seconds |
| 26 | 10:58:59.102 | `\EFI\Microsoft\Boot\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 27 | 10:58:59.102 | `\boot\x64\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 28 | 10:58:59.103 | `\boot\x64\boot.wim` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 29 | 10:58:59.104 | `\boot\boot.sdi` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 30 | 10:58:59.104 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 31 | 10:58:59.105 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.005 seconds |
| 32 | 10:58:59.118 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.288 seconds |
| 33 | 10:58:59.407 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 50.685 seconds |
| 34 | 10:59:50.240 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQs 1 and 2 come from OVMF, from client ports 1952 and 1953.
- RRQs 3 and 4 come from `wdsmgfw.efi`, 2 milliseconds after the second ACK. It requests the boot file of the second ACK with a leading `\`, probes its size, and downloads it with `blksize` 1456 and no `windowsize`.
- RRQs 5 to 34 come from `bootmgfw.efi`. They match RRQs 3 to 32 of the `uefi` run in #27, file for file and option for option. The first file it loads is `\boot\uefi.bcd`, the option 252 path of the second ACK, after `\boot\x64\BCD` gets File not found.
- Every transfer finished with no retransmits. The transfer of `boot.wim` sent 393,333 blocks with a window of 1.

### End state

The screen during RRQ 33 shows `Loading files...`, a progress bar, and `IP: 10.41.0.1, File: \boot\x64\boot.wim`. At 10:59:56, about 66 seconds after `vm.sh` started, the screen shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`. A second later the window shows a command prompt. Commands typed into that window through QEMU's `sendkey` gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `reg query hklm\system\currentcontrolset\control\secureboot\state` printed `UEFISecureBootEnabled REG_DWORD 0x1`.
- `hklm\system\controlset001\control\pxe` holds two values, `DHCPServerACK` and `BootServerReply`.
- `DHCPServerACK` starts with xid `E8CD1566`, flags `8000`, `yiaddr` `10.41.0.176`, and `siaddr` `10.41.0.1`. That is dnsmasq's ACK in frame 5.
- `BootServerReply` holds the second ACK in frame 14. Its bytes start with xid `40E20100`, `secs` `FFFF`, `ciaddr` `10.41.0.176`, and `siaddr` `10.41.0.1`. They hold the boot file `boot\x64\bootmgfw.efi` and options 53, 54, 97, 60, and 252 = `boot\uefi.bcd`, then the end option. Every byte after the end option is zero.
- `ipconfig` printed `Windows IP Configuration` and no adapter. The last packet in the capture is RRQ 34 at 10:59:50.

## The `bios` VM

### DHCP

The table lists every DHCP packet in [`two-stage-chain-bios-dhcp.txt`](captures/two-stage-chain-bios-dhcp.txt). Frame numbers are those of the committed capture.

| Frame | Time | From | To | Packet |
| --- | --- | --- | --- | --- |
| 1 | 11:03:25.080 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, xid `b00e0532`, option 93 = 0, option 97 = `4c414200-0000-4000-8000-000000000021` |
| 2 | 11:03:25.080 | `10.41.0.1:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` |
| 3 | 11:03:25.080 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER, with no boot file |
| 4 | 11:03:26.072 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. |
| 5 | 11:03:26.073 | `10.41.0.1:67` | `10.41.0.192:68` | dnsmasq's ACK |
| 6 | 11:03:26.073 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, flags `0x8000` |
| 7 | 11:03:26.074 | `10.41.0.1:4011` | `10.41.0.192:68` | First ACK, `boot\x64\wdsnbp.com`, options 53, 54, 97, 60, and 250 (30 bytes) |
| 10 | 11:03:29.464 | `10.41.0.192:68` | `10.41.0.1:4011` | `wdsnbp.com`'s first request, with no option 250 |
| 11 | 11:03:29.465 | `10.41.0.1:4011` | `10.41.0.192:68` | The first ACK again, `boot\x64\wdsnbp.com` with the same option 250 |
| 12 | 11:03:30.162 | `10.41.0.192:68` | `10.41.0.1:4011` | `wdsnbp.com`'s request with option 250 |
| 13 | 11:03:30.162 | `10.41.0.1:4011` | `10.41.0.192:68` | Second ACK, `boot\x64\pxeboot.n12`, options 53, 54, 97, 60, and 252 = `boot\bios.bcd` |
| 44 to 47 | 11:05:14.941 to 11:05:15.451 | | | WinPE's own DISCOVER, OFFER, REQUEST, and ACK, with option 60 `MSFT 5.0` and no option 93. The prototype logs them as `dhcp-ignored` with `reason=no-option-93`, and dnsmasq gives WinPE `10.41.0.192` again. |

The first ACK's option 250, 30 bytes:

```text
02 01 01  05 04 00 00 00 00  03 02 00 14  04 02 00 ba  06 08 53 43 43 4d 20 50 58 45  0b 01 01
```

Both requests from `wdsnbp.com` reuse the ROM's xid, `b00e0532`. Both have `ciaddr` `10.41.0.192`, `siaddr` 0.0.0.0, flags `0x0000`, and an empty `sname` and `file`.

- Frame 10 has `secs` 65535 and options 53 = 3, 60 = `PXEClient`, 97, 93 = 0, 55, and 255.
- Frame 12 has `secs` 0 and options 53 = 3, 60 = `PXEClient`, 97, 93 = 0, 250, 55, and 255. Option 55 asks for options 3, 1, 60, and 128 to 135 in both.
- Option 250 of frame 12, 18 bytes:

```text
0d 02 08 00  0e 01 01  01 02 00 06  05 04 00 00 00 00  ff
```

The prototype logs the three requests and its answers:

```text
2026-11-13T11:03:29.464+00:00 dhcp-received port=4011 source=10.41.0.192:68 type=REQUEST xid=b00e0532 mac=52:54:00:41:00:21 arch=0000 guid=000042414c000000408000000000000021 options=60,55
2026-11-13T11:03:29.468+00:00 dhcp-sent port=4011 destination=10.41.0.192:68 type=ACK xid=b00e0532 firmware=bios step=first file=boot\x64\wdsnbp.com option_250="next_action(2)=01,request_id(5)=00000000,poll_interval(3)=0014,poll_retries(4)=00ba,message(6)=5343434d20505845,pxe_prompt(11)=01"
2026-11-13T11:03:30.161+00:00 dhcp-received port=4011 source=10.41.0.192:68 type=REQUEST xid=b00e0532 mac=52:54:00:41:00:21 arch=0000 guid=000042414c000000408000000000000021 option_250="unknown(13)=0800,unknown(14)=01,architecture(1)=0006,request_id(5)=00000000,unparsed=ff" options=60,55
2026-11-13T11:03:30.168+00:00 dhcp-sent port=4011 destination=10.41.0.192:68 type=ACK xid=b00e0532 firmware=bios step=second file=boot\x64\pxeboot.n12 option_252=boot\bios.bcd
```

Between frames 11 and 12, the screen shows this text from iPXE and `wdsnbp.com`. The repeated words in the middle are on the screen as shown.

```text
Filename: boot\x64\wdsnbp.com
tftp://10.41.0.1/boot\x64\wdsnbp.com... ok
boot\x64\wdsnbp.com : 30832 bytes [PXE-NBP]

Downloaded WDSNBP from 10.41.0.1

WDSNBP started using DHCP Referral.
Contacting Server: 10.41.0.1 (Gateway: 0.0.0.0).
Architecture: x64

The details below show the information relating to the PXE boot request for
this computer. Please provide these detailating to the PXE boot request for
this computer. Please provide these details to your Windows Deployment Services
Administrator so that this request can be approved.

Pending Request ID: 0

Message from Administrator:
    SCCM PXE

Contacting Server: 10.41.0.1._
```

The VM's serial log is empty, because SeaBIOS and iPXE write to the screen only.

### TFTP requests

The capture holds the same 14 RRQs in the same order.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 11:03:29.208 | `boot\x64\wdsnbp.com` | `blksize=1432,tsize=0` | Complete, 30832 bytes in 0.004 seconds |
| 2 | 11:03:30.239 | `boot\x64\pxeboot.n12` | `blksize=512,tsize=0` | Complete, 25238 bytes in 0.006 seconds |
| 3 | 11:03:30.245 | `boot\x64\bootmgr.exe` | `blksize=512,tsize=0` | Complete, 814544 bytes in 0.169 seconds |
| 4 | 11:03:30.526 | `\boot\bios.bcd` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 5 | 11:03:30.527 | `\boot\bios.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.003 seconds |
| 6 | 11:03:30.530 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 7 | 11:03:30.544 | `\hiberfil.sys` | `tsize=0` | ERROR 1 File not found |
| 8 | 11:03:30.544 | `\boot\x64\boot.wim` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 9 | 11:03:30.545 | `\boot\boot.sdi` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 10 | 11:03:30.546 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | OACK, then client ERROR 0 `TFTP Aborted` |
| 11 | 11:03:30.546 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.009 seconds |
| 12 | 11:03:30.557 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.509 seconds |
| 13 | 11:03:31.067 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 90.772 seconds |
| 14 | 11:05:02.075 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQ 1 comes from the iPXE ROM.
- RRQ 2 comes from `wdsnbp.com`, 77 milliseconds after the second ACK. It requests the second ACK's boot file as written, with no leading `\`, and with `blksize` 512.
- RRQ 3 comes from `pxeboot.n12`. It requests `boot\x64\bootmgr.exe`, in the folder of its own boot file.
- RRQs 3 to 14 match RRQs 2 to 13 of the `bios` run in #27, file for file. `bootmgr.exe` requests the option 252 path of the second ACK as its first file.
- Every transfer finished with no retransmits. The transfer of `boot.wim` sent 393,333 blocks with a window of 1.

### End state

At 11:05:13, about 111 seconds after `vm.sh` started, the screen shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`. After `wpeinit`, these commands gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `ipconfig` printed `Ethernet adapter Ethernet` with the IPv4 address `10.41.0.192` and the default gateway `10.41.0.1`.
- `hklm\system\controlset001\control\pxe` holds two values, `DHCPServerACK` and `BootServerReply`.
- `BootServerReply` starts with the second ACK in frame 13. Its bytes start with xid `B00E0532`, `secs` 0, flags 0, `ciaddr` `10.41.0.192`, and `siaddr` `10.41.0.1`. They hold the boot file `boot\x64\pxeboot.n12` and options 53, 54, 97, 60, and 252 = `boot\bios.bcd`, then the end option and the prototype's 5 bytes of padding, up to byte 300.
- After byte 300, `BootServerReply` holds 50 more bytes before its zeros begin. Bytes 300 to 311 equal bytes 300 to 311 of the first ACK, the end of its option 250 and its end option. Bytes 312 to 349 are `00 00 21`, the first ACK's option 250 again, and `af 01 ff`.

The commands that read these bytes printed parts of the value with `set` substrings, so that each part fits on one line. The value names came from a `for /f` loop, because WinPE has no `findstr`.

## Sub-option names

Microsoft's `WdsBp.h` defines the option 250 sub-options and their values. The copy used here is in Microsoft's [win32metadata repository](https://github.com/microsoft/win32metadata/blob/15fc870a5e96df7c560877ff30136c0fb5a21e9b/generation/WinSDK/RecompiledIdlHeaders/um/WdsBp.h), at commit `15fc870a`:

- Lines 71 to 86 define the sub-options. 1 is `WDSBP_OPT_ARCHITECTURE`, a `USHORT`. 2 is `WDSBP_OPT_NEXT_ACTION`, 5 is `WDSBP_OPT_REQUEST_ID`, a `ULONG`, and 6 is `WDSBP_OPT_MESSAGE`. 11 is `WDSBP_OPT_PXE_CLIENT_PROMPT`. 12 is `WDSBP_OPT_PXE_PROMPT_DONE`, a `BYTE`. 13 is `WDSBP_OPT_NBP_VER`, a `USHORT`. 14 is `WDSBP_OPT_ACTION_DONE`, a `BYTE`.
- Lines 121 to 123 give `WDSBP_OPTVAL_ACTION_APPROVAL` = 1, `REFERRAL` = 3, and `ABORT` = 5 for sub-option 2.
- Lines 129 to 131 give `WDSBP_OPTVAL_PXE_PROMPT_OPTIN` = 1, `NOPROMPT` = 2, and `OPTOUT` = 3, for sub-options 11 and 12.
- Lines 137 and 138 give `WDSBP_OPTVAL_NBP_VER_7` = `0x0700` and `WDSBP_OPTVAL_NBP_VER_8` = `0x0800` for sub-option 13.

The header defines no end marker and no values for sub-options 1 and 14.

## Captures

| File | What it holds |
| --- | --- |
| [`two-stage-chain-uefi.pcapng`](captures/two-stage-chain-uefi.pcapng) | The `uefi` run, reduced |
| [`two-stage-chain-uefi-dhcp.txt`](captures/two-stage-chain-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`two-stage-chain-bios.pcapng`](captures/two-stage-chain-bios.pcapng) | The `bios` run, reduced |
| [`two-stage-chain-bios-dhcp.txt`](captures/two-stage-chain-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |

`dumpcap` captured with the filter `udp and udp[8:2] != 3`, which drops TFTP DATA packets and IP fragments. Each full capture was over 30 MB, because the boot managers ACK every block of `boot.wim`. The committed files are reduced copies that drop only the TFTP ACKs, with `tshark -r - -Y 'not tftp.opcode == 4' -w -`. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 400,088 packets, 32,012,408 bytes, SHA-256 `56d4540f552c421e91d2623f7a3d18511c56f3966f302e60c51ceececa18402f` | 400,004 | 84 packets, 11,980 bytes, SHA-256 `788a1ebf446599acc00298082061259f2d641721ea17aace2c9bb757b8593514` |
| `bios` | 397,284 packets, 31,789,404 bytes, SHA-256 `97be5367f4e3e32662205ba3d8a5a4693f4c5a06bd491f5a4672753d7f5fda81` | 397,225 | 59 packets, 11,296 bytes, SHA-256 `ae0ae6763f694ff8eefa09e1ec2b06003b973decb463f6482ecb365e12f91e87` |

- The committed `uefi` copy holds 9 DHCP packets, 34 RRQs, 14 OACKs, and 27 TFTP ERRORs.
- The committed `bios` copy holds 15 DHCP packets, 14 RRQs, 11 OACKs, 7 TFTP ERRORs, and 12 LLMNR and NetBIOS name packets from WinPE.
- `dumpcap` reported no dropped packets for either run.
- The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/28/`: the prototype logs, the serial logs, the screenshots, and `versions.txt`.

## Commands

Each run started the prototype, then the capture, then the VM, as [Run an experiment](../../lab-setup.md#run-an-experiment) says. The lab scripts ran from a checkout of `lab-120-ubuntu-2604`, here `$LAB`. `<vm>` is `uefi` or `bios`.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain two-stage --no-block-log --log-file ~/deployarr-lab/runs/28/<vm>-prototype.log
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/two-stage-chain-<vm>.pcapng
timeout 1800 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The screenshots came from the HMP command `screendump <file>.png -f png` on the monitor socket, once a second during the boot. The WinPE commands went in one key at a time with the HMP command `sendkey`. These are the commands as typed at the prompt, where `for /f` takes a single `%`:

```bat
ver
reg query hklm\system\currentcontrolset\control\secureboot\state
ipconfig
for /f "tokens=1,2" %a in ('reg query hklm\system\controlset001\control\pxe') do @echo %a %b
reg query hklm\system\controlset001\control\pxe
for /f "tokens=3" %a in ('reg query hklm\system\controlset001\control\pxe /v BootServerReply') do @set v=%a
echo %v:~0,48%
echo %v:~464,72%
```

The last command ran for the offsets 464, 536, 608, and 680. After the last packet, `sudo pkill -INT -x dumpcap` stopped the capture, and the HMP command `quit` stopped the VM.

The committed files came from these commands:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/two-stage-chain-<vm>.pcapng > docs/research/pxe-lab/captures/two-stage-chain-<vm>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/two-stage-chain-<vm>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/two-stage-chain-<vm>.pcapng > docs/research/pxe-lab/captures/two-stage-chain-<vm>-dhcp.txt
```

## What is inferred

- The meanings of the sub-options come from the names in `WdsBp.h` (inferred). The header gives names, types, and some values, not behavior.
- Sub-option 1 = 6 means x64 BIOS to `wdsnbp.com` (inferred). `wdsnbp.com` prints `Architecture: x64` on the screen in the same run. The [ConfigMgr 2509 reference](../configmgr-2509.md#two-stage-boot-chain) says `sccmpxe.exe` gives BIOS x64 for 2, 3, or 6, and UEFI x64 for 7 or 9. The lab has no x86 VM, so no run shows what `wdsnbp.com` sends on an x86 CPU.
- `wdsnbp.com` shows the approval screen because the first ACK's sub-option 2 is 1, `WDSBP_OPTVAL_ACTION_APPROVAL` (inferred). The screen shows the first ACK's sub-option 5 as `Pending Request ID: 0` and sub-option 6 as the message. Sub-option 14 = 1 in the next request suggests that the approval step is done (inferred from the name). `wdsnbp.com` sent that request 0.70 seconds after frame 11, not after the 20 seconds of sub-option 3.
- RRQs 3 and 4 on `uefi` come from `wdsmgfw.efi`, and RRQ 2 on `bios` comes from `wdsnbp.com` (inferred). Each follows the second ACK within 77 milliseconds and requests its boot file. The boot manager cannot run before it is downloaded.
- The boot managers read option 252 from the second ACK (inferred). That ACK is the only packet that carries option 252 on the wire, and each boot manager requests its path.
- The bytes after byte 300 of the `bios` `BootServerReply` are left over from an older packet in the same buffer (inferred). Bytes 300 to 311 equal the end of the first ACK, which is 312 bytes long. The second ACK is 300 bytes long. The page does not show which program keeps that buffer, or where the next 38 bytes come from.
- WinPE on the `uefi` VM has no adapter because it has no driver for the virtio-net card (inferred). The [Test lab](../../roadmap.md#test-lab) section of the roadmap says Windows 11 has no inbox virtio drivers, and the `bios` VM's `e1000` card works in the same WinPE.

## Not covered

- The `uefi2023` VM, whose `db` holds only the Windows UEFI CA 2023. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs the 2023-signed boot files there.
- The other first-stage options: the abort option, the key prompt of `pxeboot.com`, and a UEFI option 250 other than sub-option 11 = 2.
- A server that does not answer `wdsnbp.com`'s request without option 250.
- Relayed requests, and a server whose port 67 is not shared with dnsmasq.
- Physical PCs, x86 PCs, and other firmware.

## What changes in deployarr

- The two-stage chain boots WinPE on the `uefi` VM with Secure Boot on and on the `bios` VM. The single stage did too, in #27. The choice between the chains still waits for the `uefi2023` result in #32. #33 makes it.
- The network boot programs send their requests with option 250 to port 4011, by unicast, from client port 68. Port 4011 must answer them, and the reply must go to the request's source address and port, as [DHCP](../../architecture.md#dhcp) says. No lab client sent option 250 to port 67, so the port 67 rule for option 250 has no measured client. #33 decides whether to keep it.
- The option 250 parser must accept sub-options 12, 13, and 14, and a `0xFF` byte at the end of option 250. Both programs send that byte, and `WdsBp.h` does not define it.
- The server must answer a request from `wdsnbp.com` that has no option 250 with the first-stage reply again. With the reply rules in [DHCP](../../architecture.md#dhcp), every request is answered on its own, and that holds.
- The second ACK should follow sub-option 1, as `sccmpxe.exe` does, not option 93. On BIOS, option 93 stays 0 in the second request, and sub-option 1 is 6 for an x64 CPU. This is how [Boot chain](../../architecture.md#boot-chain) can tell x64 BIOS PCs from others. The x86 value still needs a measurement.
- `wdsnbp.com` shows sub-option 6 on screen as `Message from Administrator`, under an approval text. deployarr should send its own text there, not `SCCM PXE`. With sub-option 2 = 1, `wdsnbp.com` sent its next request 0.70 seconds after the reply, with no key press.
- The xid of a second request can be the firmware's xid, as on `bios`, or a new one, as on `uefi`. The server must not tie a second request to the first by xid.
- After the boot manager starts, its TFTP requests match the single stage, so the TFTP findings of #27 apply to both chains. `wdsmgfw.efi` adds a size probe and a download of `\boot\x64\bootmgfw.efi` with a leading `\`. `wdsnbp.com` and `pxeboot.n12` download with `blksize` 512.
- In WinPE, `BootServerReply` holds the second ACK in the two-stage chain. On `bios`, the value has more bytes after the ACK's end option. The agent must parse the value up to the end option and ignore the rest.
- `DHCPServerACK` in the same key holds the DHCP server's ACK. The agent can log it next to `BootServerReply`.
