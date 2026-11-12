"""TFTP read server protocol (RFC 1350, 2347, 2348, 2349, 7440)."""
import os
import struct
from dataclasses import dataclass
from enum import Enum, IntEnum
from pathlib import Path
from typing import Callable

MAX_BLKSIZE = 32768
MAX_WINDOWSIZE = 32
TIMEOUT = 5.0
RETRIES = 5
# The wire block number that follows 65535. RFC 1350 does not define it.
BLOCK_WRAP_FIRST = 0

DEFAULT_BLKSIZE = 512
MIN_BLKSIZE = 8


class Opcode(IntEnum):
    RRQ = 1
    WRQ = 2
    DATA = 3
    ACK = 4
    ERROR = 5
    OACK = 6


class TftpError(Exception):
    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(frozen=True)
class Request:
    opcode: Opcode
    filename: str
    mode: str
    options: tuple[tuple[str, str], ...]


@dataclass(frozen=True)
class Ack:
    block: int


@dataclass(frozen=True)
class ClientError:
    code: int
    message: str


def parse(datagram: bytes) -> Request | Ack | ClientError:
    if len(datagram) < 4:
        raise TftpError(4, "Malformed packet")
    opcode = int.from_bytes(datagram[:2], "big")
    if opcode in (Opcode.RRQ, Opcode.WRQ):
        fields = datagram[2:].split(b"\x00")
        if fields.pop() != b"" or len(fields) < 2 or len(fields) % 2:
            raise TftpError(4, "Malformed request")
        filename, mode, *rest = (field.decode("utf-8", "replace") for field in fields)
        return Request(Opcode(opcode), filename, mode, tuple(zip(rest[::2], rest[1::2])))
    if opcode == Opcode.ACK:
        return Ack(int.from_bytes(datagram[2:4], "big"))
    if opcode == Opcode.ERROR:
        message = datagram[4:].split(b"\x00")[0].decode("utf-8", "replace")
        return ClientError(int.from_bytes(datagram[2:4], "big"), message)
    raise TftpError(4, "Illegal TFTP operation")


def data_packet(block: int, payload: bytes) -> bytes:
    return struct.pack("!HH", Opcode.DATA, block) + payload


def oack_packet(options: tuple[tuple[str, str], ...]) -> bytes:
    fields = b"".join(f"{name}\x00{value}\x00".encode("ascii") for name, value in options)
    return struct.pack("!H", Opcode.OACK) + fields


def error_packet(error: TftpError) -> bytes:
    return struct.pack("!HH", Opcode.ERROR, error.code) + error.message.encode("ascii") + b"\x00"


@dataclass(frozen=True)
class Limits:
    max_windowsize: int = MAX_WINDOWSIZE
    timeout: float = TIMEOUT
    retries: int = RETRIES
    block_wrap_first: int = BLOCK_WRAP_FIRST


@dataclass(frozen=True)
class Negotiated:
    blksize: int = DEFAULT_BLKSIZE
    windowsize: int = 1
    oack: tuple[tuple[str, str], ...] = ()


def _number(name: str, value: str, minimum: int) -> int:
    refusal = TftpError(8, f"Unacceptable {name} {value!a}"[:80])
    if not (value.isascii() and value.isdigit()):
        raise refusal
    # int() refuses more than 4300 digits. Ten digits are past every limit.
    number = int(value.lstrip("0")[:10] or "0")
    if number < minimum:
        raise refusal
    return number


def negotiate(options: tuple[tuple[str, str], ...], file_size: int, limits: Limits) -> Negotiated:
    blksize = DEFAULT_BLKSIZE
    windowsize = 1
    oack: dict[str, tuple[str, str]] = {}
    for name, value in options:
        # A name such as "bl\u212asize" lowercases to "blksize", and an OACK
        # is ASCII.
        known = name.lower() if name.isascii() else ""
        if known in oack:
            continue
        if known == "blksize":
            blksize = min(_number(known, value, MIN_BLKSIZE), MAX_BLKSIZE)
            oack[known] = (name, str(blksize))
        elif known == "windowsize":
            windowsize = min(_number(known, value, 1), limits.max_windowsize)
            oack[known] = (name, str(windowsize))
        elif known == "tsize":
            oack[known] = (name, str(file_size))
    return Negotiated(blksize, windowsize, tuple(oack.values()))


def resolve(root: Path, filename: str) -> Path:
    """Each component is matched against the directory listing, exact spelling
    first and then ignoring case, because the lab's root is on a
    case-sensitive file system and Windows boot programs do not keep case."""
    name = filename.replace("/", "\\")
    parts = [part for part in name.split("\\") if part]
    if "\x00" in name or ":" in name or ".." in parts:
        raise TftpError(2, "Access violation")
    path = root
    for part in parts:
        try:
            names = os.listdir(path)
        except OSError:
            raise TftpError(1, "File not found") from None
        match = part if part in names else next(
            (found for found in sorted(names) if found.lower() == part.lower()), None)
        if match is None:
            raise TftpError(1, "File not found")
        path = path / match
    if not parts or not path.is_file():
        raise TftpError(1, "File not found")
    return path


