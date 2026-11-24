# Set up the PXE lab

This page builds the lab that the PXE experiments in issues #27 to #32 run in. It is a how-to for the lab host. The [Test lab](roadmap.md#test-lab) section of the roadmap says what the lab must provide. Every step is safe to run again.

The lab has three VMs on an isolated bridge:

| VM | Firmware | Secure Boot keys | MAC address | SMBIOS UUID | Baseboard serial |
| --- | --- | --- | --- | --- | --- |
| `uefi` | OVMF, Secure Boot on, TPM 2.0 | Microsoft's 2011 and 2023 keys | `52:54:00:41:00:11` | `4c414200-0000-4000-8000-000000000011` | `DEPLOYARR-LAB-UEFI` |
| `uefi2023` | OVMF, Secure Boot on, TPM 2.0 | Only the Windows UEFI CA 2023 in `db` | `52:54:00:41:00:12` | `4c414200-0000-4000-8000-000000000012` | `DEPLOYARR-LAB-UEFI2023` |
| `bios` | SeaBIOS with QEMU's iPXE ROM | None | `52:54:00:41:00:21` | `4c414200-0000-4000-8000-000000000021` | `DEPLOYARR-LAB-BIOS` |

The scripts in [`tools/lab/`](../tools/lab/) hold these settings in `lab.conf`.

