"""Loopback tests for the running server and the command line.
Run with: python3 -m unittest"""
import contextlib
import io
import os
import re
import resource
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock
from ipaddress import IPv4Address
from pathlib import Path

import events
import pxe_prototype
import reference
import tftp
from policy import Chain
from server import Config, PxeServer
from tftp import Limits

DISCOVER, OFFER, REQUEST, DECLINE, ACK, INFORM = 1, 2, 3, 4, 5, 8
HERE = Path(__file__).parent
BOOTMGFW = bytes(range(256)) * 20 + b"tail"
SMALL = b"s" * 1300
OTHER_INTERFACES = [name for _, name in socket.if_nameindex() if name != "lo"]


def log_fields(line: str) -> dict[str, str]:
    stamp, name, rest = line.rstrip("\n").split(" ", 2)
    fields = {"time": stamp, "event": name}
    for key, quoted, bare in re.findall(r'(\w+)=(?:"((?:[^"\\]|\\.)*)"|(\S*))', rest):
        fields[key] = quoted.replace('\\"', '"') if quoted else bare
    return fields


class ServerCase(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name).resolve()
        (self.root / "Boot" / "x64").mkdir(parents=True)
        (self.root / "Boot" / "x64" / "bootmgfw.efi").write_bytes(BOOTMGFW)
        (self.root / "small.bin").write_bytes(SMALL)
        (self.root / "even.bin").write_bytes(b"e" * 1024)
        self.log = io.StringIO()

    def start(self, chain: str = "two-stage", **changes) -> PxeServer:
        config = Config(
            server_ip=IPv4Address("127.0.0.1"), root=self.root, chain=Chain(chain),
            dhcp_bind="127.0.0.1", dhcp_ports=(0, 0), tftp_port=0,
            limits=Limits(timeout=0.2, retries=2), log_stream=self.log, **changes)
        pxe = PxeServer(config)
        pxe.start()
        self.addCleanup(pxe.stop)
        return pxe

    def client(self, timeout: float = 5.0) -> socket.socket:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.addCleanup(sock.close)
        sock.bind(("127.0.0.1", 0))
        sock.settimeout(timeout)
        return sock

    def events(self, name: str) -> list[dict[str, str]]:
        lines = [log_fields(line) for line in self.log.getvalue().splitlines()]
        return [fields for fields in lines if fields["event"] == name]

    def wait_for(self, name: str, count: int = 1) -> list[dict[str, str]]:
        deadline = time.monotonic() + 5.0
        while len(self.events(name)) < count and time.monotonic() < deadline:
            time.sleep(0.005)
        found = self.events(name)
        self.assertEqual(len(found), count, self.log.getvalue())
        return found

    def exchange(self, port: int, request: bytes) -> reference.DhcpReply:
        sock = self.client()
        sock.sendto(request, ("127.0.0.1", port))
        raw, source = sock.recvfrom(65535)
        self.assertEqual(source, ("127.0.0.1", port))
        return reference.decode_dhcp_reply(raw)


