"""Tests for the TFTP protocol code. Run with: python3 -m unittest"""
import collections
import tempfile
import unittest
from pathlib import Path

import tftp
from tftp import Limits, Negotiated, Outcome, Phase, TftpError, Transfer

CONTENT = bytes(range(256)) * 40


def data(block: int, payload: bytes) -> bytes:
    return b"\x00\x03" + block.to_bytes(2, "big") + payload


def chunk(block: int, blksize: int, size: int) -> bytes:
    return CONTENT[:size][(block - 1) * blksize:block * blksize]


def transfer(size: int, blksize: int = 512, windowsize: int = 1, oack=(), now: float = 100.0,
             limits: Limits = Limits()) -> Transfer:
    return Transfer(lambda block: chunk(block, blksize, size), size,
                    Negotiated(blksize, windowsize, oack), limits, now)


class ParseTest(unittest.TestCase):
    def test_read_request_keeps_the_options_as_sent(self):
        request = tftp.parse(b"\x00\x01boot\\BCD\x00octet\x00BlkSize\x001456\x00tsize\x000\x00")
        self.assertEqual(request, tftp.Request(
            tftp.Opcode.RRQ, "boot\\BCD", "octet", (("BlkSize", "1456"), ("tsize", "0"))))

    def test_write_request(self):
        self.assertEqual(tftp.parse(b"\x00\x02a\x00octet\x00"),
                         tftp.Request(tftp.Opcode.WRQ, "a", "octet", ()))

    def test_ack_and_error(self):
        self.assertEqual(tftp.parse(b"\x00\x04\xff\xfe"), tftp.Ack(65534))
        self.assertEqual(tftp.parse(b"\x00\x05\x00\x08bad option\x00"),
                         tftp.ClientError(8, "bad option"))

    def test_malformed_packets_are_illegal_operations(self):
        packets = {
            "short": b"\x00\x01",
            "no mode": b"\x00\x01file\x00",
            "no final NUL": b"\x00\x01file\x00octet",
            "option without a value": b"\x00\x01file\x00octet\x00blksize\x00",
            "data from a client": b"\x00\x03\x00\x01abc",
            "oack from a client": b"\x00\x06blksize\x00512\x00",
            "unknown opcode": b"\x00\x09\x00\x00",
        }
        for name, packet in packets.items():
            with self.subTest(name):
                with self.assertRaises(TftpError) as raised:
                    tftp.parse(packet)
                self.assertEqual(raised.exception.code, 4)

    def test_error_packet(self):
        self.assertEqual(tftp.error_packet(TftpError(1, "File not found")),
                         b"\x00\x05\x00\x01File not found\x00")


