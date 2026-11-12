# Run the PXE prototype in the lab

`pxe_prototype.py` is a throwaway proxyDHCP and TFTP server for the PXE lab experiments, issues #27 to #32. It answers PXE clients the way [the PXE design](../../docs/architecture.md#pxe) describes, and it logs every packet. The lab uses it to choose deployarr's boot chain. The production server is the future `deployarr-pxe` crate, and it shares no code with this tool.

## Requirements

- Linux, with Python 3.10 or later. The tool uses only the standard library.
- Root, because it binds UDP ports 67, 69, and 4011.
- A DHCP server on the lab network that hands out addresses. The prototype never does.

## Fill the TFTP root

The prototype serves files from one folder. It ignores case and accepts `/` or `\` in a requested path, so the names on disk can have any case. It follows symbolic links.

| Path under the root | Source |
| --- | --- |
| `boot\x64\bootmgfw.efi` | `Windows\Boot\EFI\bootmgfw.efi` in the boot image |
| `boot\x64\wdsmgfw.efi`, `boot\x64\wdsnbp.com`, `boot\x64\pxeboot.n12` | `Windows\Boot\PXE` in the boot image |
| `boot\uefi.bcd`, `boot\bios.bcd` | The BCD stores from #26 |
| `bootmgr.exe`, `boot.sdi`, `boot.wim`, and the fonts | Wherever the boot programs and the BCD ask for them. Read the paths from the `tftp-received` log lines. |

The first three rows are fixed in `BOOT_TABLE` in `policy.py`. `--bcd-uefi` and `--bcd-bios` change the BCD paths.

Never commit these files. They are Microsoft files, and the repository does not hold them.

## Share port 67 with dnsmasq

Before you start the prototype, check how the lab's DHCP server holds UDP port 67:

```bash
sudo ss -ulpn 'sport = :67'
```

- libvirt's dnsmasq shows `0.0.0.0%<bridge>:67`. It needs no change.
- A standalone dnsmasq that shows `0.0.0.0:67` is in its default mode. No second program can share the port in that mode, and dnsmasq fails to start if the prototype already holds it. Add `bind-interfaces` or `bind-dynamic`, and exactly one `interface=<lab bridge>` line, to its config. Restart it. Its log then says `DHCP, sockets bound exclusively to interface <bridge>`.

This was measured with dnsmasq 2.91 on Linux 6.18. In those modes, both programs receive every broadcast DISCOVER whichever starts first, and dnsmasq keeps its unicast lease renewals.

If port 67 still cannot be shared, pass `--dhcp-ports 4011`. The prototype then serves the second stage, but it never sends an OFFER, so #31 cannot run.

`--interface` names the lab bridge. The prototype ignores DHCP requests that arrive on any other interface, and it sends every DHCP reply through the bridge. It logs an ignored request as `dhcp-ignored` with `reason=other-interface`.

On port 67 the prototype also answers the broadcast REQUEST with which a PC accepts the DHCP server's lease. It does not read option 54, as ConfigMgr does not.

## Run an experiment

Every command needs the address of the lab bridge, the bridge name, the TFTP root, and the chain. Stop the prototype with Ctrl-C.

Single stage, for #27. The first ACK names the boot manager and carries option 252:

```bash
sudo python3 pxe_prototype.py --server-ip 192.168.100.1 --interface virbr1 --root /srv/pxe --chain single --log-file single.log
```

Two stages, for #28. The first ACK names the WDS network boot program and carries option 250. The `dhcp-received` line of the second request shows its port and its option 250 sub-options:

```bash
sudo python3 pxe_prototype.py --server-ip 192.168.100.1 --interface virbr1 --root /srv/pxe --chain two-stage --log-file two-stage.log
```

Transfer timing, for #30. Each `transfer-finished` line gives the file, its size, the block size, the window size, and the seconds from the request to the last ACK:

```bash
sudo python3 pxe_prototype.py --server-ip 192.168.100.1 --interface virbr1 --root /srv/pxe --chain single --no-block-log --log-file timing.log
```

```bash
grep transfer-finished timing.log
```

OFFER without option 43, for #31. Every OFFER the prototype sends has no option 43 and no boot file. Use either chain, keep port 67 in `--dhcp-ports`, and check that the `started` line lists `67` under `dhcp_ports`. The firmware accepted the OFFER if a REQUEST from the same MAC address follows on port 4011.

Another set of boot files, for #32. Put them in a second folder with the same layout and name it with `--root`:

```bash
sudo python3 pxe_prototype.py --server-ip 192.168.100.1 --interface virbr1 --root /srv/pxe-2023 --chain two-stage --log-file ca2023.log
```

## Read the log

Each line is one event: a UTC time in milliseconds, the event name, then `key=value` fields.

| Event | Meaning |
| --- | --- |
| `started` | What the run tests: the chain, the interface, the ports bound, the BCD paths, and the TFTP limits |
| `dhcp-received` | A DHCP packet, with its option 250 decoded when it has one |
| `dhcp-sent` | The OFFER or ACK, with the firmware type, the chain step, the boot file, and option 250 or option 252 |
| `dhcp-ignored` | A packet that got no reply, with the reason |
| `tftp-received`, `tftp-sent` | Every TFTP packet. `--no-block-log` leaves out the DATA and ACK lines |
| `tftp-ignored` | A repeated request for a transfer that is running, or a packet from an unknown port |
| `transfer-finished` | One line per transfer, with the outcome `complete`, `timeout`, `client-error`, or `stopped` |

The names of the option 250 sub-options in the log are inferred from WDS header names.

The DATA and ACK lines cost time. A 256 MiB file sent over loopback to a Python client with `blksize` 1456 and no window took a median of 9.0 seconds with the lines and 8.0 seconds without them, over 5 runs each. That is about 6 microseconds for each block. Use `--no-block-log` when you time transfers for #30.

## Differences from ConfigMgr

The reply layout follows `sccmpxe.exe` from ConfigMgr 2509. The prototype differs in these ways:

- It pads every reply to 300 bytes, the BOOTP minimum. `sccmpxe` does not pad, so its OFFER is 280 bytes.
- It zeros `sname` in every reply and `file` in an OFFER. `sccmpxe` echoes the request's bytes.
- It answers a second request whose option 250 has no sub-option 1. `sccmpxe` drops that request.
- It never sends option 243, which holds ConfigMgr's variables file and key.
- It always sends the zero-touch replies. It has no order lookup, no `abortpxe.com`, and no key prompt.

## Run the tests

```bash
cd tools/pxe-prototype && python3 -m unittest
```

The tests use loopback and unprivileged ports. `reference.py` builds the requests and decodes the replies. It imports nothing from the tool.