class TftpTest(ServerCase):
    def test_download_without_options(self):
        pxe = self.start()
        got = reference.tftp_get(pxe.tftp_address, "small.bin")
        self.assertEqual(got.data, SMALL)
        self.assertIsNone(got.oack)
        self.assertEqual(got.block_sizes, [512, 512, 276])

    def test_download_with_options(self):
        pxe = self.start()
        got = reference.tftp_get(pxe.tftp_address, r"\Boot\x64\bootmgfw.efi",
                                 {"blksize": "1024", "tsize": "0", "windowsize": "4"})
        self.assertEqual(got.data, BOOTMGFW)
        self.assertEqual(got.oack, {"blksize": "1024", "tsize": "5124", "windowsize": "4"})
        self.assertEqual(got.block_sizes, [1024, 1024, 1024, 1024, 1024, 4])

    def test_oack_states_the_clamped_values(self):
        pxe = self.start()
        got = reference.tftp_get(pxe.tftp_address, "small.bin", {"blksize": "65464", "windowsize": "64"})
        self.assertEqual(got.oack, {"blksize": "32768", "windowsize": "32"})
        self.assertEqual(got.data, SMALL)

    def test_size_on_a_block_boundary_ends_with_an_empty_block(self):
        pxe = self.start()
        got = reference.tftp_get(pxe.tftp_address, "EVEN.BIN")
        self.assertEqual(got.block_sizes, [512, 512, 0])
        self.assertEqual(got.data, b"e" * 1024)

    def test_refusals(self):
        pxe = self.start()
        cases = {
            "missing file": (reference.tftp_request("missing.bin"), 1),
            "directory": (reference.tftp_request("boot"), 1),
            "parent folder": (reference.tftp_request(r"..\small.bin"), 2),
            "write request": (reference.tftp_request("small.bin", opcode=2), 2),
            "blksize 7": (reference.tftp_request("small.bin", {"blksize": "7"}), 8),
            "not a request": (reference.tftp_ack(1), 4),
            "malformed": (b"\x00\x01small.bin\x00", 4),
            "4301 digits": (reference.tftp_request("small.bin", {"blksize": "0" * 4301}), 8),
        }
        for name, (packet, code) in cases.items():
            with self.subTest(name):
                sock = self.client()
                sock.sendto(packet, pxe.tftp_address)
                reply, source = sock.recvfrom(65535)
                self.assertEqual(source, pxe.tftp_address)
                self.assertEqual(reference.decode_tftp_error(reply)[0], code)

    def test_packet_from_another_port_gets_error_5_and_the_transfer_goes_on(self):
        pxe = self.start()
        sock, stranger = self.client(), self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        first, transfer_address = sock.recvfrom(65535)
        self.assertNotEqual(transfer_address, pxe.tftp_address)
        stranger.sendto(reference.tftp_ack(1), transfer_address)
        self.assertEqual(reference.decode_tftp_error(stranger.recvfrom(65535)[0])[0], 5)
        received = first[4:]
        for block in (1, 2, 3):
            sock.sendto(reference.tftp_ack(block), transfer_address)
            if block < 3:
                packet = sock.recvfrom(65535)[0]
                self.assertEqual(packet[:4], b"\x00\x03" + (block + 1).to_bytes(2, "big"))
                received += packet[4:]
        self.assertEqual(received, SMALL)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["outcome"], finished["retransmits"]), ("complete", "0"))

    def test_error_packet_gets_no_reply(self):
        pxe = self.start()
        sock, stranger = self.client(timeout=0.1), self.client(timeout=0.1)
        sock.sendto(b"\x00\x05\x00\x04Illegal TFTP operation\x00", pxe.tftp_address)
        with self.assertRaises(TimeoutError):
            sock.recvfrom(65535)
        received, = self.events("tftp-received")
        self.assertEqual((received["kind"], received["code"], received["message"]),
                         ("ERROR", "4", "Illegal TFTP operation"))
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        _, transfer_address = sock.recvfrom(65535)
        stranger.sendto(b"\x00\x05\x00\x05Unknown transfer ID\x00", transfer_address)
        with self.assertRaises(TimeoutError):
            stranger.recvfrom(65535)
        self.assertEqual(self.wait_for("tftp-ignored")[0]["reason"], "unknown-transfer-id")
        self.assertEqual(self.events("tftp-sent")[0]["kind"], "DATA")
        self.assertEqual([fields["kind"] for fields in self.events("tftp-sent") if fields["kind"] == "ERROR"], [])

    def test_bad_packet_from_the_client_is_ignored_and_the_transfer_goes_on(self):
        pxe = self.start()
        sock = self.client(timeout=0.1)
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        first, transfer_address = sock.recvfrom(65535)
        for packet in (b"\x00\x04\x00", b"\x00\x09\x00\x00", b"\x00\x03\x00\x01abc",
                       reference.tftp_request("small.bin")):
            sock.sendto(packet, transfer_address)
        with self.assertRaises(TimeoutError):
            sock.recvfrom(65535)
        self.assertEqual([fields["reason"] for fields in self.wait_for("tftp-ignored", 4)],
                         ["malformed", "malformed", "malformed", "request-on-transfer-port"])
        sock.settimeout(5.0)
        self.assertEqual(sock.recvfrom(65535)[0], first)
        received = first[4:]
        for block in (1, 2):
            sock.sendto(reference.tftp_ack(block), transfer_address)
            received += sock.recvfrom(65535)[0][4:]
        sock.sendto(reference.tftp_ack(3), transfer_address)
        self.assertEqual(received, SMALL)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["outcome"], finished["retransmits"]), ("complete", "1"))

    def test_unexpected_failure_in_one_request_does_not_stop_the_server(self):
        pxe = self.start()
        with mock.patch.object(tftp, "negotiate", side_effect=RuntimeError("boom")):
            sock = self.client(timeout=0.1)
            sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
            failure, = self.wait_for("internal-error")
        self.assertEqual((failure["where"], failure["error"]), ("dispatcher", "RuntimeError('boom')"))
        self.assertEqual(reference.tftp_get(pxe.tftp_address, "small.bin").data, SMALL)
        offer = self.exchange(pxe.dhcp_ports[0], reference.dhcp_request(DISCOVER, 7))
        self.assertEqual(offer.option(53), bytes((OFFER,)))

    def test_unexpected_failure_in_a_transfer_is_logged_as_its_outcome(self):
        pxe = self.start()
        sock = self.client()
        with mock.patch.object(tftp.Transfer, "on_ack", side_effect=RuntimeError("boom")):
            sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
            _, transfer_address = sock.recvfrom(65535)
            sock.sendto(reference.tftp_ack(1), transfer_address)
            finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["outcome"], finished["blocks_sent"]), ("error", "1"))
        failure, = self.events("internal-error")
        self.assertEqual((failure["where"], failure["error"]),
                         (f"transfer-127.0.0.1:{sock.getsockname()[1]}", "RuntimeError('boom')"))

    def test_repeated_request_does_not_start_a_second_transfer(self):
        pxe = self.start()
        sock = self.client(timeout=0.1)
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        sock.sendto(reference.tftp_request("SMALL.BIN"), pxe.tftp_address)
        first, transfer_address = sock.recvfrom(65535)
        self.assertEqual(first[:4], b"\x00\x03\x00\x01")
        with self.assertRaises(TimeoutError):
            sock.recvfrom(65535)
        self.assertEqual(len(self.events("tftp-ignored")), 1)
        self.assertEqual(self.events("tftp-ignored")[0]["reason"], "duplicate-request")
        sock.sendto(b"\x00\x05\x00\x00stop\x00", transfer_address)
        self.wait_for("transfer-finished")

    def test_request_right_after_a_transfer_ends_is_served(self):
        pxe = self.start()
        sock = self.client(timeout=1.0)
        for _ in range(100):
            sock.sendto(reference.tftp_request("small.bin", {"tsize": "0"}), pxe.tftp_address)
            oack, transfer_address = sock.recvfrom(65535)
            self.assertEqual(oack, b"\x00\x06tsize\x001300\x00")
            sock.sendto(b"\x00\x05\x00\x08size probe\x00", transfer_address)
        for _ in range(100):
            sock.sendto(reference.tftp_request("small.bin", {"blksize": "1300"}), pxe.tftp_address)
            oack, transfer_address = sock.recvfrom(65535)
            sock.sendto(reference.tftp_ack(0), transfer_address)
            self.assertEqual(sock.recvfrom(65535)[0], b"\x00\x03\x00\x01" + SMALL)
            sock.sendto(reference.tftp_ack(1), transfer_address)
            self.assertEqual(sock.recvfrom(65535)[0], b"\x00\x03\x00\x02")
            sock.sendto(reference.tftp_ack(2), transfer_address)
        self.wait_for("transfer-finished", 200)
        self.assertEqual(self.events("tftp-ignored"), [])

    def test_only_the_same_client_port_and_file_is_a_repeated_request(self):
        pxe = self.start()
        sock, other_port = self.client(), self.client()
        other_address = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.addCleanup(other_address.close)
        other_address.bind(("127.0.0.2", sock.getsockname()[1]))
        other_address.settimeout(5.0)
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        sock.sendto(reference.tftp_request("even.bin"), pxe.tftp_address)
        other_port.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        other_address.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        first_blocks = sorted(sock.recvfrom(65535)[0] for _ in range(2))
        self.assertEqual(first_blocks, sorted([b"\x00\x03\x00\x01" + SMALL[:512], b"\x00\x03\x00\x01" + b"e" * 512]))
        self.assertEqual(other_port.recvfrom(65535)[0], b"\x00\x03\x00\x01" + SMALL[:512])
        self.assertEqual(other_address.recvfrom(65535)[0], b"\x00\x03\x00\x01" + SMALL[:512])
        self.assertEqual(self.events("tftp-ignored"), [])

    def test_transfer_works_when_descriptor_numbers_pass_1024(self):
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        if hard != resource.RLIM_INFINITY and hard < 1200:
            self.skipTest("the descriptor limit is below 1200")
        if soft != resource.RLIM_INFINITY and soft < 1200:
            resource.setrlimit(resource.RLIMIT_NOFILE, (1200, hard))
            self.addCleanup(resource.setrlimit, resource.RLIMIT_NOFILE, (soft, hard))
        for _ in range(1030):
            self.addCleanup(os.close, os.open(os.devnull, os.O_RDONLY))
        pxe = self.start()
        got = reference.tftp_get(pxe.tftp_address, "small.bin", timeout=1.0)
        self.assertEqual(got.data, SMALL)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual(finished["outcome"], "complete")

    def test_replies_come_from_the_server_address(self):
        pxe = PxeServer(Config(
            server_ip=IPv4Address("127.0.0.2"), root=self.root, chain=Chain.SINGLE, dhcp_bind="127.0.0.2",
            dhcp_ports=(0,), tftp_port=0, log_stream=self.log))
        pxe.start()
        self.addCleanup(pxe.stop)
        self.assertEqual(pxe.tftp_address[0], "127.0.0.2")
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        self.assertEqual(sock.recvfrom(65535)[1][0], "127.0.0.2")
        sock.sendto(reference.dhcp_request(DISCOVER, 7), ("127.0.0.2", pxe.dhcp_ports[0]))
        self.assertEqual(sock.recvfrom(65535)[1], ("127.0.0.2", pxe.dhcp_ports[0]))

    def test_silent_client_times_out(self):
        pxe = self.start()
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        packets = [sock.recvfrom(65535)[0] for _ in range(3)]
        self.assertEqual(packets, [b"\x00\x03\x00\x01" + SMALL[:512]] * 3)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["outcome"], finished["retransmits"], finished["bytes_acked"]),
                         ("timeout", "2", "0"))
        self.assertGreaterEqual(float(finished["seconds"]), 0.6)

    def test_client_error_ends_the_transfer(self):
        pxe = self.start()
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        _, transfer_address = sock.recvfrom(65535)
        sock.sendto(b"\x00\x05\x00\x03Disk full\x00", transfer_address)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["outcome"], finished["client_error"]), ("client-error", "3 Disk full"))

    def test_stop_ends_a_transfer_in_progress(self):
        pxe = self.start()
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        sock.recvfrom(65535)
        began = time.monotonic()
        pxe.stop()
        self.assertLess(time.monotonic() - began, 1.5)
        self.assertEqual(self.events("transfer-finished")[0]["outcome"], "stopped")


