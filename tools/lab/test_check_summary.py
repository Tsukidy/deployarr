"""Tests for check_summary.py, with evidence shaped like a real check.sh run."""

import tempfile
import unittest.mock
import unittest
from pathlib import Path

import check_summary

UEFI = ("52:54:00:41:00:11", "4c414200-0000-4000-8000-000000000011")
UEFI2023 = ("52:54:00:41:00:12", "4c414200-0000-4000-8000-000000000012")
BIOS = ("52:54:00:41:00:21", "4c414200-0000-4000-8000-000000000021")

DENIED = """>>Start PXE over IPv4.
  NBP filename is boot\\x64\\wdsmgfw.efi
  NBP file downloaded successfully.
BdsDxe: failed to load Boot0002 "UEFI PXEv4 (MAC:525400410011)" from PciRoot(0x0)/Pci(0x3,0x0): Access Denied
"""
STARTED = """>>Start PXE over IPv4.
  NBP file downloaded successfully.
BdsDxe: starting Boot0002 "UEFI PXEv4 (MAC:525400410011)" from PciRoot(0x0)/Pci(0x3,0x0)
"""


def dhcp_rows(bios_uuid=BIOS[1]):
    rows = []
    for (mac, uuid), arch, address in ((UEFI, "7", "10.41.0.176"), (UEFI2023, "7", "10.41.0.177"), (BIOS, "0", "10.41.0.192")):
        if mac == BIOS[0]:
            uuid = bios_uuid
            # iPXE's option 61 repeats the MAC address, so tshark can print it twice.
            mac_field = f"{mac},{mac}"
        else:
            mac_field = mac
        rows.append(f"{mac_field}\t1\t{arch}\t{uuid}\t0.0.0.0")
        rows.append(f"{mac}\t2\t\t\t{address}")
        rows.append(f"{mac}\t5\t\t\t{address}")
    return "\n".join(rows) + "\n"


def prototype_log(macs=(UEFI[0], UEFI2023[0], BIOS[0])):
    lines = []
    for mac in macs:
        # Both emulated UEFI VMs used the same xid in a real run.
        xid = "d876ad38" if mac == BIOS[0] else "3d7fb416"
        lines.append(f"t dhcp-received port=4011 source=x type=REQUEST xid={xid} mac={mac} arch=0007")
        lines.append(f"t dhcp-sent port=4011 destination=x type=ACK xid={xid} firmware=uefi-x64 step=first file=boot\\x64\\wdsmgfw.efi")
    return "\n".join(lines) + "\n"


class CheckSummaryTest(unittest.TestCase):
    def run_summary(self, fields, log, serial_uefi=DENIED, serial_uefi2023=DENIED):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            (out / "dhcp-fields.tsv").write_text(fields)
            (out / "prototype.log").write_text(log)
            (out / "serial-uefi.log").write_text(serial_uefi)
            (out / "serial-uefi2023.log").write_text(serial_uefi2023)
            with unittest.mock.patch("builtins.print") as printed:
                code = check_summary.main(out)
            lines = [call.args[0] for call in printed.call_args_list]
        return code, lines

    def test_a_good_run_passes(self):
        code, lines = self.run_summary(dhcp_rows(), prototype_log())
        self.assertEqual(code, 0)
        self.assertEqual(lines[-1], "RESULT PASS")
        self.assertEqual(len([line for line in lines if line.startswith("PASS")]), 11)

    def test_a_bios_request_without_option_97_fails(self):
        code, lines = self.run_summary(dhcp_rows(bios_uuid=""), prototype_log())
        self.assertEqual(code, 1)
        self.assertIn(f"FAIL bios: PXE DISCOVER from {BIOS[0]} carries option 93 = 0 and option 97 = {BIOS[1]}", lines)

    def test_secure_boot_off_fails(self):
        code, lines = self.run_summary(dhcp_rows(), prototype_log(), serial_uefi2023=STARTED)
        self.assertEqual(code, 1)
        self.assertIn("FAIL uefi2023: Secure Boot refused the unsigned boot program", lines)
        self.assertIn("PASS uefi: Secure Boot refused the unsigned boot program", lines)

    def test_a_vm_the_prototype_did_not_answer_fails(self):
        code, lines = self.run_summary(dhcp_rows(), prototype_log(macs=(UEFI[0], BIOS[0])))
        self.assertEqual(code, 1)
        self.assertIn("FAIL uefi2023: the PXE prototype answered on port 4011", lines)
        self.assertIn("PASS uefi: the PXE prototype answered on port 4011", lines)


if __name__ == "__main__":
    unittest.main()
