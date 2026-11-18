"""DHCP option 250, which WDS network boot programs read and send.

The layout and the two recipes come from ConfigMgr 2509's PXE responder:
the sub-option writer `sccmpxe!0x140020134` and the option 250 builder
`sccmpxe!0x140020e14`. The value is a run of sub-options, each one code
byte, one length byte, and the value, with no 255 end marker inside.

The meanings of the sub-options are inferred from WDS header names. The
binary shows only the bytes.

`wdsmgfw.efi` and `wdsnbp.com` end the option 250 of their second request
with one 255 byte, as the lab runs of #28 show in
`docs/research/pxe-lab/two-stage-chain.md`. `WdsBp.h` defines no end code.
`decode` stops at a 255 where a code would start, the way the End option
ends DHCP's own options in RFC 2132, section 3.2 (inferred).
"""
from dataclasses import dataclass

from dhcp import DhcpPacket

# The 15 `WDSBP_OPT_*` defines of packet type `WDSBP_PK_TYPE_WDSNBP` in WDS's
# `WdsBp.h`, lines 71 to 85 of the copy in microsoft/win32metadata at commit
# 15fc870a, under generation/WinSDK/RecompiledIdlHeaders/um/. A name is its
# define in lower case without `WDSBP_OPT_`, except the shorter names of codes
# 4 and 11.
SUBOPTION_NAMES = {
    1: "architecture",  # WDSBP_OPT_ARCHITECTURE, line 71
    2: "next_action",  # WDSBP_OPT_NEXT_ACTION, line 72
    3: "poll_interval",  # WDSBP_OPT_POLL_INTERVAL, line 73
    4: "poll_retries",  # WDSBP_OPT_POLL_RETRY_COUNT, line 74
    5: "request_id",  # WDSBP_OPT_REQUEST_ID, line 75
    6: "message",  # WDSBP_OPT_MESSAGE, line 76
    7: "version_query",  # WDSBP_OPT_VERSION_QUERY, line 77
    8: "server_version",  # WDSBP_OPT_SERVER_VERSION, line 78
    9: "referral_server",  # WDSBP_OPT_REFERRAL_SERVER, line 79
    11: "pxe_prompt",  # WDSBP_OPT_PXE_CLIENT_PROMPT, line 80
    12: "pxe_prompt_done",  # WDSBP_OPT_PXE_PROMPT_DONE, line 81
    13: "nbp_ver",  # WDSBP_OPT_NBP_VER, line 82
    14: "action_done",  # WDSBP_OPT_ACTION_DONE, line 83
    15: "allow_server_selection",  # WDSBP_OPT_ALLOW_SERVER_SELECTION, line 84
    16: "server_features",  # WDSBP_OPT_SERVER_FEATURES, line 85
}
END_MARKER = 0xFF


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
    out = bytearray()
    for code, value in recipe:
        if isinstance(value, ClientOption):
            sent = request.option(value.code) or b""
            value = sent[:value.width].ljust(value.width, b"\x00")
        out += bytes((code, len(value))) + value
    return bytes(out)


def decode(payload: bytes) -> tuple[tuple[tuple[int, bytes], ...], bytes]:
    """The sub-options, and the bytes after them: an `END_MARKER` and what
    follows it, or a sub-option that is cut short."""
    pairs = []
    at = 0
    while (at + 1 < len(payload) and payload[at] != END_MARKER
           and at + 2 + payload[at + 1] <= len(payload)):
        end = at + 2 + payload[at + 1]
        pairs.append((payload[at], payload[at + 2:end]))
        at = end
    return tuple(pairs), payload[at:]


def describe(payload: bytes) -> str:
    pairs, rest = decode(payload)
    fields = [f"{SUBOPTION_NAMES.get(code, 'unknown')}({code})={value.hex()}" for code, value in pairs]
    if rest[:1] == bytes((END_MARKER,)):
        fields.append(f"end({END_MARKER})")
        rest = rest[1:]
    if rest:
        fields.append(f"unparsed={rest.hex()}")
    return ",".join(fields)