class NegotiateTest(unittest.TestCase):
    def test_values_within_the_limits_are_granted(self):
        terms = tftp.negotiate((("blksize", "1456"), ("windowsize", "8")), 5000, Limits())
        self.assertEqual(terms, Negotiated(1456, 8, (("blksize", "1456"), ("windowsize", "8"))))

    def test_blksize_and_windowsize_clamp_before_the_oack(self):
        terms = tftp.negotiate((("blksize", "65464"), ("windowsize", "64")), 5000, Limits())
        self.assertEqual(terms, Negotiated(32768, 32, (("blksize", "32768"), ("windowsize", "32"))))

    def test_smallest_values(self):
        terms = tftp.negotiate((("blksize", "8"), ("windowsize", "1")), 5000, Limits())
        self.assertEqual(terms, Negotiated(8, 1, (("blksize", "8"), ("windowsize", "1"))))

    def test_refused_values(self):
        for options in ((("blksize", "7"),), (("blksize", "big"),), (("blksize", "-512"),),
                        (("windowsize", "0"),), (("windowsize", ""),), (("windowsize", "4.5"),)):
            with self.subTest(options=options):
                with self.assertRaises(TftpError) as raised:
                    tftp.negotiate(options, 5000, Limits())
                self.assertEqual(raised.exception.code, 8)

    def test_value_of_any_length_is_clamped(self):
        terms = tftp.negotiate((("blksize", "0" * 5000 + "1024"), ("windowsize", "9" * 4301)), 5000, Limits())
        self.assertEqual(terms, Negotiated(1024, 32, (("blksize", "1024"), ("windowsize", "32"))))
        terms = tftp.negotiate((("blksize", "12345678901"),), 5000, Limits())
        self.assertEqual(terms, Negotiated(32768, 1, (("blksize", "32768"),)))

    def test_option_name_that_is_not_ascii_is_unknown(self):
        kelvin = "bl\u212asize"
        self.assertEqual(kelvin.lower(), "blksize")
        self.assertEqual(tftp.negotiate(((kelvin, "1024"),), 77, Limits()), Negotiated(512, 1, ()))

    def test_tsize_is_answered_with_the_file_size(self):
        terms = tftp.negotiate((("tsize", "0"),), 5_000_000_000, Limits())
        self.assertEqual(terms, Negotiated(512, 1, (("tsize", "5000000000"),)))

    def test_option_names_ignore_case(self):
        terms = tftp.negotiate((("BLKSIZE", "1024"), ("TSize", "0")), 77, Limits())
        self.assertEqual(terms, Negotiated(1024, 1, (("BLKSIZE", "1024"), ("TSize", "77"))))

    def test_unknown_options_are_dropped(self):
        terms = tftp.negotiate((("timeout", "3"), ("blksize", "1024"), ("msftwindow", "31416")),
                               77, Limits())
        self.assertEqual(terms, Negotiated(1024, 1, (("blksize", "1024"),)))

    def test_no_known_option_means_no_oack(self):
        self.assertEqual(tftp.negotiate((), 77, Limits()), Negotiated(512, 1, ()))
        self.assertEqual(tftp.negotiate((("timeout", "3"),), 77, Limits()), Negotiated(512, 1, ()))

    def test_windowsize_limit_comes_from_the_limits(self):
        terms = tftp.negotiate((("windowsize", "16"),), 77, Limits(max_windowsize=4))
        self.assertEqual(terms, Negotiated(512, 4, (("windowsize", "4"),)))


class ResolveTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        (self.root / "Boot" / "x64").mkdir(parents=True)
        (self.root / "Boot" / "x64" / "BootMgfw.EFI").write_bytes(b"efi")
        (self.root / "Boot" / "name.txt").write_bytes(b"lower")
        (self.root / "Boot" / "NAME.TXT").write_bytes(b"upper")

    def assert_refused(self, filename, code):
        with self.assertRaises(TftpError) as raised:
            tftp.resolve(self.root, filename)
        self.assertEqual(raised.exception.code, code)

    def test_case_and_separators_are_normalized(self):
        expected = self.root / "Boot" / "x64" / "BootMgfw.EFI"
        for filename in (r"Boot\x64\BootMgfw.EFI", r"boot\X64\bootmgfw.efi", r"\boot\x64\bootmgfw.efi",
                         r"\\boot\x64\bootmgfw.efi", "/boot/x64/bootmgfw.efi", r"boot\\x64/bootmgfw.efi"):
            with self.subTest(filename):
                self.assertEqual(tftp.resolve(self.root, filename), expected)

    def test_exact_spelling_wins(self):
        self.assertEqual(tftp.resolve(self.root, r"boot\name.txt").read_bytes(), b"lower")
        self.assertEqual(tftp.resolve(self.root, r"boot\NAME.TXT").read_bytes(), b"upper")

    def test_escapes_are_access_violations(self):
        for filename in (r"..\secret", r"boot\..\boot\name.txt", r"boot\x64\..", "../etc/passwd",
                         r"c:\boot\name.txt", r"boot\name.txt:stream", "boot\\name.txt\x00.efi"):
            with self.subTest(filename):
                self.assert_refused(filename, 2)

    def test_missing_names_are_not_found(self):
        for filename in (r"boot\x64\missing.efi", r"nowhere\name.txt", r"boot\x64", "boot", "",
                         "\\", r"boot\name.txt\below", r"boot\.\name.txt"):
            with self.subTest(filename):
                self.assert_refused(filename, 1)


