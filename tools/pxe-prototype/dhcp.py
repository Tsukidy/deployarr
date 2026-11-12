"""BOOTP and DHCP packet codec (RFC 951, RFC 2131, RFC 2132).

Knows no PXE policy and opens no socket. `policy.py` decides what a reply
holds. This module turns bytes into a `DhcpPacket` and back.
"""
import struct
from dataclasses import dataclass
from enum import IntEnum
from ipaddress import IPv4Address

MAGIC_COOKIE = b"\x63\x82\x53\x63"
BOOTP_MINIMUM = 300
BOOTREQUEST = 1
BOOTREPLY = 2
SERVER_PORT = 67

_HEADER = struct.Struct("!BBBBIHH4s4s4s4s16s64s128s")
_OPTIONS_START = _HEADER.size + len(MAGIC_COOKIE)
_PAD = 0
_END = 255


class MessageType(IntEnum):
    """Values of option 53."""
    DISCOVER = 1
    OFFER = 2
    REQUEST = 3
    DECLINE = 4
    ACK = 5
    NAK = 6
    RELEASE = 7
    INFORM = 8


class MalformedPacket(ValueError):
    """The datagram is not a DHCP packet. The message names the failed check."""


@dataclass(frozen=True)
class DhcpPacket:
    """One DHCP packet. `options` holds `(code, value)` pairs in wire order,
    without pad (0) and end (255)."""
    op: int
    htype: int
    hlen: int
    hops: int
    xid: int
    secs: int
    flags: int
    ciaddr: IPv4Address
    yiaddr: IPv4Address
    siaddr: IPv4Address
    giaddr: IPv4Address
    chaddr: bytes
    sname: bytes
    file: bytes
    options: tuple[tuple[int, bytes], ...]

    def option(self, code: int) -> bytes | None:
        """The first value for `code`, or None."""
        return next((value for found, value in self.options if found == code), None)


def parse(datagram: bytes) -> DhcpPacket:
    """Bytes to a packet. Raises `MalformedPacket` on a short packet, a wrong
    magic cookie, or an option that runs past the end. Option overload (52)
    is not followed."""
    if len(datagram) < _OPTIONS_START:
        raise MalformedPacket(f"{len(datagram)} bytes, shorter than {_OPTIONS_START}")
    if datagram[_HEADER.size:_OPTIONS_START] != MAGIC_COOKIE:
        raise MalformedPacket("wrong magic cookie")
    (op, htype, hlen, hops, xid, secs, flags,
     ciaddr, yiaddr, siaddr, giaddr, chaddr, sname, file) = _HEADER.unpack_from(datagram)
    options = []
    at = _OPTIONS_START
    while at < len(datagram):
        code = datagram[at]
        if code == _PAD:
            at += 1
            continue
        if code == _END:
            break
        if at + 1 >= len(datagram):
            raise MalformedPacket(f"option {code} has no length byte")
        length = datagram[at + 1]
        value = datagram[at + 2:at + 2 + length]
        if len(value) < length:
            raise MalformedPacket(f"option {code} runs past the end of the packet")
        options.append((code, value))
        at += 2 + length
    return DhcpPacket(op, htype, hlen, hops, xid, secs, flags,
                      IPv4Address(ciaddr), IPv4Address(yiaddr), IPv4Address(siaddr),
                      IPv4Address(giaddr), chaddr, sname, file, tuple(options))


def encode(packet: DhcpPacket) -> bytes:
    """A packet to bytes: the header, the cookie, each option, then 255.
    Pads with zeros to the 300-byte BOOTP minimum (RFC 951, RFC 1542)."""
    out = bytearray(_HEADER.pack(
        packet.op, packet.htype, packet.hlen, packet.hops, packet.xid, packet.secs,
        packet.flags, packet.ciaddr.packed, packet.yiaddr.packed, packet.siaddr.packed,
        packet.giaddr.packed, packet.chaddr, packet.sname, packet.file))
    out += MAGIC_COOKIE
    for code, value in packet.options:
        out += bytes((code, len(value))) + value
    out.append(_END)
    return bytes(out.ljust(BOOTP_MINIMUM, b"\x00"))
