#!/bin/bash
#
# Colorlight HUB75 LED Controller - Build Script
#
# Usage: ./build.sh [OPTIONS] [TARGETS]
#
# Run './build.sh --help' for full documentation
#

set -e

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_IMAGE="litex-hub75"
REVISION="8.2"
IP_ADDRESS="10.11.6.250"
CABLE="usb-blaster"
PANEL="128x64"
PATTERN="grid"
BUILD_DIR="build/colorlight_5a_75e"
BITSTREAM="${BUILD_DIR}/gateware/colorlight_5a_75e.bit"
FIRMWARE_DIR="sw_rust/barsign_disp"
FIRMWARE_BIN="${FIRMWARE_DIR}/target/riscv32i-unknown-none-elf/release/barsign-disp"
TFTP_DIR="${SCRIPT_DIR}/.tftp"
HOST_IP=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# =============================================================================
# Helper Functions
# =============================================================================

print_header() {
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
}

print_step() {
    echo -e "${GREEN}▶ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✖ $1${NC}"
}

print_success() {
    echo -e "${GREEN}✔ $1${NC}"
}

check_docker() {
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed or not in PATH"
        exit 1
    fi

    if ! docker info &> /dev/null; then
        print_error "Docker daemon is not running or you don't have permission"
        exit 1
    fi
}

check_docker_image() {
    if ! docker image inspect ${DOCKER_IMAGE} &> /dev/null; then
        print_warning "Docker image '${DOCKER_IMAGE}' not found"
        echo "Run './build.sh docker' to build it first"
        exit 1
    fi
}

docker_run() {
    docker run --rm -v "${SCRIPT_DIR}:/project" ${DOCKER_IMAGE} bash -c "$@"
}

docker_run_usb() {
    docker run --rm -v "${SCRIPT_DIR}:/project" -v /dev/bus/usb:/dev/bus/usb --privileged ${DOCKER_IMAGE} bash -c "$@"
}

# =============================================================================
# Build Targets
# =============================================================================

build_docker() {
    print_header "Building Docker Image"
    cd "${SCRIPT_DIR}"
    docker build -t ${DOCKER_IMAGE} .
    print_success "Docker image '${DOCKER_IMAGE}' built successfully"
}

build_bitstream() {
    print_header "Building FPGA Bitstream"
    check_docker_image

    print_step "Running LiteX build (revision ${REVISION}, IP ${IP_ADDRESS}, panel ${PANEL})"
    docker_run "./gateware/colorlight.py --revision ${REVISION} --ip-address ${IP_ADDRESS} --panel ${PANEL} --build"

    if [[ -f "${SCRIPT_DIR}/${BITSTREAM}" ]]; then
        print_success "Bitstream built: ${BITSTREAM}"
        # Copy to bitstreams/ for multi-panel support
        mkdir -p "${SCRIPT_DIR}/bitstreams"
        cp "${SCRIPT_DIR}/${BITSTREAM}" "${SCRIPT_DIR}/bitstreams/${PANEL}.bit"
        print_step "Copied to bitstreams/${PANEL}.bit"
    else
        print_error "Bitstream not found at expected location"
        exit 1
    fi
}

build_all_panels() {
    print_header "Building All Panel Bitstreams"
    check_docker_image
    mkdir -p "${SCRIPT_DIR}/bitstreams"

    for panel in 128x64 96x48 64x32 64x64; do
        echo ""
        print_step "Building bitstream for ${panel}..."
        docker_run "./gateware/colorlight.py --revision ${REVISION} --ip-address ${IP_ADDRESS} --panel ${panel} --build"
        if [[ -f "${SCRIPT_DIR}/${BITSTREAM}" ]]; then
            cp "${SCRIPT_DIR}/${BITSTREAM}" "${SCRIPT_DIR}/bitstreams/${panel}.bit"
            print_success "bitstreams/${panel}.bit"
        else
            print_error "Build failed for ${panel}"
            exit 1
        fi
    done

    # Firmware is universal — build once (PAC/CSRs are identical for all panels)
    echo ""
    build_firmware

    echo ""
    print_success "All panel bitstreams built:"
    ls -lh "${SCRIPT_DIR}/bitstreams/"
}

get_bitstream_path() {
    local panel_bit="${SCRIPT_DIR}/bitstreams/${PANEL}.bit"
    if [[ -f "${panel_bit}" ]]; then
        echo "${panel_bit}"
    elif [[ -f "${SCRIPT_DIR}/${BITSTREAM}" ]]; then
        echo "${SCRIPT_DIR}/${BITSTREAM}"
    else
        echo ""
    fi
}

