"""Tests on a network shaped like the lab host. They need root, Linux, and
the `ip` command. Run with: sudo python3 -m unittest test_lab

One namespace is the lab host. Its bridge has two addresses, and its
default route leaves through another link. A socket set up the way dnsmasq
sets up its own in `bind-dynamic` mode shares UDP port 67 with the
prototype. A second namespace is the PC. Its end of the cable to the
bridge has no address, as before a lease.
"""
import contextlib
import io
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from ipaddress import IPv4Address
from pathlib import Path

import reference
from policy import Chain
from server import Config, PxeServer
from test_server import log_fields

DISCOVER, OFFER, REQUEST = 1, 2, 3
HOST, PC = f"pxt{os.getpid()}h", f"pxt{os.getpid()}p"
BRIDGE, BRIDGE_PORT, PC_LINK, UPLINK, UPLINK_PEER = "pxbr", "pxbrp", "pxpc", "pxup", "pxuppeer"
PRIMARY, SECONDARY, PC_ADDRESS = "10.77.0.1", "10.77.0.2", "10.77.0.50"
USABLE = (sys.platform == "linux" and hasattr(os, "setns") and hasattr(os, "geteuid")
          and os.geteuid() == 0 and shutil.which("ip") is not None)


def ip(*arguments: str) -> None:
    subprocess.run(["ip", *arguments], check=True, capture_output=True, text=True)


@contextlib.contextmanager
def inside(namespace: str):
    """Sockets made in this block belong to `namespace` for their lifetime."""
    home = os.open("/proc/self/ns/net", os.O_RDONLY)
    target = os.open(f"/run/netns/{namespace}", os.O_RDONLY)
    try:
        os.setns(target, os.CLONE_NEWNET)
        yield
    finally:
        os.setns(home, os.CLONE_NEWNET)
        os.close(target)
        os.close(home)


def udp_destination_port(frame: bytes) -> int | None:
    """The UDP destination port of an Ethernet frame that holds IPv4 and UDP."""
    if frame[12:14] != b"\x08\x00" or frame[23] != 17:
        return None
    udp = 14 + (frame[14] & 15) * 4
    return int.from_bytes(frame[udp + 2:udp + 4], "big")


