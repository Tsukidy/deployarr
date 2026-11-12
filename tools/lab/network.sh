#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Creates or removes the PXE lab network: an isolated bridge, one tap device
# per lab VM, and a dnsmasq that only hands out addresses. See docs/lab-setup.md.
# Usage: sudo tools/lab/network.sh up|down|status
set -euo pipefail
# shellcheck source=lab.conf
source "$(dirname "$0")/lab.conf"

TAPS=("${VM_TAP[@]}")
PIDFILE=$RUN_DIR/dnsmasq.pid

if [[ $(id -u) -ne 0 ]]; then
    echo "Run this with sudo." >&2
    exit 1
fi

dnsmasq_running() {
    [[ -s $PIDFILE ]] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null
}

up() {
    local owner=${SUDO_USER:-root}
    mkdir -p "$RUN_DIR"
    ip link show "$BRIDGE" >/dev/null 2>&1 || ip link add "$BRIDGE" type bridge
    ip addr replace "$SERVER_IP/$PREFIX" dev "$BRIDGE"
    ip link set "$BRIDGE" up
    for tap in "${TAPS[@]}"; do
        ip link show "$tap" >/dev/null 2>&1 || ip tuntap add dev "$tap" mode tap user "$owner"
        ip link set "$tap" master "$BRIDGE" up
    done
    if ! dnsmasq_running; then
        dnsmasq --conf-file=/dev/null --port=0 \
            --interface="$BRIDGE" --bind-dynamic \
            --dhcp-range="$DHCP_RANGE" \
            --dhcp-leasefile="$RUN_DIR/dnsmasq.leases" \
            --pid-file="$PIDFILE" \
            --log-dhcp --log-facility="$RUN_DIR/dnsmasq.log"
    fi
    status
}

down() {
    if dnsmasq_running; then
        kill "$(cat "$PIDFILE")"
    fi
    rm -f "$PIDFILE"
    for tap in "${TAPS[@]}"; do
        ip link show "$tap" >/dev/null 2>&1 && ip link del "$tap"
    done
    ip link show "$BRIDGE" >/dev/null 2>&1 && ip link del "$BRIDGE"
    echo "Lab network removed."
}

status() {
    ip -br addr show "$BRIDGE"
    for tap in "${TAPS[@]}"; do
        ip -br link show "$tap"
    done
    if dnsmasq_running; then
        echo "dnsmasq is running, pid $(cat "$PIDFILE"), log $RUN_DIR/dnsmasq.log"
    else
        echo "dnsmasq is not running"
    fi
    ss -ulpn 'sport = :67'
}

case ${1:-} in
    up) up ;;
    down) down ;;
    status) status ;;
    *) echo "Usage: sudo $0 up|down|status" >&2; exit 2 ;;
esac
