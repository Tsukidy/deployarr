"""Sockets and threads. The only module that does network I/O.

One dispatcher thread reads the DHCP sockets and the TFTP listener. It is
the only reader and writer of the session table. Each transfer runs on its
own thread with its own socket and file handle, and shares only the log.
"""
import select
import selectors
import socket
import struct
import sys
import threading
import time
from dataclasses import dataclass
from ipaddress import IPv4Address
from pathlib import Path
from typing import BinaryIO, TextIO

import dhcp
import events
import policy
import tftp
import wds250

_OPTIONS_WITH_A_LOG_FIELD = (53, 93, 97, 250)
# struct in_pktinfo: ipi_ifindex, ipi_spec_dst, ipi_addr.
_IN_PKTINFO = struct.Struct("@i4s4s")


@dataclass(frozen=True)
class Config:
    server_ip: IPv4Address
    root: Path
    chain: policy.Chain
    layout: policy.Layout = policy.Layout()
    interface: str | None = None
    dhcp_bind: str = "0.0.0.0"
    dhcp_ports: tuple[int, ...] = (67, 4011)
    tftp_port: int = 69
    limits: tftp.Limits = tftp.Limits()
    log_blocks: bool = True
    answer_without_guid: bool = False
    log_stream: TextIO = sys.stderr


class BindError(Exception):
    """A socket could not be set up. The message names the address and port."""


def _address(peer: tuple[str, int]) -> str:
    return f"{peer[0]}:{peer[1]}"


def _options_text(options: tuple[tuple[str, str], ...]) -> str:
    return ",".join(f"{name}={value}" for name, value in options)


def _message_type(value: bytes | None) -> str:
    if value is None or len(value) != 1:
        return "none" if value is None else value.hex()
    try:
        return dhcp.MessageType(value[0]).name
    except ValueError:
        return str(value[0])


