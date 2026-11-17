# TFTP transfer time in the PXE lab

This page records how long `boot.wim` takes over TFTP for each block and window size, and which sizes the boot managers request when the BCD sets none. It answers risk 4 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first), for [#30](https://github.com/Tsukidy/deployarr/issues/30). The runs are from 2026-11-17, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

With no `ramdisktftpblocksize` and no `ramdisktftpwindowsize` in the BCD, both boot managers request the font, `boot.sdi`, and `boot.wim` with `blksize=1456` and no `windowsize`. The prototype then sends one block per ACK, a window of 1.

`boot.wim` is 572,691,590 bytes. The network is the lab bridge `br-lab` on the lab host, with an MTU of 1500. It is a software bridge with no physical link and no rate limit, so it has no fixed line rate. The fastest transfer, 121.4 MB/s or about 971 Mbit/s, shows that it carries at least that much. The `uefi` VM has a virtio-net card, and the `bios` VM has QEMU's emulated `e1000`. The server is the PXE prototype in Python. Each time is the prototype's `seconds` for the `boot.wim` transfer, from the request to the last ACK.

| Block size | Window size | `uefi` time | `uefi` MB/s | `bios` time | `bios` MB/s |
| --- | --- | --- | --- | --- | --- |
| 1456, no BCD element | 1, no BCD element | 48.716 s | 11.8 | 89.458 s | 6.4 |
| 1456 | 4 | 16.813 s | 34.1 | 46.682 s | 12.3 |
| 1456 | 8 | 11.141 s | 51.4 | 39.599 s | 14.5 |
| 1456 | 16 | 8.502 s | 67.4 | 39.562 s | 14.5 |
| 1456 | 32 | 7.084 s | 80.8 | 40.434 s | 14.2 |
| 4096 | 1 | 21.797 s | 26.3 | 32.331 s | 17.7 |
| 8192 | 1 | 13.441 s | 42.6 | 16.338 s | 35.1 |
| 16384 | 1 | 9.406 s | 60.9 | 17.459 s | 32.8 |
| 32768 | 1 | 6.997 s | 81.8 | 13.616 s | 42.1 |
| 8192 | 8 | 6.187 s | 92.6 | 15.431 s | 37.1 |
| 32768 | 32 | 4.716 s | 121.4 | 13.434 s | 42.6 |
| 1456, no BCD element, again | 1, no BCD element | 48.392 s | 11.8 | 90.598 s | 6.3 |

MB/s is 10<sup>6</sup> bytes a second, the image size divided by the time.

- Each boot manager requests exactly the block and window size that the BCD sets. The prototype grants each one, because each is within its limits of 32768 and 32.
- Every `boot.wim` transfer completed with no retransmits, and every boot reached WinPE 10.0.26100.9457.
- At 4096 and 8192 bytes, six `boot.sdi` transfers never completed, because the boot managers do not ACK an empty last block. See [Empty last block](#empty-last-block).
- On `uefi`, a larger window and a larger block each shorten the transfer. 32768 and 32 take 4.7 seconds, 10.3 times faster than the default's 48.7 seconds.
- On `bios`, a window above 8 gains nothing at 1456 bytes, and the transfer stays near 40 seconds. A larger block gains more. 32768 and 32 take 13.4 seconds, 6.7 times faster than the default's 89.5 seconds.

## What the boot managers request

The prototype logs every RRQ with its options. With the BCD's TFTP elements set, three requests change, on both VMs. The other requests, with their options, match those of the default store in the same series and in #27.

| File | With no TFTP elements | With `ramdisktftpblocksize` 32768 and `ramdisktftpwindowsize` 32 in the BCD |
| --- | --- | --- |
| `\boot\fonts\wgl4_boot.ttf` | `tsize=0,blksize=1456` | `tsize=0,blksize=32768,windowsize=32` |
| `\boot\boot.sdi` | `tsize=0,blksize=1456` | `tsize=0,blksize=32768,windowsize=32` |
| `\boot\x64\boot.wim` | `tsize=0,blksize=1456` | `tsize=0,blksize=32768,windowsize=32` |

- Each file also gets a size probe first, with `tsize=0` only.
- The store itself loads with `tsize=0,blksize=1456` either way, because the boot manager has not read it yet. On `uefi`, the second download of `\boot\x64\bootmgfw.efi` also keeps `blksize=1456`.
- With window 1 in the BCD, the requests carry `windowsize=1`. With no window element, they carry no `windowsize`. The prototype sends one block per ACK in both cases.
- The firmware's first download does not follow the BCD, because it comes before the store loads. OVMF requests `bootmgfw.efi` with `blksize=1468,windowsize=4`. On `bios`, the iPXE ROM requests `pxeboot.n12` with `blksize=1432`, and `pxeboot.n12` requests `bootmgr.exe` with `blksize=512` (inferred from the order of the requests).

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts and PXE prototype | `tools/lab/` and `tools/pxe-prototype/` from `main` at `d799a1b`. `tools/lab/vm.sh` and `tools/pxe-prototype/` are the same as in the #27 runs. |
| Network | `br-lab` at `10.41.0.1/24`, MTU 1500, started with `tools/lab/network.sh up`. dnsmasq hands out addresses on it, and the prototype answers PXE on ports 67 and 4011. |
| Chain | `--chain single`. The first ACK names the boot manager and carries option 252. |
| TFTP root | `~/deployarr-lab/runs/30/tftp-root`. It holds the store variants in `boot/`, and links `boot/boot.sdi`, `boot/fonts`, and `boot/x64` to the same paths in `~/deployarr-lab/tftp/2011`, which [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) builds. |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd` with Secure Boot on (inferred, see [What is inferred](#what-is-inferred)), a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |
| `bios` VM | `pc` with `smbios-entry-point-type=32`, SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, 2 vCPUs, 4 GB |

The `uefi` series ran from 19:42:39 to 19:51:39 UTC, and the `bios` series from 19:51:59 to 20:04:59. Each series booted its VM 12 times, once for each row of the result table, in the table's order. Each boot started with the VM's folder deleted, so its disk, firmware variables, and TPM state were new. Each boot used its own prototype process.

The series script wrote the wall clock and `/proc/uptime` at each step. `/proc/uptime` rose at every step, so the lab host did not restart during a series. The journal logs no clock change from 19:40 to 20:21. The wall clock stamps come from `date -u +%3N`, and the lab host's `date` is uutils coreutils 0.10.0, which drops the leading zeros when the milliseconds are below 100. So a few stamps misprint their milliseconds, and the Details table takes the one VM start this affects from `/proc/uptime`.

### BCD stores

Every store comes from the `bcd-stores` artifact of [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137), whose zip has the SHA-256 `5c5eff6591370703d027b613ec2259d79411e84fdd44301317932619c1ae8c97`.

| Source store | SHA-256 | TFTP elements |
| --- | --- | --- |
| `boot/uefi.bcd` | `c0164082153aa4eeb826b68d9e3acb4ad815fbf64b8dc1b8a51ef0f02d010224` | None |
| `boot/bios.bcd` | `5682186b6906955f5f5e610aee7dc9eb66e3d5314d31141c5ff9b1a0750e7b0d` | None |
| `boot/uefi-maxtftp.bcd` | `43a6ed3342ac557d79c9d3a863aeab05bf6793ab8b3e1757d4b0d4669fa0dbc3` | `ramdisktftpblocksize` 32768, `ramdisktftpwindowsize` 32 |
| `boot/bios-maxtftp.bcd` | `322fa4b644b726d3f0f1694030a4a5c721fe69649418ee73822a36592cf4cea6` | `ramdisktftpblocksize` 32768, `ramdisktftpwindowsize` 32 |

The rows with no BCD element use `uefi.bcd` and `bios.bcd`, copied under other names. The rows with 32768 and 32 use the `-maxtftp` stores as they are. Every other row uses a copy of a `-maxtftp` store with the two elements changed in place:

- Under `{ramdiskoptions}`, `{ae5534e0-a924-466c-b836-758539a3ee3a}`, element `35000007` is `ramdisktftpblocksize` and element `35000008` is `ramdisktftpwindowsize`. Each holds an 8-byte integer in its own data cell. In both `-maxtftp` stores, the data of `35000007` is at file offset 6420 and the data of `35000008` at offset 7044.
- The script in [Commands](#commands) finds each data cell through `hivexml`'s byte runs, checks its size, and overwrites the 8 data bytes. `cmp -l` lists 1 to 3 changed bytes for each copy, and `hivexsh` reads the new values.

`hivexsh` can set a value too, but it writes new cells at the end of the file. The in-place change keeps every other byte as it was. The SHA-256 of each copy is in `~/deployarr-lab/runs/30/stores.sha256` on the lab host.

## Versions

| Component | Version |
| --- | --- |
| `qemu-system-x86` | 1:10.2.1+ds-1ubuntu3.3 |
| `ovmf` | 2025.11-3ubuntu7.3 |
| `seabios` | 1.17.0-1ubuntu1 |
| `ipxe-qemu` | 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2 |
| `dnsmasq-base` | 2.92-1ubuntu0.5 |
| `wireshark-common` and `tshark` | 4.6.4-1 |
| `libhivex-bin` | 1.3.24-2.1build2 |
| Python | 3.14.4 |

The firmware files, the boot files, and `boot.wim` are the same as in [#61](https://github.com/Tsukidy/deployarr/issues/61), on the same lab host on the same day. Its page, [Versions](bootmgr-object-type.md#versions) of the `{bootmgr}` object type page, lists their versions and SHA-256. `boot.wim` is index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`, Microsoft Windows PE (amd64), build 26100.9457, SHA-256 `c0a18c72a7f84c36120e5681bb325e2c892e919886671e553a6129899a09097a`. `bootmgfw.efi` is 10.0.28000.367 and `bootmgr.exe` is 10.0.26100.9444, both signed through the Microsoft Windows Production PCA 2011.

## Details

| Block size | Window size | Blocks sent | `uefi`, VM start to `boot.wim` done | `bios`, VM start to `boot.wim` done |
| --- | --- | --- | --- | --- |
| 1456, no element | 1, no element | 393,333 | 57.1 s | 97.7 s |
| 1456 | 4 | 393,333 | 25.1 s | 54.6 s |
| 1456 | 8 | 393,333 | 21.1 s | 47.5 s |
| 1456 | 16 | 393,333 | 16.7 s | 47.5 s |
| 1456 | 32 | 393,333 | 17.0 s | 48.3 s |
| 4096 | 1 | 139,818 | 34.6 s | 40.3 s |
| 8192 | 1 | 69,909 | 21.7 s | 24.2 s |
| 16384 | 1 | 34,955 | 17.6 s | 25.2 s |
| 32768 | 1 | 17,478 | 15.3 s | 21.4 s |
| 8192 | 8 | 69,909 | 15.0 s | 23.2 s |
| 32768 | 32 | 17,478 | 12.9 s | 21.2 s |
| 1456, no element, again | 1, no element | 393,333 | 56.9 s | 98.8 s |

- The VM start time is when the series script started `vm.sh`, and the end is the `transfer-finished` line of `boot.wim`.
- In the `bios` run with 32768 and 1, the wall clock stamp of the VM start reads 20:00:58.896, a misprint of 20:00:58.089. `/proc/uptime` puts that step 1.12 seconds after the prototype start, as in every other run. That cell uses the corrected time.
- Every run made the same number of RRQs as #27, 32 on `uefi` and 13 on `bios`. The last RRQ, for `\EFI\Microsoft\Boot\Policies\UnlockToken.pol`, came after `boot.wim` in every run.
- A screenshot 15 seconds after that request shows a WinPE command window in every run.
- `boot.sdi`, 3,170,304 bytes, took 0.282 seconds on `uefi` and 0.532 seconds on `bios` with no BCD elements, and 0.026 and 0.071 seconds with 32768 and 32.

### Empty last block

`boot.sdi` is 3,170,304 bytes, exactly 774 blocks of 4096 and 387 blocks of 8192. At those sizes the prototype ends the file with an empty DATA block, as RFC 1350 asks. Neither boot manager ACKs that block.

| Run | Retransmits | Seconds | Outcome |
| --- | --- | --- | --- |
| `uefi`, 4096 and 1 | 5 | 30.167 | `timeout` |
| `uefi`, 8192 and 1 | 5 | 29.067 | `stopped` |
| `uefi`, 8192 and 8 | 5 | 22.449 | `stopped` |
| `bios`, 4096 and 1 | 5 | 30.221 | `timeout` |
| `bios`, 8192 and 1 | 5 | 30.134 | `timeout` |
| `bios`, 8192 and 8 | 6 | 30.117 | `timeout` |

- Each of the six logs `bytes_acked=3170304`, so the boot manager acknowledged every byte. The last ACK in the full captures is for block 774 or 387.
- The boot manager requests `boot.wim` 0.05 to 0.19 seconds after it requests `boot.sdi`. The prototype resends the empty block every 5 seconds until its retries run out, or until the series stops it first, as in the two `stopped` rows.
- The boot goes on, and `boot.wim` loads in the meantime. No other file in these runs is an exact number of blocks.
- The boot managers stop reading once they hold the `tsize` bytes (inferred).

### Default runs

The default runs on this page and on the earlier lab pages give the spread of one setting:

| Run | `uefi` | `bios` |
| --- | --- | --- |
| #27, 2026-11-13 | 52.145 s | 89.254 s |
| #29, 2026-11-13, store with no loader `path` | 51.273 s | 90.395 s |
| #61, 2026-11-17, `{bootmgr}` type `0x10200002` | 51.118 s | 91.496 s |
| This page, first run | 48.716 s | 89.458 s |
| This page, last run | 48.392 s | 90.598 s |

## Captures

| File | What it holds |
| --- | --- |
| [`tftp-transfer-time-uefi.pcapng`](captures/tftp-transfer-time-uefi.pcapng) | The 12 `uefi` boots, reduced |
| [`tftp-transfer-time-uefi-dhcp.txt`](captures/tftp-transfer-time-uefi-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`tftp-transfer-time-bios.pcapng`](captures/tftp-transfer-time-bios.pcapng) | The 12 `bios` boots, reduced |
| [`tftp-transfer-time-bios-dhcp.txt`](captures/tftp-transfer-time-bios-dhcp.txt) | `tshark -V` decode of its DHCP packets |

One `dumpcap` ran for each series, with the filter `udp and udp[8:2] != 3`. It keeps only UDP, and it drops TFTP DATA packets and IP fragments. The committed files are reduced copies that drop only the TFTP ACKs, with `tshark -r - -Y 'not tftp.opcode == 4' -w -`. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Series | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi` | 1,282,359 packets, 102,641,520 bytes, SHA-256 `3165b00d46bad5e99c4b6af9821eeba2d53cf0dee7b56d50133ca0aa4aacc936` | 1,281,435 | 924 packets, 126,612 bytes, SHA-256 `45d1b2a7118f439037368505c483240b848f1ababcf9c7ce255c7b9ea65a6813` |
| `bios` | 1,269,591 packets, 101,627,760 bytes, SHA-256 `3b2b2a03ba079cea991fea64ef7c51ffecce05f18234461e368d1a375943be0e` | 1,268,979 | 612 packets, 109,332 bytes, SHA-256 `292c048f618bddfbe935e281e2bb95270e25b57916b69b4a4641904a0c331c75` |

- The committed `uefi` copy holds 84 DHCP packets and 384 RRQs, 32 for each boot. The `bios` copy holds 132 DHCP packets and 156 RRQs, 13 for each boot.
- The `dumpcap` logs, `tftp-transfer-time-<vm>-dumpcap.log` next to the full captures, report no dropped packets.
- The TFTP data check from the lab guide prints nothing for the full and the reduced captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/30/`: a prototype log for each boot, the series script and its clock stamps, a screenshot for each boot, the stores, and `versions.txt`.

## Commands

This script makes each store variant from a `-maxtftp` store, as `python3 -I set_tftp_elements.py <source> <output> <blksize> <windowsize>`:

```python
import struct, subprocess, sys
import xml.etree.ElementTree as ET

src, dst, blksize, windowsize = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
RAMDISK = "{ae5534e0-a924-466c-b836-758539a3ee3a}"
root = ET.fromstring(subprocess.run(["hivexml", src], check=True, capture_output=True).stdout)
objects = next(n for n in root.iter("node") if n.get("name") == "Objects")
ramdisk = next(n for n in objects.findall("node") if n.get("name") == RAMDISK)
elements = next(n for n in ramdisk.findall("node") if n.get("name") == "Elements")
data = bytearray(open(src, "rb").read())
for name, new in {"35000007": blksize, "35000008": windowsize}.items():
    node = next(n for n in elements.findall("node") if n.get("name") == name)
    value = next(v for v in node.findall("value") if v.get("key") == "Element")
    runs = value.find("byte_runs").findall("byte_run")
    cell = int(runs[1].get("file_offset"))
    assert len(runs) == 2 and runs[1].get("len") == "12"
    assert struct.unpack_from("<i", data, cell)[0] == -16
    struct.pack_into("<Q", data, cell + 4, new)
open(dst, "wb").write(data)
```

Each series started one capture, then started a prototype and booted the VM once for each store. [Run an experiment](../../lab-setup.md#run-an-experiment) starts the prototype before the capture. In both orders, the prototype and the capture run before the VM starts. `$LAB` is the checkout of `main` at `d799a1b`, `<vm>` is `uefi` or `bios`, and `<cfg>` names the store, such as `b1456-w4`.

```bash
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/tftp-transfer-time-<vm>.pcapng 2> ~/deployarr-lab/captures/tftp-transfer-time-<vm>-dumpcap.log &
```

For each store:

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/runs/30/tftp-root --chain single --no-block-log --bcd-<vm> 'boot\<vm>-<cfg>.bcd' --log-file ~/deployarr-lab/runs/30/<vm>-<cfg>-prototype.log &
timeout 600 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The script waited for the `transfer-finished` line of `boot.wim` and the request for `UnlockToken.pol`, then 15 seconds more. It took a screenshot with the HMP command `screendump`, stopped the VM with the HMP command `quit`, and stopped the prototype with `sudo pkill -INT -f '^python3 -B .*pxe_prototype.py'`. After the last store, `sudo pkill -INT -x dumpcap` stopped the capture.

These commands make the committed files:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/tftp-transfer-time-<vm>.pcapng > docs/research/pxe-lab/captures/tftp-transfer-time-<vm>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/tftp-transfer-time-<vm>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/tftp-transfer-time-<vm>.pcapng > docs/research/pxe-lab/captures/tftp-transfer-time-<vm>-dhcp.txt
```

## What is inferred

- On `bios`, the client side limits the rate of small blocks (inferred). With 1456-byte blocks, the transfer runs at 9,728 to 9,942 blocks a second for windows of 8 to 32. The same prototype sent about 55,500 blocks a second to `uefi` with window 32. The runs do not show whether `bootmgr.exe`, the iPXE ROM's PXE interface, or QEMU's `e1000` sets the limit.
- The boot managers read the TFTP elements from the store they loaded (inferred). The request options change only for the files that come after the store, and only when the store changes.
- A block larger than 1468 bytes travels as IP fragments on this MTU of 1500 (inferred from the sizes). Each DATA packet adds a 4-byte TFTP header, an 8-byte UDP header, and a 20-byte IPv4 header. The capture filter drops fragments, so the captures do not show them.

- Secure Boot was on in every `uefi` run (inferred). No run here queried it. Each run started from a fresh copy of the same `OVMF_VARS_4M.ms.fd` as #61, whose WinPE reported `UEFISecureBootEnabled` = 1 on the same day with the same `vm.sh`.

## Not covered

- The `uefi2023` VM and the 2023-signed boot files. [#32](https://github.com/Tsukidy/deployarr/issues/32) runs those.
- The two-stage chain. After the boot manager starts, its TFTP requests are the same in both chains, as #28 records in [#137](https://github.com/Tsukidy/deployarr/pull/137).
- Packet loss, a slower link, and physical network cards and switches. On the lab bridge no packet was lost, so no run shows the cost of a lost fragment or block at the larger sizes.
- Several PCs at once, and the Rust server that deployarr will have.
- Other block sizes with windows above 1, beyond 8192 and 8, and 32768 and 32.

## What changes in deployarr

- With no TFTP elements in the BCD, both boot managers use 1456-byte blocks and a window of 1. [TFTP](../../architecture.md#tftp) can record this, and the time it gives on this lab, 48 to 52 seconds on `uefi` and 89 to 92 seconds on `bios`.
- The BCD templates can set `ramdisktftpblocksize` and `ramdisktftpwindowsize`, and the boot managers follow them. 32768 and 32, the server's limits in [TFTP](../../architecture.md#tftp), cut `boot.wim` to 4.7 seconds on `uefi` and 13.4 seconds on `bios` on this lab.
- On `bios`, a window above 8 gave nothing at 1456 bytes, and block size mattered more than window size. A default that sets both a large block and a window serves both firmware types here.
- The lab bridge loses no packets. #33 chooses the default values. A setting to lower them would serve networks where large blocks or windows lose packets, which no run here measured.
- The server must handle requests with and without `windowsize`, and `windowsize=1`, as the prototype does. All three occur.
- When a file is an exact number of blocks, both boot managers leave the empty last block without an ACK. [TFTP](../../architecture.md#tftp) needs a rule for that case. Without one, the server resends the block for 30 seconds and counts a delivered file as failed.
