#!/bin/bash
#
# Flash firmware to Colorlight 5A-75E
#
# Usage:
#   ./flash.sh                  # Flash network firmware (default)
#   ./flash.sh blink            # Flash blink test
#   ./flash.sh hub75            # Flash HUB75 test
#   ./flash.sh network          # Flash network firmware
#   ./flash.sh --permanent      # Flash to SPI (persistent)
#   ./flash.sh --detect         # Just detect the FPGA
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CABLE="usb-blaster"
PERMANENT=false

# Parse arguments
FIRMWARE="network"
while [[ $# -gt 0 ]]; do
    case $1 in
        blink)
            FIRMWARE="blink"
            shift
            ;;
        hub75)
            FIRMWARE="hub75"
            shift
            ;;
        network|firmware)
            FIRMWARE="network"
            shift
            ;;
        --permanent|--write-flash|-p)
            PERMANENT=true
            shift
            ;;
        --detect|-d)
            echo "Detecting FPGA..."
            openFPGALoader --detect --cable "$CABLE"
            exit 0
            ;;
        --cable|-c)
            CABLE="$2"
            shift 2
            ;;
        --help|-h)
            echo "Flash firmware to Colorlight 5A-75E"
            echo ""
            echo "Usage: $0 [firmware] [options]"
            echo ""
            echo "Firmware types:"
            echo "  blink       Flash LED blink test"
            echo "  hub75       Flash HUB75 panel test"
            echo "  network     Flash full network firmware (default)"
            echo ""
            echo "Options:"
            echo "  --permanent  Write to SPI flash (survives power cycle)"
            echo "  --detect     Just detect the FPGA, don't flash"
            echo "  --cable      JTAG cable type (default: usb-blaster)"
            echo ""
            echo "Examples:"
            echo "  $0                    # Flash network firmware to SRAM"
            echo "  $0 blink              # Flash blink test"
            echo "  $0 network --permanent # Flash network firmware to SPI"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Determine bitstream path
case $FIRMWARE in
    blink)
        BITSTREAM="$SCRIPT_DIR/firmware/build/blink_test/blink_test.bit"
        ;;
    hub75)
        BITSTREAM="$SCRIPT_DIR/firmware/build/hub75_test/hub75_test.bit"
        ;;
    network)
        BITSTREAM="$SCRIPT_DIR/firmware/build/network/top.bit"
        ;;
esac

# Check if bitstream exists
if [ ! -f "$BITSTREAM" ]; then
    echo "Error: Bitstream not found: $BITSTREAM"
    echo ""
    echo "Build it first with:"
    case $FIRMWARE in
        blink)
            echo "  ./docker_build.sh blink"
            ;;
        hub75)
            echo "  ./docker_build.sh hub75"
            ;;
        network)
            echo "  ./docker_build.sh firmware"
            ;;
    esac
    exit 1
fi

# Check if openFPGALoader is installed
if ! command -v openFPGALoader &> /dev/null; then
    echo "Error: openFPGALoader not found"
    echo ""
    echo "Install it with:"
    echo "  Fedora: sudo dnf install openFPGALoader"
    echo "  Ubuntu: sudo apt install openfpgaloader"
    echo "  Arch:   sudo pacman -S openfpgaloader"
    exit 1
fi

# Flash
echo "Flashing: $FIRMWARE"
echo "Bitstream: $BITSTREAM"

if [ "$PERMANENT" = true ]; then
    echo "Mode: SPI flash (persistent)"
    openFPGALoader --cable "$CABLE" --write-flash "$BITSTREAM"
else
    echo "Mode: SRAM (volatile)"
    openFPGALoader --cable "$CABLE" "$BITSTREAM"
fi

echo ""
echo "Done!"