build_firmware() {
    print_header "Building Rust Firmware"
    check_docker_image

    # Generate test image for current panel
    print_step "Generating test image for ${PANEL} panel (pattern: ${PATTERN})"
    docker_run "python3 /project/gateware/gen_test_image.py --panel ${PANEL} --pattern ${PATTERN} -o /project/img_data.bin"

    print_step "Compiling firmware with cargo"
    docker_run "cd /project/${FIRMWARE_DIR} && cargo build --release"

    if [[ -f "${SCRIPT_DIR}/${FIRMWARE_BIN}" ]]; then
        print_success "Firmware built: ${FIRMWARE_BIN}"

        # Show firmware size
        SIZE=$(ls -lh "${SCRIPT_DIR}/${FIRMWARE_BIN}" | awk '{print $5}')
        echo "    Size: ${SIZE}"

        # Convert and copy to TFTP directory
        mkdir -p "${TFTP_DIR}"
        print_step "Converting ELF to raw binary for TFTP"
        docker_run "riscv-none-elf-objcopy -O binary /project/${FIRMWARE_BIN} /project/.tftp/boot.bin"
        local binsize=$(ls -lh "${TFTP_DIR}/boot.bin" | awk '{print $5}')
        print_success "TFTP ready: .tftp/boot.bin (${binsize})"
    else
        print_error "Firmware binary not found at expected location"
        exit 1
    fi
}

build_pac() {
    print_header "Regenerating Peripheral Access Crate (PAC)"
    check_docker_image

    print_step "Running svd2rust and form"
    docker_run "set -e && cd /project/sw_rust/litex-pac && \
        svd2rust -i colorlight.svd --target riscv && \
        rm -rf src && \
        form -i lib.rs -o src && \
        rm lib.rs && \
        echo 'PAC files generated:' && ls src/"

    print_success "PAC regenerated in sw_rust/litex-pac/src/"
}

program_sram() {
    print_header "Programming FPGA (SRAM - Temporary)"
    check_docker_image

    local bit=$(get_bitstream_path)
    if [[ -z "${bit}" ]]; then
        print_error "No bitstream found for panel ${PANEL}"
        print_warning "Run './build.sh bitstream' or './build.sh build-all' first"
        exit 1
    fi

    local rel_bit="${bit#${SCRIPT_DIR}/}"
    print_step "Loading ${rel_bit} to SRAM via ${CABLE} (panel: ${PANEL})"
    print_warning "This is temporary - configuration will be lost on power cycle"

    docker_run_usb "openFPGALoader --cable ${CABLE} /project/${rel_bit}"

    print_success "Bitstream loaded to SRAM"
}

program_flash() {
    print_header "Programming FPGA (Flash - Persistent)"
    check_docker_image

    local bit=$(get_bitstream_path)
    if [[ -z "${bit}" ]]; then
        print_error "No bitstream found for panel ${PANEL}"
        print_warning "Run './build.sh bitstream' or './build.sh build-all' first"
        exit 1
    fi

    local rel_bit="${bit#${SCRIPT_DIR}/}"
    print_step "Flashing ${rel_bit} to SPI flash via ${CABLE} (panel: ${PANEL})"
    print_warning "This will persist across power cycles"

    docker_run_usb "openFPGALoader --board colorlight --cable ${CABLE} -f --unprotect-flash /project/${rel_bit}"

    print_success "Bitstream written to flash"
}

