# Set up the PXE lab

This page builds the lab that the PXE experiments in issues #27 to #32 run in. It is a how-to for the lab host. The [Test lab](roadmap.md#test-lab) section of the roadmap says what the lab must provide. Every step is safe to run again.

The lab has three VMs on an isolated bridge:

| VM | Firmware | Secure Boot keys | MAC address | SMBIOS UUID | Baseboard serial |
| --- | --- | --- | --- | --- | --- |
| `uefi` | OVMF, Secure Boot on, TPM 2.0 | Microsoft's 2011 and 2023 keys | `52:54:00:41:00:11` | `4c414200-0000-4000-8000-000000000011` | `DEPLOYARR-LAB-UEFI` |
| `uefi2023` | OVMF, Secure Boot on, TPM 2.0 | Only the Windows UEFI CA 2023 in `db` | `52:54:00:41:00:12` | `4c414200-0000-4000-8000-000000000012` | `DEPLOYARR-LAB-UEFI2023` |
| `bios` | SeaBIOS with QEMU's iPXE ROM | None | `52:54:00:41:00:21` | `4c414200-0000-4000-8000-000000000021` | `DEPLOYARR-LAB-BIOS` |

The scripts in [`tools/lab/`](../tools/lab/) hold these settings in `lab.conf`.

## What you need

- An x86-64 PC or server with hardware virtualization (Intel VT-x or AMD-V) turned on in its firmware settings.
- 16 GB of memory and 100 GB of free disk. The check runs the three VMs at once, with 4 GB each.
- Ubuntu 24.04 LTS, desktop or server. The scripts use its package versions and file paths.
- An account that can run `sudo`.
- About 6 GB of downloads, most of it the Windows ISO.

The lab bridge has no uplink. The VMs reach only the host, and nothing on your network reaches the VMs.

## 1. Install the packages

```bash
sudo apt-get update
```

```bash
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y qemu-system-x86 qemu-utils ovmf swtpm swtpm-tools dnsmasq-base ipxe-qemu seabios tshark wimtools p7zip-full osslsigncode python3-venv git
```

The list has `dnsmasq-base` and not `dnsmasq`. The `dnsmasq` package adds a system service that holds UDP port 67 on every interface.

Let your account use KVM, then sign out and sign in again:

```bash
sudo usermod -aG kvm "$USER"
```

Check that KVM works. The command prints `KVM ready`:

```bash
test -w /dev/kvm && echo "KVM ready"
```

## 2. Get the repository

```bash
git clone https://github.com/Tsukidy/deployarr.git ~/deployarr
```

```bash
cd ~/deployarr
```

Run every later command from this folder.

## 3. Create the Secure Boot keys for the 2023 VM

The `uefi` VM uses Ubuntu's `/usr/share/OVMF/OVMF_VARS_4M.ms.fd`. Its `db` holds the Microsoft Windows Production PCA 2011, the Microsoft Corporation UEFI CA 2011, the Microsoft UEFI CA 2023, the Microsoft Option ROM UEFI CA 2023, and the Windows UEFI CA 2023.

The `uefi2023` VM needs a store whose `db` holds only the Windows UEFI CA 2023. Ubuntu's `virt-firmware` 24.1.1 cannot make one, so install version 26.10 in its own environment:

```bash
mkdir -p ~/deployarr-lab
```

```bash
python3 -m venv ~/deployarr-lab/venv
```

```bash
~/deployarr-lab/venv/bin/pip install virt-firmware==26.10
```

```bash
~/deployarr-lab/venv/bin/virt-fw-vars --input /usr/share/OVMF/OVMF_VARS_4M.fd --output ~/deployarr-lab/OVMF_VARS_4M.win23.fd --enroll-microsoft --microsoft-db win23 --sb
```

Check the store:

```bash
~/deployarr-lab/venv/bin/virt-fw-vars --input ~/deployarr-lab/OVMF_VARS_4M.win23.fd --print --verbose | grep 'subject CN='
```

It prints four certificates: `Microsoft Corporation KEK CA 2011`, `Microsoft Corporation KEK 2K CA 2023`, `Windows OEM Devices PK`, and `Windows UEFI CA 2023`. The last one is the only `db` entry.

## 4. Start the lab network

```bash
sudo tools/lab/network.sh up
```

The script creates the bridge `br-lab` with the address `10.41.0.1/24`, and one tap device for each VM. It starts a dnsmasq that hands out `10.41.0.100` to `10.41.0.199` and does nothing else. It has no DNS and no PXE. dnsmasq holds port 67 only on `br-lab`, so the PXE prototype can share the port.

The last lines show `dnsmasq is running` and `0.0.0.0%br-lab:67`. The network does not survive a reboot. Run the command again after each reboot. To remove the network, run `sudo tools/lab/network.sh down`.

## 5. Check the lab

```bash
sudo tools/lab/check.sh
```

The check takes about a minute. It starts a packet capture and the [PXE prototype](../tools/pxe-prototype/README.md), and boots the three VMs to their network boot with no screen. The prototype serves a 1 KB unsigned test program in place of the Windows boot program. The check then prints one line for each test:

- Each VM's PXE DISCOVER carries option 93 and option 97 with the VM's UUID.
- Each VM gets an address from dnsmasq.
- The PXE prototype answers each VM on port 4011.
- Each UEFI VM refuses the unsigned program with `Access Denied`, so Secure Boot is on.

The last line is `RESULT PASS`. The check writes a folder and a zip named `lab-check-<time>` in `~/deployarr-lab/checks`. The zip holds the capture, a `tshark -V` decode of the DHCP packets, the logs, and `versions.txt` with the version of each component. The capture filter drops TFTP data, so the zip holds no file contents.

