#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Creates or removes the PXE lab network: an isolated bridge, one tap device
# per lab VM, and a dnsmasq that only hands out addresses. dnsmasq runs in its
# own network namespace, joined to the bridge by a veth pair, so it does not
# hold UDP 67 in the lab host's network stack. See docs/lab-setup.md.
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
    if [[ ! -e /run/netns/$DHCP_NETNS ]]; then
        # A veth left from a namespace that is gone would block the new pair.
        ip link show "$DHCP_VETH" >/dev/null 2>&1 && ip link del "$DHCP_VETH"
        ip netns add "$DHCP_NETNS"
    fi
    if ! ip link show "$DHCP_VETH" >/dev/null 2>&1; then
        ip link add "$DHCP_VETH" type veth peer name "$DHCP_IF" netns "$DHCP_NETNS"
    fi
    ip link set "$DHCP_VETH" master "$BRIDGE" up
    ip -n "$DHCP_NETNS" link set lo up
    ip -n "$DHCP_NETNS" addr replace "$DHCP_IP/$PREFIX" dev "$DHCP_IF"
    ip -n "$DHCP_NETNS" link set "$DHCP_IF" up
    # A dnsmasq from the earlier layout runs in the host's network stack. Stop it.
    if dnsmasq_running && [[ $(ip netns identify "$(cat "$PIDFILE")") != "$DHCP_NETNS" ]]; then
        kill "$(cat "$PIDFILE")"
        for _ in $(seq 50); do dnsmasq_running || break; sleep 0.1; done
        rm -f "$PIDFILE"
    fi
    if ! dnsmasq_running; then
        ip netns exec "$DHCP_NETNS" dnsmasq --conf-file=/dev/null --port=0 \
            --interface="$DHCP_IF" --bind-interfaces \
            --dhcp-range="$DHCP_RANGE" \
            --dhcp-option=option:router,"$SERVER_IP" \
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
    ip link show "$DHCP_VETH" >/dev/null 2>&1 && ip link del "$DHCP_VETH"
    [[ -e /run/netns/$DHCP_NETNS ]] && ip netns del "$DHCP_NETNS"
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
    ip -br link show "$DHCP_VETH" 2>/dev/null || echo "$DHCP_VETH is missing"
    if dnsmasq_running; then
        echo "dnsmasq is running in namespace $DHCP_NETNS at $DHCP_IP, pid $(cat "$PIDFILE"), log $RUN_DIR/dnsmasq.log, leases $RUN_DIR/dnsmasq.leases"
        ip netns exec "$DHCP_NETNS" ss -ulpn 'sport = :67'
    else
        echo "dnsmasq is not running"
    fi
}

case ${1:-} in
    up) up ;;
    down) down ;;
    status) status ;;
    *) echo "Usage: sudo $0 up|down|status" >&2; exit 2 ;;
esac
