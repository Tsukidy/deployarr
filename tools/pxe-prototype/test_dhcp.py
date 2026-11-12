"""Tests for the DHCP replies. Run with: python3 -m unittest"""
import unittest
from ipaddress import IPv4Address

import dhcp
import policy
import reference
import wds250
from policy import Chain, Firmware, IgnoreReason, Step

SERVER = "192.0.2.10"
DISCOVER, OFFER, REQUEST, ACK, INFORM = 1, 2, 3, 5, 8
RELAY_INFO = b"\x01\x04eth0"
NORMAL_250 = bytes.fromhex("020101" "050400000000" "03020014" "040200ba" "06085343434d20505845" "0b0101")
SECOND_REQUEST_250 = bytes.fromhex("01020009" "0b0102")

UEFI_MANAGER = (rb"boot\x64\bootmgfw.efi", 252, rb"boot\uefi.bcd")
BIOS_MANAGER = (rb"boot\x64\pxeboot.n12", 252, rb"boot\bios.bcd")
# (chain, option 93, the REQUEST's option 250) to (file, option code, option value)
EXPECTED_ACKS = {
    ("single", 0, None): BIOS_MANAGER,
    ("single", 7, None): UEFI_MANAGER,
    ("single", 9, None): UEFI_MANAGER,
    ("single", 0, SECOND_REQUEST_250): BIOS_MANAGER,
    ("single", 9, SECOND_REQUEST_250): UEFI_MANAGER,
    ("two-stage", 0, None): (rb"boot\x64\wdsnbp.com", 250, NORMAL_250),
    ("two-stage", 7, None): (rb"boot\x64\wdsmgfw.efi", 250, b"\x0b\x01\x02"),
    ("two-stage", 9, None): (rb"boot\x64\wdsmgfw.efi", 250, b"\x0b\x01\x02"),
    ("two-stage", 0, SECOND_REQUEST_250): BIOS_MANAGER,
    ("two-stage", 7, SECOND_REQUEST_250): UEFI_MANAGER,
    ("two-stage", 9, SECOND_REQUEST_250): UEFI_MANAGER,
    ("two-stage", 0, b"\x0b\x01\x02"): BIOS_MANAGER,
    ("two-stage", 9, b""): UEFI_MANAGER,
}


def answer(request: bytes, chain: str = "two-stage"):
    return policy.respond(dhcp.parse(request), policy.Policy(IPv4Address(SERVER), Chain(chain)))


def reply_to(request: bytes, chain: str = "two-stage") -> reference.DhcpReply:
    return reference.decode_dhcp_reply(dhcp.encode(answer(request, chain).packet))