## 6. Get the Windows files

The experiments serve Microsoft's boot programs and WinPE, and the repository never holds them.

1. Download the Windows 11 ISO for x64 devices from Microsoft's [download page](https://www.microsoft.com/software-download/windows11), in English (United States). Microsoft calls it a multi-edition ISO. The PXE experiments use only its WinPE and boot files, which come from `sources\boot.wim`.
2. Download the BCD stores. In GitHub, open Actions, then the BCD workflow, and choose Run workflow on `main`. When the run finishes, download its `bcd-stores` artifact. The run from the pull request that added the stores also has one, until 2027-02-10: [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137).
3. Build the TFTP folders. Use the paths of your two downloads:

	```bash
	tools/lab/media.sh ~/Downloads/<Windows ISO>.iso ~/Downloads/bcd-stores.zip
	```

The script exports index 1 of `sources\boot.wim`, which is WinPE, as a single-image `boot.wim`. It copies the boot programs from that image, `boot\boot.sdi` from the ISO, and the BCD stores from the zip. It writes two folders and a manifest:

| Folder | Boot managers | For |
| --- | --- | --- |
| `~/deployarr-lab/tftp/2011` | `bootmgfw.efi` and `wdsmgfw.efi`, signed by the Microsoft Windows Production PCA 2011 | #27 to #31 |
| `~/deployarr-lab/tftp/2023` | `bootmgfw_EX.efi` and `wdsmgfw_EX.efi`, renamed, signed by the Windows UEFI CA 2023 | #32 |

The last lines show the signer of each boot manager. The `_EX` copies come from `\Windows\Boot\EFI_EX` and `\Windows\Boot\PXE_EX` in the image. Their signers were read with `osslsigncode` in the `winpe.wim` of the ADK WinPE add-on 10.1.26100.9457. The `boot.wim` of the Windows 11 ISO was not checked, and the script says so if it has no `_EX` copies.

Both folders have the same layout:

| Path | Holds |
| --- | --- |
| `boot/x64/bootmgfw.efi`, `wdsmgfw.efi`, `wdsnbp.com`, `pxeboot.n12`, `pxeboot.com`, `bootmgr.exe`, `abortpxe.com` | The boot programs |
| `boot/x64/boot.wim`, `boot/boot.sdi`, `boot/fonts/` | The paths that the BCD stores name |
| `boot/uefi.bcd`, `boot/bios.bcd` | The default stores, which the prototype serves unless told otherwise |
| `boot/uefi-nopath.bcd`, `boot/bios-nopath.bcd` | The stores with no loader `path`, for #29 |
| `boot/uefi-maxtftp.bcd`, `boot/bios-maxtftp.bcd` | The stores with a TFTP block size of 32768 and a window size of 32, for #30 |

## 7. Report the result

Comment on issue #25 with:

- the `~/deployarr-lab/checks/lab-check-<time>.zip` from step 5
- the contents of `~/deployarr-lab/tftp/manifest.txt` from step 6
- the host's CPU, and anything you did that this page does not say

Never attach the Windows files or a capture that holds file data.

## Run an experiment

Start a VM:

```bash
tools/lab/vm.sh uefi
```

Use `uefi2023` or `bios` for the other VMs. On a desktop, the VM opens a window. On a server, the script prints a VNC address on `127.0.0.1`. Reach it through SSH, for example with `ssh -L 5901:127.0.0.1:5901 <host>`, and a VNC viewer. The firmware's console text goes to `~/deployarr-lab/<vm>/serial.log`. Close the window or press Ctrl-C to stop the VM.

The VMs use the host's CPU model, because QEMU's default model lacks processor features that Windows 11 requires (#75). The SeaBIOS VM runs with `smbios-entry-point-type=32`, so its iPXE ROM sends option 97 (#56). The UEFI VMs use a virtio network card with no option ROM, so OVMF's own PXE client runs with Secure Boot on.

Capture on the bridge, as the experiment issues ask:

```bash
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w <name>.pcapng
```

Run the PXE prototype with one of the TFTP folders. Its [README](../tools/pxe-prototype/README.md) gives the command for each experiment. In the lab, the address is `10.41.0.1`, the interface is `br-lab`, and the root is `~/deployarr-lab/tftp/2011` or `~/deployarr-lab/tftp/2023`. For example:

```bash
sudo python3 tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain two-stage --log-file two-stage.log
```

To reset a VM, stop it and delete its folder, such as `~/deployarr-lab/uefi`. That removes its disk, its firmware variables, and its TPM state. The next start creates them again.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `No usable /dev/kvm` | Turn on VT-x or AMD-V in the host's firmware settings. Run `sudo usermod -aG kvm "$USER"`, then sign out and sign in again. |
| `network.sh up` stops at dnsmasq, or `ss` shows another program on port 67 | Another DHCP server holds port 67 on every interface. Common ones are the `dnsmasq` service and libvirt's `default` network. Stop it, or bind it to its own interface as the [prototype README](../tools/pxe-prototype/README.md#share-port-67-with-dnsmasq) says. |
| The check fails on `dnsmasq gave an address` for every VM | A firewall drops DHCP on the bridge. If `sudo ufw status` says `active`, run `sudo ufw allow in on br-lab`. |
| The check fails on option 97 for `bios` | The VM did not get the SMBIOS setting. Check that `tools/lab/vm.sh` is unchanged. |
| The check fails on `Access Denied` | Secure Boot is off in that VM. Delete `~/deployarr-lab/uefi/vars.fd` or `~/deployarr-lab/uefi2023/vars.fd`, and redo step 3 for `uefi2023`. |
| `media.sh` says the ISO has no `sources/boot.wim` | The file is not a Windows installation ISO. Download it again. |