is_tftp_running() {
    if [[ -f "${TFTP_DIR}/tftpd.pid" ]]; then
        local pid=$(cat "${TFTP_DIR}/tftpd.pid" 2>/dev/null)
        if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

stop_tftp() {
    if is_tftp_running; then
        local pid=$(cat "${TFTP_DIR}/tftpd.pid" 2>/dev/null)
        print_step "Stopping TFTP server (PID ${pid})"
        kill "${pid}" 2>/dev/null || true
        rm -f "${TFTP_DIR}/tftpd.pid"
        sleep 1
    fi
}

detect_host_ip() {
    if [[ -z "${HOST_IP}" ]]; then
        HOST_IP=$(ip -4 addr show | grep -oP '10\.11\.6\.\d+' | head -1)
        if [[ -z "${HOST_IP}" ]]; then
            HOST_IP=$(ip -4 addr show | grep -oP 'inet \K[\d.]+' | grep -v '^127\.' | head -1)
        fi
        if [[ -z "${HOST_IP}" ]]; then
            print_error "Could not auto-detect host IP. Use --host-ip option."
            exit 1
        fi
    fi
}

ensure_tftp() {
    # Already running — nothing to do
    if is_tftp_running; then
        local pid=$(cat "${TFTP_DIR}/tftpd.pid" 2>/dev/null)
        print_step "TFTP server already running (PID ${pid})"
        return 0
    fi

    # Need boot.bin to serve
    if [[ ! -f "${TFTP_DIR}/boot.bin" ]]; then
        print_warning "No boot.bin — skipping TFTP server"
        return 1
    fi

    # Check python3 and tftpy are available
    if ! python3 -c "import tftpy" 2>/dev/null; then
        print_warning "python3 tftpy not installed — skipping TFTP server (pip install tftpy)"
        return 1
    fi

    detect_host_ip

    mkdir -p "${TFTP_DIR}"
    print_step "Starting TFTP server on ${HOST_IP}"
    python3 "${SCRIPT_DIR}/tools/tftpd.py" \
        --root "${TFTP_DIR}" --host "${HOST_IP}" --port 6969 \
        --log "${TFTP_DIR}/tftpd.log" \
        --pid "${TFTP_DIR}/tftpd.pid" &
    disown

    sleep 1
    if is_tftp_running; then
        print_success "TFTP server started (PID $(cat ${TFTP_DIR}/tftpd.pid))"
    else
        print_warning "Could not start TFTP server (run manually: ./build.sh start)"
        return 1
    fi
}

do_boot() {
    print_header "Boot Sequence (SRAM + TFTP)"

    if [[ ! -f "${TFTP_DIR}/boot.bin" ]]; then
        print_error "No boot.bin found. Run './build.sh firmware' first."
        exit 1
    fi

    # Ensure TFTP server is running BEFORE programming SRAM
    ensure_tftp

    # Program SRAM (this triggers the board to request boot.bin)
    echo ""
    program_sram

    # Wait for TFTP transfer
    echo ""
    print_step "Waiting for firmware transfer..."
    sleep 3

    # Check if transfer happened
    if grep -q "boot.bin" "${TFTP_DIR}/tftpd.log" 2>/dev/null; then
        print_success "Firmware transferred successfully"
    else
        print_warning "Transfer not detected in log (may still have worked)"
    fi

    # Show log
    echo ""
    echo -e "${BLUE}TFTP Log:${NC}"
    cat "${TFTP_DIR}/tftpd.log" 2>/dev/null | tail -5

    # Test connectivity
    echo ""
    print_step "Testing connectivity..."
    sleep 2
    if ping -c 1 -W 2 "${IP_ADDRESS}" &>/dev/null; then
        print_success "Board responding at ${IP_ADDRESS}"
        echo ""
        echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
        echo -e "${GREEN}  Ready! Connect with: telnet ${IP_ADDRESS} 23${NC}"
        echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
    else
        print_warning "Board not responding to ping (may need more time)"
    fi

    echo ""
    print_success "Boot complete (TFTP server stays running — './build.sh stop' to stop)"
}

do_stop() {
    print_header "Stopping Services"
    stop_tftp
    print_success "Stopped"
}

do_start() {
    print_header "TFTP Server"
    ensure_tftp
}

build_all() {
    print_header "Building Everything"

    if ! docker image inspect ${DOCKER_IMAGE} &> /dev/null; then
        build_docker
    fi

    build_bitstream
    build_firmware

    print_success "All builds completed successfully"
}

show_help() {
    cat << 'EOF'
Colorlight HUB75 LED Controller - Build Script

USAGE:
    ./build.sh [OPTIONS] [TARGETS...]

TARGETS:
    docker          Build the Docker build environment
    bitstream       Build FPGA bitstream for current --panel (saved to bitstreams/)
    build-all       Build bitstreams for ALL panel sizes + firmware
    firmware        Build the Rust firmware
    pac             Regenerate the Peripheral Access Crate (after SoC changes)
    sram            Program FPGA via JTAG (temporary, uses --panel to select bitstream)
    flash           Program FPGA via JTAG (persistent, uses --panel to select bitstream)
    boot            Combined: program SRAM + ensure TFTP server
    start           Start TFTP server (if not already running)
    stop            Stop the background TFTP server
    all             Build docker (if needed), bitstream, and firmware

    If no target is specified, 'all' is assumed.

    The TFTP server is started automatically by 'boot'. It stays running
    in the background and is not restarted if already running. Use 'stop'
    to shut it down, or 'start' to launch it manually.

    Firmware is universal — one binary works for all panel sizes.
    Only bitstreams differ. Use 'build-all' to pre-build all panels.

OPTIONS:
    -h, --help              Show this help message
    -r, --revision REV      Board revision (default: 8.2)
    -i, --ip IP             IP address for firmware (default: 10.11.6.250)
    -c, --cable CABLE       JTAG cable type (default: usb-blaster)
    -p, --panel PANEL       Panel type: 128x64, 96x48, 64x32, 64x64 (default: 128x64)
    -t, --pattern PATTERN   Test pattern: grid, rainbow, solid_white, solid_red,
                            solid_green, solid_blue (default: grid)
    --host-ip IP            Host IP for TFTP server (auto-detected if not set)
    -v, --verbose           Enable verbose output

EXAMPLES:
    # Build everything (docker image if needed, bitstream, firmware)
    ./build.sh

    # Build bitstreams for ALL panel sizes + firmware
    ./build.sh build-all

    # Flash a specific panel's bitstream (no rebuild needed)
    ./build.sh --panel 96x48 flash

    # Build and boot a specific panel
    ./build.sh --panel 64x32 bitstream boot

    # Rebuild firmware only (universal, works with all panels)
    ./build.sh firmware

    # Regenerate PAC after modifying gateware/colorlight.py
    ./build.sh pac firmware

JTAG CABLES:
    usb-blaster     Intel/Altera USB Blaster (default)
    ft2232          FTDI FT2232-based cables
    dirtyjtag       DirtyJTAG open-source probe

WORKFLOW:
    1. First time setup:
       ./build.sh docker

    2. Development cycle:
       ./build.sh firmware boot
       # TFTP server auto-starts and stays running
       telnet 10.11.6.250 23

    3. Full rebuild and test:
       ./build.sh all boot

    4. Deploy to flash (once flash boot is fixed):
       ./build.sh flash

    5. After modifying gateware/colorlight.py (SoC changes):
       ./build.sh bitstream pac firmware

    6. Stop TFTP server when done:
       ./build.sh stop

TROUBLESHOOTING:
    - If 'sram' or 'flash' fails with permission errors:
      Ensure your user is in the 'plugdev' group or run with sudo

    - If Docker build fails:
      Check Dockerfile exists and Docker daemon is running

    - If bitstream build fails with timing errors:
      This is usually due to complex logic; may need to reduce features

For more information, see:
    - README.md      Project overview and quick start
    - ARCH.md        Architecture and internals
    - CHANGELOG.md   Version history

EOF
}

# =============================================================================
# Main
# =============================================================================

TARGETS=()
VERBOSE=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -r|--revision)
            REVISION="$2"
            shift 2
            ;;
        -i|--ip)
            IP_ADDRESS="$2"
            shift 2
            ;;
        -c|--cable)
            CABLE="$2"
            shift 2
            ;;
        --host-ip)
            HOST_IP="$2"
            shift 2
            ;;
        -p|--panel)
            PANEL="$2"
            shift 2
            ;;
        -t|--pattern)
            PATTERN="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=1
            set -x
            shift
            ;;
        -*)
            print_error "Unknown option: $1"
            echo "Run './build.sh --help' for usage"
            exit 1
            ;;
        *)
            TARGETS+=("$1")
            shift
            ;;
    esac
done

# Change to script directory
cd "${SCRIPT_DIR}"

# Check Docker is available
check_docker

# Default target is 'all'
if [[ ${#TARGETS[@]} -eq 0 ]]; then
    TARGETS=("all")
fi

# Execute targets in order
for target in "${TARGETS[@]}"; do
    case $target in
        docker)
            build_docker
            ;;
        bitstream|gateware|bit)
            build_bitstream
            ;;
        build-all)
            build_all_panels
            ;;
        firmware|rust|fw)
            build_firmware
            ;;
        pac)
            build_pac
            ;;
        sram|load)
            program_sram
            ;;
        flash|program)
            program_flash
            ;;
        tftp|serve|start)
            do_start
            ;;
        boot)
            do_boot
            ;;
        stop)
            do_stop
            ;;
        all)
            build_all
            ;;
        *)
            print_error "Unknown target: $target"
            echo "Run './build.sh --help' for available targets"
            exit 1
            ;;
    esac
done

echo ""
print_success "Done!"