class ReplyTest(unittest.TestCase):
    def assert_common_fields(self, reply, message_type, option_82):
        self.assertEqual(reply.op, 2)
        self.assertEqual(reply.siaddr, SERVER)
        self.assertEqual(reply.option(53), bytes((message_type,)))
        self.assertEqual(reply.option(54), bytes((192, 0, 2, 10)))
        self.assertEqual(reply.option(60), b"PXEClient")
        self.assertEqual(reply.option(97), b"\x00" + reference.GUID)
        self.assertEqual(reply.option(82), option_82)
        for absent in (43, 66, 67, 243):
            self.assertNotIn(absent, reply.codes)
        self.assertEqual(reply.sname, bytes(64))
        self.assertEqual(reply.xid, 0x1234ABCD)
        self.assertEqual(reply.secs, 4)
        self.assertEqual(reply.flags, 0x8000)
        self.assertEqual(reply.giaddr, "10.9.8.7")
        self.assertEqual(reply.ciaddr, "0.0.0.0")
        self.assertEqual(reply.yiaddr, "0.0.0.0")
        self.assertEqual(reply.hlen, 6)
        self.assertEqual(reply.chaddr, b"\x52\x54\x00\xaa\xbb\xcc" + bytes(10))
        self.assertGreaterEqual(len(reply.raw), 300)

    def test_offer(self):
        for chain in ("single", "two-stage"):
            for arch in (0, 7, 9):
                for option_82 in (None, RELAY_INFO):
                    with self.subTest(chain=chain, arch=arch, option_82=option_82):
                        reply = reply_to(reference.dhcp_request(
                            DISCOVER, arch, giaddr="10.9.8.7", option_82=option_82), chain)
                        self.assert_common_fields(reply, OFFER, option_82)
                        self.assertEqual(reply.file, bytes(128))
                        self.assertEqual(reply.codes, [53, 54, 97, 60] + ([82] if option_82 else []))

    def test_ack(self):
        for (chain, arch, option_250), (file, code, value) in EXPECTED_ACKS.items():
            for option_82 in (None, RELAY_INFO):
                with self.subTest(chain=chain, arch=arch, option_250=option_250, option_82=option_82):
                    reply = reply_to(reference.dhcp_request(
                        REQUEST, arch, giaddr="10.9.8.7", option_82=option_82,
                        option_250=option_250), chain)
                    self.assert_common_fields(reply, ACK, option_82)
                    self.assertEqual(reply.file, file.ljust(128, b"\x00"))
                    self.assertEqual(reply.option(code), value)
                    self.assertEqual(reply.codes, [53, 54, 97, 60] + ([82] if option_82 else []) + [code])

    def test_offer_to_a_discover_that_carries_option_250_has_no_boot(self):
        reply = reply_to(reference.dhcp_request(DISCOVER, 9, option_250=SECOND_REQUEST_250))
        self.assertEqual(reply.codes, [53, 54, 97, 60])
        self.assertEqual(reply.file, bytes(128))

    def test_offer_is_padded_to_the_bootp_minimum(self):
        raw = dhcp.encode(answer(reference.dhcp_request(DISCOVER, 7)).packet)
        self.assertEqual(len(raw), 300)
        self.assertEqual(raw[240:280], bytes.fromhex(
            "350102" "3604c000020a" "611100101112131415161718191a1b1c1d1e1f"
            "3c09505845436c69656e74" "ff"))
        self.assertEqual(raw[280:], bytes(20))

    def test_first_ack_on_bios_copies_the_request_option_5(self):
        cases = {
            None: b"\x00\x00\x00\x00",
            b"\xde\xad\xbe\xef": b"\xde\xad\xbe\xef",
            b"\xde\xad\xbe\xef\x01\x02": b"\xde\xad\xbe\xef",
            b"\xde\xad": b"\xde\xad\x00\x00",
        }
        for option_5, sub_option_5 in cases.items():
            with self.subTest(option_5=option_5):
                reply = reply_to(reference.dhcp_request(REQUEST, 0, option_5=option_5))
                self.assertEqual(reply.option(250),
                                 NORMAL_250[:5] + sub_option_5 + NORMAL_250[9:])
                self.assertEqual(len(reply.option(250)), 30)

    def test_first_ack_option_250_on_the_wire(self):
        bios = dhcp.encode(answer(reference.dhcp_request(REQUEST, 0)).packet)
        self.assertEqual(bios[279:], b"\xfa\x1e" + NORMAL_250 + b"\xff")
        self.assertEqual(len(bios), 312)
        uefi = dhcp.encode(answer(reference.dhcp_request(REQUEST, 7)).packet)
        self.assertEqual(uefi[279:], bytes.fromhex("fa030b0102ff") + bytes(15))

    def test_reply_names_its_firmware_and_step(self):
        cases = [
            ("single", DISCOVER, 0, None, Firmware.BIOS, None),
            ("single", REQUEST, 9, None, Firmware.UEFI_X64, Step.ONLY),
            ("single", REQUEST, 7, SECOND_REQUEST_250, Firmware.UEFI_X64, Step.ONLY),
            ("two-stage", REQUEST, 0, None, Firmware.BIOS, Step.FIRST),
            ("two-stage", REQUEST, 7, SECOND_REQUEST_250, Firmware.UEFI_X64, Step.SECOND),
        ]
        for chain, message_type, arch, option_250, firmware, step in cases:
            with self.subTest(chain=chain, message_type=message_type, arch=arch):
                reply = answer(reference.dhcp_request(message_type, arch, option_250=option_250), chain)
                self.assertEqual((reply.firmware, reply.step), (firmware, step))

    def test_boot_table_covers_every_step_and_firmware(self):
        self.assertEqual(set(policy.BOOT_TABLE), {
            (Step.ONLY, Firmware.BIOS), (Step.ONLY, Firmware.UEFI_X64),
            (Step.FIRST, Firmware.BIOS), (Step.FIRST, Firmware.UEFI_X64),
            (Step.SECOND, Firmware.BIOS), (Step.SECOND, Firmware.UEFI_X64),
        })


