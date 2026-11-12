"""DHCP option 250, which WDS network boot programs read and send.

The layout and the two recipes come from ConfigMgr 2509's PXE responder:
the sub-option writer `sccmpxe!0x140020134` and the option 250 builder
`sccmpxe!0x140020e14`. The value is a run of sub-options, each one code
byte, one length byte, and the value, with no 255 end marker inside.

The meanings of the sub-options are inferred from WDS header names. The
binary shows only the bytes.
"""
from dataclasses import dataclass

from dhcp import DhcpPacket

# Inferred from WDS header names. Used only to label log fields.
SUBOPTION_NAMES = {
    1: "architecture",
    2: "next_action",
    3: "poll_interval",
    4: "poll_retries",
    5: "request_id",
    6: "message",
    11: "pxe_prompt",
}
UNPARSED = -1


@dataclass(frozen=True)
class ClientOption:
    """The first `width` bytes of the request's top-level DHCP option `code`,
    padded with zeros. A request without the option gives all zeros
    (`sccmpxe!0x140020375`, `sccmpxe!0x140023827`)."""
    code: int
    width: int


Value = bytes | ClientOption
Recipe = tuple[tuple[int, Value], ...]

NORMAL: Recipe = (
    (2, b"\x01"),
    (5, ClientOption(code=5, width=4)),
    (3, b"\x00\x14"),
    (4, b"\x00\xba"),
    (6, b"SCCM PXE"),
    (11, b"\x01"),
)
ZERO_TOUCH_UEFI: Recipe = ((11, b"\x02"),)


def render(recipe: Recipe, request: DhcpPacket) -> bytes:
    """A recipe to the option 250 value for a reply to `request`."""
    out = bytearray()
    for code, value in recipe:
        if isinstance(value, ClientOption):
            sent = request.option(value.code) or b""
            value = sent[:value.width].ljust(value.width, b"\x00")
        out += bytes((code, len(value))) + value
    return bytes(out)


def decode(payload: bytes) -> tuple[tuple[int, bytes], ...]:
    """An option 250 value to `(code, value)` pairs. Never raises. Bytes that
    do not parse come back as a last pair with code `UNPARSED`."""
    pairs = []
    at = 0
    while at < len(payload):
        end = at + 2 + payload[at + 1] if at + 1 < len(payload) else len(payload) + 1
        if end > len(payload):
            pairs.append((UNPARSED, payload[at:]))
            break
        pairs.append((payload[at], payload[at + 2:end]))
        at = end
    return tuple(pairs)


def describe(payload: bytes) -> str:
    """One log field, such as `architecture(1)=0009,pxe_prompt(11)=02`."""
    fields = []
    for code, value in decode(payload):
        if code == UNPARSED:
            fields.append(f"unparsed={value.hex()}")
        else:
            fields.append(f"{SUBOPTION_NAMES.get(code, 'unknown')}({code})={value.hex()}")
    return ",".join(fields)
