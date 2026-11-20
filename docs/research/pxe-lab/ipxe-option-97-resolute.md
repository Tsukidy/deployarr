# iPXE option 97 on Ubuntu 26.04's own machine types in the PXE lab

This page records whether the iPXE ROM of the lab's SeaBIOS VM sends option 97 on Ubuntu 26.04's own machine types, `pc-i440fx-resolute` and `pc-q35-resolute`, and with no `-machine` option at all, for [#344](https://github.com/Tsukidy/deployarr/issues/344). It also records which SMBIOS entry point the guest had in each run, and how it was read. It compares each run with the `pc` and `q35` runs of [#217](ipxe-option-97.md). The runs are from 2026-11-20, on the lab host.

Paths in code font that start with `runs/` or `captures/` are relative to `~/deployarr-lab` on the lab host. They are outside the repository.

## Result

| Run | `-machine` | Machine type | `smbios-entry-point-type` | SMBIOS entry point | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1, `i440fx` | `pc-i440fx-resolute` | `pc-i440fx-resolute` | `auto` | Only `_SM_`, version 2.8, at `0xf5210` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |
| 2, `q35` | `pc-q35-resolute` | `pc-q35-resolute` | `auto` | Only `_SM_`, version 2.8, at `0xf5220` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |
| 3, `default` | None | `pc-i440fx-resolute` | `auto` | Only `_SM_`, version 2.8, at `0xf5210` | In the DISCOVER, the lease REQUEST, and the boot file REQUEST |

The machine type and the property's value come from the HMP commands `qom-get /machine type` and `qom-get /machine smbios-entry-point-type` in each run, in `runs/344/<run>/hmp.txt`. `qom-get` prints the type with `-machine` at the end, such as `pc-i440fx-resolute-machine`.

- The ROM, from ipxe-qemu 1.21.1+git-20250829.969ce2c55, sent option 97 in all three requests in every run. Each option 97 is 17 bytes long. It holds type 0 and the VM's UUID, `4c414200-0000-4000-8000-000000000021`.
- With no `-machine` option, QEMU ran `pc-i440fx-resolute`. Run 3's `qom-get /machine type` reads `"pc-i440fx-resolute-machine"`. This confirms what the brief inferred from the `(default)` mark in `-machine help`.
- Run 3's SMBIOS dump is the same file as run 1's, byte for byte. Both have the SHA-256 `ea400fcb1cb92d765312a904300f9b8cc3c7bf6d4fa0ae19e5cbd0603ba8af30`.
- Both resolute types gave the guest a 32-bit entry point, `_SM_`, and no 64-bit one.
- In each run, the UUID in the guest's SMBIOS type 1 structure holds the same 16 bytes that follow the type byte in option 97.
- In each run, the ROM then downloaded `boot\x64\pxeboot.n12` in full.

### Compared with #217

| This page | #217 run | Machine type | Entry point | Table | Option 97 |
| --- | --- | --- | --- | --- | --- |
| Run 1 and run 3 | Run 1, `pc` | `pc-i440fx-resolute` against `pc-i440fx-10.2` | `_SM_` 2.8 at `0xf5210` against `_SM_` 2.8 at `0xf5220` | 544 bytes at `0xf5230` against 520 bytes at `0xf5240` | In frames 1, 4, and 6 of both |
| Run 2 | Run 2, `q35` | `pc-q35-resolute` against `pc-q35-10.2` | `_SM_` 2.8 at `0xf5220` against `_SM_` 2.8 at `0xf5230` | 526 bytes at `0xf5240` against 502 bytes at `0xf5250` | In frames 1, 4, and 6 of both |

The #217 values come from its [Result](ipxe-option-97.md#result), its run sections, and `runs/217/pc-r2/smbios-scan.txt` and `runs/217/q35-r2/smbios-scan.txt`.

- The result for option 97 is the same. Every request carries it, with the same length, type, and UUID.
- The entry point type and version are the same, `_SM_` 2.8, with `smbios-entry-point-type` `auto`.
- On each resolute type, the entry point sits 16 bytes lower than on the upstream type, and the structure table is 24 bytes longer. Each table still holds 11 structures of the same types, `0, 1, 2, 3, 4, 16, 17, 19, 19, 32, 127`. The page records these addresses and lengths as measured. It does not explain them.
- The type 1 and type 2 structures name the resolute type. On run 1, type 1 reads product `Ubuntu 26.04 PC (i440FX + PIIX, 1996)` and version `pc-i440fx-resolute`, where #217's run 1 read `Standard PC (i440FX + PIIX, 1996)` and `pc-i440fx-10.2`. Run 2 reads `Ubuntu 26.04 PC (Q35 + ICH9, 2009)` and `pc-q35-resolute`. The source is each run's `smbios-walk.txt`.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM. `nproc` prints 6. |
| PXE prototype | `tools/pxe-prototype/pxe_prototype.py` in the lab checkout `~/deployarr-lab/main` at `dbea61c`, with `--chain single --answer-without-guid --no-block-log`. Its `started` line shows `chain=single`, `dhcp_ports=67,4011`, and `answer_without_guid=true` (`runs/344/<run>/prototype-summary.txt`). |
| Network | `br-lab` at `10.41.0.1/24`, where the prototype answers on ports 67 and 4011. dnsmasq runs in the network namespace `deployarr-lab-dhcp` at `10.41.0.2`, PID 81819, and hands out addresses from there. The uplink is off, with `net.ipv4.ip_forward = 0` (`runs/344/<run>/preflight.txt`). |
| Chain | `--chain single`, with the default store `boot\bios.bcd` |
| TFTP root | `~/deployarr-lab/tftp/2011`, built by `tools/lab/media.sh` as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says |
| `bios` VM | The `bios` values in `tools/lab/lab.conf` and `tools/lab/vm.sh` at `dbea61c`: SeaBIOS, QEMU's `e1000` card with its iPXE ROM, MAC `52:54:00:41:00:21`, `-uuid 4c414200-0000-4000-8000-000000000021`, baseboard serial `DEPLOYARR-LAB-BIOS`, 2 vCPUs, 4 GB. Only `-machine` differs between the runs, and run 3 has none. |
| Disk, serial log, and monitor | Each run had a new empty 64 GB disk and its serial log in `runs/344/<run>/vm/`, in place of `~/deployarr-lab/bios`. Each run added the HMP monitor socket `runs/344/<run>/mon.sock`. |

`git diff --stat 3fc0d3f dbea61c -- tools/lab/vm.sh tools/pxe-prototype` lists only `tools/pxe-prototype/README.md` (`runs/344/host/env-before.txt`). So `vm.sh` and `pxe_prototype.py` are the same as in #217's runs, at `3fc0d3f`. The working checkout of this ticket was also at `dbea61c`, on branch `lab-344-resolute-option-97`. No lab script ran from it.

The runs did not use `tools/lab/vm.sh`, for the reason the #217 page's [Setup](ipxe-option-97.md#setup) gives. For the `bios` VM, that script always passes `-machine pc,smbios-entry-point-type=32`, and QEMU merges a later `-machine` into it. So each run started `qemu-system-x86_64` with the script's `bios` arguments in the script's order, and its own `-machine` value or none. The run script checked `/proc/<pid>/cmdline` of the QEMU process for exactly one `-machine` with the expected value, or for none in run 3 (`runs/344/tools/run.sh:379-384`). Each run's section gives the full command line.

The three runs went one after another, from 17:01:30 to 17:02:28 UTC on 2026-11-20 (`runs/344/<run>/times.txt`). The run script wrote `$EPOCHREALTIME`, truncated to milliseconds, and `/proc/uptime` at each step.

- Every `env.txt` and `env-before.txt` gives the boot time `2026-11-18 20:40:58`. That is the same boot as #217's runs, whose page puts the host's last start at about 20:41 UTC.
- Uptime rises at every stamp, from 159,628.85 seconds in `runs/344/host/env-before.txt` to 159,732.14 seconds in `runs/344/host/env-after.txt`. So the host did not restart during the runs.
- In each run, the wall clock less uptime stays within 10 ms across all stamps. So the wall-clock stamps did not jump, as some did in #217's runs. The intervals below come from the wall-clock stamps, truncated to milliseconds.

`free -m` before the runs shows 15,428 MB of memory, 12,049 MB free and 14,253 MB available (`runs/344/host/env-before.txt`). At the start of each run, free memory was 11,994, 12,089, and 12,065 MB (`runs/344/<run>/env.txt`).

### Machine types on the lab host

`qemu-system-x86_64 -machine help` on the lab host prints these lines for the types that the runs and #217 name (`runs/344/host/machine-help.txt`):

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

- These lines are the same as the ones the #217 page's [Machine types on the lab host](ipxe-option-97.md#machine-types-on-the-lab-host) quotes.
- `-machine help` marks `pc-i440fx-resolute` as the default. Run 3 shows that QEMU uses it when no `-machine` is given.
- `-machine pc-i440fx-resolute,help` and `-machine pc,help` print the same 42 lines, except the first, which names the type (`runs/344/host/machine-pc-i440fx-resolute-help.txt` and `machine-pc-help.txt`). `-machine pc-q35-resolute,help` and `-machine q35,help` also differ only in the first line.
- Each of the four lists `smbios-entry-point-type=<str> - SMBIOS Entry Point type [32, 64]`. None lists `auto`, but `qom-get` reads `auto` in all three runs.
- The help output lists 75 Ubuntu types besides the aliases `ubuntu` and `ubuntu-q35`, from Ubuntu 17.04's `zesty` to 26.04's `resolute`, with variants such as `pc-i440fx-noble-v2` and `pc-q35-jammy-hpb`. The runs used only `resolute`.

## Versions

`runs/344/host/host-facts.sh` recorded the versions right before the runs, in `runs/344/host/versions-before.txt` and `firmware-sha256-before.txt`. It compared them with #217's `runs/217/host/versions-r2.txt` and `firmware-sha256.txt`, which hold the values in the #217 page's [Versions](ipxe-option-97.md#versions). `runs/344/host/control-before.txt` prints `VERSIONS-SAME`, `FIRMWARE-SAME`, `TFTP-SAME`, and `CONTROL=not-needed`.

| Component | Version | In [Versions](ipxe-option-97.md#versions) of the #217 page |
| --- | --- | --- |
| `qemu-system-x86` | 1:10.2.1+ds-1ubuntu3.3 | Same |
| `qemu-system-common`, `qemu-system-data`, and `qemu-utils` | 1:10.2.1+ds-1ubuntu3.3 | Same |
| `seabios` | 1.17.0-1ubuntu1 | Same |
| `ipxe-qemu` | 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2 | Same |
| `dnsmasq-base` | 2.92-1ubuntu0.5 | Same |
| `wireshark-common` and `tshark` | 4.6.4-1 | Same |
| Python | 3.14.4 | Same |
| `ovmf` | 2025.11-3ubuntu7.3 | Same. These runs do not use it. |
| `swtpm` | 0.10.1+ds-1ubuntu3 | Same. These runs do not use it. |
| `osslsigncode` | 2.13-1 | Same. These runs do not use it. |

- None of the three packages that the Owner input on #344 names, `qemu-system-x86`, `seabios`, and `ipxe-qemu`, has changed since #217. So the page has no `-machine pc` control run, as the owner decided.
- `runs/344/host/dpkg-since-217-before.txt` finds no change to these packages in `/var/log/dpkg.log` since 2026-11-18.
- The HMP command `info version` prints `10.2.1Debian 1:10.2.1+ds-1ubuntu3.3` in each run. `qemu-system-x86_64 --version` prints `QEMU emulator version 10.2.1 (Debian 1:10.2.1+ds-1ubuntu3.3)` (`runs/344/host/qemu-version.txt`).
- Each run's `env.txt` lists the same package versions and firmware hashes. `runs/344/host/versions-after.txt` and `firmware-sha256-after.txt`, recorded after the runs, match the files from before.

| Firmware file | SHA-256 | In [Versions](ipxe-option-97.md#versions) of the #217 page |
| --- | --- | --- |
| `/usr/share/seabios/bios-256k.bin`, from `seabios` | `e26615f9ad430328f49ca105e570b2dc4490a08a34ea73d27cae8b809a30ee06` | Same |
| `/usr/share/qemu/efi-e1000.rom`, from `ipxe-qemu` | `73e68ac2bde85830847d087724f848f286d3691ae2440100a5a13e7337100c40` | Same. The HMP command `info qtree` shows it as the `e1000` card's `romfile` in each run. |
| `/usr/share/qemu/pxe-e1000.rom`, from `ipxe-qemu` | `d011d9d60664b17312bef960d6ebeae08c8f380b209dceba5f93fdf250879556` | Same. No device in `info qtree` names it. |
| `/usr/share/seabios/bios.bin`, from `seabios` | `b54fed696aee70cd4df5237d2b41e9e66282451cf0e10cfdd6f0152bb23d99b0` | Not listed. Recorded here only. |
| `/usr/share/OVMF/OVMF_CODE_4M.ms.fd` and `OVMF_VARS_4M.ms.fd` | Not recorded | Not recorded there either. These runs do not use them. |

`qemu-system-x86_64 -L help` lists `/usr/share/qemu` and `/usr/share/seabios` (`runs/344/host/qemu-datadirs.txt`). `runs/344/host/datadir-listing.txt` lists both folders. `/usr/share/qemu` holds no `bios-256k.bin`, so the only `bios-256k.bin` in QEMU's data folders is the one in `/usr/share/seabios`. That QEMU loaded this file for these machine types is inferred. The SMBIOS type 0 structure in each dump gives the BIOS version `1.17.0-debian-1.17.0-1ubuntu1`, which ends with the version of the `seabios` package that holds that file.

| File in the TFTP root | SHA-256 | In [Versions](ipxe-option-97.md#versions) of the #217 page |
| --- | --- | --- |
| `boot/x64/pxeboot.n12` | `9e34cbc08fc26902f351889b8c29c80b5f6ec99f1bb44dcf083ba2b988e37267` | Same. Each run downloaded it in full. |
| `boot/x64/bootmgr.exe` | `364c896a4fd4690949d1f7c386332f4f32541b01a05adfc13b7178258571a7ad` | Same. Run 1 stopped during its download. Runs 2 and 3 downloaded it in full. |
| `boot/bios.bcd` | `5682186b6906955f5f5e610aee7dc9eb66e3d5314d31141c5ff9b1a0750e7b0d` | Not in the table. The same as #217's `runs/217/host/tftp-sha256.txt`. No run requested it. |

The TFTP hashes are in `runs/344/host/tftp-sha256-before.txt`. No version or hash on this page differs from the #217 page.

## How the entry point was read

The method is the one in the #217 page's [How the entry point was read](ipxe-option-97.md#how-the-entry-point-was-read), with the same three readers. `runs/344/tools/smbios-tools.sha256` holds their hashes, which match #217's `runs/217/tools/smbios-tools.sha256`.

- Each run dumped the guest's BIOS segment, `0xF0000` to `0xFFFFF`, with the HMP command `pmemsave 0xf0000 0x10000 "<absolute path>"`, with the file name in double quotes. The run script sent it as soon as the prototype logged the first DISCOVER from `52:54:00:41:00:21` (`runs/344/tools/run.sh:390-399`). That DISCOVER already carried option 97.
- `pmemsave` printed nothing in each run (`runs/344/<run>/hmp-pmemsave.txt` is empty), and each dump is 65,536 bytes (`pmemsave_bytes=65536` in `times.txt`).
- The dump file's modification time came 20 to 32 ms after the captured DISCOVER, and before the lease REQUEST.
- `smbios_scan.py` lists every `_SM_` and `_SM3_` string in the dump. It counts one as an entry point only when it starts on a 16-byte boundary and its checksum is valid. For `_SM_` it also checks the intermediate anchor `_DMI_` and its checksum (`smbios-scan.txt`).
- `smbios_checksum.py`, a separate piece of code, repeated the checksum of each counted anchor (`smbios-checksum.txt`). It printed a sum of 0 modulo 256 for each.
- `smbios_walk.py` walked the structure table from each counted entry point (`smbios-walk.txt`). In each dump the walk found 11 structures that end with type 127, and covered the table's whole length. It also printed the UUID and the strings of each dump's type 1 and type 2 structures.

Every dump also holds an unaligned `_SM3_` at `0xf1941`, which fails its checksum (`smbios-scan.txt`). The scan does not count it. A byte search of each dump also finds an unaligned `_DMI_` at `0xf1934` (`runs/344/<run>/anchor-search.txt`). #217's dumps hold both strings at the same addresses.

## Run 1, `pc-i440fx-resolute`

### Command line

The command line, from `/proc/<pid>/cmdline` of the QEMU process (`runs/344/i440fx/qemu-cmdline.txt`), with line breaks added:

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/344/i440fx/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/344/i440fx/vm/serial.log
  -machine pc-i440fx-resolute
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/home/user/deployarr-lab/runs/344/i440fx/mon.sock,server,nowait
```

### Machine

- `qom-get /machine type` reads `"pc-i440fx-resolute-machine"`. `qom-get /machine smbios-entry-point-type` reads `"auto"`. `info status` reads `VM status: paused` (`runs/344/i440fx/hmp.txt`).
- `info qtree` shows the `e1000` card at PCI address `04.0` with `romfile = "efi-e1000.rom"` (`runs/344/i440fx/hmp-qtree.txt`).

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `ea400fcb1cb92d765312a904300f9b8cc3c7bf6d4fa0ae19e5cbd0603ba8af30` (`runs/344/i440fx/fseg-f0000.sha256`). Its modification time is 17:01:35.473, 32 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM_` at `0xf5210`, length `0x1f`, version 2.8. Its `_DMI_` anchor and checksum are valid. Its table is 544 bytes at `0x000f5230`, with 11 structures (`runs/344/i440fx/smbios-scan.txt`).
- No aligned `_SM3_` with a valid checksum is in the dump.
- The walk reads type 1 with product `Ubuntu 26.04 PC (i440FX + PIIX, 1996)`, version `pc-i440fx-resolute`, and UUID `4c414200-0000-4000-8000-000000000021` (`runs/344/i440fx/smbios-walk.txt`).

### DHCP

Frames 1 to 7 of [`ipxe-option-97-resolute-i440fx-dhcp.txt`](captures/ipxe-option-97-resolute-i440fx-dhcp.txt), xid `c7ee6222`. The times and option 97 come from `runs/344/i440fx/dhcp97.tsv`. "UUID" is `4c414200-0000-4000-8000-000000000021`.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 17:01:35.440 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 17:01:35.440 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 3 | 17:01:35.441 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 4 | 17:01:36.433 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 17:01:36.434 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 17:01:36.434 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 17:01:36.434 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

### Timing

`runs/344/i440fx/times.txt` gives 3.249 s from `vm-start` to `discover-seen`, 4.127 s from `discover-seen` to `pxeboot-finished`, and 12.212 s from `start` to `end`. The run ended with `qemu_exit=0`, `capture_discovers=1`, and `run_exit=0`. `leftovers.txt` lists no process.

## Run 2, `pc-q35-resolute`

### Command line

From `runs/344/q35/qemu-cmdline.txt`:

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/344/q35/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/344/q35/vm/serial.log
  -machine pc-q35-resolute
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/home/user/deployarr-lab/runs/344/q35/mon.sock,server,nowait
```

### Machine

- `qom-get /machine type` reads `"pc-q35-resolute-machine"`. `qom-get /machine smbios-entry-point-type` reads `"auto"` (`runs/344/q35/hmp.txt`).
- `info qtree` shows the `e1000` card at PCI address `03.0` with `romfile = "efi-e1000.rom"` (`runs/344/q35/hmp-qtree.txt`).

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `78cdac462abcf94d79f6008aa0c7f54c8f0524345a9c60546393e4d0ebe19d05` (`runs/344/q35/fseg-f0000.sha256`). Its modification time is 17:01:59.350, 20 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM_` at `0xf5220`, length `0x1f`, version 2.8. Its `_DMI_` anchor and checksum are valid. Its table is 526 bytes at `0x000f5240`, with 11 structures (`runs/344/q35/smbios-scan.txt`).
- No aligned `_SM3_` with a valid checksum is in the dump.
- The walk reads type 1 with product `Ubuntu 26.04 PC (Q35 + ICH9, 2009)`, version `pc-q35-resolute`, and the same UUID (`runs/344/q35/smbios-walk.txt`).

### DHCP

Frames 1 to 7 of [`ipxe-option-97-resolute-q35-dhcp.txt`](captures/ipxe-option-97-resolute-q35-dhcp.txt), xid `21139613`, from `runs/344/q35/dhcp97.tsv`. "UUID" is `4c414200-0000-4000-8000-000000000021`.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 17:01:59.329 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 17:01:59.329 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 3 | 17:01:59.329 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 4 | 17:02:00.322 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 17:02:00.322 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 17:02:00.323 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 17:02:00.323 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

### Timing

`runs/344/q35/times.txt` gives 3.242 s from `vm-start` to `discover-seen`, 4.325 s from `discover-seen` to `pxeboot-finished`, and 12.400 s from `start` to `end`. The run ended with `qemu_exit=0`, `capture_discovers=1`, and `run_exit=0`. `leftovers.txt` lists no process.

## Run 3, no `-machine`

### Command line

From `runs/344/default/qemu-cmdline.txt`. It holds no `-machine` option:

```text
qemu-system-x86_64 -accel kvm -cpu host -name deployarr-lab-bios
  -smp 2 -m 4096
  -uuid 4c414200-0000-4000-8000-000000000021
  -smbios type=2,serial=DEPLOYARR-LAB-BIOS
  -netdev tap,id=net0,ifname=tap-bios,script=no,downscript=no
  -drive if=none,id=disk0,format=qcow2,file=/home/user/deployarr-lab/runs/344/default/vm/disk.qcow2
  -device virtio-blk-pci,drive=disk0,bootindex=2
  -serial file:/home/user/deployarr-lab/runs/344/default/vm/serial.log
  -device e1000,netdev=net0,mac=52:54:00:41:00:21,bootindex=1
  -display none -vnc 127.0.0.1:3
  -monitor unix:/home/user/deployarr-lab/runs/344/default/mon.sock,server,nowait
```

### Machine

- `qom-get /machine type` reads `"pc-i440fx-resolute-machine"`. `qom-get /machine smbios-entry-point-type` reads `"auto"` (`runs/344/default/hmp.txt`).
- `info qtree` shows the `e1000` card at PCI address `04.0` with `romfile = "efi-e1000.rom"` (`runs/344/default/hmp-qtree.txt`).

### SMBIOS entry point

- The dump is 65,536 bytes, SHA-256 `ea400fcb1cb92d765312a904300f9b8cc3c7bf6d4fa0ae19e5cbd0603ba8af30` (`runs/344/default/fseg-f0000.sha256`), the same as run 1's. Its modification time is 17:02:20.984, 22 ms after the DISCOVER in frame 1.
- The scan counts one entry point, `_SM_` at `0xf5210`, length `0x1f`, version 2.8. Its `_DMI_` anchor and checksum are valid. Its table is 544 bytes at `0x000f5230`, with 11 structures (`runs/344/default/smbios-scan.txt`).
- No aligned `_SM3_` with a valid checksum is in the dump.
- The walk reads type 1 with product `Ubuntu 26.04 PC (i440FX + PIIX, 1996)`, version `pc-i440fx-resolute`, and the same UUID (`runs/344/default/smbios-walk.txt`).

### DHCP

Frames 1 to 7 of [`ipxe-option-97-resolute-default-dhcp.txt`](captures/ipxe-option-97-resolute-default-dhcp.txt), xid `855bdb35`, from `runs/344/default/dhcp97.tsv`. "UUID" is `4c414200-0000-4000-8000-000000000021`.

| Frame | Time | From | To | Packet | Option 97 |
| --- | --- | --- | --- | --- | --- |
| 1 | 17:02:20.961 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's DISCOVER, with option 93 = 0 and option 60 `PXEClient:Arch:00000:UNDI:002001` | Type 0, UUID |
| 2 | 17:02:20.962 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's OFFER of `10.41.0.192` | None |
| 3 | 17:02:20.962 | `10.41.0.1:67` | `255.255.255.255:68` | The prototype's OFFER | Type 0, UUID |
| 4 | 17:02:21.955 | `0.0.0.0:68` | `255.255.255.255:67` | The ROM's lease REQUEST, with option 54 = `10.41.0.2`. The prototype logs it as `dhcp-ignored` with `reason=lease-request`. | Type 0, UUID |
| 5 | 17:02:21.955 | `10.41.0.2:67` | `10.41.0.192:68` | dnsmasq's ACK | None |
| 6 | 17:02:21.956 | `10.41.0.192:68` | `10.41.0.1:4011` | The ROM's boot file REQUEST, with option 54 = `10.41.0.1` | Type 0, UUID |
| 7 | 17:02:21.956 | `10.41.0.1:4011` | `10.41.0.192:68` | The prototype's ACK, with `boot\x64\pxeboot.n12` and option 252 = `boot\bios.bcd` | Type 0, UUID |

### Timing

`runs/344/default/times.txt` gives 3.233 s from `vm-start` to `discover-seen`, 4.327 s from `discover-seen` to `pxeboot-finished`, and 12.390 s from `start` to `end`. The run ended with `qemu_exit=0`, `capture_discovers=1`, and `run_exit=0`. `leftovers.txt` lists no process.

## Requests in every run

- The DISCOVER comes from MAC `52:54:00:41:00:21`. It carries the options 53, 57, 93, 94, 60, 77, 55, 175, 61, and 97, in that order. Option 61 holds hardware type 1 and the MAC. These are the same options in the same order as in #217's runs.
- The lease REQUEST carries the same options, then 54 and 50. The boot file REQUEST carries the same options, then 54 (`runs/344/<run>/dhcp97.tsv`).
- Two parsers read option 97. `dhcp97.py` reads the raw BOOTP payload, and Wireshark's dissector reads `dhcp.client_id.uuid`. `runs/344/<run>/dhcp-compare.txt` prints `AGREE` for frames 1, 4, and 6 of each run, and no `MISMATCH` line.
- In all three runs, dnsmasq's OFFER is frame 2 and the prototype's is frame 3.
- dnsmasq logs `user class: iPXE` for the DISCOVER and for the lease REQUEST. It logs DHCPDISCOVER, DHCPOFFER, DHCPREQUEST, and DHCPACK of `10.41.0.192` for `52:54:00:41:00:21` in each run (`runs/344/<run>/dnsmasq.log`). Its OFFER and ACK carry no option 97.
- The prototype logs `guid=000042414c000000408000000000000021` on each request it receives (`runs/344/<run>/prototype-summary.txt`). That is option 97's type byte and UUID.
- The screen when the prototype logged the first DISCOVER shows `SeaBIOS (version 1.17.0-debian-1.17.0-1ubuntu1)`, `Machine UUID 4c414200-0000-4000-8000-000000000021`, and `iPXE 1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2` (`runs/344/<run>/shot-discover.png`).
- The serial log of each run is empty (`runs/344/<run>/serial.log`).

## TFTP

| Run | Frame 8, RRQ for `boot\x64\pxeboot.n12` | Its outcome | Frame 10, RRQ for `boot\x64\bootmgr.exe` | Its outcome |
| --- | --- | --- | --- | --- |
| 1 | 17:01:39.570 | Complete, 25,238 bytes in 18 blocks, no retransmits | 17:01:39.594 | Stopped, with 43 blocks sent and 21,504 bytes acknowledged |
| 2 | 17:02:03.458 | Complete, 25,238 bytes in 18 blocks, no retransmits | 17:02:03.484 | Complete, 814,544 bytes in 1,591 blocks in 0.164 s, no retransmits |
| 3 | 17:02:25.093 | Complete, 25,238 bytes in 18 blocks, no retransmits | 17:02:25.117 | Complete, 814,544 bytes in 1,591 blocks in 0.165 s, no retransmits |

The RRQ times come from `runs/344/<run>/tftp-rrq.tsv`. The outcomes come from the prototype's `transfer-finished` lines in `runs/344/<run>/prototype-summary.txt`.

- The RRQ for `pxeboot.n12` asks for `blksize=1432,tsize=0`. The OACK in frame 9 gives `blksize` 1432 and `tsize` 25238.
- The RRQ for `bootmgr.exe` comes from `pxeboot.n12` (inferred, as on the [single-stage page](single-stage-chain.md#tftp-requests-1)). It asks for `blksize=512,tsize=0`. The OACK in frame 11 gives `tsize` 814544.
- The run script paused the VM once it saw the prototype log the end of `pxeboot.n12`, then stopped the VM and the prototype. So the boot never went past `bootmgr.exe`. That is how each run ended, not a boot failure.
- In #217's runs, every `bootmgr.exe` transfer stopped part way. Here runs 2 and 3 finished it.

The last screen of each run shows the ROM's view (`runs/344/<run>/shot-end.png`). On run 2 the card is at PCI `00:03.0`, and on runs 1 and 3 at `00:04.0`. The rest reads:

```text
net0: 10.41.0.192/255.255.255.0 gw 10.41.0.1
net0: fe80::5054:ff:fe41:21/64
Next server: 10.41.0.1
Filename: boot\x64\pxeboot.n12
tftp://10.41.0.1/boot\x64\pxeboot.n12... ok
boot\x64\pxeboot.n12 : 25238 bytes [PXE-NBP]
```

### Why runs 2 and 3 captured more packets

The full captures of runs 2 and 3 hold 1,622 packets each, and run 1's holds 73 (`runs/344/<run>/capture-checks.txt`). The extra packets are TFTP ACKs of `bootmgr.exe`.

- `capture-checks.txt` counts 62 TFTP ACKs in run 1's full capture and 1,611 in each of runs 2 and 3. Each full capture holds the same 11 other packets, 7 DHCP, 2 RRQs, and 2 OACKs.
- Counted by the client's source port, the ACKs are 19 for `pxeboot.n12` in each run, and 43 for `bootmgr.exe` in run 1 against 1,592 in runs 2 and 3. [Commands](#commands) gives the count command.
- The prototype logs `bootmgr.exe` with `outcome=stopped` and `blocks_sent=43` in run 1, and with `outcome=complete` and `blocks_sent=1591` in runs 2 and 3.

Why `bootmgr.exe` finished in runs 2 and 3 is inferred. The script waits for the end of `pxeboot.n12` by reading the prototype's log every 0.2 s (`runs/344/tools/run.sh:404-405`). Its `pxeboot-finished` stamp came 14 ms after the prototype's `transfer-finished` line for `pxeboot.n12` in run 1, and 200 ms and 203 ms after it in runs 2 and 3. The whole `bootmgr.exe` transfer took 0.164 s and 0.165 s, so it ended before the script paused the VM. In run 1 the VM paused 13 ms after the RRQ for `bootmgr.exe`. The difference comes from where each run's end of `pxeboot.n12` fell in the 0.2 s poll (inferred). It does not touch the DHCP packets, which come before.

## Captures

| File | What it holds |
| --- | --- |
| [`ipxe-option-97-resolute-i440fx.pcapng`](captures/ipxe-option-97-resolute-i440fx.pcapng) | Run 1, reduced |
| [`ipxe-option-97-resolute-i440fx-dhcp.txt`](captures/ipxe-option-97-resolute-i440fx-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`ipxe-option-97-resolute-q35.pcapng`](captures/ipxe-option-97-resolute-q35.pcapng) | Run 2, reduced |
| [`ipxe-option-97-resolute-q35-dhcp.txt`](captures/ipxe-option-97-resolute-q35-dhcp.txt) | `tshark -V` decode of its DHCP packets |
| [`ipxe-option-97-resolute-default.pcapng`](captures/ipxe-option-97-resolute-default.pcapng) | Run 3, reduced |
| [`ipxe-option-97-resolute-default-dhcp.txt`](captures/ipxe-option-97-resolute-default-dhcp.txt) | `tshark -V` decode of its DHCP packets |

`dumpcap` captured with the filter `udp and udp[8:2] != 3`, which drops TFTP DATA packets and IP fragments. The reduced copies went through `tshark -r - -Y 'not tftp.opcode == 4' -w -`, which drops the TFTP ACKs. Each committed file is a byte copy of its run's reduced copy in `runs/344/commit/`. The full captures stay on the lab host in `captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| 1 | `ipxe-option-97-resolute-i440fx.pcapng`, 73 packets, 8,740 bytes, SHA-256 `c65b1f0302f1fa85005542e7ffefa9468a227d941cc79256f91044ecfadba1b4` | 62 | 11 packets, 3,672 bytes, SHA-256 `941cb420581d27dc3c809f82ee401277389b97f44017ab646a849c391b7fe7db` |
| 2 | `ipxe-option-97-resolute-q35.pcapng`, 1,622 packets, 132,660 bytes, SHA-256 `5da0054ff7ffdd359fa7e57246572ab2b742e5f8348067fd075a5592ab5ccf1f` | 1,611 | 11 packets, 3,672 bytes, SHA-256 `8bd107a89a779b406b0dc768860dd14440db7783c253bf06e4b4bf6d70ce0690` |
| 3 | `ipxe-option-97-resolute-default.pcapng`, 1,622 packets, 132,660 bytes, SHA-256 `2dbbebcccffd23042c62756e4e24022b65a9ce36cbdb054fbda92bc0dae81591` | 1,611 | 11 packets, 3,672 bytes, SHA-256 `a2d431fd5db05d39822bf22d58f09d6df762fe53bda0e7094460a94e7fa25e36` |

The counts and hashes come from `runs/344/<run>/capture-checks.txt`.

- The `dumpcap` logs, `captures/ipxe-option-97-resolute-<run>-dumpcap.log`, report 73/0 and 1,622/0 packets received and dropped, so no packets were dropped.
- The TFTP data check from the lab guide, `tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0`, matches no packet in the full or the committed captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them. `capture-checks.txt` records `data_or_fragments=0 raw_data=0` for each file.
- Each committed copy holds the 7 DHCP packets, the two RRQs, and their two OACKs, as #217's do.
- `dhcp97.py` read the same rows from each full capture and its reduced copy (`dhcp97.tsv` and `dhcp97-copy.tsv`).
- The decode files have the SHA-256 `86c6df1cac5b988e7168f00f89df5a66f9e0bb8e3df5ba03c0b693871fe58343` (run 1), `4e424cfef11a45538b9ebe21af12567641ac83e00534150dbc46f777939682b0` (run 2), and `cb72d3df3eebf8259e4e9dc6145ae4b9608d9893e3510ee9e0e6c50286a5ef4f` (run 3), as `runs/344/<run>/derive-ok` records.

The raw evidence stays on the lab host in `runs/344/`. Each run's folder holds the prototype log, its slice of the dnsmasq log, the QEMU command lines, the HMP output, the dump, the screenshots, the clock stamps, and the output of the dump's scan, second checksum, and walk. `runs/344/host/` holds the host facts from before and after the runs. `runs/344/tools/` holds the scripts and the parsers.

## Commands

The scripts live on the lab host in `~/deployarr-lab/runs/344/tools/`, outside the repository. `run.sh` is based on #217's `run-r2.sh`, which is kept beside it as `run-r2.sh.from-217`, with the diff in `run.sh.diff`. The plan the runs followed is `~/deployarr-lab/plans/344/plan.md`.

| File | SHA-256 | What it does |
| --- | --- | --- |
| `run.sh` | `e56f5660b4ceed01773318b5c2608ee7ce3c02f915fb7fa109c4fb725bc8753a` | One run: the idle guards, the prototype, the capture, QEMU, the dump, the HMP reads, and the stop |
| `idle.sh` | `ef4d55f883a70a372f26027ce54e1533f341bd743028688ecd45d0337da40679` | Checks that no lab VM, QEMU, `dumpcap`, prototype, or lab check runs, and that the uplink is off |
| `host-facts.sh` | `d4ce16d3e3e66824c5f8ce4cfc1ce48d9a0cf642887a50cc55107d4408363d26` | Host facts, the version and firmware comparison with #217, and the `CONTROL=` decision |
| `derive.sh` | `99d0092c3d2970d52271c00d21e9b860ec20cba76862a47b87d7b1d5e9b633c8` | The dump readers, the reduced copy, its checks, and the DHCP tables, for one run |
| `baseline.sh` | `1018a6be328c802c72e9d139dbca35bd871aed2cd5f3273a497f922ac7d519d2` | A snapshot of the host's network state, to compare with the session baseline |
| `mon.py` | `08ab1ac9343e872cfc954601a0ecce02d934379f28b5ecb7946ed91587f81bbe` | Sends one HMP command per connection. The same as #217's. |
| `dhcp97.py` | `f489eaa2ac5c62de32fd942fb57b38230a6b889239d5368fc7589a0decd918e0` | Reads options 53, 54, and 97 from the BOOTP payload. The same as #217's. |
| `smbios_scan.py` | `6de382a131bad555946afbba3f895d13b2cdfa3c93d8d56764a9b7cf1e0e96b4` | The same as #217's |
| `smbios_checksum.py` | `d8a53eebf6dacb2bf4e712d0221920a3327f4115b6ea10ac5a2ceace87e7c1ff` | The same as #217's |
| `smbios_walk.py` | `efe376ed34693cb569ef192421bad488cca88a5da1f2d6567790b9e2c48962fc` | The same as #217's |

The hashes are in `runs/344/tools/tools.sha256` and `smbios-tools.sha256`.

These commands recorded the host and made the three runs, one after another:

```bash
bash ~/deployarr-lab/runs/344/tools/host-facts.sh before
bash ~/deployarr-lab/runs/344/tools/run.sh i440fx pc-i440fx-resolute
bash ~/deployarr-lab/runs/344/tools/run.sh q35 pc-q35-resolute
bash ~/deployarr-lab/runs/344/tools/run.sh default default
```

`run.sh` refuses to start unless `idle.sh` prints `IDLE` and the lab checkout is clean. Each run's `preflight.txt` ends with `IDLE`. The script then creates the run's folders and disk, and starts the prototype, the capture, and QEMU, in the order that [Run an experiment](../../lab-setup.md#run-an-experiment) gives. These are the key lines, from `run.sh:300-302`, `323-324`, `357`, and `365`:

```bash
sudo -n python3 -B "$LAB/tools/pxe-prototype/pxe_prototype.py" --server-ip 10.41.0.1 --interface br-lab \
    --root "$LABDIR/tftp/2011" --chain single --answer-without-guid --no-block-log \
    --log-file "$log" > "$out/prototype.out" 2>&1 &
sudo -n dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -a "duration:$cap_seconds" -w - \
    > "$CAP/$name.pcapng" 2> "$CAP/$name-dumpcap.log" &
[[ $machine != default ]] && args+=(-machine "$machine")
timeout -k 15 "$qemu_timeout" qemu-system-x86_64 "${args[@]}" > "$out/qemu.out" 2>&1 &
```

`$LAB` is `~/deployarr-lab/main`, `$cap_seconds` is 360, and `$qemu_timeout` is 300. Each run's `prototype-proc.txt` and `dumpcap-proc.txt` record the `/proc/<pid>/cmdline` of the real prototype and `dumpcap` processes.

When the prototype logged the first DISCOVER from `52:54:00:41:00:21`, the script sent these HMP commands on the monitor socket (`run.sh:398-400`):

```text
pmemsave 0xf0000 0x10000 "/home/user/deployarr-lab/runs/344/<run>/fseg-f0000.bin"
screendump /home/user/deployarr-lab/runs/344/<run>/shot-discover.png -f png
```

The file name in `pmemsave` must be in double quotes, as the #217 page's [Commands](ipxe-option-97.md#commands) explains. When the prototype logged the end of `boot\x64\pxeboot.n12`, the script sent `stop`. While the VM was paused, it sent `qom-get /machine type`, `qom-get /machine smbios-entry-point-type`, `info version`, `info status`, `info qtree`, and a `screendump` (`run.sh:411-422`). Then it sent `quit` and stopped the prototype and `dumpcap` with SIGINT, by the process IDs it had started.

After the runs, `derive.sh <run>` ran for each run. It needs no root and uses no lab resource. These are its key commands, run from `~/deployarr-lab/runs/344`, with `F` the full capture and `C` the reduced copy in `commit/` (`derive.sh:59-65`, `103`, and `106-108`):

```bash
python3 -I tools/smbios_scan.py <run>/fseg-f0000.bin > <run>/smbios-scan.txt
python3 -I tools/smbios_checksum.py <run>/fseg-f0000.bin > <run>/smbios-checksum.txt
python3 -I tools/smbios_walk.py <run>/fseg-f0000.bin > <run>/smbios-walk.txt
sha256sum <run>/fseg-f0000.bin > <run>/fseg-f0000.sha256
tshark -r - -Y 'not tftp.opcode == 4' -w - < $F > $C
tshark -r - -Y dhcp -V < $C > commit/ipxe-option-97-resolute-<run>-dhcp.txt
tshark -r - -Y dhcp -T fields -E separator=/t -e frame.number -e frame.time_epoch -e ip.src -e udp.srcport -e ip.dst -e udp.dstport -e udp.payload < $F | python3 -I tools/dhcp97.py > <run>/dhcp97.tsv
```

It runs both leak checks on `F` and on `C`, and writes `derive-ok` only when both print nothing and every `tshark` call succeeds:

```bash
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < $F
tshark -r - -Y 'udp.payload[0:2] == 00:03' < $F
```

The copy into the repository ran both checks again on the staged copy before `cp`, and on the repository copy after it. It compared the two with `cmp`, and compared a fresh `tshark -r - -Y dhcp -V` of the repository copy with its decode file.

This command counts the TFTP ACKs of each transfer in a full capture, by the client's source port, for [Why runs 2 and 3 captured more packets](#why-runs-2-and-3-captured-more-packets):

```bash
tshark -r - -Y 'tftp.opcode == 4' -T fields -e udp.srcport < ~/deployarr-lab/captures/ipxe-option-97-resolute-<run>.pcapng | sort | uniq -c
```

The `_DMI_` at `0xf1934` came from a byte search of each `fseg-f0000.bin` for `_DMI_`, `_SM_`, and `_SM3_`, saved as `runs/344/<run>/anchor-search.txt`. It also finds the counted `_SM_` of each run, and the `_DMI_` 16 bytes after it.

After the runs, these commands recorded the host again:

```bash
bash ~/deployarr-lab/runs/344/tools/idle.sh
bash ~/deployarr-lab/runs/344/tools/host-facts.sh after
bash ~/deployarr-lab/runs/344/tools/baseline.sh ~/deployarr-lab/runs/344/host/baseline-after.txt
```

- `runs/344/host/idle-after.txt` ends with `IDLE`. `runs/344/host/control-after.txt` ends with `CONTROL=not-needed`.
- `network-status-after.txt` is the same as `network-status-before.txt`, with the same dnsmasq PID, and `tap-bios` back to `DOWN` with `NO-CARRIER`.
- `baseline-after.txt` differs from the session baseline, `runs/session-20261120T155451Z/baseline.txt`, only in its first line and in the `available` column of `free -g`, 14 before and 13 after.
- No `mon.sock` remains in any run folder (`runs/344/host/post-run-checks.txt`).

## What is inferred

- The ROM takes the UUID for option 97 from the SMBIOS type 1 structure that the entry point leads to (inferred, as on the #217 page). In each run, option 97 holds the same 16 bytes as that structure's UUID field.
- The dump shows the entry point that the ROM read (inferred). Each dump came 20 to 32 ms after a DISCOVER that already carried option 97, not at the moment the ROM read SMBIOS.
- QEMU loaded `/usr/share/seabios/bios-256k.bin` (inferred). It is the only `bios-256k.bin` in the two data folders, and its package version matches the type 0 BIOS version.
- The page records, and does not explain, why each resolute type puts the entry point 16 bytes lower and makes the table 24 bytes longer than the upstream type. The longer type 1 and type 2 strings are one visible difference. Whether they account for all of it is not shown.
- The reason `bootmgr.exe` finished in runs 2 and 3 and not in run 1 is inferred from the script's 0.2 s poll and the stamps, as [Why runs 2 and 3 captured more packets](#why-runs-2-and-3-captured-more-packets) says.
- The unaligned `_DMI_` and `_SM3_` at `0xf1934` and `0xf1941` are strings in SeaBIOS's code, not entry points (inferred, as on the #217 page).
- Nobody read Ubuntu's patches to QEMU, SeaBIOS, or iPXE, or the code that defines the resolute types.

## Not covered

- A `-machine pc` control run on the same day. The three packages had not changed since #217, so the owner's condition for it was not met.
- `smbios-entry-point-type=32` or `64` on a resolute type.
- Ubuntu's other machine types, such as `pc-i440fx-noble` or `pc-q35-noble`.
- The `fbbdc3926` ROM on any of these machine types.
- A guest with no SMBIOS UUID, or with no SMBIOS entry point.
- The UEFI VMs, physical PCs, and other firmware.
- The boot after `bootmgr.exe`. Each run stopped there.

## What this changes in deployarr

The three runs match #217's `pc` and `q35` runs. On `pc-i440fx-resolute`, on `pc-q35-resolute`, and with no `-machine`, the guest had only a 32-bit entry point, `_SM_` 2.8. The iPXE ROM sent option 97, type 0 with the VM's UUID, in its DISCOVER, its lease REQUEST, and its boot file REQUEST.

- The [PXE prototype's requirements](../../../tools/pxe-prototype/README.md#requirements) do not change.
- The [DHCP](../../architecture.md#dhcp) section of the architecture does not change. With the lab's versions, Ubuntu 26.04's default machine type behaves for option 97 as the upstream `pc` and `q35` do.
- The lab does not change. The `bios` VM's settings, `tools/lab/vm.sh`, and the lab guide stay as they are.