class NoReplyTest(unittest.TestCase):
    def test_reasons(self):
        cases = {
            "inform": (reference.dhcp_request(INFORM, 7), IgnoreReason.NOT_DISCOVER_OR_REQUEST),
            "offer": (reference.dhcp_request(OFFER, 7), IgnoreReason.NOT_DISCOVER_OR_REQUEST),
            "bootreply": (reference.dhcp_request(DISCOVER, 7, op=2), IgnoreReason.NOT_A_REQUEST),
            "no option 93": (reference.dhcp_request(DISCOVER, None), IgnoreReason.NO_OPTION_93),
            "no option 97": (reference.dhcp_request(REQUEST, 7, None), IgnoreReason.NO_OPTION_97),
            "ia32 uefi": (reference.dhcp_request(DISCOVER, 6), IgnoreReason.UNSUPPORTED_ARCH),
            "arm64 uefi": (reference.dhcp_request(REQUEST, 11), IgnoreReason.UNSUPPORTED_ARCH),
        }
        for name, (request, reason) in cases.items():
            with self.subTest(name):
                self.assertEqual(answer(request), policy.NoReply(reason))


class DestinationTest(unittest.TestCase):
    def test_source_without_an_address_gets_a_broadcast(self):
        self.assertEqual(policy.reply_destination(("0.0.0.0", 68)), ("255.255.255.255", 68))

    def test_unicast_source_gets_a_unicast(self):
        self.assertEqual(policy.reply_destination(("192.0.2.77", 4011)), ("192.0.2.77", 4011))

    def test_relay_gets_a_unicast_on_its_port(self):
        self.assertEqual(policy.reply_destination(("10.9.8.7", 67)), ("10.9.8.7", 67))


class LayoutTest(unittest.TestCase):
    def test_bcd_paths_come_from_the_layout(self):
        layout = policy.Layout(bcd_uefi=r"lab\a.bcd", bcd_bios=r"lab\b.bcd")
        for arch, bcd in ((9, rb"lab\a.bcd"), (0, rb"lab\b.bcd")):
            packet = policy.respond(
                dhcp.parse(reference.dhcp_request(REQUEST, arch)),
                policy.Policy(IPv4Address(SERVER), Chain.SINGLE, layout)).packet
            self.assertEqual(reference.decode_dhcp_reply(dhcp.encode(packet)).option(252), bcd)

    def test_path_that_does_not_fit_the_file_field_is_refused(self):
        policy.Layout(bcd_uefi="a" * 127)
        with self.assertRaisesRegex(ValueError, "longer than 127 bytes"):
            policy.Layout(bcd_uefi="a" * 128)

    def test_path_that_is_not_ascii_is_refused(self):
        with self.assertRaisesRegex(ValueError, "not ASCII"):
            policy.Layout(bcd_bios="boot\\bïos.bcd")


class ParseTest(unittest.TestCase):
    def test_short_packet(self):
        with self.assertRaisesRegex(dhcp.MalformedPacket, "shorter than 240"):
            dhcp.parse(reference.dhcp_request()[:239])

    def test_wrong_magic_cookie(self):
        request = bytearray(reference.dhcp_request())
        request[236] = 0
        with self.assertRaisesRegex(dhcp.MalformedPacket, "magic cookie"):
            dhcp.parse(bytes(request))

    def test_truncated_option(self):
        request = reference.dhcp_request()[:240] + b"\x35\x01\x01\x61\x11\x00\x01"
        with self.assertRaisesRegex(dhcp.MalformedPacket, "option 97"):
            dhcp.parse(request)
        with self.assertRaisesRegex(dhcp.MalformedPacket, "option 97"):
            dhcp.parse(reference.dhcp_request()[:240] + b"\x61")

    def test_pad_is_skipped_and_end_stops_the_options(self):
        head = reference.dhcp_request()[:240]
        packet = dhcp.parse(head + b"\x00\x00\x35\x01\x03\x00\x5d\x02\x00\x07\xff\x61\x01\x00")
        self.assertEqual(packet.options, ((53, b"\x03"), (93, b"\x00\x07")))


class Option250Test(unittest.TestCase):
    def test_decode(self):
        self.assertEqual(wds250.decode(SECOND_REQUEST_250), ((1, b"\x00\x09"), (11, b"\x02")))

    def test_decode_keeps_bytes_that_do_not_parse(self):
        self.assertEqual(wds250.decode(b"\x01\x02\x00\x09\x0b\x04\x02"),
                         ((1, b"\x00\x09"), (-1, b"\x0b\x04\x02")))
        self.assertEqual(wds250.decode(b"\x01\x02\x00\x09\x0b"), ((1, b"\x00\x09"), (-1, b"\x0b")))
        self.assertEqual(wds250.decode(b""), ())

    def test_describe_a_second_request(self):
        self.assertEqual(wds250.describe(SECOND_REQUEST_250), "architecture(1)=0009,pxe_prompt(11)=02")

    def test_describe_unknown_and_unparsed_bytes(self):
        self.assertEqual(wds250.describe(b"\x07\x01\xaa\x0b\x04\x02"), "unknown(7)=aa,unparsed=0b0402")


if __name__ == "__main__":
    unittest.main()
