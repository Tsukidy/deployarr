import dataclasses
import re
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from enum import Enum
from typing import Callable, ClassVar, TextIO


@dataclass(frozen=True)
class Started:
    name: ClassVar[str] = "started"
    chain: str
    server_ip: str
    interface: str | None
    root: str
    dhcp_bind: str
    dhcp_ports: tuple[int, ...]
    tftp: str
    bcd_uefi: str
    bcd_bios: str
    max_blksize: int
    max_windowsize: int
    timeout: float
    retries: int
    block_wrap_first: int
    block_log: bool
    answer_without_guid: bool


@dataclass(frozen=True)
class DhcpReceived:
    name: ClassVar[str] = "dhcp-received"
    port: int
    source: str
    type: str
    xid: str
    mac: str
    arch: str | None
    guid: str | None
    option_250: str | None
    options: tuple[int, ...]


@dataclass(frozen=True)
class DhcpIgnored:
    name: ClassVar[str] = "dhcp-ignored"
    port: int
    source: str
    reason: str
    detail: str | None = None


@dataclass(frozen=True)
class DhcpSent:
    name: ClassVar[str] = "dhcp-sent"
    port: int
    destination: str
    type: str
    xid: str
    firmware: str
    step: str | None
    file: str | None
    option_250: str | None
    option_252: str | None


@dataclass(frozen=True)
class TftpReceived:
    name: ClassVar[str] = "tftp-received"
    client: str
    kind: str
    block: int | None = None
    file: str | None = None
    mode: str | None = None
    options: str | None = None
    code: int | None = None
    message: str | None = None


@dataclass(frozen=True)
class TftpSent:
    name: ClassVar[str] = "tftp-sent"
    client: str
    kind: str
    block: int | None = None
    bytes: int | None = None
    options: str | None = None
    code: int | None = None
    message: str | None = None


@dataclass(frozen=True)
class TftpIgnored:
    name: ClassVar[str] = "tftp-ignored"
    client: str
    reason: str


@dataclass(frozen=True)
class SendFailed:
    name: ClassVar[str] = "send-failed"
    destination: str
    error: str


@dataclass(frozen=True)
class InternalError:
    name: ClassVar[str] = "internal-error"
    where: str
    error: str


@dataclass(frozen=True)
class TransferFinished:
    name: ClassVar[str] = "transfer-finished"
    client: str
    file: str
    path: str
    size: int
    options: str
    blksize: int
    windowsize: int
    seconds: float
    blocks_sent: int
    retransmits: int
    bytes_acked: int
    outcome: str
    client_error: str | None = None


Event = (Started | DhcpReceived | DhcpIgnored | DhcpSent | TftpReceived | TftpSent
         | TftpIgnored | SendFailed | InternalError | TransferFinished)

_NEEDS_QUOTES = re.compile(r'[\s"=]')
_NOT_PRINTABLE = re.compile(r"[^\x20-\x7e]")


def _text(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, float):
        return f"{value:.6f}"
    if isinstance(value, Enum):
        return str(value.value)
    if isinstance(value, tuple):
        return ",".join(_text(item) for item in value)
    return str(value)


def _field(value: object) -> str:
    text = _text(value)
    quote = not text or _NEEDS_QUOTES.search(text)
    text = _NOT_PRINTABLE.sub(lambda found: f"\\u{ord(found[0]):04x}", text)
    return '"' + text.replace('"', '\\"') + '"' if quote else text


def format_line(event: Event) -> str:
    fields = [f"{field.name}={_field(getattr(event, field.name))}"
              for field in dataclasses.fields(event) if getattr(event, field.name) is not None]
    return " ".join([event.name, *fields])


class Sink:
    def __init__(self, stream: TextIO, clock: Callable[[], float] = time.time):
        self._stream = stream
        self._clock = clock
        self._lock = threading.Lock()

    def __call__(self, event: Event) -> None:
        stamp = datetime.fromtimestamp(self._clock(), timezone.utc).isoformat(timespec="milliseconds")
        line = f"{stamp} {format_line(event)}\n"
        with self._lock:
            self._stream.write(line)
