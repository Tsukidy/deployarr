"""Summarize a check.sh run: one PASS or FAIL line per check, then RESULT.

Usage: python3 check_summary.py <evidence folder>
The folder holds dhcp-fields.tsv, prototype.log, and serial-<vm>.log, which
check.sh writes. Exits 1 when a check fails.
"""

import re
import sys
from pathlib import Path

# name: (MAC address, SMBIOS UUID, option 93 value). These match lab.conf.
VMS = {
    "uefi": ("52:54:00:41:00:11", "4c414200-0000-4000-8000-000000000011", "7"),
    "uefi2023": ("52:54:00:41:00:12", "4c414200-0000-4000-8000-000000000012", "7"),
    "bios": ("52:54:00:41:00:21", "4c414200-0000-4000-8000-000000000021", "0"),
}
DISCOVER, ACK = "1", "5"


def dhcp_rows(path):
    """Rows of chaddr, message type, option 93, option 97 UUID, yiaddr."""
    rows = []
    for line in path.read_text().splitlines():
        fields = (line.split("\t") + [""] * 5)[:5]
        rows.append([f.split(",")[0] for f in fields])
    return rows


def answered_on_4011(log_lines):
    """MAC addresses whose port 4011 REQUEST got an ACK from the prototype.

    Two VMs can use the same xid, so each ACK goes to the MAC address of the
    latest REQUEST with that xid. The prototype logs a reply right after the
    request it answers.
    """
    latest, answered = {}, set()
    for line in log_lines:
        received = re.search(r"dhcp-received port=4011 .*xid=(\w+) mac=([0-9a-f:]+)", line)
        if received:
            latest[received.group(1)] = received.group(2)
        sent = re.search(r"dhcp-sent port=4011 .*type=ACK xid=(\w+) ", line)
        if sent and sent.group(1) in latest:
            answered.add(latest[sent.group(1)])
    return answered


def main(out):
    rows = dhcp_rows(out / "dhcp-fields.tsv")
    answered = answered_on_4011((out / "prototype.log").read_text().splitlines())
    results = []
    for vm, (mac, uuid, arch) in VMS.items():
        mine = [r for r in rows if r[0] == mac]
        results.append((
            any(r[1] == DISCOVER and r[2] == arch and r[3] == uuid for r in mine),
            f"{vm}: PXE DISCOVER from {mac} carries option 93 = {arch} and option 97 = {uuid}",
        ))
        leases = [r[4] for r in mine if r[1] == ACK and r[4] not in ("", "0.0.0.0")]
        results.append((bool(leases), f"{vm}: dnsmasq gave an address" + (f" ({leases[0]})" if leases else "")))
        results.append((mac in answered, f"{vm}: the PXE prototype answered on port 4011"))
        if arch == "7":
            serial = out / f"serial-{vm}.log"
            text = serial.read_text(errors="replace") if serial.exists() else ""
            refused = "NBP file downloaded successfully" in text and re.search(
                r"failed to load Boot\w+ .*Access Denied", text
            )
            results.append((bool(refused), f"{vm}: Secure Boot refused the unsigned boot program"))
    failed = sum(1 for ok, _ in results if not ok)
    for ok, text in results:
        print(("PASS " if ok else "FAIL ") + text)
    print("RESULT " + ("PASS" if failed == 0 else f"FAIL ({failed} checks failed)"))
    return 1 if failed else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.exit(main(Path(sys.argv[1])))
