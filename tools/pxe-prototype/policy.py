"""Which reply a PXE request gets. Packets in, a packet or a reason out.

`BOOT_TABLE` is the whole boot-chain policy. The reply layout follows
ConfigMgr 2509's reply builder, `sccmpxe!0x140020f38`, which edits the
request in place and writes the options in a fixed order.
"""
import dataclasses
from dataclasses import dataclass
from enum import Enum
from ipaddress import IPv4Address

import wds250
from dhcp import BOOTREPLY, BOOTREQUEST, DhcpPacket, MessageType

FILE_FIELD = 128
SNAME_FIELD = 64


class Chain(Enum):
    SINGLE = "single"
    TWO_STAGE = "two-stage"


class Firmware(Enum):
    BIOS = "bios"
    UEFI_X64 = "uefi-x64"


class Step(Enum):
    """Where an ACK sits in its chain. `ONLY` belongs to `Chain.SINGLE`.
    `FIRST` and `SECOND` belong to `Chain.TWO_STAGE`."""
    ONLY = "only"
    FIRST = "first"
    SECOND = "second"


class IgnoreReason(Enum):
    NOT_A_REQUEST = "not-a-request"
    NOT_DISCOVER_OR_REQUEST = "not-discover-or-request"
    NO_OPTION_93 = "no-option-93"
    NO_OPTION_97 = "no-option-97"
    UNSUPPORTED_ARCH = "unsupported-arch"
    OTHER_INTERFACE = "other-interface"


@dataclass(frozen=True)
class NetworkBootProgram:
    """A first-stage ACK: a WDS network boot program and option 250."""
    file: str
    option_250: wds250.Recipe


@dataclass(frozen=True)
class BootManager:
    """An ACK that starts Windows boot: the boot manager and option 252,
    the BCD store of firmware `bcd` in the `Layout`."""
    file: str
    bcd: Firmware


Boot = NetworkBootProgram | BootManager

# Zero touch is the only mode. `pxeboot.n12` skips the key prompt on BIOS,
# and sub-option 11 = 2 skips it on UEFI. A BIOS first stage gets the normal
# recipe, as ConfigMgr sends for a required deployment (sccmpxe!0x140020e14).
BOOT_TABLE: dict[tuple[Step, Firmware], Boot] = {
    (Step.ONLY, Firmware.UEFI_X64): BootManager(r"boot\x64\bootmgfw.efi", Firmware.UEFI_X64),
    (Step.ONLY, Firmware.BIOS): BootManager(r"boot\x64\pxeboot.n12", Firmware.BIOS),
    (Step.FIRST, Firmware.UEFI_X64): NetworkBootProgram(r"boot\x64\wdsmgfw.efi", wds250.ZERO_TOUCH_UEFI),
    (Step.FIRST, Firmware.BIOS): NetworkBootProgram(r"boot\x64\wdsnbp.com", wds250.NORMAL),
    (Step.SECOND, Firmware.UEFI_X64): BootManager(r"boot\x64\bootmgfw.efi", Firmware.UEFI_X64),
    (Step.SECOND, Firmware.BIOS): BootManager(r"boot\x64\pxeboot.n12", Firmware.BIOS),
}

# Option 93's first entry. Every BIOS PC sends 0, whatever its CPU.
_FIRMWARE_BY_ARCH = {0: Firmware.BIOS, 7: Firmware.UEFI_X64, 9: Firmware.UEFI_X64}


def _path_bytes(path: str) -> bytes:
    """A TFTP path as it goes on the wire. It must fit the BOOTP `file`
    field with room for a NUL."""
    if not path.isascii():
        raise ValueError(f"path {path!r} is not ASCII")
    if len(path) >= FILE_FIELD:
        raise ValueError(f"path {path!r} is longer than {FILE_FIELD - 1} bytes")
    return path.encode("ascii")


