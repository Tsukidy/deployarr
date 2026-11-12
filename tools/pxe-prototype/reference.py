"""Independent wire code for the tests. Imports nothing from the tool.

The tests build requests and decode replies with this module, so a codec
bug in the tool cannot hide behind the same bug in a test.
"""
import socket
import struct
from dataclasses import dataclass

GUID = bytes(range(0x10, 0x20))


def tlv(code: int, value: bytes) -> bytes:
    return bytes((code, len(value))) + value


def decode_tlv(data: bytes) -> list[tuple[int, bytes]]:
    """A run of code, length, value entries, with no pad and no end marker."""
    pairs = []
    at = 0
    while at < len(data):
        length = data[at + 1]
        pairs.append((data[at], data[at + 2:at + 2 + length]))
        at += 2 + length
    return pairs


def dhcp_request(message_type: int = 1, arch: int | None = 7, guid: bytes | None = GUID, *,
                 op: int = 1, xid: int = 0x1234ABCD, secs: int = 4, flags: int = 0x8000,
                 mac: bytes = b"\x52\x54\x00\xaa\xbb\xcc", giaddr: str = "0.0.0.0",
                 option_5: bytes | None = None, option_82: bytes | None = None,
                 option_250: bytes | None = None) -> bytes:
    """A PXE client's DHCP packet. `arch` None leaves out option 93, and
    `guid` None leaves out option 97."""
    packet = struct.pack("!BBBBIHH", op, 1, len(mac), 0, xid, secs, flags)
    packet += bytes(12) + socket.inet_aton(giaddr)
    packet += mac.ljust(16, b"\x00") + bytes(64) + bytes(128)
    packet += b"\x63\x82\x53\x63"
    packet += tlv(53, bytes((message_type,)))
    packet += tlv(55, bytes((1, 3, 43, 54, 60, 66, 67, 97)))
    packet += tlv(57, b"\x05\xc0")
    if guid is not None:
        packet += tlv(97, b"\x00" + guid)
    packet += tlv(94, b"\x01\x03\x10")
    if arch is not None:
        packet += tlv(93, struct.pack("!H", arch))
    packet += tlv(60, b"PXEClient:Arch:%05d:UNDI:003016" % (arch or 0))
    if option_5 is not None:
        packet += tlv(5, option_5)
    if option_82 is not None:
        packet += tlv(82, option_82)
    if option_250 is not None:
        packet += tlv(250, option_250)
    return packet + b"\xff"


@dataclass
class DhcpReply:
    """A reply read at the fixed RFC 2131 offsets."""
    raw: bytes
    op: int
    hlen: int
    xid: int
    secs: int
    flags: int
    ciaddr: str
    yiaddr: str
    siaddr: str
    giaddr: str
    chaddr: bytes
    sname: bytes
    file: bytes
    options: list[tuple[int, bytes]]

    def option(self, code: int) -> bytes | None:
        return next((value for found, value in self.options if found == code), None)

    @property
    def codes(self) -> list[int]:
        return [code for code, _ in self.options]


def decode_dhcp_reply(raw: bytes) -> DhcpReply:
    assert raw[236:240] == b"\x63\x82\x53\x63", "magic cookie"
    options = []
    at = 240
    while raw[at] != 255:
        length = raw[at + 1]
        options.append((raw[at], raw[at + 2:at + 2 + length]))
        at += 2 + length
    assert not any(raw[at + 1:]), "only zero padding follows the end option"
    xid, secs, flags = struct.unpack_from("!IHH", raw, 4)
    return DhcpReply(
        raw=raw, op=raw[0], hlen=raw[2], xid=xid, secs=secs, flags=flags,
        ciaddr=socket.inet_ntoa(raw[12:16]), yiaddr=socket.inet_ntoa(raw[16:20]),
        siaddr=socket.inet_ntoa(raw[20:24]), giaddr=socket.inet_ntoa(raw[24:28]),
        chaddr=raw[28:44], sname=raw[44:108], file=raw[108:236], options=options)
