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

# The start of the flags line of an Intel Xeon in a cloud container. It has
# all eight processor features that Windows 11 needs.
FLAGS = (
    "fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush mmx fxsr sse sse2 ss ht"
    " syscall nx pdpe1gb rdtscp lm constant_tsc rep_good nopl xtopology nonstop_tsc cpuid tsc_known_freq pni"
    " pclmulqdq ssse3 fma cx16 pcid sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c"
    " rdrand hypervisor lahf_lm abm 3dnowprefetch cpuid_fault ssbd ibrs ibpb stibp ibrs_enhanced fsgsbase"
    " tsc_adjust bmi1 hle avx2 smep bmi2 erms invpcid rtm avx512f avx512dq rdseed adx smap"
)
# Name and /proc/cpuinfo flag of each feature, as in the Test lab section.
FEATURES = (
    ("PAE", "pae"),
    ("NX", "nx"),
    ("SSE4.1", "sse4_1"),
    ("CMPXCHG16B", "cx16"),
    ("LAHF/SAHF", "lahf_lm"),
    ("PrefetchW", "3dnowprefetch"),
    ("SSE4.2", "sse4_2"),
    ("POPCNT", "popcnt"),
)


def cpu_flags(*lines):
    """The flags lines that check.sh copies from /proc/cpuinfo, one per processor."""
    return "".join(f"flags\t\t: {line}\n" for line in lines)


def without(flag, line=FLAGS):
    words = line.split()
    assert flag in words
    return " ".join(word for word in words if word != flag)


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
    def run_summary(self, fields, log, serial_uefi=DENIED, serial_uefi2023=DENIED, flags=cpu_flags(*[FLAGS] * 4)):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            if flags is not None:
                (out / "cpu-flags.txt").write_text(flags)
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
        self.assertEqual(len([line for line in lines if line.startswith("PASS")]), 12)
        self.assertIn("PASS host CPU: has PAE, NX, SSE4.1, CMPXCHG16B, LAHF/SAHF, PrefetchW, SSE4.2, and POPCNT", lines)

    def test_a_cpu_without_one_feature_fails(self):
        for name, flag in FEATURES:
            with self.subTest(flag=flag):
                code, lines = self.run_summary(dhcp_rows(), prototype_log(), flags=cpu_flags(*[without(flag)] * 4))
                self.assertEqual(code, 1)
                self.assertIn(f"FAIL host CPU: lacks {name} ({flag})", lines)
                self.assertEqual(lines[-1], "RESULT FAIL (1 checks failed)")
                # The VM tests still run, so one run reports every problem.
                self.assertEqual(len([line for line in lines if line.startswith("PASS")]), 11)

    def test_a_cpu_without_several_features_names_each(self):
        code, lines = self.run_summary(
            dhcp_rows(), prototype_log(), flags=cpu_flags(without("3dnowprefetch", without("sse4_1")))
        )
        self.assertEqual(code, 1)
        self.assertIn("FAIL host CPU: lacks SSE4.1 (sse4_1) and PrefetchW (3dnowprefetch)", lines)

    def test_a_feature_missing_from_one_processor_fails(self):
        code, lines = self.run_summary(
            dhcp_rows(), prototype_log(), flags=cpu_flags(FLAGS, FLAGS, without("nx"), FLAGS)
        )
        self.assertEqual(code, 1)
        self.assertIn("FAIL host CPU: lacks NX (nx)", lines)

    def test_no_flags_line_fails(self):
        # On a CPU with VMX, /proc/cpuinfo also has a "vmx flags" line, which
        # is not the flags line.
        no_flags = "processor\t: 0\nmodel name\t: Intel(R) Xeon(R) Processor\nvmx flags\t: vnmi ept\n"
        for flags in (no_flags, "", None):
            with self.subTest(flags=flags):
                code, lines = self.run_summary(dhcp_rows(), prototype_log(), flags=flags)
                self.assertEqual(code, 1)
                self.assertIn("FAIL host CPU: found no flags line in cpu-flags.txt", lines)
                self.assertEqual(lines[-1], "RESULT FAIL (1 checks failed)")

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