class TransferTest(unittest.TestCase):
    def test_without_options_data_goes_first(self):
        t = transfer(1000)
        self.assertEqual(t.start(100.0), [data(1, CONTENT[:512])])
        self.assertEqual(t.on_ack(1, 100.1), [data(2, CONTENT[512:1000])])
        self.assertEqual(t.on_ack(2, 100.2), [])
        self.assertEqual((t.phase, t.outcome), (Phase.DONE, Outcome.COMPLETE))
        self.assertEqual((t.blocks_sent, t.retransmits, t.bytes_acked), (2, 0, 1000))

    def test_oack_waits_for_ack_0(self):
        t = transfer(1000, oack=(("blksize", "512"), ("tsize", "1000")))
        self.assertEqual(t.start(100.0), [b"\x00\x06blksize\x00512\x00tsize\x001000\x00"])
        self.assertEqual(t.phase, Phase.AWAIT_OACK_ACK)
        self.assertEqual(t.on_ack(1, 100.1), [])
        self.assertEqual(t.on_ack(0, 100.2), [data(1, CONTENT[:512])])
        self.assertEqual(t.phase, Phase.SENDING)

    def test_ack_0_after_an_oack_starts_a_window(self):
        t = transfer(4000, blksize=512, windowsize=4, oack=(("windowsize", "4"),))
        t.start(100.0)
        self.assertEqual(t.on_ack(0, 100.1), [data(n, chunk(n, 512, 4000)) for n in (1, 2, 3, 4)])
        self.assertEqual(t.on_ack(4, 100.2), [data(n, chunk(n, 512, 4000)) for n in (5, 6, 7, 8)])
        self.assertEqual(len(chunk(8, 512, 4000)), 416)
        self.assertEqual(t.on_ack(8, 100.3), [])
        self.assertEqual((t.phase, t.blocks_sent, t.retransmits, t.bytes_acked), (Phase.DONE, 8, 0, 4000))

    def test_size_on_a_block_boundary_ends_with_an_empty_block(self):
        t = transfer(1024)
        self.assertEqual(t.start(100.0), [data(1, CONTENT[:512])])
        self.assertEqual(t.on_ack(1, 100.1), [data(2, CONTENT[512:1024])])
        self.assertEqual(t.on_ack(2, 100.2), [data(3, b"")])
        self.assertEqual(t.phase, Phase.SENDING)
        self.assertEqual(t.on_ack(3, 100.3), [])
        self.assertEqual((t.phase, t.bytes_acked), (Phase.DONE, 1024))

    def test_empty_file_is_one_empty_block(self):
        t = transfer(0)
        self.assertEqual(t.start(100.0), [data(1, b"")])
        self.assertEqual(t.on_ack(1, 100.1), [])
        self.assertEqual((t.phase, t.outcome, t.bytes_acked), (Phase.DONE, Outcome.COMPLETE, 0))

    def test_ack_inside_the_window_resends_from_the_next_block(self):
        t = transfer(8000, windowsize=4)
        t.start(100.0)
        self.assertEqual(t.on_ack(2, 100.1), [data(n, chunk(n, 512, 8000)) for n in (3, 4, 5, 6)])
        self.assertEqual((t.blocks_sent, t.retransmits), (8, 2))

    def test_repeated_ack_without_a_window_sends_nothing(self):
        t = transfer(8000)
        t.start(100.0)
        self.assertEqual(t.on_ack(1, 100.1), [data(2, chunk(2, 512, 8000))])
        self.assertEqual(t.on_ack(1, 100.2), [])
        self.assertEqual((t.blocks_sent, t.retransmits), (2, 0))

    def test_repeated_ack_with_a_window_resends_the_window_once(self):
        t = transfer(8000, windowsize=4)
        t.start(100.0)
        window = [data(n, chunk(n, 512, 8000)) for n in (5, 6, 7, 8)]
        self.assertEqual(t.on_ack(4, 100.1), window)
        self.assertEqual(t.on_ack(4, 100.2), window)
        self.assertEqual(t.on_ack(4, 100.3), [])
        self.assertEqual((t.blocks_sent, t.retransmits), (12, 4))

    def test_repeated_ack_that_follows_a_resend_sends_nothing(self):
        t = transfer(8000, windowsize=4)
        t.start(100.0)
        t.on_ack(4, 100.1)
        t.on_ack(4, 100.2)
        self.assertEqual(t.on_ack(8, 100.3), [data(n, chunk(n, 512, 8000)) for n in (9, 10, 11, 12)])
        self.assertEqual(t.on_ack(8, 100.4), [])
        last_window = [data(n, chunk(n, 512, 8000)) for n in (13, 14, 15, 16)]
        self.assertEqual(t.on_ack(12, 100.5), last_window)
        self.assertEqual(t.on_ack(12, 100.6), last_window)
        self.assertEqual((t.blocks_sent, t.retransmits), (24, 8))

    def test_repeated_ack_after_a_timeout_resend_sends_nothing(self):
        t = transfer(8000, windowsize=4)
        first = t.start(100.0)
        self.assertEqual(t.on_timeout(105.0), first)
        self.assertEqual(t.on_ack(4, 105.1), [data(n, chunk(n, 512, 8000)) for n in (5, 6, 7, 8)])
        self.assertEqual(t.on_ack(4, 105.2), [])

    def blocks_sent_when_block_2_is_lost_once(self, windowsize: int, acks_per_gap: int) -> int:
        """A receiver that gets every packet in order except the first copy
        of block 2. It answers the first `acks_per_gap` out-of-order blocks
        in a row with an ACK for its last good block."""
        t = transfer(10100, windowsize=windowsize)
        wire = collections.deque(t.start(0.0))
        expected, in_window, gap_acks, lost = 1, 0, 0, False
        while wire:
            self.assertLess(t.blocks_sent, 1000, "retransmit storm")
            block = int.from_bytes(wire.popleft()[2:4], "big")
            if block == 2 and not lost:
                lost = True
            elif block != expected:
                in_window = 0
                if gap_acks < acks_per_gap:
                    gap_acks += 1
                    wire.extend(t.on_ack(expected - 1, 0.0))
            else:
                expected, in_window, gap_acks = expected + 1, in_window + 1, 0
                if expected > t.last_block or in_window == windowsize:
                    in_window = 0
                    wire.extend(t.on_ack(expected - 1, 0.0))
        self.assertEqual((t.phase, t.bytes_acked), (Phase.DONE, 10100))
        return t.blocks_sent

    def test_one_lost_block_costs_about_one_window(self):
        self.assertEqual(transfer(10100).last_block, 20)
        for windowsize, acks_per_gap, blocks_sent in ((4, 1, 23), (4, 99, 23), (8, 1, 27), (8, 99, 27)):
            with self.subTest(windowsize=windowsize, acks_per_gap=acks_per_gap):
                self.assertEqual(self.blocks_sent_when_block_2_is_lost_once(windowsize, acks_per_gap), blocks_sent)

    def test_ack_for_a_block_not_sent_is_ignored(self):
        t = transfer(8000, windowsize=4)
        t.start(100.0)
        self.assertEqual(t.on_ack(5, 100.1), [])
        self.assertEqual(t.on_ack(65535, 100.1), [])
        self.assertEqual(t.acked, 0)

    def test_nothing_is_resent_before_the_deadline(self):
        t = transfer(8000)
        t.start(100.0)
        self.assertEqual(t.on_timeout(104.9), [])
        self.assertEqual(t.on_timeout(105.0), [data(1, CONTENT[:512])])
        self.assertEqual(t.on_timeout(109.9), [])
        self.assertEqual(t.retransmits, 1)

    def test_stray_ack_does_not_move_the_deadline(self):
        t = transfer(8000)
        t.start(100.0)
        self.assertEqual(t.on_ack(9, 104.0), [])
        self.assertEqual(t.on_timeout(105.0), [data(1, CONTENT[:512])])

    def test_silent_client_fails_the_transfer_after_five_resends(self):
        t = transfer(8000, windowsize=2)
        first = t.start(100.0)
        self.assertEqual(first, [data(1, CONTENT[:512]), data(2, CONTENT[512:1024])])
        for now in (105.0, 110.0, 115.0, 120.0, 125.0):
            self.assertEqual(t.on_timeout(now), first)
        self.assertEqual(t.phase, Phase.SENDING)
        self.assertEqual(t.on_timeout(130.0), [])
        self.assertEqual((t.phase, t.outcome), (Phase.FAILED, Outcome.TIMEOUT))
        self.assertEqual((t.blocks_sent, t.retransmits, t.bytes_acked), (12, 10, 0))
        self.assertEqual(t.on_ack(2, 130.1), [])

    def test_unanswered_oack_is_resent_then_fails(self):
        t = transfer(8000, oack=(("tsize", "8000"),))
        oack = [b"\x00\x06tsize\x008000\x00"]
        self.assertEqual(t.start(100.0), oack)
        for now in (105.0, 110.0, 115.0, 120.0, 125.0):
            self.assertEqual(t.on_timeout(now), oack)
        self.assertEqual(t.on_timeout(130.0), [])
        self.assertEqual((t.phase, t.outcome, t.retransmits), (Phase.FAILED, Outcome.TIMEOUT, 5))

    def test_new_ack_gives_back_every_retry(self):
        t = transfer(8000)
        t.start(100.0)
        for now in (105.0, 110.0, 115.0, 120.0, 125.0):
            t.on_timeout(now)
        self.assertEqual(t.on_ack(1, 126.0), [data(2, chunk(2, 512, 8000))])
        for now in (131.0, 136.0, 141.0, 146.0, 151.0):
            self.assertEqual(t.on_timeout(now), [data(2, chunk(2, 512, 8000))])
        self.assertEqual(t.phase, Phase.SENDING)

    def test_shorter_timeout_comes_from_the_limits(self):
        t = transfer(8000, limits=Limits(timeout=0.5, retries=1))
        t.start(100.0)
        self.assertEqual(t.on_timeout(100.4), [])
        self.assertEqual(t.on_timeout(100.5), [data(1, CONTENT[:512])])
        self.assertEqual(t.on_timeout(101.0), [])
        self.assertEqual(t.outcome, Outcome.TIMEOUT)

    def test_client_error_fails_the_transfer(self):
        t = transfer(8000)
        t.start(100.0)
        t.on_error(3, "Disk full")
        self.assertEqual((t.phase, t.outcome), (Phase.FAILED, Outcome.CLIENT_ERROR))
        self.assertEqual(t.client_error, tftp.ClientError(3, "Disk full"))
        self.assertEqual(t.on_ack(1, 100.1), [])
        self.assertEqual(t.on_timeout(200.0), [])

    def test_abort_fails_the_transfer(self):
        t = transfer(8000)
        t.start(100.0)
        t.abort()
        self.assertEqual((t.phase, t.outcome), (Phase.FAILED, Outcome.ERROR))
        self.assertEqual(Outcome.ERROR.value, "error")
        self.assertEqual(t.on_timeout(200.0), [])

    def wire_numbers_around_the_wrap(self, limits):
        """Sends 65540 blocks of 8 bytes, 4 to a window. Each block holds its
        own number, so the test maps wire numbers to blocks without help
        from the code under test."""
        size = 8 * 65539 + 3
        t = Transfer(lambda block: block.to_bytes(8, "big")[:size - 8 * (block - 1)], size,
                     Negotiated(8, 4, ()), limits, 0.0)
        wire = {}
        window = t.start(0.0)
        while window:
            for packet in window:
                wire[int.from_bytes(packet[4:].ljust(8, b"\x00"), "big")] = int.from_bytes(packet[2:4], "big")
            window = t.on_ack(int.from_bytes(window[-1][2:4], "big"), 0.0)
        self.assertEqual((t.phase, t.acked, t.bytes_acked), (Phase.DONE, 65540, size))
        return wire

    def test_block_numbers_wrap_to_0(self):
        self.assertEqual(tftp.BLOCK_WRAP_FIRST, 0)
        wire = self.wire_numbers_around_the_wrap(Limits())
        self.assertEqual([wire[block] for block in (65534, 65535, 65536, 65537, 65539)],
                         [65534, 65535, 0, 1, 3])

    def test_block_numbers_can_wrap_to_1(self):
        wire = self.wire_numbers_around_the_wrap(Limits(block_wrap_first=1))
        self.assertEqual([wire[block] for block in (65534, 65535, 65536, 65537, 65539)],
                         [65534, 65535, 1, 2, 4])


if __name__ == "__main__":
    unittest.main()