The lab host has a full GitHub login, so the cloud session limits in [GitHub from a cloud session](../CLAUDE.md#github-from-a-cloud-session), such as `gh` for search only, do not apply to its session. [Tools](agents/issue-tracker.md#tools) gives the `gh` form of each issue operation, and the rules in [`AGENTS.md`](../AGENTS.md) still apply, such as draft pull requests, claims, no commits to `main`, and no deleted branches.

## What you need

- An x86-64 PC or server with hardware virtualization (Intel VT-x or AMD-V) turned on in its firmware settings.
- A CPU with every processor feature that the [Test lab](roadmap.md#test-lab) section lists, such as SSE4.2 and PrefetchW. The VMs use the host's CPU model, and Windows 11 needs these features. The check in step 5 tests them.
- 16 GB of memory and 100 GB of free disk. The check runs the three VMs at once, with 4 GB each.
- Ubuntu 26.04 LTS, desktop or server. The steps ran on Ubuntu 26.04.1 LTS, and the scripts use its package versions and file paths. Nobody has run this version of the page on Ubuntu 24.04.
- An account that can run `sudo`.
- About 10 GB of downloads. The Windows ISO is 9 GB of it.

By default the lab bridge has no uplink. The VMs reach only the host, and nothing on your network reaches the VMs. This stays true for IPv4 when the host forwards IPv4 for another reason. `up` adds rules that stop the host from forwarding IPv4 between the bridge and another interface. [Step 4](#4-start-the-lab-network) describes them. Only the [M7 check](roadmap.md#m7-applications) of the `InstallUpdates` step turns on the uplink. [Turn on the uplink](#turn-on-the-uplink) says how.

## 1. Install the packages

```bash
sudo apt-get update
```

```bash
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y qemu-system-x86 qemu-utils ovmf swtpm swtpm-tools dnsmasq-base nftables ipxe-qemu seabios tshark wimtools p7zip-full osslsigncode python3-venv git
```

The list has `dnsmasq-base` and not `dnsmasq`. The `dnsmasq` package adds a system service that holds UDP port 67 on every interface.

`tools/lab/network.sh` sets its firewall rules with `nft`, from the `nftables` package. Without `nft`, `up` stops with `The lab network needs nft. Install the nftables package.` before it changes anything.

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

The `uefi2023` VM needs a store whose `db` holds only the Windows UEFI CA 2023. Ubuntu's `python3-virt-firmware` 25.12 cannot make one, because it has no `--enroll-microsoft` option. So install version 26.10 in its own environment.

On Python 3.13 and later, `virt-firmware` needs `crypt_r`. PyPI has no wheel for `crypt_r`, so `pip` compiles it. Install a C compiler, the Python headers, and the `crypt` headers first:

```bash
sudo apt-get install -y gcc python3-dev libcrypt-dev
```

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

The script creates the bridge `br-lab` with the address `10.41.0.1/24`, and one tap device for each VM. It starts a dnsmasq that hands out `10.41.0.100` to `10.41.0.199` and does nothing else. It has no DNS and no PXE. It gives each VM `10.41.0.1` as its gateway. It hands out no DNS server unless the uplink is on.

When the uplink is off, the script adds the nftables table `ip deployarr_lab_isolate` before it creates the bridge. Its rules drop every IPv4 packet that the host forwards between `br-lab` and another interface, whatever `net.ipv4.ip_forward` says. So no IPv4 connection from outside the lab host reaches a VM. The VMs reach no IPv4 address past the host. Adding the table changes no `sysctl` setting. The script replaces the table in one transaction, so a second `up` adds no duplicate rule. While the uplink is on, the uplink's table takes the place of this one.

The table accepts a packet that comes in and goes out through `br-lab`. So DHCP and other traffic between the ports of `br-lab` pass the table. The same holds when `br_netfilter` hands bridged traffic to the host's firewall. Another firewall on the host can still drop the bridged traffic, as [Troubleshooting](#troubleshooting) says. A packet that the host routes from `br-lab` back into `br-lab` passes too. It stays in the lab. The host's own connections to the VMs pass, because the host does not forward them.

The table matches IPv4 only. The script gives the VMs no IPv6 address. Its dnsmasq serves no DHCPv6 and sends no router advertisements. So a VM has only its link-local IPv6 address, unless another program on the host sends router advertisements on `br-lab` or someone sets an address by hand. An IPv6 router must not forward a packet with a link-local source or destination address to another link, as [RFC 4291, section 2.5.6](https://www.rfc-editor.org/rfc/rfc4291#section-2.5.6) says.

dnsmasq runs in its own network namespace, `deployarr-lab-dhcp`. A veth pair joins the namespace to `br-lab`. Its end on the bridge is `veth-lab-dhcp`, and its end in the namespace is `dhcp0`, with the address `10.41.0.2`. So dnsmasq holds UDP port 67 only in that namespace, and any program on the lab host, such as the PXE prototype or deployarr, can bind port 67 on `br-lab`. [DHCP](architecture.md#dhcp) says why deployarr needs that.

The last lines show `dnsmasq is running in namespace deployarr-lab-dhcp`, `dnsmasq options: option:router,10.41.0.1`, `0.0.0.0%dhcp0:67`, and a line that starts with `The uplink is off`. A line that starts with `The host forwards no IPv4` follows, and then the rules of `ip deployarr_lab_isolate`, without their packet and byte counts. dnsmasq writes its log to `/run/deployarr-lab/dnsmasq.log` and its leases to `/run/deployarr-lab/dnsmasq.leases`. Only root can read the log:

```bash
sudo tail /run/deployarr-lab/dnsmasq.log
```

Any user can read the leases:

```bash
cat /run/deployarr-lab/dnsmasq.leases
```

The network does not survive a reboot. Run the command again after each reboot. Running it again also moves a dnsmasq that an earlier version of the script started on the lab host into the namespace. To remove the network, run `sudo tools/lab/network.sh down`. It removes the namespace and the veth pair too, and then the table `ip deployarr_lab_isolate`. If the uplink is on, `down` also turns it off.

## 5. Check the lab

```bash
sudo tools/lab/check.sh
```

The check takes about a minute. It starts a packet capture and the [PXE prototype](../tools/pxe-prototype/README.md), and boots the three VMs to their network boot with no screen. The prototype serves a 1 KB unsigned test program in place of the Windows boot program. The check then prints one line for each test:

- The host's CPU has the eight processor features that the [Test lab](roadmap.md#test-lab) section lists. The check reads the `flags` line of each processor in `/proc/cpuinfo`, and a feature counts only when every line lists it. A `FAIL` line names each missing feature and its flag. The VM tests run either way.
- Each VM's PXE DISCOVER carries option 93 and option 97 with the VM's UUID.
- Each VM gets an address from dnsmasq.
- The PXE prototype answers each VM on port 4011.
- Each UEFI VM refuses the unsigned program with `Access Denied`, so Secure Boot is on.

The last line is `RESULT PASS`. The check writes a folder and a zip named `lab-check-<time>` in `~/deployarr-lab/checks`. The zip holds the capture, a `tshark -V` decode of the DHCP packets, the logs, `cpu-flags.txt` with the host's `flags` lines, and `versions.txt` with the version of each component. The capture filter drops TFTP data, so the zip holds no file contents. The folder and any zip belong to you, also when the check fails. A check that stops early, such as when `dumpcap` or the PXE prototype fails to start, writes the folder and no zip.

## 6. Get the Windows files

The experiments serve Microsoft's boot programs and WinPE, and the repository never holds them.

1. Download the Windows 11 ISO from Microsoft's [download page](https://www.microsoft.com/software-download/windows11). A person opens the page in a browser. Choose Windows 11 (multi-edition ISO for x64 devices), then English, not English International, then 64-bit Download. Either let the browser download the ISO, or copy the download link for the lab host to fetch. The PXE experiments use only the ISO's `sources\boot.wim`, which holds WinPE and the boot programs, and its `boot\boot.sdi`.

	An agent on the lab host does not try to get the ISO or its download link from Microsoft's page itself. Claude Code's auto mode blocks that as bot-check circumvention (#198). The agent fetches only a link that a person gives it.

2. Check the ISO. Its SHA-256 must match the English 64-bit line under Verify your download on the same page:

	```bash
	sha256sum ~/Downloads/<Windows ISO>.iso
	```

	Run this check before you build the TFTP folders in item 4. When an agent works on the lab host, the agent runs the check. The agent does not read the expected value from Microsoft's page. The person who gets the ISO copies the English 64-bit line and gives it to the agent. For the known ISO, `Windows11_Client_x64_en-us_26300_9457.iso`, the agent can compare against the SHA-256 in the table later in this step instead.

3. Download the BCD stores. In GitHub, open Actions, then the BCD workflow, and choose Run workflow on `main`. When the run finishes, download its `bcd-stores` artifact. The run from the pull request that added the stores also has one, until 2027-02-10: [run 43169481137](https://github.com/Tsukidy/deployarr/actions/runs/43169481137).
4. Build the TFTP folders. Use the paths of your two downloads:

	```bash
	tools/lab/media.sh ~/Downloads/<Windows ISO>.iso ~/Downloads/bcd-stores.zip
	```

The script exports index 1 of `sources\boot.wim`, which is WinPE, as a single-image `boot.wim`. It copies the boot programs from that image, `boot\boot.sdi` from the ISO, and the BCD stores from the zip. It writes two folders and a manifest:

| Folder | Boot managers | For |
| --- | --- | --- |
| `~/deployarr-lab/tftp/2011` | `bootmgfw.efi` and `wdsmgfw.efi`, signed by the Microsoft Windows Production PCA 2011 | #27 to #31 |
| `~/deployarr-lab/tftp/2023` | `bootmgfw_EX.efi` and `wdsmgfw_EX.efi`, renamed, signed by the Windows UEFI CA 2023 | #32 |

The last lines show the CA that issued each boot manager's signing certificate. The `_EX` copies come from `\Windows\Boot\EFI_EX` and `\Windows\Boot\PXE_EX` in the image. Their signers were first read with `osslsigncode` in the `winpe.wim` of the ADK WinPE add-on 10.1.26100.9457. The script says so if an ISO's `boot.wim` has no `_EX` copies.

A later download can have a different build and hash. The lab uses this ISO:

| Item | Value |
| --- | --- |
| File | `Windows11_Client_x64_en-us_26300_9457.iso`, 9,047,330,816 bytes |
| SHA-256 | `bd4307df32bc8af33b39ccecb1174aeb345386630f89a2b86c7a4e36b55ea650` |
| `sources\install.wim` | Build 26300.9457, 11 editions. Index 1 is Windows 11 Home and index 6 is Windows 11 Pro. |
| `sources\boot.wim` index 1 | Microsoft Windows PE (amd64), build 26100.9457. It has the `_EX` boot managers, and their signers match the table above. |

`media.sh` prints the WinPE name and build number. `manifest.txt` records the full WinPE build and the ISO's SHA-256.

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

An experiment runs the PXE prototype, a capture, and a VM, each in its own terminal. Start them in that order. On the lab host, the `uefi` VM sent its first DHCP Discover 1.4 seconds after `vm.sh` started, so the prototype and the capture must already run.

Run the PXE prototype with one of the TFTP folders. Its [README](../tools/pxe-prototype/README.md) gives the command for each experiment. In the lab, the address is `10.41.0.1`, the interface is `br-lab`, and the root is `~/deployarr-lab/tftp/2011` or `~/deployarr-lab/tftp/2023`. For example:

```bash
sudo python3 -B tools/pxe-prototype/pxe_prototype.py --server-ip 10.41.0.1 --interface br-lab --root ~/deployarr-lab/tftp/2011 --chain two-stage --log-file two-stage.log
```

`-B` stops Python from writing a `__pycache__` folder owned by root into the checkout.

Capture on the bridge, as the experiment issues ask. The captures go in `~/deployarr-lab/captures`, outside the repository, until you check them:

```bash
mkdir -p ~/deployarr-lab/captures
```

```bash
sudo dumpcap -i br-lab -f 'udp and udp[8:2] != 3' -w - > ~/deployarr-lab/captures/<name>.pcapng
```

The filter drops TFTP DATA packets and IP fragments, so the capture holds no file bytes.

`dumpcap` writes the capture to standard output, and your shell writes the file, so the file belongs to you. `dumpcap` cannot open a file in your home folder itself. It drops root's capabilities once it opens the interface, and Ubuntu's home folders are closed to other users.

Start a VM:

```bash
tools/lab/vm.sh uefi
```

Use `uefi2023` or `bios` for the other VMs. On a desktop, the VM opens a window. On a server, the script prints a VNC address on `127.0.0.1`. Reach it through SSH, for example with `ssh -L 5901:127.0.0.1:5901 <host>`, and a VNC viewer. The firmware's console text goes to `~/deployarr-lab/<vm>/serial.log`. Close the window or press Ctrl-C to stop the VM.

The VMs use the host's CPU model, because QEMU's default model lacks processor features that Windows 11 requires (#75). The SeaBIOS VM runs with `smbios-entry-point-type=32`. On the lab's versions, its iPXE ROM sends option 97 without that property too, and also when the guest has only a 64-bit SMBIOS entry point ([iPXE option 97 in the PXE lab](research/pxe-lab/ipxe-option-97.md#result)). The property stays for the older ROM in ipxe-qemu 1.21.1+git-20220113.fbbdc3926, from Ubuntu 24.04, which sends option 97 only with a 32-bit entry point (#56). OVMF sends option 97 too ([DHCP](architecture.md#dhcp)), so no lab VM sends a PXE request without option 97, and the lab does not test the reply to such a request. The UEFI VMs use a virtio network card with no option ROM, so OVMF's own PXE client runs with Secure Boot on.

Wait a few seconds after the last packet you need. Then press Ctrl-C in the capture's terminal to stop the capture, and in the prototype's terminal to stop the prototype. To stop the capture after a set time instead, add `-a duration:<seconds>` to the `dumpcap` command before `-w`.

Ubuntu 26.04 confines `tshark` with an AppArmor profile, `/etc/apparmor.d/tshark`. The profile lets `tshark` open capture files only in temporary folders such as `/tmp`, even as root. So give `tshark` the capture on standard input with `-r -`, and let your shell open the file. On the lab host's Linux 7.0 kernel, AppArmor does not recheck a file that your shell opened. To list the packets:

```bash
tshark -r - < ~/deployarr-lab/captures/<name>.pcapng
```

Each `tshark` run still adds an `apparmor="DENIED"` line for `/proc/sys/vm/nr_hugepages` to the kernel log, and a `ptrace` line when it runs as root. They do not stop `tshark`, and no line names the capture.

An experiment commits its captures with its research page, in `docs/research/pxe-lab/captures/`. Start each capture's name with the page's name, such as `single-stage-chain-uefi.pcapng` for `single-stage-chain.md`.

Before you commit a capture, check that it holds no TFTP data and no fragments. For a capture made with the filter above, this command prints nothing:

```bash
tshark -r - -Y 'tftp.opcode == 3 or ip.flags.mf == 1 or ip.frag_offset > 0' < ~/deployarr-lab/captures/<name>.pcapng
```

Copy the capture into the repository:

```bash
mkdir -p docs/research/pxe-lab/captures
```

```bash
cp ~/deployarr-lab/captures/<name>.pcapng docs/research/pxe-lab/captures/
```

Commit a decode of the capture's DHCP packets next to it, as `<name>-dhcp.txt`:

```bash
tshark -r - -Y dhcp -V < docs/research/pxe-lab/captures/<name>.pcapng > docs/research/pxe-lab/captures/<name>-dhcp.txt
```

Never commit a capture that holds file data, or any of the Windows files.

To start a UEFI VM without its TPM, set `LAB_NO_TPM=1`:

```bash
LAB_NO_TPM=1 tools/lab/vm.sh uefi
```

The script then starts no `swtpm` and gives QEMU no TPM device, and it prints a line that says so. It leaves the TPM state in `~/deployarr-lab/uefi/tpm` as it is, so the next start without the variable gets the same TPM back. The variable works for `uefi` and `uefi2023`, and changes nothing for `bios`, which has no TPM. The BitLocker check of [M7](roadmap.md#m7-applications) uses it to run the sequence on a PC with no TPM.

To reset a VM, stop it and delete its folder, such as `~/deployarr-lab/uefi`. That removes its disk, its firmware variables, and its TPM state. The next start creates them again.

## Turn on the uplink

The lab bridge has no uplink by default. The uplink lets the VMs reach the internet through the lab host. Only the [M7 check](roadmap.md#m7-applications) of the `InstallUpdates` step uses it, because a failed Windows Update search fails that step. Run every other check and experiment with the uplink off.

The uplink uses `nft`, which [step 1](#1-install-the-packages) installs with the `nftables` package.

Turn on the uplink:

```bash
sudo env LAB_UPLINK=1 tools/lab/network.sh up
```

`sudo` resets the environment, so the variable goes after `sudo env`. With `LAB_UPLINK=1 sudo tools/lab/network.sh up`, the script never sees the variable and leaves the uplink off. With a value that is not `0`, `1`, or empty, `up` stops with `LAB_UPLINK must be 0 or 1.` before it changes anything. An empty value counts as `0`.

With the variable, `up` builds the lab network as in [step 4](#4-start-the-lab-network), and then it does four more things:

1. It saves `net.ipv4.ip_forward` and `net.ipv4.conf.all.accept_redirects` in `/run/deployarr-lab/uplink-sysctl.conf`, unless an earlier `up` saved them already. So a second `up` keeps the values that the host had before the first.
2. It replaces the nftables table `ip deployarr_lab`, and deletes the table `ip deployarr_lab_isolate` from [step 4](#4-start-the-lab-network), in one transaction. So a second `up` adds no duplicate rule. The bridge is never without rules either. The isolation table cannot stay while the uplink is on, because a drop in any base chain on the forward hook ends the packet.
3. It turns on `net.ipv4.ip_forward`. The kernel then sets `net.ipv4.conf.all.accept_redirects` to `0`.
4. It starts dnsmasq again with DHCP option 6, unless dnsmasq already runs with it. The option names the DNS servers in `UPLINK_DNS` in `tools/lab/lab.conf`, which are `1.1.1.1` and `9.9.9.9`. The router option stays `10.41.0.1`.

The output shows `dnsmasq options: option:router,10.41.0.1 option:dns-server,1.1.1.1,9.9.9.9`. It ends with a line that starts with `The uplink is on`, and then the table's rules with their counters. `sudo tools/lab/network.sh status` shows the same lines.

With the uplink on:

- A VM reaches whatever the lab host reaches through the interface of its IPv4 default route. That is the internet and also the host's own network. The host masquerades the traffic, so it leaves with the host's address.
- The table forwards a packet from `br-lab` out of the uplink only when its source is in `10.41.0.0/24`, the lab's subnet. It drops a packet with any other source.
- The table drops a packet from `br-lab` out of the uplink when connection tracking marks it invalid. NAT does not rewrite such a packet, so without the drop it would leave with its address in `10.41.0.0/24`.
- Only replies come back in. The table drops every other packet that the host forwards into `br-lab`. So no connection that starts outside the lab host reaches a VM.
- Programs on the lab host itself still reach the VMs, as they do with the uplink off.
- The table never drops traffic between the ports of `br-lab`.
- Forwarding is on for the whole host, because `net.ipv4.ip_forward` turns on `forwarding` for every interface. So the host also forwards IPv4 that comes in on its other interfaces, such as from a machine on its network that uses the host as a gateway. The lab's table leaves that traffic to the host's own firewall, unless it goes into `br-lab` or its source is in `10.41.0.0/24`.
- The table drops a forwarded packet with a source in `10.41.0.0/24` that does not come in on `br-lab`. So a machine outside the bridge cannot use a VM's address through the uplink.
- The uplink is IPv4 only. Its table is an `ip` table, and `up` changes no IPv6 setting.

Turn on the uplink before you start a VM. A VM that already holds a lease gets the DNS server only from its next DHCP exchange, so restart it.

To turn off the uplink and keep the lab network, run `up` without the variable:

```bash
sudo tools/lab/network.sh up
```

`up` without the variable and `down` both put back the two settings that the first `up` saved. Then, in one transaction, both delete the table `ip deployarr_lab` and add the table `ip deployarr_lab_isolate` from [step 4](#4-start-the-lab-network) again. `down` then deletes `ip deployarr_lab_isolate`, after it removes the bridge. `up` also restarts dnsmasq without option 6. It ends with a line that starts with `The uplink is off`, a line that starts with `The host forwards no IPv4`, and the rules of `ip deployarr_lab_isolate`.

The uplink loads the kernel's connection tracking and NAT modules, such as `nf_conntrack`, `nf_nat`, `nft_ct`, and `nft_masq`. They stay loaded after the uplink is off, until the host restarts.

A host firewall can drop the forwarded traffic, and the lab's table cannot override that drop. nftables runs every base chain on the forward hook, and a drop in any of them ends the packet. The section `OVERALL EVALUATION OF THE RULESET` in `man nft` of nftables 1.1.6 says so. These two settings drop the traffic:

- ufw, when it is active. On Ubuntu 26.04, `/etc/default/ufw` sets `DEFAULT_FORWARD_POLICY="DROP"`. `sudo ufw status verbose` shows it as `deny (routed)` while forwarding is on, and as `disabled (routed)` while forwarding is off. Allow the lab's traffic with `sudo ufw route allow in on br-lab out on <interface>`. `<interface>` is the device that `ip -4 route show default` names. ufw's default rules already accept the replies.
- Docker, when it turns on IP forwarding itself. It then sets the policy of the iptables `FORWARD` chain to `DROP`, as [Docker with iptables](https://docs.docker.com/engine/network/firewall-iptables/#docker-and-iptables-chains) says. Accept the lab's traffic out with `sudo iptables -I DOCKER-USER -i br-lab -o <interface> -j ACCEPT`. That is the rule in [Allow forwarding between host interfaces](https://docs.docker.com/engine/network/firewall-iptables/#allow-forwarding-between-host-interfaces), with `br-lab` as `src_if` and `<interface>` as `dst_if`. It accepts only the traffic out. To accept the replies too, add `sudo iptables -I DOCKER-USER -m state --state RELATED,ESTABLISHED -j ACCEPT`, the rule in [Restrict external connections to containers](https://docs.docker.com/engine/network/firewall-iptables/#restrict-external-connections-to-containers). It accepts every packet of a connection that another rule accepted already.

To look for another firewall that drops forwarded traffic, look for a `hook forward` chain with `policy drop` in `sudo nft list ruleset`, or for `:FORWARD DROP` in `sudo iptables-save`.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `No usable /dev/kvm` | Turn on VT-x or AMD-V in the host's firmware settings. Run `sudo usermod -aG kvm "$USER"`, then sign out and sign in again. |
| `pip install virt-firmware` stops at `x86_64-linux-gnu-gcc`, `Python.h`, or `crypt.h` | The build packages are missing. Run the `apt-get install` command in step 3, then run the `pip install` again. |
| The check stops at `dumpcap or the PXE prototype failed to start` | Read `dumpcap.log` and `prototype.out` in the evidence folder. Often another program holds UDP port 69 or 4011, such as a PXE prototype that still runs. `sudo ss -ulpn 'sport = :69 or sport = :4011'` shows it. |
| `dumpcap` says that the file `could not be opened: Permission denied` | `dumpcap` cannot open a file in your home folder. Add `-w -` and redirect its output to the file, as in [Run an experiment](#run-an-experiment). |
| `tshark` says `You don't have permission to read the file` | The AppArmor profile for `tshark` blocks the path. Give the file on standard input with `tshark -r - < <file>`, as in [Run an experiment](#run-an-experiment). |
| The PXE prototype cannot bind port 67, or `sudo ss -ulpn 'sport = :67'` on the lab host shows a program | Another DHCP server holds port 67 in the lab host's network stack. The lab's dnsmasq runs in its own namespace and never shows here. Common ones are the `dnsmasq` service and libvirt's `default` network. Stop it, or bind it to its own interface as the [prototype README](../tools/pxe-prototype/README.md#share-port-67-with-dnsmasq) says. |
| The check fails on `dnsmasq gave an address` for every VM | The lab host filters traffic that crosses the bridge. That happens when the `br_netfilter` module is loaded, which Docker does, and `cat /proc/sys/net/bridge/bridge-nf-call-iptables` prints `1`. Allow the bridged traffic with `sudo ufw route allow in on br-lab out on br-lab`, or set `net.bridge.bridge-nf-call-iptables` to `0`. |
| The check fails on `the PXE prototype answered on port 4011` for every VM | A firewall on the lab host drops the requests to the prototype. If `sudo ufw status` says `active`, run `sudo ufw allow in on br-lab`. |
| The check fails on `host CPU` | The line names each processor feature that the host's CPU lacks. The VMs use the host's CPU model, so Windows 11 in a lab VM lacks the same features. Use a host whose CPU has every feature in the [Test lab](roadmap.md#test-lab) section. |
| The check fails on option 97 for `bios` | The DISCOVER from `bios` carried no option 97, or a UUID other than `4c414200-0000-4000-8000-000000000021`. On the lab's versions, the iPXE ROM sends option 97 with or without the SMBIOS setting, so a missing setting is not the cause ([iPXE option 97 in the PXE lab](research/pxe-lab/ipxe-option-97.md#result)). The ROM sends the guest's SMBIOS UUID, and without `-uuid`, QEMU gives the guest an all-zero UUID. Check that `tools/lab/vm.sh` and the `VM_UUID` entry for `bios` in `tools/lab/lab.conf` are unchanged. |
| The check fails on `Access Denied` | Secure Boot is off in that VM. Delete `~/deployarr-lab/uefi/vars.fd` or `~/deployarr-lab/uefi2023/vars.fd`, and redo step 3 for `uefi2023`. |
| `media.sh` says the ISO has no `sources/boot.wim` | The file is not a Windows installation ISO. Download it again. |
| `network.sh up` says `The host has no IPv4 default route, so the lab has no uplink.` | The uplink forwards out of the interface of the host's IPv4 default route. `ip -4 route show default` names no interface, or it names `br-lab`. Connect the host to a network that gives it a default route, then run the command again. Or run `up` without `LAB_UPLINK=1`. If the uplink was on before this `up`, it is still on, with its old rules, and `status` says `The uplink is on`. `up` without `LAB_UPLINK=1` or `down` turns it off. |
| With the uplink on, a VM gets no answer from the internet | Check first that the lab host itself reaches the internet. If it does, a host firewall may drop the forwarded traffic. Allow it as [Turn on the uplink](#turn-on-the-uplink) says. Or the VM took its lease before the uplink was on, so it has no DNS server. Restart the VM. |
