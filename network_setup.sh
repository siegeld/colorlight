#!/bin/bash
#
# Configure network interface for Colorlight communication
#
# Usage:
#   ./network_setup.sh              # Auto-detect interface, configure IP
#   ./network_setup.sh eth0         # Use specific interface
#   ./network_setup.sh --status     # Show current network status
#   ./network_setup.sh --reset      # Remove configuration
#

set -e

BOARD_IP="192.168.178.50"
HOST_IP="192.168.178.100"
NETMASK="24"

# Parse arguments
INTERFACE=""
ACTION="setup"

while [[ $# -gt 0 ]]; do
    case $1 in
        --status|-s)
            ACTION="status"
            shift
            ;;
        --reset|-r)
            ACTION="reset"
            shift
            ;;
        --help|-h)
            echo "Configure network for Colorlight communication"
            echo ""
            echo "Usage: $0 [interface] [options]"
            echo ""
            echo "Options:"
            echo "  --status   Show current network status"
            echo "  --reset    Remove IP configuration"
            echo ""
            echo "Board IP: $BOARD_IP"
            echo "Host IP:  $HOST_IP"
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            exit 1
            ;;
        *)
            INTERFACE="$1"
            shift
            ;;
    esac
done

# Find Ethernet interface if not specified
if [ -z "$INTERFACE" ]; then
    # Look for Ethernet interfaces (not wifi, not loopback, not virtual)
    for iface in /sys/class/net/*; do
        iface=$(basename "$iface")
        # Skip loopback, wifi, virtual interfaces
        if [[ "$iface" == "lo" ]] || [[ "$iface" == wl* ]] || [[ "$iface" == veth* ]] || [[ "$iface" == docker* ]] || [[ "$iface" == br-* ]]; then
            continue
        fi
        # Check if it's a physical Ethernet device
        if [ -d "/sys/class/net/$iface/device" ]; then
            INTERFACE="$iface"
            break
        fi
    done

    if [ -z "$INTERFACE" ]; then
        echo "Error: Could not find Ethernet interface"
        echo "Specify interface manually: $0 eth0"
        exit 1
    fi
fi

case $ACTION in
    status)
        echo "Network Status"
        echo "=============="
        echo ""
        echo "Interface: $INTERFACE"
        ip addr show "$INTERFACE" 2>/dev/null || echo "  Interface not found"
        echo ""
        echo "Board connectivity:"
        if ping -c 1 -W 1 "$BOARD_IP" &>/dev/null; then
            echo "  [OK] Board responding at $BOARD_IP"
        else
            echo "  [!!] Board not responding at $BOARD_IP"
        fi
        ;;

    reset)
        echo "Removing IP $HOST_IP from $INTERFACE..."
        sudo ip addr del "$HOST_IP/$NETMASK" dev "$INTERFACE" 2>/dev/null || true
        echo "Done."
        ;;

    setup)
        echo "Configuring network for Colorlight"
        echo "==================================="
        echo ""
        echo "Interface: $INTERFACE"
        echo "Host IP:   $HOST_IP"
        echo "Board IP:  $BOARD_IP"
        echo ""

        # Check if already configured
        if ip addr show "$INTERFACE" 2>/dev/null | grep -q "$HOST_IP"; then
            echo "IP $HOST_IP already configured on $INTERFACE"
        else
            echo "Adding IP $HOST_IP to $INTERFACE..."
            sudo ip addr add "$HOST_IP/$NETMASK" dev "$INTERFACE"
        fi

        # Bring interface up
        sudo ip link set "$INTERFACE" up

        echo ""
        echo "Testing connectivity..."
        sleep 1

        if ping -c 1 -W 2 "$BOARD_IP" &>/dev/null; then
            echo "[OK] Board responding at $BOARD_IP"
        else
            echo "[!!] Board not responding at $BOARD_IP"
            echo ""
            echo "Check:"
            echo "  - Ethernet cable connected to board"
            echo "  - Board is powered on"
            echo "  - Firmware is flashed and running"
            echo "  - Using Gigabit Ethernet port"
        fi
        ;;
esac
