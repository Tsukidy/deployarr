# Windows UEFI CA 2023 boot programs in the PXE lab

This page records whether boot programs signed with the Windows UEFI CA 2023 boot each chain on firmware that trusts only that certificate, for [#32](https://github.com/Tsukidy/deployarr/issues/32). It answers risk 6 of [Risks to settle with a prototype first](../../architecture.md#risks-to-settle-with-a-prototype-first). The runs are from 2026-11-17, on the lab host. [#33](https://github.com/Tsukidy/deployarr/issues/33) applies the results to the architecture.

## Result

| VM | Chain | `wdsmgfw.efi` | `bootmgfw.efi` | Result |
| --- | --- | --- | --- | --- |
| `uefi2023` | Single stage | None | 2023 | WinPE 10.0.26100.9457 starts |
| `uefi2023` | Two stage | 2023 | 2023 | WinPE 10.0.26100.9457 starts |
| `uefi2023` | Single stage | None | 2011 | OVMF downloads `bootmgfw.efi` and refuses it with `Access Denied -- rejected probably by Secure Boot` |
| `uefi2023` | Two stage | 2011 | None | OVMF downloads `wdsmgfw.efi` and refuses it the same way |
| `uefi2023` | Two stage | 2023 | 2011 | `wdsmgfw.efi` runs and downloads `bootmgfw.efi`, then shows `Error Code: 0xc0000022` |
| `uefi` | Single stage | None | 2023 | WinPE 10.0.26100.9457 starts |
| `uefi` | Two stage | 2023 | 2023 | WinPE 10.0.26100.9457 starts |

"2023" means signed through the Windows UEFI CA 2023, and "2011" means signed through the Microsoft Windows Production PCA 2011. The `uefi2023` VM's `db` holds only the Windows UEFI CA 2023. The `uefi` VM's `db` holds the 2011 and the 2023 certificates.

- Both chains boot on the `uefi2023` VM when every boot program is 2023-signed. The 2011-signed copies of the same programs do not start there.
- In the two-stage chain, `bootmgfw.efi` needs the 2023 signature too. A 2023-signed `wdsmgfw.efi` cannot start a 2011-signed `bootmgfw.efi` on the `uefi2023` VM.
- The 2023-signed programs also boot both chains on the `uefi` VM, which trusts both certificates.
- With the 2023-signed programs, each boot makes the same TFTP requests, with the same options, as the 2011-signed programs make in #27 and #28 on the `uefi` VM.

## Where the 2023-signed programs come from

The lab's `boot.wim` is index 1 of `sources\boot.wim` in `Windows11_Client_x64_en-us_26300_9457.iso`, whose SHA-256 is `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650`. Index 1 is Microsoft Windows PE (amd64), build 26100.9457. It holds two copies of `bootmgfw.efi` and `wdsmgfw.efi`, and two sets of boot fonts:

- `Windows\Boot\EFI\bootmgfw.efi` and `Windows\Boot\PXE\wdsmgfw.efi`, signed through the Microsoft Windows Production PCA 2011
- `Windows\Boot\EFI_EX\bootmgfw_EX.efi` and `Windows\Boot\PXE_EX\wdsmgfw_EX.efi`, signed through the Windows UEFI CA 2023
- `Windows\Boot\Fonts` and `Windows\Boot\Fonts_EX`, the two sets of boot fonts

`tools/lab/media.sh` copies the `_EX` programs to `~/deployarr-lab/tftp/2023` under the names `bootmgfw.efi` and `wdsmgfw.efi`, with the `Fonts_EX` fonts, as [step 6 of the lab guide](../../lab-setup.md#6-get-the-windows-files) says. The lab guide also records that the signers of the `_EX` copies were first read in the `winpe.wim` of the ADK WinPE add-on 10.1.26100.9457.

The issuer is the `Issuer :` line under `Signer's certificate:` in the output of `osslsigncode verify`, and the subject is its `Subject:` line. The version is the file version in each file's version resource.

| File | Bytes | SHA-256 | Issuer | Version |
| --- | --- | --- | --- | --- |
| `2011/boot/x64/bootmgfw.efi` | 3087400 | `4bb32c300df0815d8b6c54a0d3793ae1d78fcdc07f8e2b9f831289b2eb109092` | Microsoft Windows Production PCA 2011 | 10.0.28000.367 |
| `2011/boot/x64/wdsmgfw.efi` | 1171400 | `b3794319f7b6d68f993801d232737e6d73b95340590d23814f651562e534b9fc` | Microsoft Windows Production PCA 2011 | 10.0.26100.30227 |
| `2023/boot/x64/bootmgfw.efi` | 3087200 | `25d8869e3c79083ae62c698a9db9030748e3267bd53593357447f3f72761d1f2` | Windows UEFI CA 2023 | 10.0.28000.367 |
| `2023/boot/x64/wdsmgfw.efi` | 1171224 | `be1610e1904099da412df50bfe044b57fd1c25b6796d9472771233c8ab8e14b1` | Windows UEFI CA 2023 | 10.0.26100.30227 |
| `2011/boot/fonts/wgl4_boot.ttf` | 50096 | `23872d2eba60bbb3bd2585e925c4b48769fb6bb0cfa7f4a6ba4e8f03bcaf2c35` | Not a PE file | None |
| `2023/boot/fonts/wgl4_boot.ttf` | 50120 | `74e2d0cead72021c944bc4a2941406ffa6def152818c5faec88472452ae42d00` | Not a PE file | None |

- The subject of every signer above is `Microsoft Windows`.
- Each 2023-signed program has the same file version as its 2011-signed copy.
- The two folders share the BCD stores, `boot.sdi`, `boot.wim`, and `bootmgr.exe`, byte for byte. The `winload.efi` inside `boot.wim` is signed through the Microsoft Windows Production PCA 2011, as [Versions](loader-without-path.md#versions) of the loader page records.

## Setup

| Item | Value |
| --- | --- |
| Lab host | Ubuntu 26.04.1 LTS, kernel `7.0.0-43-generic`, AMD Ryzen 7 7700X, KVM |
| Lab scripts and PXE prototype | `tools/lab/` and `tools/pxe-prototype/` from `main` at `f148db8`. `tools/lab/vm.sh` and `tools/pxe-prototype/` are the same as in the #27 runs. |
| Network | `br-lab` at `10.41.0.1/24`. dnsmasq hands out addresses on it, and the prototype answers PXE on ports 67 and 4011. |
| Chains | `--chain single` and `--chain two-stage`, with the default store `boot\uefi.bcd` |
| TFTP roots | `~/deployarr-lab/tftp/2023` for the 2023 programs and `~/deployarr-lab/tftp/2011` for the 2011 programs. The mixed run used `~/deployarr-lab/runs/32/tftp-mixed`, which links every file to the `2011` folder except `boot/x64/wdsmgfw.efi`, which it links to the `2023` folder. |
| `uefi2023` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `~/deployarr-lab/OVMF_VARS_4M.win23.fd`, Secure Boot on, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:12`, 2 vCPUs, 4 GB. [Step 3 of the lab guide](../../lab-setup.md#3-create-the-secure-boot-keys-for-the-2023-vm) makes the variable store, whose only `db` entry is the Windows UEFI CA 2023. |
| `uefi` VM | `q35` with SMM, OVMF `OVMF_CODE_4M.ms.fd`, a fresh copy of `OVMF_VARS_4M.ms.fd` with Microsoft's 2011 and 2023 keys, Secure Boot on, TPM 2.0 through swtpm, a virtio-net card with no option ROM, MAC `52:54:00:41:00:11`, 2 vCPUs, 4 GB |

Each VM started with its folder under `~/deployarr-lab` deleted, so its disk, firmware variables, and TPM state were new. The component versions and the firmware files are the same as in [Versions](bootmgr-object-type.md#versions) of the `{bootmgr}` object type page, on the same lab host on the same day.

The lab host went down at about 20:27 UTC, during the first try of the `uefi2023` two-stage run with the 2023 programs, and booted again at 20:55. That try's files are set aside in `~/deployarr-lab/runs/32/interrupted/`, and this page does not use them. The run on this page started at 20:56:59, after the restart. The first run, the `uefi2023` single stage with the 2023 programs, ended at about 20:26, before the host went down.

## The runs that boot

| VM | Chain | VM start | `boot.wim` done | `boot.wim` transfer |
| --- | --- | --- | --- | --- |
| `uefi2023` | Single stage | 20:24:41.674 | 20:25:40.425 | 50.738 s |
| `uefi2023` | Two stage | 20:56:59.192 | 20:58:01.269 | 53.432 s |
| `uefi` | Single stage | 21:06:13.214 | 21:07:13.780 | 51.188 s |
| `uefi` | Two stage | 21:07:39.498 | 21:08:41.135 | 52.701 s |

- Each single-stage run made the same 32 RRQs, file for file and option for option, as the `uefi` run of #27 in [Single-stage boot chain in the PXE lab](single-stage-chain.md#tftp-requests). Each two-stage run made the same 34 RRQs as the `uefi` run of #28 in [Two-stage boot chain in the PXE lab](two-stage-chain.md#tftp-requests).
- In each run, the request for `\EFI\Microsoft\Boot\Policies\UnlockToken.pol` came after `boot.wim`. The last screenshot of each run, about 20 seconds later, shows WinPE's `cmd.exe` window at a prompt after `wpeinit`.
- On the two-stage runs, the serial log shows `wdsmgfw.efi`'s `Windows Deployment Services (Server IP: 10.41.0.1)` and `Contacting Server (10.41.0.1)` lines, as in #28.

The VM start times come from `date`, the lab host's uutils coreutils 0.10.0, whose `%3N` misprints milliseconds below 100. Each of these four stamps agrees with `/proc/uptime` and with the stamps before and after it, so the misprint does not touch them.

## The runs that do not boot

### 2011 programs on the `uefi2023` VM

In the single-stage run, OVMF took its address, got the ACK for `boot\x64\bootmgfw.efi`, and downloaded it at 20:58:35.021. Its serial log then shows:

```text
  NBP filename is boot\x64\bootmgfw.efi
  NBP filesize is 3087400 Bytes
 Downloading NBP file...
  NBP file downloaded successfully.
BdsDxe: loading Boot0003 "UEFI PXEv4 (MAC:525400410012)" ...
BdsDxe: failed to load Boot0003 "UEFI PXEv4 (MAC:525400410012)" ...: Access Denied -- rejected probably by Secure Boot
```

The `...` marks device paths left out. OVMF then tried PXE over IPv6, then HTTP boot over IPv4 and IPv6. The prototype logged no TFTP request after the download. It ignored the DHCP DISCOVERs of OVMF's HTTP boot client, which carry architecture `0x0010`.

The two-stage run went the same way with `boot\x64\wdsmgfw.efi`, downloaded at 21:01:08.748, 1171400 bytes. OVMF refused it with the same `Access Denied` line, and nothing ran after the firmware.

### 2023 `wdsmgfw.efi` with a 2011 `bootmgfw.efi`

OVMF downloaded the 2023-signed `wdsmgfw.efi`, 1171224 bytes, at 21:03:44.245 and started it. `wdsmgfw.efi` sent its second request, got the second ACK, and downloaded `\boot\x64\bootmgfw.efi`, the 2011-signed copy, at 21:03:44.693. No request followed. The screenshot from 21:03:45 on shows:

```text
Windows Deployment Services (Server IP: 10.41.0.1)

Windows Deployment Services encountered an error:

Error Code: 0xc0000022

ENTER=Reboot                                       ESC=Exit
```

The screen stayed until the run stopped the VM about 150 seconds after it started. `0xc0000022` is `STATUS_ACCESS_DENIED` (inferred from the NTSTATUS value). This screen waits for a key.

## Captures

| File | What it holds |
| --- | --- |
| [`uefi-ca-2023-uefi2023-single-2023.pcapng`](captures/uefi-ca-2023-uefi2023-single-2023.pcapng) | `uefi2023`, single stage, 2023 programs |
| [`uefi-ca-2023-uefi2023-two-stage-2023.pcapng`](captures/uefi-ca-2023-uefi2023-two-stage-2023.pcapng) | `uefi2023`, two stage, 2023 programs |
| [`uefi-ca-2023-uefi2023-single-2011.pcapng`](captures/uefi-ca-2023-uefi2023-single-2011.pcapng) | `uefi2023`, single stage, 2011 programs |
| [`uefi-ca-2023-uefi2023-two-stage-2011.pcapng`](captures/uefi-ca-2023-uefi2023-two-stage-2011.pcapng) | `uefi2023`, two stage, 2011 programs |
| [`uefi-ca-2023-uefi2023-two-stage-mixed.pcapng`](captures/uefi-ca-2023-uefi2023-two-stage-mixed.pcapng) | `uefi2023`, two stage, 2023 `wdsmgfw.efi` and 2011 `bootmgfw.efi` |
| [`uefi-ca-2023-uefi-single-2023.pcapng`](captures/uefi-ca-2023-uefi-single-2023.pcapng) | `uefi`, single stage, 2023 programs |
| [`uefi-ca-2023-uefi-two-stage-2023.pcapng`](captures/uefi-ca-2023-uefi-two-stage-2023.pcapng) | `uefi`, two stage, 2023 programs |

Each capture has a `tshark -V` decode of its DHCP packets next to it, with `-dhcp.txt` in place of `.pcapng`. `dumpcap` captured with the filter `udp and udp[8:2] != 3`, which drops TFTP DATA packets and IP fragments. The committed files went through `tshark -r - -Y 'not tftp.opcode == 4' -w -`, which drops the TFTP ACKs. The full captures stay on the lab host in `~/deployarr-lab/captures/`.

| Run | Full capture | Dropped TFTP ACKs | Committed copy |
| --- | --- | --- | --- |
| `uefi2023-single-2023` | 398,285 packets, SHA-256 `e68b990765ee2bcc48708987eb95dde03022c7ff0ad6be7a9a9b824792d0dc5f` | 398,208 | 77 packets, SHA-256 `9b8c937d08fc5eb73390bb8ed795fd39716fddc19cda8188b6d950a5d518d68d` |
| `uefi2023-two-stage-2023` | 400,088 packets, SHA-256 `b90ba2299db200669bbcd702b221d8b6f7b88c65efbf783e191a4554c48fce7b` | 400,004 | 84 packets, SHA-256 `7d00540bcde4625feb570c8fdf14c36960732f929882e459504424bfa94a24e6` |
| `uefi2023-single-2011` | 547 packets, SHA-256 `06e66f1e7c733bda3ce0e124df734387a8f699844819a0b3b79b41d516342170` | 527 | 20 packets, SHA-256 `dd45c124685c826d0dade668db06bd2db347a93ae119f749e325971d5b12a27e` |
| `uefi2023-two-stage-2011` | 221 packets, SHA-256 `1e3608f5e80b8d837434181940668295810615e712fb280e88be3fdf36e331d1` | 201 | 20 packets, SHA-256 `840d4c512600f69d974af65720cd4b6b94a1cc721add26fc4f59385ed7e65acd` |
| `uefi2023-two-stage-mixed` | 2,342 packets, SHA-256 `73a47237ab97df851008446896f53c876e724adcb741603f1ed771603349533a` | 2,323 | 19 packets, SHA-256 `3d5cc2b819c07aa835110cf8a7581e588e5506a5c6619b1c4799e65a68d4ed8a` |
| `uefi-single-2023` | 398,285 packets, SHA-256 `7894a3dbda037381e54c33959ebfc71f00cd63961eaff3695ad531fb2cff4cb6` | 398,208 | 77 packets, SHA-256 `f23a50d00d16c3ccd3143092ed1cc93e0609cc42672d3686f02a84e5b81399f1` |
| `uefi-two-stage-2023` | 400,088 packets, SHA-256 `651747d4fa66b627640827620ca6261968977be626bfd1789794060bf83606a2` | 400,004 | 84 packets, SHA-256 `d01e2fd7bd4b63039c7e21811a99d76a61b56d7092d1a3abbf69e784291916ff` |

- The `dumpcap` logs, next to the full captures, report no dropped packets.
- The TFTP data check from the lab guide prints nothing for the full and the committed captures. A filter on the raw payload, `udp.payload[0:2] == 00:03`, also matches no packet in any of them.

The raw evidence stays on the lab host in `~/deployarr-lab/runs/32/`: the prototype logs, the serial logs, a screenshot every 2 seconds of each run, and the run scripts.

## Commands

Each run started the prototype, then the capture, then the VM, as [Run an experiment](../../lab-setup.md#run-an-experiment) says. `$LAB` is the checkout of `main` at `f148db8`. `<vm>` is `uefi2023` or `uefi`, `<root>` is the TFTP root, `<chain>` is `single` or `two-stage`, and `<run>` names the run as in the captures.

```bash
rm -rf ~/deployarr-lab/<vm>
sudo python3 -B $LAB/tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root <root> --chain <chain> --no-block-log --log-file ~/deployarr-lab/runs/32/uefi-ca-2023-<run>-prototype.log &
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/uefi-ca-2023-<run>.pcapng 2> ~/deployarr-lab/captures/uefi-ca-2023-<run>-dumpcap.log &
timeout 900 $LAB/tools/lab/vm.sh <vm> -display none -monitor unix:/tmp/lab-<vm>-mon.sock,server,nowait
```

The run script took a screenshot every 2 seconds with the HMP command `screendump`. It stopped 20 seconds after the request for `UnlockToken.pol`, or 150 seconds after the VM started when no such request came. Then the HMP command `quit` stopped the VM, `sudo pkill -INT -f '^python3 -B .*pxe_prototype.py'` stopped the prototype, and `sudo pkill -INT -x dumpcap` stopped the capture.

These commands read the signer and make the committed files:

```bash
osslsigncode verify -in ~/deployarr-lab/tftp/2023/boot/x64/bootmgfw.efi
tshark -r - -Y 'not tftp.opcode == 4' -w - < ~/deployarr-lab/captures/uefi-ca-2023-<run>.pcapng > docs/research/pxe-lab/captures/uefi-ca-2023-<run>.pcapng
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < docs/research/pxe-lab/captures/uefi-ca-2023-<run>.pcapng
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/uefi-ca-2023-<run>.pcapng > docs/research/pxe-lab/captures/uefi-ca-2023-<run>-dhcp.txt
```

## What is inferred

- In the mixed run, the firmware refused the 2011-signed `bootmgfw.efi` that `wdsmgfw.efi` asked it to start (inferred). `0xc0000022` is the NTSTATUS for access denied, the download completed, and the same file is refused by the firmware in the single-stage run.
- The 2023-signed `bootmgfw.efi` starts the 2011-signed `winload.efi` in `boot.wim` (inferred). WinPE starts on the `uefi2023` VM, whose `db` lacks the 2011 certificate, and `boot.wim` holds only that `winload.efi`. So the boot manager checks the loader itself, not through the firmware's `db`.
- No run on this page queried `UEFISecureBootEnabled` in WinPE. The `Access Denied -- rejected probably by Secure Boot` lines show that Secure Boot was on in the `uefi2023` VM.

## Not covered

- A `dbx` that revokes the 2011 boot programs. The `uefi2023` VM trusts no 2011 certificate, which blocks the same files in a different way.
- The BIOS chain. SeaBIOS has no Secure Boot, and `bootmgr.exe` is the same file in both folders.
- Other builds of WinPE or the ADK, and the `_EX` programs of a later build.
- Physical PCs whose firmware ships with only the 2023 certificates.

## What changes in deployarr

- On firmware that trusts only the Windows UEFI CA 2023, deployarr must serve the 2023-signed `bootmgfw.efi` in the single stage, and the 2023-signed `wdsmgfw.efi` and `bootmgfw.efi` together in the two-stage chain. One 2011-signed program in the chain stops the boot.
- The 2023-signed programs boot both chains on firmware that trusts both certificates. So serving them to every UEFI PC boots both lab VMs. [Boot chain](../../architecture.md#boot-chain) can choose them as the default for UEFI.
- The importer can take the 2023-signed programs from `Windows\Boot\EFI_EX` and `Windows\Boot\PXE_EX` in the boot image, with the `Fonts_EX` fonts, and report which certificate signed each program, as [Boot chain](../../architecture.md#boot-chain) plans.
- A 2011-signed program on 2023-only firmware fails in two ways. The firmware refuses it and moves on to its next boot option, or `wdsmgfw.efi` shows `Error Code: 0xc0000022` and waits for a key. Neither tells the technician which certificate is missing.
- #33 can close risk 6 with these results.
