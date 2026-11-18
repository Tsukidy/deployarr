# iPXE option 97 on QEMU 10.2.1's default machine types in the PXE lab

This page records whether the iPXE ROM of the lab's SeaBIOS VM sends option 97 on QEMU 10.2.1's default `pc` and `q35` machine types and with only a 64-bit SMBIOS entry point, for [#217](https://github.com/Tsukidy/deployarr/issues/217). It also records which SMBIOS entry point the guest had in each run, and how it was read. The runs are from 2026-11-18, on the lab host. A later pull request applies the result to [DHCP](../../architecture.md#dhcp).

## Result

| Run | `-machine` | Machine type | `smbios-entry-point-type` | SMBIOS entry point | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | `pc` | `pc-i440fx-10.2` | `auto` | Only `_SM_`, version 2.8, at `0xf5220` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |
| 2 | `q35` | `pc-q35-10.2` | `auto` | Only `_SM_`, version 2.8, at `0xf5230` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |
| 3 | `pc,smbios-entry-point-type=64` | `pc-i440fx-10.2` | `64` | Only `_SM3_`, version 3.0.0, at `0xf5220` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |

The machine type and the property's value come from the HMP commands `qom-get /machine type` and `qom-get /machine smbios-entry-point-type` in each run.

- The ROM, from ipxe-qemu 1.21.1+git-20250829.969ce2c55, sent option 97 in all three requests in every run. Each option 97 is 17 bytes long. It holds type 0 and the VM's UUID, `4c414200-0000-4000-8000-000000000021`.
- With no `smbios-entry-point-type`, `pc` and `q35` gave the guest a 32-bit entry point, `_SM_`, and no 64-bit one.
- With `smbios-entry-point-type=64`, the guest had only a 64-bit entry point, `_SM3_`. Its table is at `0xf5240`, below 4 GiB. The ROM still sent option 97.
- In each run, the UUID in the guest's SMBIOS type 1 structure holds the same 16 bytes that follow the type byte in option 97.
- In each run, the ROM then downloaded `boot\x64\pxeboot.n12` in full.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| PXE prototype | `tools/pxe-prototype/` at `3fc0d3f`, with `--chain single --answer-without-guid --no-block-log`. Its `started` line shows `dhcp_ports=67,4011` and `answer_without_guid=true`. |
| Network | `br-lab` at `10.41.0.1/24`, where the prototype answers on ports 67 and 4011. Since [#206](https://github.com/Tsukidy/deployarr/pull/206), dnsmasq runs in the network namespace `deployarr-lab-dhcp` at `10.41.0.2` and hands out addresses from there. |
| Chain | `--chain single`, with the default store `boot\bios.bcd` |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| `bios` VM | The `bios` values in `tools/lab/lab.conf` and `tools/lab/vm.sh` at `3fc0d3f`: SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, `-uuid 4c414200-0000-4000-8000-000000000021`, baseboard serial `DEPLOYARR-LAB-BIOS`, 2 vCPUs, 4 GB. Only `-machine` differs between the runs. |
| Disk, serial log, and monitor | Each run had a new empty 64 GB disk and its serial log in its own folder, `~/deployarr-lab/runs/217/<run>/vm`, in place of `~/deployarr-lab/bios`. Each run added the HMP monitor socket `/tmp/lab-217-mon.sock`. |

The runs did not use `tools/lab/vm.sh`. For the `bios` VM, that script always passes `-machine pc,smbios-entry-point-type=32`, and it puts the caller's arguments after its own. QEMU parses every `-machine` into one set of machine options ([`system/vl.c:3405-3410`](https://github.com/qemu/qemu/blob/v10.2.1/system/vl.c#L3405-L3410)), so a later `-machine pc` keeps the property. So each run started `qemu-system-x86_64` with the script's `bios` arguments in the script's order, and its own `-machine` value. Each run's section gives the full command line.

The three runs went one after another, from 20:51:48 to 20:52:43 UTC. Uptime at the first stamp puts the host's last start at about 20:41 UTC, so the host did not restart during the runs. The run script wrote the wall clock and `/proc/uptime` at each step. Some wall-clock stamps run up to 0.8 seconds ahead of uptime and then fall back, in these runs and in the first set below. Uptime, the capture times, the prototype log, and the file times agree with each other, so the page takes its intervals from them.

An earlier set of the same three runs, from 20:22 to 20:24 UTC, wrote no SMBIOS dump. Its `pmemsave` command left the file name unquoted, as [Commands](#commands) explains. All three runs then ran again with the file name quoted, as `pc-r2`, `q35-r2`, and `pc-ep64-r2`, so that each dump and its capture come from the same boot. This page uses only that second set. The first set's captures show option 97 with the same UUID in the same three requests.

### Machine types on the lab host

`qemu-system-x86_64 -machine help` on the lab host prints these lines for the i440FX and Q35 types that the runs name, and for Ubuntu's own types:

```text
ubuntu               Ubuntu 26.04 PC (i440FX + PIIX, 1996) (alias of pc-i440fx-resolute)
pc-i440fx-resolute   Ubuntu 26.04 PC (i440FX + PIIX, 1996) (default)
pc                   Standard PC (i440FX + PIIX, 1996) (alias of pc-i440fx-10.2)
pc-i440fx-10.2       Standard PC (i440FX + PIIX, 1996)
ubuntu-q35           Ubuntu 26.04 PC (Q35 + ICH9, 2009) (alias of pc-q35-resolute)
pc-q35-resolute      Ubuntu 26.04 PC (Q35 + ICH9, 2009)
q35                  Standard PC (Q35 + ICH9, 2009) (alias of pc-q35-10.2)
pc-q35-10.2          Standard PC (Q35 + ICH9, 2009)
```

- `-machine help` marks `pc-i440fx-resolute` as the default. `ubuntu` and `ubuntu-q35` name Ubuntu 26.04's own types.
- `pc` and `q35` name the upstream 10.2 types. In each run, `qom-get /machine type` agrees, with `pc-i440fx-10.2-machine` or `pc-q35-10.2-machine`. The SMBIOS type 1 structure in each dump names `pc-i440fx-10.2` or `pc-q35-10.2` too.
- No run used a resolute type.
- `qemu-system-x86_64 -machine pc,help` lists `smbios-entry-point-type=<str> - SMBIOS Entry Point type [32, 64]`. It does not list `auto`, but `qom-get` reads `auto` in runs 1 and 2.

## Versions

| Component | Version | In [Versions](single-stage-chain.md#versions) of the single-stage page |
| --- | --- | --- |
| `qemu-system-x86` | 1:10.2.1+ds-1ubuntu3.3 | Same |
| `qemu-system-common`, `qemu-system-data`, and `qemu-utils` | 1:10.2.1+ds-1ubuntu3.3 | Not listed |
| `seabios` | 1.17.0-1ubuntu1 | Same |
| `ipxe-qemu` | 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2 | Same |
| `dnsmasq-base` | 2.92-1ubuntu0.5 | Same |
| `wireshark-common` and `tshark` | 4.6.4-1 | Same |
| Python | 3.14.4 | Same |
| `ovmf` | 2025.11-3ubuntu7.3 | Same. These runs do not use it. |
| `swtpm` | 0.10.1+ds-1ubuntu3 | Same. These runs do not use it. |
| `osslsigncode` | 2.13-1 | Same. These runs do not use it. |

The HMP command `info version` prints `10.2.1Debian 1:10.2.1+ds-1ubuntu3.3` in each run.

| Firmware file | SHA-256 | In [Versions](single-stage-chain.md#versions) of the single-stage page |
| --- | --- | --- |
| `/usr/share/seabios/bios-256k.bin`, from `seabios` | `e26615f9ad430328f49ca105e570b2dc4490a08a34ea73d27cae8b809a30ee06` | Same |
| `/usr/share/qemu/efi-e1000.rom`, from `ipxe-qemu` | `73e68ac2bde85830847d087724f848f286d3691ae2440100a5a13e7337100c40` | Not listed. The HMP command `info qtree` shows it as the `e1000` card's `romfile` in each run. |
| `/usr/share/qemu/pxe-e1000.rom`, from `ipxe-qemu` | `d011d9d60664b17312bef960d6ebeae08c8f380b209dceba5f93fdf250879556` | Not listed. No device in `info qtree` names it. |
| `/usr/share/OVMF/OVMF_CODE_4M.ms.fd` and `OVMF_VARS_4M.ms.fd` | Not recorded | Listed. These runs do not use them. |

`qemu-system-x86_64 -L help` lists `/usr/share/qemu` and `/usr/share/seabios`. Only the second holds `bios-256k.bin`.

| File in the TFTP root | SHA-256 | In [Versions](single-stage-chain.md#versions) of the single-stage page |
| --- | --- | --- |
| `boot/x64/pxeboot.n12` | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Same hash and size. Each run downloaded it in full. |
| `boot/x64/bootmgr.exe` | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Same hash and size. Each run stopped during its download. |
| `boot/x64/bootmgfw.efi`, `boot/boot.sdi`, `boot/fonts/wgl4_boot.ttf`, and `boot/x64/boot.wim` | Not recorded | Listed. These runs do not use them. |

Every version and hash that both pages list is the same. `boot/bios.bcd`, which option 252 names, has the hash in [BCD stores](single-stage-chain.md#bcd-stores). No run requested it.

## How the entry point was read

Each run dumped the guest's BIOS segment, `0xF0000` to `0xFFFFF`, with the HMP command `pmemsave 0xf0000 0x10000 "<absolute path>"` on the monitor socket. The run script sent it as soon as the prototype logged the first DISCOVER from `52:54:00:41:00:21`. That DISCOVER already carried option 97. The dump file's modification time came 34 to 119 ms after the captured DISCOVER, and before the lease REQUEST.

A scan lists every `_SM_` and `_SM3_` string in the dump. It counts one as an entry point only when it starts on a 16-byte boundary and its checksum is valid. For `_SM_` it also checks the intermediate anchor `_DMI_` and its checksum. A second, separate piece of code repeated the checksum of each counted anchor. A walk of the structure table from each counted entry point found 11 structures that end with type 127, and covered the table's whole length.

Every dump also holds an unaligned `_DMI_` at `0xf1934` and an unaligned `_SM3_` at `0xf1941`. The scan does not count them, and that `_SM3_` fails its checksum.

## Run 1, `pc`

### Command line

The command line, from `/proc/<pid>/cmdline` of the QEMU process, with line breaks added:

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/217/pc-r2/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/217/pc-r2/vm/serial.log
  -machine pc
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/tmp/lab-217-mon.sock,server,nowait
```

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `cddb17e8e365efaa92452d9029382eee90024c4d95abf129ddf78a7f7ed39ab1`. Its modification time is 20:51:53.349, 38 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM_` at `0xf5220`, length `0x1f`, version 2.8. Its `_DMI_` anchor and checksum are valid. Its table is 520 bytes at `0x000f5240`, with 11 structures.
- No aligned `_SM3_` with a valid checksum is in the dump.

### DHCP

Frames 1 to 7 of [`ipxe-option-97-pc-dhcp.txt`](captures/ipxe-option-97-pc-dhcp.txt), xid `ac637e24`. "UUID" is `4c414200-0000-4000-8000-000000000021`.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 20:51:53.310 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 20:51:53.311 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 3 | 20:51:53.311 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 4 | 20:51:54.304 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 20:51:54.304 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 20:51:54.304 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 20:51:54.305 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

## Run 2, `q35`

### Command line

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/217/q35-r2/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/217/q35-r2/vm/serial.log
  -machine q35
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/tmp/lab-217-mon.sock,server,nowait
```

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `0b234f02dc505ed0319c019b21231551eb1eab2e6dcb6a59cc63e6f41c404b57`. Its modification time is 20:52:15.010, 119 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM_` at `0xf5230`, length `0x1f`, version 2.8. Its `_DMI_` anchor and checksum are valid. Its table is 502 bytes at `0x000f5250`, with 11 structures.
- No aligned `_SM3_` with a valid checksum is in the dump.

### DHCP

Frames 1 to 7 of [`ipxe-option-97-q35-dhcp.txt`](captures/ipxe-option-97-q35-dhcp.txt), xid `10acc346`. "UUID" is `4c414200-0000-4000-8000-000000000021`.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 20:52:14.891 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 20:52:14.891 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 3 | 20:52:14.892 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 4 | 20:52:15.884 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 20:52:15.884 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 20:52:15.884 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 20:52:15.884 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

## Run 3, `pc,smbios-entry-point-type=64`

### Command line

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/217/pc-ep64-r2/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/217/pc-ep64-r2/vm/serial.log
  -machine pc,smbios-entry-point-type=64
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/tmp/lab-217-mon.sock,server,nowait
```

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `f8fb6ca3ee7b91f58ba1fe0a549afb4d2fbb39e02bf42763ce2ea9bc436619ba`. Its modification time is 20:52:35.926, 34 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM3_` at `0xf5220`, length `0x18`, version 3.0.0, entry point revision 1. Its table is at most 526 bytes, at `0x00000000000f5240`, below 4 GiB. The walk found 11 structures in 526 bytes.
- The dump holds no `_SM_` string anywhere.

### DHCP

Frames 1 to 7 of [`ipxe-option-97-pc-ep64-dhcp.txt`](captures/ipxe-option-97-pc-ep64-dhcp.txt), xid `74f4e11a`. "UUID" is `4c414200-0000-4000-8000-000000000021`. In this run, the prototype's OFFER is frame 2 and dnsmasq's is frame 3, the other way round from runs 1 and 2.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 20:52:35.892 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 20:52:35.892 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 3 | 20:52:35.893 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 4 | 20:52:36.886 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 20:52:36.886 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 20:52:36.887 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 20:52:36.887 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

## Requests in every run

- The DISCOVER comes from MAC `52:54:00:41:00:21`. It carries the options 53, 57, 93, 94, 60, 77, 55, 175, 61, and 97, in that order. Option 61 holds hardware type 1 and the MAC.
- The lease REQUEST carries the same options, then 54 and 50. The boot file REQUEST carries the same options, then 54.
- dnsmasq logs `user class: iPXE` for the DISCOVER and for the lease REQUEST.
- dnsmasq logs DHCPDISCOVER, DHCPOFFER, DHCPREQUEST, and DHCPACK of `10.41.0.192` for `52:54:00:41:00:21` in each run. Its OFFER and ACK carry no option 97.
- The prototype logs `guid=000042414c000000408000000000000021` on each request it receives. That is option 97's type byte and UUID.
- The screen when the prototype logged the first DISCOVER shows `iPXE 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2`, `SeaBIOS (version 1.17.0-debian-1.17.0-1ubuntu1)`, and `Machine UUID 4c414200-0000-4000-8000-000000000021`.

## TFTP

| Run | Frame 8, RRQ for `boot\x64\pxeboot.n12` | Its outcome | Frame 10, RRQ for `boot\x64\bootmgr.exe` | Its outcome |
| --- | --- | --- | --- | --- |
| 1 | 20:51:57.440 | Complete, 25,238 bytes in 18 blocks, no retransmits | 20:51:57.463 | Stopped after 116,224 bytes |
| 2 | 20:52:19.021 | Complete, 25,238 bytes in 18 blocks, no retransmits | 20:52:19.044 | Stopped after 488,448 bytes |
| 3 | 20:52:40.022 | Complete, 25,238 bytes in 18 blocks, no retransmits | 20:52:40.045 | Stopped after 62,464 bytes |

- The RRQ for `pxeboot.n12` asks for `blksize=1432,tsize=0`. The OACK in frame 9 gives `blksize` 1432 and `tsize` 25238.
- The RRQ for `bootmgr.exe` comes from `pxeboot.n12` (inferred, as on the [single-stage page](single-stage-chain.md#tftp-requests-1)). It asks for `blksize=512,tsize=0`. The OACK in frame 11 gives `tsize` 814544.
- The run script paused the VM once the prototype logged the end of `pxeboot.n12`, then stopped the VM and the prototype. So the `bootmgr.exe` transfer ended part way, and the prototype logs it with `outcome=stopped`. That is how each run ended, not a boot failure.

The last screen of each run shows the ROM's view:

```text
net0: 10.41.0.192/255.255.255.0 gw 10.41.0.1
net0: fe80::5054:ff:fe41:21/64
Next server: 10.41.0.1
Filename: boot\x64\pxeboot.n12
tftp://10.41.0.1/boot\x64\pxeboot.n12... ok
boot\x64\pxeboot.n12 : 25238 bytes [PXE-NBP]
```

## Captures

| File | What it holds |
| --- | --- |
| [`ipxe-option-97-pc.pcapng`](captures/ipxe-option-97-pc.pcapng) | Run 1, reduced |
| [`ipxe-option-97-pc-dhcp.txt`](captures/ipxe-option-97-pc-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`ipxe-option-97-q35.pcapng`](captures/ipxe-option-97-q35.pcapng) | Run 2, reduced |
| [`ipxe-option-97-q35-dhcp.txt`](captures/ipxe-option-97-q35-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`ipxe-option-97-pc-ep64.pcapng`](captures/ipxe-option-97-pc-ep64.pcapng) | Run 3, reduced |
| [`ipxe-option-97-pc-ep64-dhcp.txt`](captures/ipxe-option-97-pc-ep64-dhcp.txt) | `tshark -V` decode of its DHCP packets |

`dumpcap` captured with the filter `udp and udp[8:2] != 3`, which drops TFTP DATA packets and IP fragments. The reduced copies went through `tshark -r - -Y 'not tftp.opcode == 4' -w -`, which drops the TFTP ACKs. Each committed file is a byte copy of its run's reduced copy. The full captures stay on the lab host in `~/deployarr-lab/captures/`, with `-r2` in their names.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| 1 | `ipxe-option-97-pc-r2.pcapng`, 258 packets, 23,540 bytes, SHA-256 `d41c5a5485e1b0203328aab45f5852de6f337346564972e95895cecbde1dd0e7` | 247 | 11 packets, 3,672 bytes, SHA-256 `28c34b7228e5009259bf979b01104ad009b20019ea32de80013be79163e4fab4` |
| 2 | `ipxe-option-97-q35-r2.pcapng`, 985 packets, 81,700 bytes, SHA-256 `8933983035e1e671718b6e7b15b79256ab8e9e344c323b92626269e4dba9e023` | 974 | 11 packets, 3,672 bytes, SHA-256 `ca076c21b5657b60a4fe6a4f5bcbbf9333820560f0cd5d77a967d4dfbff97210` |
| 3 | `ipxe-option-97-pc-ep64-r2.pcapng`, 153 packets, 15,140 bytes, SHA-256 `29713cebe6788fdc46863d8e3443115de4238a531ecdc47a98d44a35fa694a15` | 142 | 11 packets, 3,672 bytes, SHA-256 `868626e8e84af2b3898d6cc811aeaba3f50f937006fffc559ce728713bf137dc` |

- The `dumpcap` logs, `ipxe-option-97-<run>-r2-dumpcap.log` next to the full captures, report no dropped packets.
- The TFTP data check from the lab guide prints nothing for the full and the committed captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.
- Each committed copy holds the 7 DHCP packets, the two RRQs, and their two OACKs.
- Wireshark's dissector and `dhcp97.py`, a separate parser of the raw BOOTP payload, read the same option 97 in frames 1, 4, and 6 of each capture.
- The first set's full captures in that folder have the same names without `-r2`. They are not the source of the committed files.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/217/`. Each run's folder holds the prototype log, its slice of the dnsmasq log, the QEMU command lines, the HMP output, the dump and its scan, the screenshots, and the clock stamps. `tools/` holds the run script and the parsers. The first set's folders have the run names without `-r2`.

## Commands

`$WT` is the checkout at `3fc0d3f`. `<run>` is `pc`, `q35`, or `pc-ep64`, and `<machine>` is `pc`, `q35`, or `pc,smbios-entry-point-type=64`. The run script, `run-r2.sh <run>-r2 <machine>`, started the prototype, then the capture, then QEMU, in the order that [Run an experiment](../../lab-setup.md#run-an-experiment) gives:

```bash
sudo python3 -B $WT/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain single --answer-without-guid --no-block-log --log-file ~/deployarr-lab/runs/217/<run>-r2/prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/ipxe-option-97-<run>-r2.pcapng 2> ~/deployarr-lab/captures/ipxe-option-97-<run>-r2-dumpcap.log &
qemu-img create -q -f qcow2 ~/deployarr-lab/runs/217/<run>-r2/vm/disk.qcow2 64G
timeout 300 qemu-system-x86_64 <the arguments in the run's section> &
```

When the prototype logged the first DISCOVER from `52:54:00:41:00:21`, the script sent these HMP commands on the monitor socket:

```text
pmemsave 0xf0000 0x10000 "/home/user/deployarr-lab/runs/217/<run>-r2/fseg-f0000.bin"
screendump <file>.png -f png
```

The file name in `pmemsave` must be in double quotes. `pmemsave` reads its size as an expression ([`hmp-commands.hx:821-826`](https://github.com/qemu/qemu/blob/v10.2.1/hmp-commands.hx#L821-L826)). The expression parser reads a `/` after a number as division ([`monitor/hmp.c:425-457`](https://github.com/qemu/qemu/blob/v10.2.1/monitor/hmp.c#L425-L457)). So it takes the `/` that starts an unquoted absolute path as part of the size. It then stops at the next letter with `invalid char 'h' in expression` ([`monitor/hmp.c:408-415`](https://github.com/qemu/qemu/blob/v10.2.1/monitor/hmp.c#L408-L415)). The first set's `pmemsave` printed that message and wrote no file. A `"` is not an operator, so the expression ends before a quoted name, and the string parser reads the name ([`monitor/hmp.c:94-96`](https://github.com/qemu/qemu/blob/v10.2.1/monitor/hmp.c#L94-L96)).

When the prototype logged the end of `boot\x64\pxeboot.n12`, the script sent `stop`. While the VM was paused, it sent `qom-get /machine type`, `qom-get /machine smbios-entry-point-type`, `info version`, `info qtree`, and a `screendump`. Then it sent `quit` and stopped the prototype and `dumpcap` by the process IDs it had started.

These commands make the reduced and the committed files, and check them:

```bash
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/ipxe-option-97-<run>-r2.pcapng > ~/deployarr-lab/captures/ipxe-option-97-<run>-r2-reduced.pcapng
cp ~/deployarr-lab/captures/ipxe-option-97-<run>-r2-reduced.pcapng docs/research/pxe-lab/captures/ipxe-option-97-<run>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/ipxe-option-97-<run>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/ipxe-option-97-<run>.pcapng > docs/research/pxe-lab/captures/ipxe-option-97-<run>-dhcp.txt
tshark -r - -Y 'dhcp and eth.src == 52:54:00:41:00:21' -T fields -E separator=/t -e frame.number -e ip.dst -e udp.dstport -e dhcp.option.dhcp -e dhcp.option.dhcp_server_id -e dhcp.client_id.uuid < docs/research/pxe-lab/captures/ipxe-option-97-<run>.pcapng
```

## What is inferred

- The ROM takes the UUID for option 97 from the SMBIOS type 1 structure that the entry point leads to (inferred). In each run, option 97 holds the same 16 bytes as that structure's UUID field, and iPXE `969ce2c55` finds SMBIOS through [`bios_find_smbios`](https://github.com/ipxe/ipxe/blob/969ce2c55/src/arch/x86/interface/pcbios/bios_smbios.c#L100-L112).
- In run 3, the ROM found the table through the 64-bit entry point (inferred). The dump holds no `_SM_`, and `bios_find_smbios` tries `_SM3_` when it finds no `_SM_`. [`bios_find_smbios3`](https://github.com/ipxe/ipxe/blob/969ce2c55/src/arch/x86/interface/pcbios/bios_smbios.c#L78-L83) refuses a table address that does not fit the ROM's physical address type. Run 3's table is at `0xf5240`.
- On `pc` and `q35`, QEMU gave the 32-bit entry point because the 32-bit tables could describe the VM (inferred from [`hw/smbios/smbios.c:1215-1239`](https://github.com/qemu/qemu/blob/v10.2.1/hw/smbios/smbios.c#L1215-L1239)). The runs show only the property's value, `auto`, and the entry point.
- The dump shows the entry point that the ROM read (inferred). Each dump came 34 to 119 ms after a DISCOVER that already carried option 97, not at the moment the ROM read SMBIOS.
- The unaligned `_DMI_` and `_SM3_` at `0xf1934` and `0xf1941` are strings in SeaBIOS's code, not entry points (inferred). The bytes around them read `PIR`, `_DMI_`, `SMBIOS`, `_SM3_`, `SMBIOS 3.0`, and `Skipping MPTABLE copy due to large size`.
- The source citations are upstream. Nobody read Ubuntu's patches to QEMU, SeaBIOS, or iPXE.

## Not covered

- Ubuntu 26.04's own machine types, `pc-i440fx-resolute` and `pc-q35-resolute`, also named `ubuntu` and `ubuntu-q35`. `-machine help` marks `pc-i440fx-resolute` as the default. No run used them.
- The `fbbdc3926` ROM on QEMU 10.2.1's machine types, or on any machine type 9.0 or later.
- `-machine pc,smbios-entry-point-type=32`, the lab's own setting. The [single-stage page](single-stage-chain.md#dhcp-1) records option 97 from it on the same versions.
- A guest with no SMBIOS UUID, or with no SMBIOS entry point.
- The UEFI VMs, physical PCs, and other firmware.
- The boot after `pxeboot.n12`. Each run stopped before `bootmgr.exe` finished.

## What changes in deployarr

In the [DHCP](../../architecture.md#dhcp) section:

- The sentence that a SeaBIOS guest with the lab's versions "should send option 97 (inferred, not run)" can state the measured result. With QEMU 10.2.1, SeaBIOS 1.17.0, and ipxe-qemu 1.21.1+git-20250829.969ce2c55, the guest sent option 97 in its DISCOVER, its lease REQUEST, and its boot file REQUEST. It did so on `pc` and `q35` with no `smbios-entry-point-type`, and on `pc` with only a 64-bit entry point.
- The two reasons the section gives match the runs. `pc` and `q35` with `auto` gave a 32-bit entry point. With only a 64-bit entry point, the lab's ROM still sent option 97.
- The facts from the triage of [#56](https://github.com/Tsukidy/deployarr/issues/56), measured on ipxe-qemu `fbbdc3926` and QEMU 8.2.2, stay. These runs do not test those versions.
- The rule that deployarr answers a request without option 97 does not change. Its reason is wipe safety, and the runs do not touch it.

In the lab's `bios` VM settings:

- On the lab's versions, `smbios-entry-point-type=32` does not decide whether the ROM sends option 97. The ROM sent it without the property, and with only a 64-bit entry point.
- None of the four SeaBIOS settings measured on the lab's versions makes a request without option 97. They are the three settings on this page and the lab's own `pc,smbios-entry-point-type=32`. So the lab has no SeaBIOS client that tests the reply to such a request.
- The lab guide's sentence in [Run an experiment](../../lab-setup.md#run-an-experiment) says that the SeaBIOS VM runs with `smbios-entry-point-type=32`, "so its iPXE ROM sends option 97". Its option 97 row in [Troubleshooting](../../lab-setup.md#troubleshooting) says that a failed check means "The VM did not get the SMBIOS setting." On the lab's versions, the ROM sends option 97 without that setting.
- The comment on the `bios` VM's `-machine` line in `tools/lab/vm.sh` says "With a 32-bit SMBIOS entry point, the iPXE ROM sends option 97." Runs 1 and 2 agree with it. Run 3 shows that the ROM also sends option 97 with only a 64-bit entry point.
- The property still matters for the `fbbdc3926` ROM, which needs a 32-bit entry point, as the [PXE prototype's requirements](../../../tools/pxe-prototype/README.md#requirements) say.
