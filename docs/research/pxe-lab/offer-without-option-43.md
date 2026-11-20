# OFFER without option 43 in the PXE lab

This page records whether OVMF and SeaBIOS accept a proxyDHCP OFFER that carries no option 43, the OFFER that [DHCP](../../architecture.md#dhcp) specifies. It answers risk 5 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first), for [#31](https://github.com/Tsukidy/deployarr/issues/31). The runs are from 2026-11-17, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

| VM | With the prototype's OFFER | Control, with no OFFER from the prototype |
| --- | --- | --- |
| `uefi`, OVMF with Secure Boot on | OVMF takes its address from dnsmasq, sends its boot file REQUEST to port 4011, and boots WinPE 10.0.26100.9457. | OVMF sends four DISCOVERs and no REQUEST. Its serial log shows `PXE-E16: No valid offer received.`, and it moves on to PXE over IPv6 and HTTP boot. |
| `bios`, SeaBIOS with QEMU's iPXE ROM | The ROM takes its address from dnsmasq, sends its boot file REQUEST to port 4011, and boots WinPE 10.0.26100.9457. | The ROM takes its address from dnsmasq and sends nothing to port 4011. It shows `Nothing to boot: No such file or directory`, and SeaBIOS moves on to its other boot devices. |

- Both firmware types accept the OFFER with no option 43. Each one goes on to the port 4011 exchange and boots.
- Without the prototype's OFFER, neither firmware type sends a request to port 4011 or boots. So in the runs with it, the OFFER with no option 43 is what led each firmware to the PXE boot.
- No OFFER in any of the four captures carries option 43.

## The OFFER

The prototype sends the same OFFER on both VMs, frame 3 of each capture. These are its fields, from the committed DHCP decodes:

| Field | `uefi` | `bios` |
| --- | --- | --- |
| UDP | `10.41.0.1:67` to `255.255.255.255:68` | `10.41.0.1:67` to `255.255.255.255:68` |
| `op`, xid | 2, `0a2f592d` | 2, `b80c8609` |
| `flags` | `0x8000`, as in the DISCOVER | `0x0000`, as in the DISCOVER |
| `yiaddr` | 0.0.0.0 | 0.0.0.0 |
| `siaddr` | 10.41.0.1 | 10.41.0.1 |
| `sname`, `file` | Empty | Empty |
| Options | 53 = 2 (OFFER), 54 = 10.41.0.1, 97 = `4c414200-0000-4000-8000-000000000011`, 60 = `PXEClient`, then 255 | 53 = 2 (OFFER), 54 = 10.41.0.1, 97 = `4c414200-0000-4000-8000-000000000021`, 60 = `PXEClient`, then 255 |
| DHCP payload | 300 bytes, zeros after option 255 | 300 bytes, zeros after option 255 |

This matches the OFFER in [DHCP](../../architecture.md#dhcp). The OFFER there also echoes option 82, which no lab request carries.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts and PXE prototype | `tools/lab/` and `tools/pxe-prototype/` from `main` at `0029949`. `tools/lab/vm.sh` and `tools/pxe-prototype/` are the same as in the #27 runs. |
| Network | `br-lab` at `10.41.0.1/24`. dnsmasq hands out addresses on it from `0.0.0.0%br-lab:67`. |
| Chain | `--chain single`, with the default stores `boot\uefi.bcd` and `boot\bios.bcd` |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| Runs with the OFFER | The prototype answers DHCP on its default ports, 67 and 4011. Its `started` line shows `dhcp_ports=67,4011`. |
| Controls | The prototype runs with `--dhcp-ports 4011`, so it sends no OFFER. Its `started` line shows `dhcp_ports=4011`. |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

The four runs went one after another, from 20:10:23 to 20:19:57 UTC. Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new. The run script wrote the wall clock and `/proc/uptime` at each step. Wall clock minus uptime stays within 0.33 seconds across all four runs. Uptime at the first stamp puts the host's last start at about 18:16 UTC, so the host did not restart during the runs.

The component versions, firmware files, boot files, and `boot.wim` are the same as in [Versions](bootmgr-object-type.md#versions) of the `{bootmgr}` object type page, on the same lab host on the same day. `boot.wim` is index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`, Microsoft Windows PE (amd64), build 26100.9457. `bootmgfw.efi` is 10.0.28000.367 and `bootmgr.exe` is 10.0.26100.9444, both signed through the Microsoft Windows Production PCA 2011.

## The `uefi` VM

### With the OFFER

| Frame | Time | From | To | Packet |
| --- | --- | --- | --- | --- |
| 1 | 20:10:27.983 | `0.0.0.0:68` | `255.255.255.255:67` | OVMF's DISCOVER, xid `0a2f592d`, option 60 `PXEClient:Arch:00007:UNDI:003000` |
| 2 | 20:10:27.984 | `10.41.0.1:67` | `255.255.255.255:68` | dnsmasq's OFFER of `10.41.0.176` |
| 3 | 20:10:27.984 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER, with no option 43 |
| 4 | 20:10:31.708 | `0.0.0.0:68` | `255.255.255.255:67` | OVMF's lease REQUEST. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. |
| 5 | 20:10:31.708 | `10.41.0.1:67` | `255.255.255.255:68` | dnsmasq's ACK |
| 6 | 20:10:31.709 | `10.41.0.176:4011` | `10.41.0.1:4011` | OVMF's boot file REQUEST |
| 7 | 20:10:31.709 | `10.41.0.1:4011` | `10.41.0.176:4011` | The prototype's ACK, with `boot\x64\bootmgfw.efi` and option 252 = `boot\uefi.bcd` |

The serial log shows what OVMF took from the exchange:

```text
>>Start PXE over IPv4.
  Station IP address is 10.41.0.176
  Server IP address is 10.41.0.1
  NBP filename is boot\x64\bootmgfw.efi
  NBP filesize is 3087400 Bytes
 Downloading NBP file...
  NBP file downloaded successfully.
```

The boot then made the same 32 RRQs as #27. `boot.wim` completed at 20:11:23.561, and the screenshot written at 20:11:42.37 shows WinPE's `cmd.exe` window at a prompt after `wpeinit`.

### Control

| Frames | Times | Packets |
| --- | --- | --- |
| 1 to 8 | 20:14:39.264 to 20:15:06.989 | Four PXE DISCOVERs from OVMF, at 0, 3.7, 11.7, and 27.7 seconds, each with option 60 `PXEClient:Arch:00007:UNDI:003000`. dnsmasq answers each with an OFFER of `10.41.0.176`. |
| 9 to 14 | 20:16:42.075 to 20:16:54.018 | Three DISCOVERs with option 60 `HTTPClient:Arch:00016:UNDI:003000`, from OVMF's HTTP boot, each answered by dnsmasq |

OVMF sent no REQUEST, so it took no address. Nothing reached port 4011. The serial log shows the PXE attempts:

```text
>>Start PXE over IPv4.
  PXE-E16: No valid offer received.
BdsDxe: failed to load Boot0003 "UEFI PXEv4 (MAC:525400410011)" ...: Not Found
>>Start PXE over IPv6.
  PXE-E16: No valid offer received.
BdsDxe: failed to load Boot0004 "UEFI PXEv6 (MAC:525400410011)" ...: Not Found
>>Start HTTP Boot over IPv4....
```

The `...` marks device paths left out.

## The `bios` VM

### With the OFFER

| Frame | Time | From | To | Packet |
| --- | --- | --- | --- | --- |
| 1 | 20:12:14.668 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, xid `b80c8609`, option 60 `PXEClient:Arch:00000:UNDI:002001` |
| 2 | 20:12:14.668 | `10.41.0.1:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` |
| 3 | 20:12:14.668 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER, with no option 43 |
| 4 | 20:12:15.660 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = 10.41.0.1. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. |
| 5 | 20:12:15.660 | `10.41.0.1:67` | `10.41.0.192:68` | dnsmasq's ACK |
| 6 | 20:12:15.661 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST |
| 7 | 20:12:15.661 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` |

The boot then made the same 13 RRQs as #27. `boot.wim` completed at 20:13:55.894. WinPE's DISCOVER followed at 20:14:09.545 and its REQUEST at 20:14:10.057. The screenshot written at 20:14:15.42 shows WinPE's `cmd.exe` window.

### Control

| Frame | Time | Packet |
| --- | --- | --- |
| 1 | 20:17:27.023 | The ROM's DISCOVER, xid `253c3876` |
| 2 | 20:17:27.024 | dnsmasq's OFFER of `10.41.0.192` |
| 3 | 20:17:28.053 | The ROM's DISCOVER again |
| 4 | 20:17:28.053 | dnsmasq's OFFER again |
| 5 | 20:17:30.085 | The ROM's lease REQUEST |
| 6 | 20:17:30.085 | dnsmasq's ACK |

The ROM took the address and sent nothing more. The screen shows:

```text
Configuring (net0 52:54:00:41:00:21)...... ok
net0: 10.41.0.192/255.255.255.0 gw 10.41.0.1
net0: fe80::5054:ff:fe41:21/64
Nothing to boot: No such file or directory (https://ipxe.org/2d03e13b)
No more network devices

Booting from Hard Disk...
Boot failed: not a bootable disk
```

SeaBIOS then tried the floppy and the CD, and the last screenshot shows `No bootable device.`

## Captures

| File | What it holds |
| --- | --- |
| [`offer-without-option-43-uefi.pcapng`](captures/offer-without-option-43-uefi.pcapng) | The `uefi` run with the OFFER, reduced |
| [`offer-without-option-43-uefi-dhcp.txt`](captures/offer-without-option-43-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`offer-without-option-43-bios.pcapng`](captures/offer-without-option-43-bios.pcapng) | The `bios` run with the OFFER, reduced |
| [`offer-without-option-43-bios-dhcp.txt`](captures/offer-without-option-43-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`offer-without-option-43-uefi-control.pcapng`](captures/offer-without-option-43-uefi-control.pcapng) | The `uefi` control |
| [`offer-without-option-43-uefi-control-dhcp.txt`](captures/offer-without-option-43-uefi-control-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`offer-without-option-43-bios-control.pcapng`](captures/offer-without-option-43-bios-control.pcapng) | The `bios` control |
| [`offer-without-option-43-bios-control-dhcp.txt`](captures/offer-without-option-43-bios-control-dhcp.txt) | `tshark -V` decode of its DHCP packets |

`dumpcap` captured with the filter `udp and udp[8:2] != 3`, which drops TFTP DATA packets and IP fragments. The committed files went through `tshark -r - -Y 'not tftp.opcode == 4' -w -`, which drops the TFTP ACKs. The controls have no TFTP packets, so their committed copies hold every packet. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 398,285 packets, 31,867,440 bytes, SHA-256 `5fffdcd4249e1d504cd1a9779ecb1507e443147d10c7f83db2f72e431520ddd9` | 398,208 | 77 packets, 10,692 bytes, SHA-256 `a99fc360fc0fbf38a7bf50305f1a3db2cac35817b681df9fb03cc9b391d76da0` |
| `bios` | 397,223 packets, 31,783,248 bytes, SHA-256 `583695237638d7e3b5f1bcf8bafe4088c58950f7039209aa311768547db38d0f` | 397,170 | 53 packets, 9,540 bytes, SHA-256 `4df9a5cfebf0691b39bcf0415e2ce280a43c734a6e6ab3a4cb9d5d2187fa91df` |
| `uefi` control | 14 packets, 5,928 bytes | None | 14 packets, 5,820 bytes, SHA-256 `10178fb9e3abc5bbe787f45ecf48c01cead1bf995cf5da7d80ade32d739cd666` |
| `bios` control | 6 packets, 2,944 bytes | None | 6 packets, 2,836 bytes, SHA-256 `473b1ee7755b62249b3826a27ad487bcac94677cfc92f1b12c776a307f9641ca` |

- The `dumpcap` logs, `offer-without-option-43-<run>-dumpcap.log` next to the full captures, report no dropped packets.
- The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.
- `dhcp.option.dhcp == 2 and dhcp.option.type == 43` matches no packet in any capture.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/31/`: the prototype logs, the `uefi` serial logs, a screenshot every 2 seconds of each control and the end of each boot, and the run script with its clock stamps.

## Commands

Each run started the prototype, then the capture, then the VM, as [Run an experiment](../../lab-setup.md#run-an-experiment) says. `$LAB` is the checkout of `main` at `0029949`, `<vm>` is `uefi` or `bios`, and `<run>` is `<vm>` or `<vm>-control`. The controls add `--dhcp-ports 4011` to the prototype.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain single --no-block-log --log-file ~/deployarr-lab/runs/31/offer-without-option-43-<run>-prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/offer-without-option-43-<run>.pcapng 2> ~/deployarr-lab/captures/offer-without-option-43-<run>-dumpcap.log &
timeout 900 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

A run with the OFFER waited for the request for `UnlockToken.pol` after `boot.wim`, then took screenshots for 20 seconds. A control took a screenshot every 2 seconds for 150 seconds. The screenshots came from the HMP command `screendump`. Then the HMP command `quit` stopped the VM, `sudo pkill -INT -f '^python3 -B .*pxe_prototype.py'` stopped the prototype, and `sudo pkill -INT -x dumpcap` stopped the capture.

These commands make the committed files:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/offer-without-option-43-<run>.pcapng > docs/research/pxe-lab/captures/offer-without-option-43-<run>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/offer-without-option-43-<run>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/offer-without-option-43-<run>.pcapng > docs/research/pxe-lab/captures/offer-without-option-43-<run>-dhcp.txt
```

## What is inferred

- Each firmware type treats the prototype's OFFER as a PXE offer because of option 60 `PXEClient` (inferred). Besides option 60, the prototype's OFFER differs from dnsmasq's in option 97, in its empty `yiaddr`, and in the address options it leaves out. Both OFFERs have `siaddr` 10.41.0.1. The runs do not change one field at a time.
- OVMF needs a PXE offer before it takes an address at all (inferred from the control). In the control it sent no REQUEST for dnsmasq's offer. The iPXE ROM took dnsmasq's address in its control, and stopped only when it found no boot file.

## Not covered

- A DHCP server on another address. In the lab, dnsmasq and the prototype both answer from `10.41.0.1`, so the runs do not show whether a firmware sends its port 4011 request to the proxy OFFER's `siaddr` or option 54, or to the DHCP server. [#139](https://github.com/Tsukidy/deployarr/issues/139) moves dnsmasq off the lab host's network stack.
- An OFFER without option 97. The `bios` VM's DISCOVER carries option 97, and every OFFER in the runs carries option 97. On the lab's versions, its iPXE ROM sends option 97 with or without the VM's `smbios-entry-point-type=32` ([iPXE option 97 in the PXE lab](ipxe-option-97.md#result)). [DHCP](../../architecture.md#dhcp) gives a request with no option 97 an OFFER without one.
- The `uefi2023` VM. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs it.
- The two-stage chain. Its OFFER is the same, and the boots of #28 also passed through it, as [Two-stage boot chain in the PXE lab](two-stage-chain.md) records.
- Relayed requests, a request that carries option 82, and physical PCs and other firmware.

## What changes in deployarr

- Nothing for the OFFER. The OFFER in [DHCP](../../architecture.md#dhcp) for a request with option 97, with no option 43, no boot file, and option 60 `PXEClient`, leads OVMF and the iPXE ROM to the port 4011 exchange, and both boot. Risk 5 can close.
- In the control, OVMF took no address from dnsmasq without a PXE offer, and other UEFI firmware may act the same (inferred). If deployarr stays silent for a PC, as [DHCP](../../architecture.md#dhcp) says it does for a finished PC on UEFI, OVMF ends PXE with `PXE-E16` and moves on to its next boot option. That is the behavior that rule expects.