class DhcpTest(ServerCase):
    def test_discover_on_each_port_gets_an_offer_at_the_sender(self):
        pxe = self.start()
        self.assertEqual(len(pxe.dhcp_ports), 2)
        for port in pxe.dhcp_ports:
            with self.subTest(port=port):
                offer = self.exchange(port, reference.dhcp_request(DISCOVER, 7))
                self.assertEqual(offer.option(53), bytes((OFFER,)))
                self.assertEqual(offer.siaddr, "127.0.0.1")
                self.assertEqual(offer.option(54), bytes((127, 0, 0, 1)))
                self.assertEqual(offer.xid, 0x1234ABCD)
                self.assertEqual(offer.file, bytes(128))

    def test_requests_that_get_no_reply_are_logged_with_the_reason(self):
        pxe = self.start()
        sock = self.client(timeout=0.1)
        sock.sendto(reference.dhcp_request(INFORM, 7), ("127.0.0.1", pxe.dhcp_ports[0]))
        sock.sendto(b"not dhcp", ("127.0.0.1", pxe.dhcp_ports[1]))
        with self.assertRaises(TimeoutError):
            sock.recvfrom(65535)
        ignored = self.wait_for("dhcp-ignored", 2)
        self.assertEqual({(fields["port"], fields["reason"]) for fields in ignored},
                         {(str(pxe.dhcp_ports[0]), "not-discover-or-request"),
                          (str(pxe.dhcp_ports[1]), "malformed")})

    def test_packet_types_are_logged_by_name(self):
        pxe = self.start()
        sock = self.client()
        for message_type in (DECLINE, 6, 7, INFORM, 9):
            sock.sendto(reference.dhcp_request(message_type, 7), ("127.0.0.1", pxe.dhcp_ports[0]))
        self.wait_for("dhcp-ignored", 5)
        self.assertEqual([fields["type"] for fields in self.events("dhcp-received")],
                         ["DECLINE", "NAK", "RELEASE", "INFORM", "9"])

    def test_request_without_a_guid_is_answered_when_the_config_says_so(self):
        pxe = self.start(answer_without_guid=True)
        ack = self.exchange(pxe.dhcp_ports[1], reference.dhcp_request(REQUEST, 0, None))
        self.assertEqual(ack.file.rstrip(b"\x00"), rb"boot\x64\wdsnbp.com")
        self.assertIsNone(ack.option(97))
        self.assertEqual(self.events("started")[0]["answer_without_guid"], "true")
        self.assertNotIn("guid", self.events("dhcp-received")[0])

    def test_reply_is_logged_with_its_port_and_xid(self):
        pxe = self.start()
        self.exchange(pxe.dhcp_ports[1], reference.dhcp_request(
            REQUEST, 7, xid=0xABCD, option_250=b"\x0b\x01\x02"))
        received, = self.events("dhcp-received")
        self.assertEqual((received["port"], received["xid"], received["options"]),
                         (str(pxe.dhcp_ports[1]), "0000abcd", "55,57,94,60"))
        sent, = self.wait_for("dhcp-sent")
        self.assertEqual((sent["port"], sent["xid"]), (str(pxe.dhcp_ports[1]), "0000abcd"))
        self.assertRegex(sent["destination"], r"^127\.0\.0\.1:\d+$")

    def test_reply_that_cannot_be_sent_is_logged_as_a_failure(self):
        pxe = self.start()
        sock = self.client()
        sock.connect(("127.0.0.1", pxe.dhcp_ports[0]))
        with mock.patch.object(socket.socket, "sendto", side_effect=OSError(101, "Network is unreachable")):
            sock.send(reference.dhcp_request(DISCOVER, 7))
            failed, = self.wait_for("send-failed")
        self.assertEqual(failed["destination"], "127.0.0.1:%d" % sock.getsockname()[1])
        self.assertEqual(failed["error"], "[Errno 101] Network is unreachable")
        self.assertEqual(self.events("dhcp-sent"), [])

    def test_two_stage_conversation_ends_with_the_boot_manager(self):
        pxe = self.start("two-stage")
        port_67, port_4011 = pxe.dhcp_ports
        offer = self.exchange(port_67, reference.dhcp_request(DISCOVER, 7))
        self.assertEqual(offer.option(53), bytes((OFFER,)))
        first = self.exchange(port_4011, reference.dhcp_request(REQUEST, 7))
        self.assertEqual(first.file.rstrip(b"\x00"), rb"boot\x64\wdsmgfw.efi")
        self.assertEqual(first.option(250), b"\x0b\x01\x02")
        second = self.exchange(port_4011, reference.dhcp_request(
            REQUEST, 7, option_250=bytes.fromhex("01020009" "0b0102")))
        self.assertEqual(second.option(53), bytes((ACK,)))
        self.assertEqual(second.file.rstrip(b"\x00"), rb"boot\x64\bootmgfw.efi")
        self.assertEqual(second.option(252), rb"boot\uefi.bcd")
        self.assertIsNone(second.option(250))
        got = reference.tftp_get(pxe.tftp_address, second.file.rstrip(b"\x00").decode(),
                                 {"tsize": "0", "blksize": "1456"})
        self.assertEqual(got.data, BOOTMGFW)

        received = self.events("dhcp-received")
        self.assertEqual([fields.get("option_250") for fields in received],
                         [None, None, "architecture(1)=0009,pxe_prompt(11)=02"])
        self.assertEqual(received[0] | {"time": "", "source": ""}, {
            "time": "", "event": "dhcp-received", "port": str(port_67), "source": "", "type": "DISCOVER",
            "xid": "1234abcd", "mac": "52:54:00:aa:bb:cc", "arch": "0007",
            "guid": "00101112131415161718191a1b1c1d1e1f", "options": "55,57,94,60"})
        sent = [{key: fields.get(key) for key in ("type", "firmware", "step", "file", "option_250", "option_252")}
                for fields in self.events("dhcp-sent")]
        self.assertEqual(sent, [
            {"type": "OFFER", "firmware": "uefi-x64", "step": None, "file": None,
             "option_250": None, "option_252": None},
            {"type": "ACK", "firmware": "uefi-x64", "step": "first", "file": r"boot\x64\wdsmgfw.efi",
             "option_250": "pxe_prompt(11)=02", "option_252": None},
            {"type": "ACK", "firmware": "uefi-x64", "step": "second", "file": r"boot\x64\bootmgfw.efi",
             "option_250": None, "option_252": r"boot\uefi.bcd"},
        ])

    @unittest.skipUnless(sys.platform == "linux", "needs IP_PKTINFO")
    def test_discover_that_arrives_on_the_configured_interface_is_answered(self):
        pxe = self.start(interface="lo")
        offer = self.exchange(pxe.dhcp_ports[0], reference.dhcp_request(DISCOVER, 7))
        self.assertEqual(offer.option(53), bytes((OFFER,)))
        self.assertEqual(self.events("started")[0]["interface"], "lo")

    @unittest.skipUnless(OTHER_INTERFACES and sys.platform == "linux", "needs a second interface")
    def test_discover_that_arrives_on_another_interface_is_ignored(self):
        pxe = self.start(interface=OTHER_INTERFACES[0])
        sock = self.client(timeout=0.1)
        sock.sendto(reference.dhcp_request(DISCOVER, 7), ("127.0.0.1", pxe.dhcp_ports[0]))
        with self.assertRaises(TimeoutError):
            sock.recvfrom(65535)
        ignored, = self.wait_for("dhcp-ignored")
        self.assertEqual(ignored["reason"], "other-interface")
        self.assertEqual(ignored["detail"], f"ifindex={socket.if_nametoindex('lo')}")
        self.assertEqual(self.events("dhcp-received"), [])

    def test_single_chain_names_the_boot_manager_in_the_first_ack(self):
        pxe = self.start("single")
        ack = self.exchange(pxe.dhcp_ports[0], reference.dhcp_request(REQUEST, 0))
        self.assertEqual(ack.file.rstrip(b"\x00"), rb"boot\x64\pxeboot.n12")
        self.assertEqual(ack.option(252), rb"boot\bios.bcd")


