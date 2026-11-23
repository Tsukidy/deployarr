#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Creates or removes the PXE lab network: an isolated bridge, one tap device
# per lab VM, and a dnsmasq that only hands out addresses. dnsmasq runs in its
# own network namespace, joined to the bridge by a veth pair, so it does not
# hold UDP 67 in the lab host's network stack. Before it creates the bridge,
# "up" adds an nftables table that drops every IPv4 packet the host forwards
# into or out of the bridge, whatever net.ipv4.ip_forward says. "down" removes
# the table after the bridge. See docs/lab-setup.md.
#
# LAB_UPLINK=1 also gives the bridge an uplink for the M7 Windows Update check:
# the host forwards IPv4 from the bridge out of its default route with
# masquerade NAT, and dnsmasq hands out a DNS server. The uplink's table takes
# the place of the isolation table. It is off by default. An "up" without it
# removes an uplink that an earlier "up" added, and "down" always removes it.
# Usage: sudo tools/lab/network.sh up|down|status
#        sudo env LAB_UPLINK=1 tools/lab/network.sh up
set -euo pipefail
# shellcheck source=lab.conf
source "$(dirname "$0")/lab.conf"

TAPS=("${VM_TAP[@]}")
PIDFILE=$RUN_DIR/dnsmasq.pid
# The host's settings as the first "up" with the uplink found them.
SAVED_SYSCTL=$RUN_DIR/uplink-sysctl.conf
UPLINK=${LAB_UPLINK:-0}

if [[ $(id -u) -ne 0 ]]; then
    echo "Run this with sudo." >&2
    exit 1
fi

dnsmasq_running() {
    [[ -s $PIDFILE ]] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null
}

stop_dnsmasq() {
    if dnsmasq_running; then
        kill "$(cat "$PIDFILE")"
        for _ in $(seq 50); do dnsmasq_running || break; sleep 0.1; done
    fi
    rm -f "$PIDFILE"
}

# The arguments dnsmasq runs with. The DNS server option follows the switch.
# shellcheck disable=SC2054 # dnsmasq options use commas.
dnsmasq_args() {
    args=(--conf-file=/dev/null --port=0
        --interface="$DHCP_IF" --bind-interfaces
        --dhcp-range="$DHCP_RANGE"
        --dhcp-option=option:router,"$SERVER_IP")
    if [[ $UPLINK == 1 ]]; then
        args+=(--dhcp-option=option:dns-server,"$UPLINK_DNS")
    fi
    args+=(--dhcp-leasefile="$RUN_DIR/dnsmasq.leases"
        --pid-file="$PIDFILE"
        --log-dhcp --log-facility="$RUN_DIR/dnsmasq.log")
}

uplink_on() {
    command -v nft >/dev/null && nft list table ip "$UPLINK_TABLE" >/dev/null 2>&1
}

isolated() {
    command -v nft >/dev/null && nft list table ip "$ISOLATE_TABLE" >/dev/null 2>&1
}

# The nft commands that replace the isolation table. The host forwards no IPv4
# packet into or out of the bridge. A packet between two ports of the bridge
# passes, also when br_netfilter hands it to this hook with the bridge as both
# devices. The host's own traffic with the bridge takes the input and output
# hooks, so the table never sees it. The rules match the bridge by name, so
# they work before the bridge exists.
isolation_nft() {
    cat <<NFT
table ip $ISOLATE_TABLE
delete table ip $ISOLATE_TABLE
table ip $ISOLATE_TABLE {
    chain forward {
        type filter hook forward priority filter; policy accept;
        iifname "$BRIDGE" oifname "$BRIDGE" accept
        iifname "$BRIDGE" counter drop
        oifname "$BRIDGE" counter drop
    }
}
NFT
}

add_uplink() {
    local out
    out=$(ip -4 route show default | awk '{ for (i = 1; i < NF; i++) if ($i == "dev") { print $(i + 1); exit } }')
    if [[ -z $out || $out == "$BRIDGE" ]]; then
        echo "The host has no IPv4 default route, so the lab has no uplink." >&2
        exit 1
    fi
    if [[ ! -e $SAVED_SYSCTL ]]; then
        # ip_forward comes first, because changing it also sets
        # all.accept_redirects to its opposite.
        {
            echo "net.ipv4.ip_forward = $(sysctl -n net.ipv4.ip_forward)"
            echo "net.ipv4.conf.all.accept_redirects = $(sysctl -n net.ipv4.conf.all.accept_redirects)"
        } >"$SAVED_SYSCTL.new"
        mv "$SAVED_SYSCTL.new" "$SAVED_SYSCTL"
    fi
    # One transaction replaces the whole table, so a second "up" adds no
    # duplicate rule. The policy stays accept, so other forwarded traffic on
    # the host is left to the host's own rules. NAT rewrites no packet that
    # connection tracking marks invalid, so the table drops such a packet from
    # the bridge instead of forwarding it with its lab address. A packet from
    # another interface with a source in the lab's subnet is forged, so the
    # table drops it before the masquerade can rewrite it and let replies in.
    # The same transaction deletes the isolation table. A drop in any base
    # chain on the forward hook ends the packet, so its drops would also stop
    # the uplink's traffic. The uplink's table drops the rest itself.
    nft -f - <<NFT
table ip $UPLINK_TABLE
delete table ip $UPLINK_TABLE
table ip $UPLINK_TABLE {
    chain forward {
        type filter hook forward priority filter; policy accept;
        iifname "$BRIDGE" oifname "$BRIDGE" accept
        iifname != "$BRIDGE" ip saddr $SUBNET counter drop
        iifname "$BRIDGE" oifname "$out" ct state invalid counter drop
        iifname "$BRIDGE" ip saddr $SUBNET oifname "$out" counter accept
        iifname "$out" oifname "$BRIDGE" ct state established,related counter accept
        iifname "$BRIDGE" counter drop
        oifname "$BRIDGE" counter drop
    }
    chain postrouting {
        type nat hook postrouting priority srcnat; policy accept;
        ip saddr $SUBNET oifname "$out" counter masquerade
    }
}
table ip $ISOLATE_TABLE
delete table ip $ISOLATE_TABLE
NFT
    sysctl -qw net.ipv4.ip_forward=1
}

