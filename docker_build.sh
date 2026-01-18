#!/bin/bash
#
# Build firmware using Docker
# Keeps FPGA tools isolated in a container
#
# Usage:
#   ./docker_build.sh                    # Build Docker image first time
#   ./docker_build.sh blink              # Build blink test
#   ./docker_build.sh hub75              # Build HUB75 test
#   ./docker_build.sh firmware           # Build full network firmware
#   ./docker_build.sh firmware --ip X    # Build with custom IP
#   ./docker_build.sh shell              # Interactive shell in container
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE_NAME="colorlight-fpga-tools"

# Build Docker image if it doesn't exist
build_image() {
    echo "Building Docker image '$IMAGE_NAME'..."
    docker build -t "$IMAGE_NAME" "$SCRIPT_DIR"
    echo "Docker image built successfully."
}

# Check if image exists
if ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    build_image
fi

# Parse command
CMD="${1:-help}"
shift || true

case "$CMD" in
    build-image|image)
        build_image
        ;;

    blink)
        echo "Building blink test firmware..."
        docker run --rm -v "$SCRIPT_DIR:/build" "$IMAGE_NAME" bash -c "
            cd /build/firmware
            mkdir -p build/blink_test
            cd build/blink_test
            yosys -p 'read_verilog /build/firmware/src/blink_test.v; synth_ecp5 -top blink_test -json blink_test.json'
            nextpnr-ecp5 --25k --package CABGA256 --speed 6 --json blink_test.json --lpf /build/firmware/src/blink_test.lpf --textcfg blink_test.config
            ecppack blink_test.config blink_test.bit
            echo ''
            echo '========================================'
            echo 'Build complete!'
            echo 'Output: firmware/build/blink_test/blink_test.bit'
            echo '========================================'
        "
        ;;

    hub75)
        echo "Building HUB75 test firmware..."
        docker run --rm -v "$SCRIPT_DIR:/build" "$IMAGE_NAME" bash -c "
            cd /build/firmware
            mkdir -p build/hub75_test
            cd build/hub75_test
            yosys -p 'read_verilog /build/firmware/src/hub75_test.v; synth_ecp5 -top hub75_test -json hub75_test.json'
            nextpnr-ecp5 --25k --package CABGA256 --speed 6 --json hub75_test.json --lpf /build/firmware/src/hub75_test.lpf --textcfg hub75_test.config
            ecppack hub75_test.config hub75_test.bit
            echo ''
            echo '========================================'
            echo 'Build complete!'
            echo 'Output: firmware/build/hub75_test/hub75_test.bit'
            echo '========================================'
        "
        ;;

    firmware|full)
        # Parse IP argument
        IP_ADDRESS="192.168.1.50"
        MAC_ADDRESS="0x10E2D5000001"
        PORT="1337"

        while [[ $# -gt 0 ]]; do
            case $1 in
                --ip) IP_ADDRESS="$2"; shift 2 ;;
                --mac) MAC_ADDRESS="$2"; shift 2 ;;
                --port) PORT="$2"; shift 2 ;;
                *) shift ;;
            esac
        done

        echo "Building full network firmware..."
        echo "  IP: $IP_ADDRESS"
        echo "  Port: $PORT"

        docker run --rm -v "$SCRIPT_DIR:/build" "$IMAGE_NAME" bash -c "
            set -e
            cd /build/firmware/colorlight-led-cube/fpga

            # Generate PLL if needed
            if [ ! -f pll.v ]; then
                ecppll -i 25 --clkout0_name clock --clkout0 125 --clkout1_name panel_clock --clkout1 52 -f pll.v
            fi

            # Use existing liteeth_core.v (pre-generated, works with this codebase)
            # The IP/MAC are hardcoded in liteeth_core.v - would need manual edit for different IP

            # Copy constraints for 5A-75E V8.x
            cp /build/firmware/constraints/5a-75e-v8.lpf syn/top.lpf

            # Build
            cd syn
            echo 'Running synthesis...'
            yosys -s syn.ys -o top.json

            echo 'Running place and route...'
            nextpnr-ecp5 --pre-pack clocks.py --25k --freq 125 --timing-allow-fail --package CABGA256 --speed 6 --json top.json --lpf top.lpf --textcfg top.config

            echo 'Generating bitstream...'
            ecppack top.config top.bit

            echo 'Converting to SVF...'
            python3 bit_to_svf.py top.bit top.svf

            # Copy outputs
            mkdir -p /build/firmware/build/network
            cp top.bit top.svf /build/firmware/build/network/

            echo ''
            echo '========================================'
            echo 'Build complete!'
            echo 'Output: firmware/build/network/top.bit'
            echo '        firmware/build/network/top.svf'
            echo 'NOTE: IP is hardcoded in liteeth_core.v'
            echo '      Default: 192.168.178.50 port 6000'
            echo '========================================'
        "
        ;;

    shell)
        echo "Starting interactive shell..."
        docker run --rm -it -v "$SCRIPT_DIR:/build" "$IMAGE_NAME" bash
        ;;

    help|*)
        echo "Colorlight FPGA Build System (Docker)"
        echo ""
        echo "Usage: $0 <command> [options]"
        echo ""
        echo "Commands:"
        echo "  build-image     Build/rebuild the Docker image"
        echo "  blink           Build LED blink test firmware"
        echo "  hub75           Build HUB75 panel test firmware"
        echo "  firmware        Build full network firmware"
        echo "  shell           Interactive shell in container"
        echo ""
        echo "Options for 'firmware':"
        echo "  --ip ADDRESS    Set board IP (default: 192.168.1.50)"
        echo "  --port PORT     Set UDP port (default: 1337)"
        echo ""
        echo "Examples:"
        echo "  $0 blink"
        echo "  $0 firmware --ip 192.168.1.100"
        echo ""
        echo "To flash (run on host with USB Blaster connected):"
        echo "  openFPGALoader --cable usb-blaster firmware/build/blink_test/blink_test.bit"
        ;;
esac
