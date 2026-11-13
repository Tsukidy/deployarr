# Single-stage boot chain in the PXE lab

This page records whether `bootmgfw.efi`, sent in the first proxyDHCP ACK with option 252, loads the BCD that option 252 names. It answers risk 1 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first), for [#27](https://github.com/Tsukidy/deployarr/issues/27). The runs are from 2026-11-13, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

| VM | Boot file in the first ACK | Option 252 | BCD requests | End state |
| --- | --- | --- | --- | --- |
| `uefi`, OVMF with Secure Boot on | `boot\x64\bootmgfw.efi` | `boot\uefi.bcd` | `\boot\x64\BCD` gets File not found. `\boot\uefi.bcd` loads. | WinPE 10.0.26100.9457 at a command prompt. WinPE reports `UEFISecureBootEnabled` = 1. |
| `bios`, SeaBIOS with QEMU's iPXE ROM | `boot\x64\pxeboot.n12` | `boot\bios.bcd` | `\boot\bios.bcd` loads. | WinPE 10.0.26100.9457 at a command prompt, with an address from dnsmasq. |

- On the `uefi` VM, `bootmgfw.efi` requests the option 252 path over TFTP and loads it. It then loads the font, `boot.sdi`, and `boot.wim` that the store names, and WinPE starts. The single stage boots this VM with Secure Boot on.
- Before the option 252 path, `bootmgfw.efi` requests `\boot\x64\BCD`. That is `BCD` in the folder of the boot file.
- On the `bios` VM, `pxeboot.n12` loads `boot\x64\bootmgr.exe`. `bootmgr.exe` requests the option 252 path as its first file, and WinPE starts.
- Neither VM waited for a key press. No key went to either VM before WinPE showed its command prompt.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts | `tools/lab/` from [#127](https://github.com/Tsukidy/deployarr/pull/127), branch `lab-120-ubuntu-2604` at `786c13b`. Its `tools/lab/` differs from `main` at `ef0bf5e` only in `check.sh` and in the swtpm socket path of `vm.sh`. |
| PXE prototype | `tools/pxe-prototype/`, the same on that branch as on `main` at `ef0bf5e` |
| Network | `br-lab` at `10.41.0.1/24`. dnsmasq hands out addresses on it, and the prototype answers PXE on ports 67 and 4011. |
| Chain | `--chain single`. The first ACK names the boot manager and carries option 252. |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new.

### BCD stores

Each boot used the prototype's default store for its firmware type. Both stores come from the `bcd-stores` artifact of [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137), whose zip has the SHA-256 `5c5eff6591370703d027b613ec2259d79411e84fdd44301317932619c1ae8c97`. The hashes match `~/deployarr-lab/tftp/manifest.txt` and the files on disk.

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

The boot files come from index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`. The ISO has the SHA-256 `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650`. Index 1 is Microsoft Windows PE (amd64), build 26100.9457. The issuer is the `Issuer :` line under `Signer's certificate:` in the output of `osslsigncode verify`, which reads only PE files. The version is the file version in each file's version resource.

| File in the TFTP root | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `boot/x64/bootmgfw.efi` | 3087400 | `4bb32c300df0815d8b6c54a0d3793ae1d78fcdc07f8e2b9f831289b2eb109092` | Microsoft Windows Production PCA 2011 | 10.0.28000.367 |
| `boot/x64/pxeboot.n12` | 25238 | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Not a PE file | None |
| `boot/x64/bootmgr.exe` | 814544 | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Microsoft Windows Production PCA 2011 | 10.0.26100.9444 |
| `boot/boot.sdi` | 3170304 | `cd2c00ce027687ce4a8bdc967f26a8ab82f651c9becd703658ba282ec49702bd` | Not a PE file | None |
| `boot/fonts/wgl4_boot.ttf` | 50096 | `23872d2eba60bbb3bd2585e925c4b48769fb6bb0cfa7f4a6ba4e8f03bcaf2c35` | Not a PE file | None |
| `boot/x64/boot.wim` | 572691590 | `c0a18c72a7f84c36120e5681bb325e2c892e919886671e553a6129899a09097a` | Not a PE file | WinPE 10.0.26100.9457 |

The `boot.wim` hash differs from the one in the [#25 comment](https://github.com/Tsukidy/deployarr/issues/25#issuecomment-6836099194) that first listed the manifest. On the lab host, `manifest.txt` and `boot.wim` date from 22:18 UTC on 2026-11-12, after that comment, and both give the hash above.

## The `uefi` VM

### DHCP

Frames 1 to 7 of [`single-stage-chain-uefi-dhcp.txt`](captures/single-stage-chain-uefi-dhcp.txt):

1. The DISCOVER carries option 93 = 7, option 97 = `4c414200-0000-4000-8000-000000000011`, and option 60 `PXEClient:Arch:00007:UNDI:003000`.
2. dnsmasq offers `10.41.0.176`, and the prototype sends its OFFER with `siaddr` `10.41.0.1`, option 60 `PXEClient`, and no boot file.
3. The firmware takes the lease from dnsmasq on port 67. The prototype logs that REQUEST as `dhcp-ignored` with `reason=lease-request`.
4. The firmware sends its boot file REQUEST from `10.41.0.176:4011` to port 4011.
5. The prototype's ACK in frame 7 goes to `10.41.0.176:4011`. It has `siaddr` `10.41.0.1`, the boot file `boot\x64\bootmgfw.efi`, and options 53, 54, 97, 60, and 252 in that order. Option 252 is `boot\uefi.bcd`, 13 bytes with no NUL.

The serial log shows the firmware's view:

```text
>>Start PXE over IPv4.
  Station IP address is 10.41.0.176

  Server IP address is 10.41.0.1
  NBP filename is boot\x64\bootmgfw.efi
  NBP filesize is 3087400 Bytes
 Downloading NBP file...

  NBP file downloaded successfully.
```

### TFTP requests

The table lists every RRQ in order, from the prototype log. The capture holds the same 32 RRQs in the same order. Times are UTC. "Size probe" means that the server sent an OACK with `tsize`, and the client then ended the transfer with an ERROR, as the outcome column quotes.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 10:39:07.838 | `boot\x64\bootmgfw.efi` | `tsize=0,blksize=1468,windowsize=4` | Size probe, then client ERROR 8 `User aborted the transfer` |
| 2 | 10:39:07.843 | `boot\x64\bootmgfw.efi` | `blksize=1468,windowsize=4` | Complete, 3087400 bytes in 0.105 seconds |
| 3 | 10:39:09.383 | `\boot\x64\BCD` | `tsize=0` | ERROR 1 File not found |
| 4 | 10:39:09.383 | `\boot\uefi.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 5 | 10:39:09.384 | `\boot\uefi.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.002 seconds |
| 6 | 10:39:09.387 | `\EFI\Microsoft\Boot\Policies\SbcpFlightToken.p7b` | `tsize=0` | ERROR 1 File not found |
| 7 | 10:39:09.441 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 8 to 21 | 10:39:09.457 to 10:39:09.460 | `\EFI\Microsoft\Boot\SiPolicy.p7b`, `SkuSiPolicy.p7b`, `WinSiPolicy.p7b`, `ATPSiPolicy.p7b`, then those four and `VbsSiPolicy.p7b` twice more | `tsize=0` | ERROR 1 File not found, 14 times |
| 22 | 10:39:09.484 | `\boot\x64\bootmgfw.efi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 23 | 10:39:09.485 | `\boot\x64\bootmgfw.efi` | `tsize=0,blksize=1456` | Complete, 3087400 bytes in 0.258 seconds |
| 24 | 10:39:09.749 | `\EFI\Microsoft\Boot\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 25 | 10:39:09.749 | `\boot\x64\boot.stl` | `tsize=0` | ERROR 1 File not found |
| 26 | 10:39:09.750 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 27 | 10:39:09.751 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 28 | 10:39:09.751 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 29 | 10:39:09.752 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.005 seconds |
| 30 | 10:39:09.765 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.286 seconds |
| 31 | 10:39:10.053 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 52.145 seconds |
| 32 | 10:40:02.352 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQs 1 and 2 come from OVMF. It asks for `blksize` 1468 and `windowsize` 4, and the prototype grants both.
- RRQs 3 to 32 come from `bootmgfw.efi`. Every path it sends starts with `\`, including the option 252 path, which arrives as `\boot\uefi.bcd`.
- `bootmgfw.efi` asks for `blksize` 1456 and never for `windowsize`, so each of its transfers runs with a window of 1. The transfer of `boot.wim` sent 393,333 blocks with no retransmits.
- `bootmgfw.efi` downloads its own file again, in RRQ 23. The log does not show why.
- The BCD sets `timeout` 30 (`New-BcdStores.ps1` line 73), but the requests after the BCD follow without a pause. The store has one loader in `displayorder` (line 97).

### End state

The screen during RRQ 31 shows `Loading files...`, a progress bar, and `IP: 10.41.0.1, File: \boot\x64\boot.wim`. A screenshot at 10:40:23.456 shows a window titled `Administrator: X:\windows\system32\cmd.exe` after `wpeinit`. So the window appeared after `boot.wim` finished at 10:40:02.197 and by 10:40:23.456, at most 83.2 seconds after `vm.sh` started at 10:39:00.246. A screenshot's time is the modification time of its file. Commands typed into that window through QEMU's `sendkey` gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `reg query hklm\system\currentcontrolset\control\secureboot\state` printed `UEFISecureBootEnabled REG_DWORD 0x1`.
- `reg query hklm\system\controlset001\control\pxe` printed a `BootServerReply` value. Its BOOTP header has xid `3EA517B6`, `secs` 0, `ciaddr` `10.41.0.176`, `siaddr` `10.41.0.1`, and MAC `525400410011`. It holds the boot file `boot\x64\bootmgfw.efi`, then options 53, 54, 97, 60, and 252 = `boot\uefi.bcd`, then the End option. Its header and options match the ACK in frame 7 byte for byte.
- `ipconfig` printed `Windows IP Configuration` and no adapter. The capture holds no UDP packet from WinPE. Its filter keeps only UDP. It ran until 10:41:24.852, at least 61 seconds after the window appeared. Its last packet is the ERROR reply to RRQ 32, at 10:40:02.352.

## The `bios` VM

### DHCP

Frames 1 to 7 of [`single-stage-chain-bios-dhcp.txt`](captures/single-stage-chain-bios-dhcp.txt) follow the same pattern as on the `uefi` VM:

- The DISCOVER carries option 93 = 0, option 97 = `4c414200-0000-4000-8000-000000000021`, and option 60 `PXEClient:Arch:00000:UNDI:002001`.
- The ROM takes the lease `10.41.0.192` from dnsmasq on port 67, then sends its boot file REQUEST from `10.41.0.192:68` to port 4011.
- The prototype's ACK in frame 7 goes to `10.41.0.192:68`. It has the boot file `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd`. It copies the broadcast flag, `0x8000`, from the REQUEST in frame 6.

Frames 38 to 41 are WinPE's own DISCOVER, OFFER, REQUEST, and ACK, with option 60 `MSFT 5.0` and no option 93. The prototype logs them as `dhcp-ignored` with `reason=no-option-93`, and dnsmasq gives WinPE `10.41.0.192` again.

### TFTP requests

The capture holds the same 13 RRQs in the same order.

| # | Time | File | Options | Outcome |
| --- | --- | --- | --- | --- |
| 1 | 10:42:34.097 | `boot\x64\pxeboot.n12` | `blksize=1432,tsize=0` | Complete, 25238 bytes in 0.004 seconds |
| 2 | 10:42:34.120 | `boot\x64\bootmgr.exe` | `blksize=512,tsize=0` | Complete, 814544 bytes in 0.163 seconds |
| 3 | 10:42:34.378 | `\boot\bios.bcd` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 4 | 10:42:34.379 | `\boot\bios.bcd` | `tsize=0,blksize=1456` | Complete, 12288 bytes in 0.004 seconds |
| 5 | 10:42:34.383 | `\EFI\Microsoft\Boot\SecureBootPolicy.p7b` | `tsize=0` | ERROR 1 File not found |
| 6 | 10:42:34.398 | `\hiberfil.sys` | `tsize=0` | ERROR 1 File not found |
| 7 | 10:42:34.398 | `\boot\x64\boot.wim` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 8 | 10:42:34.399 | `\boot\boot.sdi` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 9 | 10:42:34.399 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0` | Size probe, then client ERROR 0 `TFTP Aborted` |
| 10 | 10:42:34.400 | `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | Complete, 50096 bytes in 0.009 seconds |
| 11 | 10:42:34.410 | `\boot\boot.sdi` | `tsize=0,blksize=1456` | Complete, 3170304 bytes in 0.508 seconds |
| 12 | 10:42:34.920 | `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | Complete, 572691590 bytes in 89.254 seconds |
| 13 | 10:44:04.443 | `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` | `tsize=0` | ERROR 1 File not found |

- RRQ 1 comes from the iPXE ROM, and RRQ 2 from `pxeboot.n12`. `pxeboot.n12` asks for `boot\x64\bootmgr.exe`, in the folder of its own boot file.
- RRQs 3 to 13 come from `bootmgr.exe`. Its first request is the option 252 path, and it never asks for `\boot\x64\BCD`.
- `bootmgr.exe` uses the same `blksize` 1456 and no `windowsize` as `bootmgfw.efi`. The transfer of `boot.wim` sent 393,333 blocks with no retransmits.

### End state

A screenshot at 10:44:17.246 shows a window titled `X:\windows\system32\cmd.exe - startnet.cmd` running `wpeinit`. So the window appeared after `boot.wim` finished at 10:44:04.173 and by 10:44:17.246, at most 110.5 seconds after `vm.sh` started at 10:42:26.726. After `wpeinit`, these commands gave these results:

- `ver` printed `Microsoft Windows [Version 10.0.26100.9457]`.
- `ipconfig` printed `Ethernet adapter Ethernet` with the IPv4 address `10.41.0.192` and the default gateway `10.41.0.1`.
- `reg query hklm\system\controlset001\control\pxe` printed a `BootServerReply` value. It carries the xid `DCB2760B`, flags `8000`, `ciaddr` `10.41.0.192`, `siaddr` `10.41.0.1`, and boot file `boot\x64\pxeboot.n12` of the ACK in frame 7. It also carries that ACK's values for options 53, 54, 60, 97, and 252 = `boot\bios.bcd`.
- That `BootServerReply` value is not a copy of frame 7. Its `secs` is 0, where frame 7 has 15. Its options come in the order 53, 54, 60, 97, 252, where frame 7 has 53, 54, 97, 60, 252. It has no End option, and zeros follow option 252.
- `reg query hklm\system\currentcontrolset\control /v pefirmwaretype` found no value, before and after `wpeutil updatebootinfo`. That command printed `The command failed with status 0x8007000d.`

## Captures

| File | What it holds |
| --- | --- |
| [`single-stage-chain-uefi.pcapng`](captures/single-stage-chain-uefi.pcapng) | The `uefi` run, reduced |
| [`single-stage-chain-uefi-dhcp.txt`](captures/single-stage-chain-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`single-stage-chain-bios.pcapng`](captures/single-stage-chain-bios.pcapng) | The `bios` run, reduced |
| [`single-stage-chain-bios-dhcp.txt`](captures/single-stage-chain-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |

The capture filter is `udp and udp[8:2] != 3`. It keeps only UDP, and it drops TFTP DATA packets and IP fragments. Each full capture is over 30 MB, because `bootmgfw.efi` and `bootmgr.exe` ACK every block of `boot.wim`. The committed files are reduced copies that drop only the TFTP ACKs, with `tshark -r - -Y 'not tftp.opcode == 4' -w -`. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 398,285 packets, 31,867,440 bytes, SHA-256 `a30849469852878a5967e378af2c36b1c1546065c64957983a748779007a6afe` | 398,208 | 77 packets, 10,692 bytes |
| `bios` | 397,223 packets, 31,783,248 bytes, SHA-256 `1d04f1a273b133df3663bac6f46b5b1b96d0f9a4b48af459d88952e47b8aaa2d` | 397,170 | 53 packets, 9,540 bytes |

The `dumpcap` logs, `single-stage-chain-<vm>-dumpcap.log` next to the full captures, report no dropped packets for either run. The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/27/`: the prototype logs, the serial logs, the screenshots, and `versions.txt`.

## Commands

Each run started the prototype, then the capture, then the VM. That is the order that "Run an experiment" gives in the lab guide of [#127](https://github.com/Tsukidy/deployarr/pull/127). The lab scripts ran from a checkout of that branch, `lab-120-ubuntu-2604`, here `$LAB`. `<vm>` is `uefi` or `bios`. The prototype and `dumpcap` run in the background, and `dumpcap` writes its log, with the dropped-packet count, to a file next to the capture.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain single --no-block-log --log-file ~/deployarr-lab/runs/27/<vm>-prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/single-stage-chain-<vm>.pcapng 2> ~/deployarr-lab/captures/single-stage-chain-<vm>-dumpcap.log &
timeout 1800 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The screenshots came from the HMP command `screendump <file>.png -f png` on the monitor socket. The WinPE commands went in one key at a time with the HMP command `sendkey`. After the last packet, `sudo pkill -INT -x dumpcap` stopped the capture, and the HMP command `quit` stopped the VM.

These commands make the committed files:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/single-stage-chain-<vm>.pcapng > docs/research/pxe-lab/captures/single-stage-chain-<vm>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/single-stage-chain-<vm>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/single-stage-chain-<vm>.pcapng > docs/research/pxe-lab/captures/single-stage-chain-<vm>-dhcp.txt
```

## What is inferred

- `bootmgfw.efi` reads the BCD path from option 252 of the ACK that booted it (inferred). It asks for exactly that path, with a leading `\`, after its own folder's `BCD` fails. No other source on the network names `uefi.bcd`. The `bios` run supports the same reading for `bootmgr.exe`.
- The order of the two UEFI requests suggests that `bootmgfw.efi` tries `BCD` beside itself first and option 252 second (inferred). No run served a file at `\boot\x64\BCD`, so the page does not show which store wins when both exist.
- The `\EFI\Microsoft\Boot\*` requests, `\boot\x64\boot.stl`, and `\hiberfil.sys` are probes for files that a boot from disk would have (inferred from their names). Both boots go on after File not found. The [ConfigMgr 2509 reference](../configmgr-2509.md#tftp-server) says that `sccmpxe.exe` does not log misses for `EFI\Microsoft\Boot\*`, `smsboot\*\x*\BCD`, and `hiberfil.sys`, and infers that these are the boot manager's probes. In these runs the boot managers request paths of all three shapes.
- On the `bios` VM, the iPXE ROM builds the reply that `BootServerReply` holds, and does not copy frame 7 (inferred). `pxeboot.n12` probably reads the reply through the ROM's PXE API, as a cached reply. The value's `secs` is 0, and its options are in ascending code order, while frame 7 has `secs` 15 and option 97 before option 60. On the `uefi` VM, the value's header and options match frame 7 byte for byte.
- WinPE on the `uefi` VM has no adapter because it has no driver for the virtio-net card (inferred). The [Test lab](../../roadmap.md#test-lab) section of the roadmap says Windows 11 has no inbox virtio drivers, and the `bios` VM's `e1000` card works in the same WinPE.
- The single stage sends `pxeboot.n12` and `bootmgr.exe` for x64 to every BIOS PC, because every BIOS PC sends option 93 = 0 ([Boot chain](../../architecture.md#boot-chain)). The lab has no x86 BIOS VM, so no run shows what an x86 PC does with them.

## Not covered

- The `uefi2023` VM, whose `db` holds only the Windows UEFI CA 2023. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs the 2023-signed boot files there.
- The two-stage chain, which [#28](https://github.com/Tsukidy/deployarr/issues/28) runs.
- A BCD with no loader `path` ([#29](https://github.com/Tsukidy/deployarr/issues/29)), and other block and window sizes ([#30](https://github.com/Tsukidy/deployarr/issues/30)).
- Physical PCs and other firmware.

## What changes in deployarr

- The single stage meets the [Boot chain](../../architecture.md#boot-chain) rule on the `uefi` VM with Secure Boot on and on the `bios` VM. The rule names every lab firmware, so the choice between the chains waits for the `uefi2023` result in #32. #33 makes it.
- The TFTP server must answer each probe with ERROR 1 at once, as [TFTP](../../architecture.md#tftp) already says. The `uefi` boot sent 20 requests for missing files, and the `bios` boot sent 3.
- The TFTP server must treat a client ERROR right after its OACK as the normal end of a size probe. Both boot managers send ERROR 0 `TFTP Aborted` there, and OVMF sends ERROR 8. The prototype ends those transfers with the outcome `client-error`.
- The server must not put a file at the boot file's folder plus `BCD`, such as `boot\x64\BCD`, unless a lab run shows which store wins. With the file absent, option 252 decides the store.
- Option 252 can hold the path without a leading `\`. The boot managers add one, and path normalization removes it, as [TFTP](../../architecture.md#tftp) says.
- With no TFTP elements in the BCD, both boot managers moved `boot.wim` with a window of 1, in 52 seconds on `uefi` and 89 seconds on `bios`, from the Python prototype on a local bridge. #30 measures the other settings.
- In WinPE, `HKLM\SYSTEM\ControlSet001\Control\PXE\BootServerReply` holds the xid, the addresses, the boot file, and the option values of the ACK from port 4011 on both firmware types. The agent can read the server's address from it, as [Boot chain](../../architecture.md#boot-chain) plans. On `bios` the value is not a byte copy of the ACK. Its `secs` and option order differ, and it has no End option. So the agent must find each option by its code, with no assumption about option order or an End option, and must stop at the end of the value.