remove_uplink() {
    # Forwarding goes back to its saved value before the rules go, so
    # forwarding that the uplink turned on never runs without the rules. One
    # transaction then puts the isolation table back and deletes the uplink's
    # table, so the bridge is never without rules.
    if [[ -e $SAVED_SYSCTL ]]; then
        sysctl -q -p "$SAVED_SYSCTL"
        rm -f "$SAVED_SYSCTL"
    fi
    if uplink_on; then
        {
            isolation_nft
            echo "table ip $UPLINK_TABLE"
            echo "delete table ip $UPLINK_TABLE"
        } | nft -f -
    fi
}

up() {
    local owner=${SUDO_USER:-root} pid args
    if [[ $UPLINK != 0 && $UPLINK != 1 ]]; then
        echo "LAB_UPLINK must be 0 or 1." >&2
        exit 2
    fi
    if ! command -v nft >/dev/null; then
        echo "The lab network needs nft. Install the nftables package." >&2
        exit 1
    fi
    # The isolation table goes in before the bridge exists. An uplink that is
    # on keeps its own table until add_uplink or remove_uplink replaces it.
    if ! uplink_on; then
        isolation_nft | nft -f -
    fi
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
    if [[ $UPLINK == 1 ]]; then
        add_uplink
    else
        remove_uplink
    fi
    dnsmasq_args
    # Restart a dnsmasq that runs in the host's network stack, from the layout
    # before the namespace, or with other options, such as after the uplink
    # switch changed.
    if dnsmasq_running; then
        pid=$(cat "$PIDFILE")
        if [[ $(ip netns identify "$pid") != "$DHCP_NETNS" ||
            $(tr '\0' '\n' <"/proc/$pid/cmdline" | tail -n +2) != "$(printf '%s\n' "${args[@]}")" ]]; then
            stop_dnsmasq
        fi
    fi
    if ! dnsmasq_running; then
        ip netns exec "$DHCP_NETNS" dnsmasq "${args[@]}"
    fi
    status
}

down() {
    remove_uplink
    stop_dnsmasq
    ip link show "$DHCP_VETH" >/dev/null 2>&1 && ip link del "$DHCP_VETH"
    [[ -e /run/netns/$DHCP_NETNS ]] && ip netns del "$DHCP_NETNS"
    for tap in "${TAPS[@]}"; do
        ip link show "$tap" >/dev/null 2>&1 && ip link del "$tap"
    done
    ip link show "$BRIDGE" >/dev/null 2>&1 && ip link del "$BRIDGE"
    # The isolation table goes last, after the bridge that it guards.
    if isolated; then
        nft delete table ip "$ISOLATE_TABLE"
    fi
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
        echo "dnsmasq options: $(tr '\0' '\n' <"/proc/$(cat "$PIDFILE")/cmdline" | sed -n 's/^--dhcp-option=//p' | paste -sd ' ')"
        ip netns exec "$DHCP_NETNS" ss -ulpn 'sport = :67'
    else
        echo "dnsmasq is not running"
    fi
    if uplink_on; then
        echo "The uplink is on. net.ipv4.ip_forward = $(sysctl -n net.ipv4.ip_forward). Its rules:"
        nft list table ip "$UPLINK_TABLE"
    else
        echo "The uplink is off. net.ipv4.ip_forward = $(sysctl -n net.ipv4.ip_forward)."
        if isolated; then
            # Without counters (-s), so two status runs give the same text.
            echo "The host forwards no IPv4 into or out of $BRIDGE. The rules:"
            nft -s list table ip "$ISOLATE_TABLE"
        else
            echo "No rule keeps forwarded IPv4 out of $BRIDGE. Run up to add it."
        fi
    fi
}

case ${1:-} in
    up) up ;;
    down) down ;;
    status) status ;;
    *) echo "Usage: sudo $0 up|down|status" >&2; exit 2 ;;
esac