@dataclass(frozen=True)
class Layout:
    """The option 252 path for each firmware, relative to the TFTP root."""
    bcd_uefi: str = r"boot\uefi.bcd"
    bcd_bios: str = r"boot\bios.bcd"

    def __post_init__(self) -> None:
        _path_bytes(self.bcd_uefi)
        _path_bytes(self.bcd_bios)

    def bcd(self, firmware: Firmware) -> str:
        return {Firmware.UEFI_X64: self.bcd_uefi, Firmware.BIOS: self.bcd_bios}[firmware]


for _boot in BOOT_TABLE.values():
    _path_bytes(_boot.file)


@dataclass(frozen=True)
class Policy:
    server_ip: IPv4Address
    chain: Chain
    layout: Layout = Layout()


@dataclass(frozen=True)
class Reply:
    """A packet to send. `step` is None for an OFFER."""
    packet: DhcpPacket
    firmware: Firmware
    step: Step | None


@dataclass(frozen=True)
class NoReply:
    reason: IgnoreReason


def respond(request: DhcpPacket, policy: Policy) -> Reply | NoReply:
    """The reply to one request. A DISCOVER gets an OFFER, and a REQUEST
    gets an ACK with a boot file. Options 54 and 60 of the request are
    never read, as in ConfigMgr."""
    if request.op != BOOTREQUEST:
        return NoReply(IgnoreReason.NOT_A_REQUEST)
    kind = request.option(53)
    if kind not in (bytes((MessageType.DISCOVER,)), bytes((MessageType.REQUEST,))):
        return NoReply(IgnoreReason.NOT_DISCOVER_OR_REQUEST)
    arch = request.option(93)
    if arch is None or len(arch) < 2:
        return NoReply(IgnoreReason.NO_OPTION_93)
    if request.option(97) is None:
        return NoReply(IgnoreReason.NO_OPTION_97)
    firmware = _FIRMWARE_BY_ARCH.get(int.from_bytes(arch[:2], "big"))
    if firmware is None:
        return NoReply(IgnoreReason.UNSUPPORTED_ARCH)
    if kind[0] == MessageType.DISCOVER:
        return Reply(_reply(request, policy, MessageType.OFFER, None), firmware, None)
    step = _step(request, policy.chain)
    packet = _reply(request, policy, MessageType.ACK, BOOT_TABLE[step, firmware])
    return Reply(packet, firmware, step)


def _step(request: DhcpPacket, chain: Chain) -> Step:
    if chain is Chain.SINGLE:
        return Step.ONLY
    # Any option 250 marks a second request, whatever it holds (sccmpxe!0x140020385).
    return Step.SECOND if request.option(250) is not None else Step.FIRST


def _reply(request: DhcpPacket, policy: Policy, kind: MessageType, boot: Boot | None) -> DhcpPacket:
    options = [
        (53, bytes((kind,))),
        (54, policy.server_ip.packed),
        (97, request.option(97)),
        (60, b"PXEClient"),
    ]
    relay_agent = request.option(82)
    if relay_agent is not None:
        options.append((82, relay_agent))
    file = b""
    match boot:
        case NetworkBootProgram(file=path, option_250=recipe):
            file = _path_bytes(path)
            options.append((250, wds250.render(recipe, request)))
        case BootManager(file=path, bcd=firmware):
            file = _path_bytes(path)
            options.append((252, _path_bytes(policy.layout.bcd(firmware))))
    return dataclasses.replace(
        request, op=BOOTREPLY, siaddr=policy.server_ip, sname=bytes(SNAME_FIELD),
        file=file.ljust(FILE_FIELD, b"\x00"), options=tuple(options))


def reply_destination(source: tuple[str, int]) -> tuple[str, int]:
    """Where a reply goes: back to the request's source address and port.
    A source of 0.0.0.0 cannot be reached by unicast, so that reply is
    broadcast. A relayed request comes from the relay, so it is answered
    by unicast to the relay."""
    address, port = source
    return ("255.255.255.255" if address == "0.0.0.0" else address, port)
