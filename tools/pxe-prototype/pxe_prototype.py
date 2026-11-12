import argparse
import signal
import sys
from ipaddress import IPv4Address
from pathlib import Path

import policy
import server


def _port(text: str) -> int:
    if not (text.isascii() and text.isdigit()) or int(text) > 65535:
        raise argparse.ArgumentTypeError(f"{text!r} is not a UDP port")
    return int(text)


def _ports(text: str) -> tuple[int, ...]:
    ports = tuple(_port(port) for port in text.split(","))
    for port in ports:
        if ports.count(port) > 1:
            raise argparse.ArgumentTypeError(f"port {port} is listed twice")
    return ports


def _host_address(text: str) -> IPv4Address:
    try:
        address = IPv4Address(text)
    except ValueError:
        raise argparse.ArgumentTypeError(f"{text!r} is not an IPv4 address") from None
    if address.is_unspecified or address.is_multicast or address == IPv4Address("255.255.255.255"):
        raise argparse.ArgumentTypeError(f"{text} is not a host address")
    return address


def _directory(text: str) -> Path:
    path = Path(text)
    if not path.is_dir():
        raise argparse.ArgumentTypeError(f"{text} is not a directory")
    return path.resolve()


def parse_args(argv: list[str] | None = None) -> server.Config:
    parser = argparse.ArgumentParser(
        prog="pxe_prototype.py",
        description="Throwaway proxyDHCP and TFTP server for the deployarr PXE lab experiments.")
    parser.add_argument("--server-ip", required=True, type=_host_address, metavar="ADDRESS",
                        help="this host's address on the lab network: siaddr, option 54, and the TFTP address")
    parser.add_argument("--root", required=True, type=_directory, metavar="DIR", help="the TFTP root")
    parser.add_argument("--chain", required=True, choices=[chain.value for chain in policy.Chain],
                        help="the boot chain under test")
    parser.add_argument("--interface", required=True, metavar="NAME",
                        help="the lab bridge: DHCP requests from other interfaces are ignored, "
                             "and every DHCP reply leaves through it")
    parser.add_argument("--dhcp-ports", type=_ports, default=(67, 4011), metavar="PORTS",
                        help="comma-separated UDP ports that answer DHCP (default: 67,4011)")
    parser.add_argument("--tftp-port", type=_port, default=69, metavar="PORT", help="default: 69")
    parser.add_argument("--bcd-uefi", default=policy.Layout.bcd_uefi, metavar="PATH",
                        help=r"option 252 for UEFI clients (default: %(default)s)")
    parser.add_argument("--bcd-bios", default=policy.Layout.bcd_bios, metavar="PATH",
                        help=r"option 252 for BIOS clients (default: %(default)s)")
    parser.add_argument("--no-block-log", action="store_true",
                        help="leave the line for each DATA and ACK out of the log")
    parser.add_argument("--answer-without-guid", action="store_true",
                        help="also answer a request that has no option 97, as QEMU's iPXE ROM sends, "
                             "with a reply that has no option 97")
    parser.add_argument("--log-file", metavar="FILE", help="append the log to FILE (default: stderr)")
    args = parser.parse_args(argv)
    try:
        layout = policy.Layout(bcd_uefi=args.bcd_uefi, bcd_bios=args.bcd_bios)
    except ValueError as error:
        parser.error(str(error))
    log_stream = sys.stderr
    if args.log_file is not None:
        try:
            log_stream = open(args.log_file, "a", buffering=1, encoding="utf-8")
        except OSError as error:
            parser.error(f"cannot open {args.log_file}: {error.strerror}")
    return server.Config(
        server_ip=args.server_ip, root=args.root, chain=policy.Chain(args.chain), layout=layout,
        interface=args.interface, dhcp_ports=args.dhcp_ports, tftp_port=args.tftp_port,
        log_blocks=not args.no_block_log, answer_without_guid=args.answer_without_guid,
        log_stream=log_stream)


def main(argv: list[str] | None = None) -> int:
    config = parse_args(argv)
    try:
        pxe = server.PxeServer(config)
    except server.BindError as error:
        print(f"pxe_prototype.py: {error}", file=sys.stderr)
        return 1
    # Blocked signals stay pending until sigwait takes one. A KeyboardInterrupt
    # could instead land between two start-up steps and leave a thread running.
    signals = {signal.SIGINT, signal.SIGTERM}
    signal.pthread_sigmask(signal.SIG_BLOCK, signals)
    with pxe:
        signal.sigwait(signals)
    return 0


if __name__ == "__main__":
    sys.exit(main())