class Phase(Enum):
    AWAIT_OACK_ACK = "await-oack-ack"
    SENDING = "sending"
    DONE = "done"
    FAILED = "failed"


class Outcome(Enum):
    COMPLETE = "complete"
    TIMEOUT = "timeout"
    CLIENT_ERROR = "client-error"
    STOPPED = "stopped"
    ERROR = "error"


class Transfer:
    """The last block is short, and it is empty when the size is a multiple
    of the block size."""

    def __init__(self, read_block: Callable[[int], bytes], file_size: int,
                 negotiated: Negotiated, limits: Limits, now: float):
        self._read_block = read_block
        self._negotiated = negotiated
        self._limits = limits
        self.file_size = file_size
        self.last_block = file_size // negotiated.blksize + 1
        self.phase = Phase.AWAIT_OACK_ACK if negotiated.oack else Phase.SENDING
        self.outcome: Outcome | None = None
        self.client_error: ClientError | None = None
        self.acked = 0
        self.sent_upto = 0
        self._resent_upto = -1
        self.tries_left = limits.retries
        self.deadline = now + limits.timeout
        self.blocks_sent = 0
        self.retransmits = 0

    @property
    def active(self) -> bool:
        return self.phase in (Phase.AWAIT_OACK_ACK, Phase.SENDING)

    @property
    def bytes_acked(self) -> int:
        return min(self.acked * self._negotiated.blksize, self.file_size)

    def start(self, now: float) -> list[bytes]:
        if self.phase is Phase.AWAIT_OACK_ACK:
            return self._oack(now)
        return self._window(now)

    def on_ack(self, wire_block: int, now: float) -> list[bytes]:
        if self.phase is Phase.AWAIT_OACK_ACK:
            if wire_block != 0:
                return []
            self.phase = Phase.SENDING
            self.tries_left = self._limits.retries
            return self._window(now)
        if self.phase is not Phase.SENDING:
            return []
        index = next((i for i in range(self.acked, self.sent_upto + 1)
                      if self._wire_number(i) == wire_block), None)
        if index is None:
            return []
        if index == self.acked:
            # RFC 1123 4.2.3.1: answering a repeated ACK in lock-step mode
            # doubles every later packet (the Sorcerer's Apprentice bug).
            # With a window, a repeat means the client missed block acked + 1,
            # unless a second copy of an earlier block caused it. Answering
            # that repeat too doubles every later window.
            if self._negotiated.windowsize > 1 and self.acked > self._resent_upto:
                return self._window(now)
            return []
        self.acked = index
        self.tries_left = self._limits.retries
        if index == self.last_block:
            self._finish(Phase.DONE, Outcome.COMPLETE)
            return []
        return self._window(now)

    def on_timeout(self, now: float) -> list[bytes]:
        if not self.active or now < self.deadline:
            return []
        if self.tries_left == 0:
            self._finish(Phase.FAILED, Outcome.TIMEOUT)
            return []
        self.tries_left -= 1
        if self.phase is Phase.AWAIT_OACK_ACK:
            self.retransmits += 1
            return self._oack(now)
        return self._window(now)

    def on_error(self, code: int, message: str) -> None:
        if self.active:
            self.client_error = ClientError(code, message)
            self._finish(Phase.FAILED, Outcome.CLIENT_ERROR)

    def stop(self) -> None:
        if self.active:
            self._finish(Phase.FAILED, Outcome.STOPPED)

    def abort(self) -> None:
        if self.active:
            self._finish(Phase.FAILED, Outcome.ERROR)

    def _finish(self, phase: Phase, outcome: Outcome) -> None:
        self.phase = phase
        self.outcome = outcome

    def _oack(self, now: float) -> list[bytes]:
        self.deadline = now + self._limits.timeout
        return [oack_packet(self._negotiated.oack)]

    def _window(self, now: float) -> list[bytes]:
        first = self.acked + 1
        last = min(self.acked + self._negotiated.windowsize, self.last_block)
        self.blocks_sent += last - first + 1
        resent_upto = min(last, self.sent_upto)
        if resent_upto >= first:
            self.retransmits += resent_upto - first + 1
            self._resent_upto = max(self._resent_upto, resent_upto)
        self.sent_upto = max(self.sent_upto, last)
        self.deadline = now + self._limits.timeout
        return [data_packet(self._wire_number(i), self._read_block(i))
                for i in range(first, last + 1)]

    def _wire_number(self, index: int) -> int:
        if index <= 0xFFFF:
            return index
        first = self._limits.block_wrap_first
        return first + (index - 0x10000) % (0x10000 - first)