class PxeServer:
    """The running prototype. Binds every socket when it is created, so a
    port in use fails in the caller's thread.

        with PxeServer(config) as server:
            server.dhcp_ports      # the ports bound, in the order of config.dhcp_ports
            server.tftp_address    # ("192.0.2.10", 69)
    """

    def __init__(self, config: Config):
        self._config = config
        self._policy = policy.Policy(config.server_ip, config.chain, config.layout, config.answer_without_guid)
        self._root = config.root.resolve()
        self._log = events.Sink(config.log_stream)
        # `stop` writes to one end. Every thread waits on the other end next
        # to its own socket and never reads it, so one byte wakes them all.
        self._stopping, self._stopper = socket.socketpair()
        self._sessions: dict[tuple[str, int, Path], threading.Thread] = {}
        self._dispatcher = threading.Thread(target=self._dispatch, name="dispatcher")
        self._ifindex = None if config.interface is None else self._interface_index(config.interface)
        self._dhcp_sockets: list[socket.socket] = []
        try:
            for port in config.dhcp_ports:
                self._dhcp_sockets.append(self._bind_dhcp(port))
            self._listener = self._bind((str(config.server_ip), config.tftp_port))
        except BindError:
            for sock in (*self._dhcp_sockets, self._stopping, self._stopper):
                sock.close()
            raise
        self.dhcp_ports = tuple(sock.getsockname()[1] for sock in self._dhcp_sockets)
        self.tftp_address: tuple[str, int] = self._listener.getsockname()

    @staticmethod
    def _interface_index(interface: str) -> int:
        if not hasattr(socket, "IP_PKTINFO"):
            raise BindError("an interface needs IP_PKTINFO, which this platform lacks")
        try:
            return socket.if_nametoindex(interface)
        except OSError:
            raise BindError(f"no interface named {interface}") from None

    def _bind_dhcp(self, port: int) -> socket.socket:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        # SO_REUSEADDR lets a dnsmasq that runs with bind-interfaces or
        # bind-dynamic share port 67. SO_REUSEPORT or SO_BINDTODEVICE would
        # also take dnsmasq's unicast lease renewals (measured with dnsmasq
        # 2.91 on Linux 6.18), so the interface is chosen with IP_PKTINFO.
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        if self._ifindex is not None:
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_PKTINFO, 1)
        return self._bind((self._config.dhcp_bind, port), sock)

    @staticmethod
    def _bind(address: tuple[str, int], sock: socket.socket | None = None) -> socket.socket:
        sock = sock or socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            sock.bind(address)
        except OSError as error:
            sock.close()
            raise BindError(f"cannot bind UDP {_address(address)}: {error.strerror}") from None
        return sock

    def __enter__(self) -> "PxeServer":
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()

    def start(self) -> None:
        config = self._config
        self._log(events.Started(
            chain=config.chain.value, server_ip=str(config.server_ip), interface=config.interface,
            root=str(self._root), dhcp_bind=config.dhcp_bind, dhcp_ports=self.dhcp_ports,
            tftp=_address(self.tftp_address), bcd_uefi=config.layout.bcd_uefi,
            bcd_bios=config.layout.bcd_bios, max_blksize=tftp.MAX_BLKSIZE,
            max_windowsize=config.limits.max_windowsize, timeout=config.limits.timeout,
            retries=config.limits.retries, block_wrap_first=config.limits.block_wrap_first,
            block_log=config.log_blocks, answer_without_guid=config.answer_without_guid))
        self._dispatcher.start()

    def stop(self) -> None:
        """Ends every thread, then closes the sockets. A transfer in progress
        is logged with the outcome `stopped`."""
        if self._stopper.fileno() == -1:
            return
        self._stopper.send(b"\x00")
        if self._dispatcher.is_alive():
            self._dispatcher.join()
        for thread in self._sessions.values():
            thread.join()
        for sock in (*self._dhcp_sockets, self._listener, self._stopping, self._stopper):
            sock.close()

    def _dispatch(self) -> None:
        with selectors.DefaultSelector() as selector:
            for sock in self._dhcp_sockets:
                selector.register(sock, selectors.EVENT_READ, self._on_dhcp)
            selector.register(self._listener, selectors.EVENT_READ, self._on_tftp_request)
            selector.register(self._stopping, selectors.EVENT_READ, None)
            while True:
                for key, _ in selector.select():
                    if key.data is None:
                        return
                    key.data(key.fileobj)

    def _on_dhcp(self, sock: socket.socket) -> None:
        port = sock.getsockname()[1]
        if self._ifindex is None:
            datagram, source = sock.recvfrom(65535)
        else:
            datagram, ancillary, _, source = sock.recvmsg(65535, socket.CMSG_SPACE(_IN_PKTINFO.size))
            arrived = [_IN_PKTINFO.unpack(data)[0] for level, kind, data in ancillary
                       if (level, kind) == (socket.IPPROTO_IP, socket.IP_PKTINFO)]
            if arrived != [self._ifindex]:
                self._log(events.DhcpIgnored(
                    port, _address(source), policy.IgnoreReason.OTHER_INTERFACE.value,
                    "ifindex=" + (",".join(map(str, arrived)) or "none")))
                return
        try:
            request = dhcp.parse(datagram)
        except dhcp.MalformedPacket as error:
            self._log(events.DhcpIgnored(port, _address(source), "malformed", str(error)))
            return
        xid = f"{request.xid:08x}"
        arch, guid, option_250 = request.option(93), request.option(97), request.option(250)
        self._log(events.DhcpReceived(
            port=port, source=_address(source), type=_message_type(request.option(53)), xid=xid,
            mac=request.chaddr[:request.hlen].hex(":"),
            arch=None if arch is None else arch.hex(),
            guid=None if guid is None else guid.hex(),
            option_250=None if option_250 is None else wds250.describe(option_250),
            options=tuple(code for code, _ in request.options if code not in _OPTIONS_WITH_A_LOG_FIELD)))
        result = policy.respond(request, self._policy, port)
        if isinstance(result, policy.NoReply):
            self._log(events.DhcpIgnored(port, _address(source), result.reason.value))
            return
        reply = result.packet
        destination = policy.reply_destination(source)
        if not self._send(sock, dhcp.encode(reply), destination, self._ifindex):
            return
        sent_250, sent_252 = reply.option(250), reply.option(252)
        self._log(events.DhcpSent(
            port=port, destination=_address(destination), type=_message_type(reply.option(53)), xid=xid,
            firmware=result.firmware.value, step=None if result.step is None else result.step.value,
            file=reply.file.rstrip(b"\x00").decode("ascii") or None,
            option_250=None if sent_250 is None else wds250.describe(sent_250),
            option_252=None if sent_252 is None else sent_252.decode("ascii")))

    def _send(self, sock: socket.socket, datagram: bytes, destination: tuple[str, int],
              ifindex: int | None = None) -> bool:
        """Sends one datagram. With `ifindex` it leaves through that interface,
        as dnsmasq sends its replies. A broadcast would otherwise follow the
        default route and miss the lab bridge."""
        try:
            if ifindex is None:
                sock.sendto(datagram, destination)
            else:
                pktinfo = _IN_PKTINFO.pack(ifindex, bytes(4), bytes(4))
                sock.sendmsg([datagram], [(socket.IPPROTO_IP, socket.IP_PKTINFO, pktinfo)], 0, destination)
        except OSError as error:
            self._log(events.SendFailed(_address(destination), str(error)))
            return False
        return True

    def _refuse(self, sock: socket.socket, client: tuple[str, int], error: tftp.TftpError) -> None:
        if self._send(sock, tftp.error_packet(error), client):
            self._log(events.TftpSent(_address(client), "ERROR", code=error.code, message=error.message))

    def _on_tftp_request(self, sock: socket.socket) -> None:
        datagram, client = sock.recvfrom(65535)
        received = time.monotonic()
        for key in [key for key, thread in self._sessions.items() if not thread.is_alive()]:
            del self._sessions[key]
        try:
            request = self._read_request(datagram, client)
            path = tftp.resolve(self._root, request.filename)
            session = (client[0], client[1], path)
            if session in self._sessions:
                # The running transfer's own timer resends what the client missed.
                self._log(events.TftpIgnored(_address(client), "duplicate-request"))
                return
            try:
                handle = open(path, "rb")
            except OSError:
                raise tftp.TftpError(2, "Access violation") from None
            try:
                size = handle.seek(0, 2)
                negotiated = tftp.negotiate(request.options, size, self._config.limits)
            except tftp.TftpError:
                handle.close()
                raise
        except tftp.TftpError as error:
            self._refuse(sock, client, error)
            return
        thread = threading.Thread(
            target=self._serve, name=f"transfer-{_address(client)}",
            args=(handle, client, request, path, size, negotiated, received))
        self._sessions[session] = thread
        thread.start()

    def _read_request(self, datagram: bytes, client: tuple[str, int]) -> tftp.Request:
        try:
            request = tftp.parse(datagram)
        except tftp.TftpError:
            self._log(events.TftpReceived(_address(client), "malformed"))
            raise
        if not isinstance(request, tftp.Request):
            self._log(events.TftpReceived(_address(client), "ACK" if isinstance(request, tftp.Ack) else "ERROR"))
            raise tftp.TftpError(4, "Illegal TFTP operation")
        self._log(events.TftpReceived(
            _address(client), request.opcode.name, file=request.filename, mode=request.mode,
            options=_options_text(request.options)))
        if request.opcode is tftp.Opcode.WRQ:
            raise tftp.TftpError(2, "Read-only server")
        return request

    def _serve(self, handle: BinaryIO, client: tuple[str, int], request: tftp.Request, path: Path,
               size: int, negotiated: tftp.Negotiated, received: float) -> None:
        """One transfer thread. It owns its socket and `handle`."""
        name = _address(client)
        log_blocks = self._config.log_blocks

        def read_block(block: int) -> bytes:
            handle.seek((block - 1) * negotiated.blksize)
            return handle.read(negotiated.blksize)

        def send(datagrams: list[bytes]) -> None:
            for datagram in datagrams:
                if not self._send(sock, datagram, client):
                    return
                opcode, block = struct.unpack_from("!HH", datagram)
                if opcode == tftp.Opcode.OACK:
                    self._log(events.TftpSent(name, "OACK", options=_options_text(negotiated.oack)))
                elif log_blocks:
                    self._log(events.TftpSent(name, "DATA", block=block, bytes=len(datagram) - 4))

        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock, handle:
            sock.bind((str(self._config.server_ip), 0))
            transfer = tftp.Transfer(read_block, size, negotiated, self._config.limits, time.monotonic())
            send(transfer.start(time.monotonic()))
            while transfer.active:
                now = time.monotonic()
                send(transfer.on_timeout(now))
                if not transfer.active:
                    break
                readable = select.select([sock, self._stopping], [], [], max(transfer.deadline - now, 0.0))[0]
                if self._stopping in readable:
                    transfer.stop()
                    break
                if not readable:
                    continue
                datagram, source = sock.recvfrom(65535)
                if source != client:
                    self._log(events.TftpIgnored(_address(source), "unknown-transfer-id"))
                    self._refuse(sock, source, tftp.TftpError(5, "Unknown transfer ID"))
                    continue
                try:
                    packet = tftp.parse(datagram)
                except tftp.TftpError as error:
                    self._log(events.TftpReceived(name, "malformed"))
                    self._refuse(sock, client, error)
                    continue
                match packet:
                    case tftp.Ack(block=block):
                        if log_blocks:
                            self._log(events.TftpReceived(name, "ACK", block=block))
                        send(transfer.on_ack(block, time.monotonic()))
                    case tftp.ClientError(code=code, message=message):
                        self._log(events.TftpReceived(name, "ERROR", code=code, message=message))
                        transfer.on_error(code, message)
                    case tftp.Request():
                        self._log(events.TftpReceived(name, packet.opcode.name, file=packet.filename))
                        self._refuse(sock, client, tftp.TftpError(4, "Illegal TFTP operation"))
            seconds = time.monotonic() - received
        error = transfer.client_error
        self._log(events.TransferFinished(
            client=name, file=request.filename, path=str(path), size=size,
            options=_options_text(request.options), blksize=negotiated.blksize,
            windowsize=negotiated.windowsize, seconds=seconds, blocks_sent=transfer.blocks_sent,
            retransmits=transfer.retransmits, bytes_acked=transfer.bytes_acked,
            outcome=transfer.outcome.value,
            client_error=None if error is None else f"{error.code} {error.message}"))