class LogTest(ServerCase):
    def test_started_line_says_what_the_run_tests(self):
        pxe = self.start("single")
        started, = self.events("started")
        del started["time"]
        self.assertEqual(started, {
            "event": "started", "chain": "single", "server_ip": "127.0.0.1", "root": str(self.root),
            "dhcp_bind": "127.0.0.1", "dhcp_ports": f"{pxe.dhcp_ports[0]},{pxe.dhcp_ports[1]}",
            "tftp": f"127.0.0.1:{pxe.tftp_address[1]}", "bcd_uefi": r"boot\uefi.bcd",
            "bcd_bios": r"boot\bios.bcd", "max_blksize": "32768", "max_windowsize": "32",
            "timeout": "0.200000", "retries": "2", "block_wrap_first": "0", "block_log": "true",
            "answer_without_guid": "false"})

    def test_started_line_shows_the_chain_and_the_limits(self):
        pxe = PxeServer(Config(
            server_ip=IPv4Address("127.0.0.1"), root=self.root, chain=Chain.TWO_STAGE, dhcp_bind="127.0.0.1",
            dhcp_ports=(0,), tftp_port=0, log_stream=self.log,
            limits=Limits(max_windowsize=8, timeout=1.5, retries=3, block_wrap_first=1)))
        pxe.start()
        self.addCleanup(pxe.stop)
        started, = self.events("started")
        self.assertEqual([started[key] for key in ("chain", "max_windowsize", "timeout", "retries", "block_wrap_first")],
                         ["two-stage", "8", "1.500000", "3", "1"])

    def test_transfer_time_and_terms_are_logged(self):
        pxe = self.start()
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin", {"windowsize": "4"}), pxe.tftp_address)
        oack, transfer_address = sock.recvfrom(65535)
        self.assertEqual(oack, b"\x00\x06windowsize\x004\x00")
        time.sleep(0.15)
        sock.sendto(reference.tftp_ack(0), transfer_address)
        for _ in range(3):
            sock.recvfrom(65535)
        sock.sendto(reference.tftp_ack(3), transfer_address)
        finished, = self.wait_for("transfer-finished")
        self.assertEqual((finished["windowsize"], finished["blksize"], finished["blocks_sent"]), ("4", "512", "3"))
        self.assertGreaterEqual(float(finished["seconds"]), 0.15)
        self.assertLess(float(finished["seconds"]), 1.0)
        self.assertEqual([fields["bytes"] for fields in self.events("tftp-sent") if fields["kind"] == "DATA"],
                         ["512", "512", "276"])

    def test_transfer_is_logged_packet_by_packet(self):
        pxe = self.start()
        reference.tftp_get(pxe.tftp_address, "Small.bin", {"blksize": "1024", "tsize": "0"})
        finished, = self.wait_for("transfer-finished")
        self.assertGreater(float(finished.pop("seconds")), 0.0)
        self.assertRegex(finished.pop("time"), r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}\+00:00$")
        client = finished.pop("client")
        self.assertEqual(finished, {
            "event": "transfer-finished", "file": "Small.bin", "path": str(self.root / "small.bin"),
            "size": "1300", "options": "blksize=1024,tsize=0", "blksize": "1024", "windowsize": "1",
            "blocks_sent": "2", "retransmits": "0", "bytes_acked": "1300", "outcome": "complete"})
        packets = [(fields["event"], fields["kind"], fields.get("block"), fields.get("options"))
                   for fields in self.events("tftp-received") + self.events("tftp-sent")
                   if fields["client"] == client]
        self.assertEqual(packets, [
            ("tftp-received", "RRQ", None, "blksize=1024,tsize=0"),
            ("tftp-received", "ACK", "0", None),
            ("tftp-received", "ACK", "1", None),
            ("tftp-received", "ACK", "2", None),
            ("tftp-sent", "OACK", None, "blksize=1024,tsize=1300"),
            ("tftp-sent", "DATA", "1", None),
            ("tftp-sent", "DATA", "2", None),
        ])

    def test_block_lines_can_be_left_out(self):
        pxe = self.start(log_blocks=False)
        reference.tftp_get(pxe.tftp_address, "missing.bin")
        reference.tftp_get(pxe.tftp_address, "small.bin", {"blksize": "1024"})
        self.wait_for("transfer-finished")
        kinds = [(fields["event"], fields["kind"]) for fields in self.events("tftp-received") + self.events("tftp-sent")]
        self.assertEqual(kinds, [("tftp-received", "RRQ"), ("tftp-received", "RRQ"),
                                 ("tftp-sent", "ERROR"), ("tftp-sent", "OACK")])
        self.assertEqual(self.events("started")[0]["block_log"], "false")

    def test_client_error_is_logged_when_block_lines_are_left_out(self):
        pxe = self.start(log_blocks=False)
        sock = self.client()
        sock.sendto(reference.tftp_request("small.bin"), pxe.tftp_address)
        _, transfer_address = sock.recvfrom(65535)
        sock.sendto(b"\x00\x05\x00\x03Disk full\x00", transfer_address)
        self.wait_for("transfer-finished")
        self.assertEqual([(fields["kind"], fields.get("message")) for fields in self.events("tftp-received")],
                         [("RRQ", None), ("ERROR", "Disk full")])

    def test_line_layout(self):
        self.assertEqual(
            events.format_line(events.TftpSent("192.0.2.7:1024", "ERROR", code=1, message="")),
            'tftp-sent client=192.0.2.7:1024 kind=ERROR code=1 message=""')
        self.assertEqual(
            events.format_line(events.DhcpIgnored(67, "0.0.0.0:68", "other-interface", "ifindex=3")),
            'dhcp-ignored port=67 source=0.0.0.0:68 reason=other-interface detail="ifindex=3"')

    def test_file_name_from_the_network_stays_on_one_line(self):
        pxe = self.start()
        reference.tftp_get(pxe.tftp_address, 'a b\n2026 started chain="x"')
        self.wait_for("tftp-sent")
        self.assertEqual(len(self.log.getvalue().splitlines()), 3)
        self.assertEqual(self.events("tftp-received")[0]["file"], 'a b\\u000a2026 started chain="x"')


class CommandLineTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name).resolve()
        self.required = ["--server-ip", "127.0.0.1", "--root", str(self.root), "--chain", "single",
                         "--interface", "lo"]

    def test_defaults(self):
        config = pxe_prototype.parse_args(self.required)
        self.assertEqual(config, Config(server_ip=IPv4Address("127.0.0.1"), root=self.root, chain=Chain.SINGLE,
                                        interface="lo"))
        self.assertEqual((config.dhcp_ports, config.tftp_port, config.dhcp_bind), ((67, 4011), 69, "0.0.0.0"))
        self.assertEqual((config.layout.bcd_uefi, config.layout.bcd_bios), (r"boot\uefi.bcd", r"boot\bios.bcd"))
        self.assertEqual((config.log_blocks, config.log_stream), (True, sys.stderr))
        self.assertIs(config.answer_without_guid, False)

    def test_every_flag(self):
        log_file = self.root / "run.log"
        config = pxe_prototype.parse_args([
            "--server-ip", "192.0.2.10", "--root", str(self.root), "--chain", "two-stage",
            "--interface", "br-lab", "--dhcp-ports", "4011", "--tftp-port", "6969",
            "--bcd-uefi", r"lab\a.bcd", "--bcd-bios", r"lab\b.bcd", "--no-block-log",
            "--answer-without-guid", "--log-file", str(log_file)])
        self.addCleanup(config.log_stream.close)
        self.assertEqual((config.server_ip, config.chain), (IPv4Address("192.0.2.10"), Chain.TWO_STAGE))
        self.assertEqual((config.interface, config.dhcp_ports, config.tftp_port), ("br-lab", (4011,), 6969))
        self.assertEqual((config.layout.bcd_uefi, config.layout.bcd_bios), (r"lab\a.bcd", r"lab\b.bcd"))
        self.assertIs(config.log_blocks, False)
        self.assertIs(config.answer_without_guid, True)
        self.assertEqual(config.log_stream.name, str(log_file))

    def test_bad_arguments_exit_with_a_message(self):
        cases = {
            "no chain": (self.required[:4] + self.required[6:], "--chain"),
            "no interface": (self.required[:6], "--interface"),
            "unknown chain": (self.required[:5] + ["three-stage"] + self.required[6:], "invalid choice"),
            "bad address": (["--server-ip", "lab-host"] + self.required[2:], "--server-ip"),
            "missing root": (self.required[:3] + [str(self.root / "none")] + self.required[4:], "is not a directory"),
            "root is a file": (self.required[:3] + [__file__] + self.required[4:], "is not a directory"),
            "port 65536": (self.required + ["--tftp-port", "65536"], "'65536' is not a UDP port"),
            "port in other digits": (self.required + ["--tftp-port", "\u0666\u0669"], "is not a UDP port"),
            "log file in a missing folder": (self.required + ["--log-file", str(self.root / "none" / "run.log")],
                                             "No such file or directory"),
            "bad port": (self.required + ["--dhcp-ports", "67,70000"], "'70000' is not a UDP port"),
            "repeated port": (self.required + ["--dhcp-ports", "67,4011,67"], "port 67 is listed twice"),
            "no address": (["--server-ip", "0.0.0.0"] + self.required[2:], "0.0.0.0 is not a host address"),
            "broadcast address": (["--server-ip", "255.255.255.255"] + self.required[2:],
                                  "255.255.255.255 is not a host address"),
            "multicast address": (["--server-ip", "224.0.0.1"] + self.required[2:], "224.0.0.1 is not a host address"),
            "long BCD path": (self.required + ["--bcd-uefi", "a" * 128], "longer than 127 bytes"),
        }
        for name, (argv, message) in cases.items():
            with self.subTest(name):
                stderr = io.StringIO()
                with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as exit_:
                    pxe_prototype.parse_args(argv)
                self.assertEqual(exit_.exception.code, 2)
                self.assertIn(message, stderr.getvalue())
                self.assertNotIn("Traceback", stderr.getvalue())

    def test_port_in_use_exits_with_a_message_that_names_it(self):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as busy:
            busy.bind(("127.0.0.1", 0))
            port = busy.getsockname()[1]
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                code = pxe_prototype.main(self.required + ["--dhcp-ports", "0", "--tftp-port", str(port)])
        self.assertEqual(code, 1)
        self.assertIn(f"cannot bind UDP 127.0.0.1:{port}: Address already in use", stderr.getvalue())

    def test_unknown_interface_exits_with_a_message_that_names_it(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            code = pxe_prototype.main(self.required[:7] + ["no-such-if0", "--dhcp-ports", "0", "--tftp-port", "0"])
        self.assertEqual(code, 1)
        self.assertIn("no interface named no-such-if0", stderr.getvalue())

    def test_highest_port_is_accepted(self):
        config = pxe_prototype.parse_args(self.required + ["--tftp-port", "65535", "--dhcp-ports", "65535"])
        self.assertEqual((config.tftp_port, config.dhcp_ports), (65535, (65535,)))

    def test_runs_until_interrupted_or_terminated(self):
        for signal_number in (signal.SIGINT, signal.SIGTERM):
            with self.subTest(signal=signal_number.name):
                process = subprocess.Popen(
                    [sys.executable, str(HERE / "pxe_prototype.py"), *self.required,
                     "--dhcp-ports", "0", "--tftp-port", "0"],
                    stderr=subprocess.PIPE, text=True)
                self.addCleanup(process.stderr.close)
                self.addCleanup(process.kill)
                # A server that never logs must fail the test, not hang it.
                watchdog = threading.Timer(10.0, process.kill)
                watchdog.start()
                self.addCleanup(watchdog.cancel)
                started = log_fields(process.stderr.readline())
                self.assertEqual((started["event"], started["chain"]), ("started", "single"))
                process.send_signal(signal_number)
                self.assertEqual(process.wait(timeout=5), 0)
                self.assertEqual(process.stderr.read(), "")

    def test_log_file_keeps_the_lines_of_earlier_runs(self):
        log_file = self.root / "run.log"
        for lines in (1, 2):
            config = pxe_prototype.parse_args(
                self.required + ["--dhcp-ports", "0", "--tftp-port", "0", "--log-file", str(log_file)])
            with PxeServer(config):
                self.assertEqual(len(log_file.read_text().splitlines()), lines)
            config.log_stream.close()
        self.assertEqual([log_fields(line)["event"] for line in log_file.read_text().splitlines()],
                         ["started", "started"])


if __name__ == "__main__":
    unittest.main()