@unittest.skipUnless(USABLE, "needs root, Linux, Python 3.12, and the ip command")
class LabNetworkTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.addClassCleanup(subprocess.run, ["ip", "netns", "del", HOST], capture_output=True)
        cls.addClassCleanup(subprocess.run, ["ip", "netns", "del", PC], capture_output=True)
        ip("netns", "add", HOST)
        ip("netns", "add", PC)
        ip("link", "add", BRIDGE_PORT, "netns", HOST, "type", "veth", "peer", "name", PC_LINK, "netns", PC)
        ip("-n", HOST, "link", "add", BRIDGE, "type", "bridge", "forward_delay", "0", "stp_state", "0")
        ip("-n", HOST, "link", "set", BRIDGE_PORT, "master", BRIDGE)
        ip("-n", HOST, "link", "add", UPLINK, "type", "veth", "peer", "name", UPLINK_PEER)
        ip("-n", HOST, "addr", "add", f"{PRIMARY}/24", "dev", BRIDGE)
        ip("-n", HOST, "addr", "add", f"{SECONDARY}/24", "dev", BRIDGE)
        ip("-n", HOST, "addr", "add", "10.78.0.1/24", "dev", UPLINK)
        for link in ("lo", BRIDGE_PORT, BRIDGE, UPLINK, UPLINK_PEER):
            ip("-n", HOST, "link", "set", link, "up")
        ip("-n", HOST, "route", "add", "default", "via", "10.78.0.254", "dev", UPLINK)
        for link in ("lo", PC_LINK):
            ip("-n", PC, "link", "set", link, "up")
        folder = tempfile.TemporaryDirectory()
        cls.addClassCleanup(folder.cleanup)
        cls.root = Path(folder.name)
        # A frame sent before the bridge port forwards is lost.
        time.sleep(1.0)

    def setUp(self):
        self.log = io.StringIO()
        with inside(HOST):
            # dnsmasq sets these three options on its DHCP socket in
            # bind-dynamic mode (src/dhcp.c, dhcp_common.c).
            self.dhcp_server = self.udp_socket(socket.SO_REUSEADDR, socket.SO_REUSEPORT)
            self.dhcp_server.setsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, BRIDGE.encode())
            self.dhcp_server.bind(("0.0.0.0", 67))
            self.uplink = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(0x0800))
            self.addCleanup(self.uplink.close)
            self.uplink.bind((UPLINK_PEER, 0))
            self.uplink.settimeout(0.3)
        with inside(PC):
            self.pc = self.udp_socket(socket.SO_REUSEADDR, socket.SO_BROADCAST)
            self.pc.setsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, PC_LINK.encode())
            self.pc.setsockopt(socket.IPPROTO_IP, socket.IP_PKTINFO, 1)
            self.pc.bind(("0.0.0.0", 68))

    def udp_socket(self, *options: int) -> socket.socket:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.addCleanup(sock.close)
        for option in options:
            sock.setsockopt(socket.SOL_SOCKET, option, 1)
        sock.settimeout(0.3)
        return sock

    def start(self, server_ip: str = PRIMARY) -> PxeServer:
        with inside(HOST):
            pxe = PxeServer(Config(
                server_ip=IPv4Address(server_ip), root=self.root, chain=Chain.TWO_STAGE,
                interface=BRIDGE, dhcp_ports=(67, 4011), tftp_port=0, log_stream=self.log))
        pxe.start()
        self.addCleanup(pxe.stop)
        return pxe

    def events(self, name: str) -> list[dict[str, str]]:
        lines = [log_fields(line) for line in self.log.getvalue().splitlines()]
        return [fields for fields in lines if fields["event"] == name]

    def receive_at_the_pc(self) -> tuple[reference.DhcpReply, tuple[str, int], str]:
        """A reply, its source, and the address it was sent to."""
        raw, ancillary, _, source = self.pc.recvmsg(65535, socket.CMSG_SPACE(12))
        sent_to, = [socket.inet_ntoa(data[8:12]) for level, kind, data in ancillary
                    if (level, kind) == (socket.IPPROTO_IP, socket.IP_PKTINFO)]
        return reference.decode_dhcp_reply(raw), source, sent_to

    def test_offer_reaches_a_pc_without_an_address_as_a_broadcast_on_the_bridge(self):
        self.start()
        self.pc.sendto(reference.dhcp_request(DISCOVER, 7), ("255.255.255.255", 67))
        offer, source, sent_to = self.receive_at_the_pc()
        self.assertEqual(offer.option(53), bytes((OFFER,)))
        self.assertEqual((source, sent_to), ((PRIMARY, 67), "255.255.255.255"))
        self.assertEqual(self.dhcp_server.recvfrom(65535)[1], ("0.0.0.0", 68))
        with self.assertRaises(TimeoutError):
            while udp_destination_port(self.uplink.recv(65535)) != 68:
                pass
        sent, = self.events("dhcp-sent")
        self.assertEqual((sent["port"], sent["destination"]), ("67", "255.255.255.255:68"))

    def test_reply_comes_from_the_server_address_when_the_bridge_has_two(self):
        self.start(SECONDARY)
        self.pc.sendto(reference.dhcp_request(DISCOVER, 7), ("255.255.255.255", 67))
        offer, source, _ = self.receive_at_the_pc()
        self.assertEqual(offer.option(54), bytes((10, 77, 0, 2)))
        self.assertEqual(source, (SECONDARY, 67))

    def test_lease_request_gets_no_reply(self):
        self.start()
        self.pc.sendto(reference.dhcp_request(REQUEST, 7), ("255.255.255.255", 67))
        with self.assertRaises(TimeoutError):
            self.pc.recvmsg(65535)
        ignored, = self.events("dhcp-ignored")
        self.assertEqual((ignored["port"], ignored["reason"]), ("67", "lease-request"))

    def test_unicast_to_port_67_goes_to_the_dhcp_server_alone(self):
        self.start()
        ip("-n", PC, "addr", "add", f"{PC_ADDRESS}/24", "dev", PC_LINK)
        self.addCleanup(ip, "-n", PC, "addr", "del", f"{PC_ADDRESS}/24", "dev", PC_LINK)
        for _ in range(6):
            self.pc.sendto(reference.dhcp_request(REQUEST, 7), (PRIMARY, 67))
        for _ in range(6):
            self.assertEqual(self.dhcp_server.recvfrom(65535)[1], (PC_ADDRESS, 68))
        self.pc.sendto(reference.dhcp_request(REQUEST, 7), (PRIMARY, 4011))
        ack, source, sent_to = self.receive_at_the_pc()
        self.assertEqual(ack.file.rstrip(b"\x00"), rb"boot\x64\wdsmgfw.efi")
        self.assertEqual((source, sent_to), ((PRIMARY, 4011), PC_ADDRESS))
        self.assertEqual([fields["port"] for fields in self.events("dhcp-received")], ["4011"])


if __name__ == "__main__":
    unittest.main()
