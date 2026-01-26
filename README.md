# Colorlight HUB75 LED Controller

[![License](https://img.shields.io/badge/license-BSD--2--Clause-blue.svg)](LICENSE)
[![FPGA](https://img.shields.io/badge/FPGA-Lattice%20ECP5-green.svg)](https://www.latticesemi.com/Products/FPGAandCPLD/ECP5)
[![Board](https://img.shields.io/badge/Board-Colorlight%205A--75E-orange.svg)](http://www.colorlight-led.com/)

A complete FPGA-based LED panel controller for **HUB75** displays, built on the **Colorlight 5A-75E** receiver card. Features a LiteX SoC with VexRiscv CPU, Ethernet connectivity, and a Rust-based firmware with telnet management console.

## Features

- **HUB75 LED Panel Driver** - Supports up to 8 output chains, 4 panels per chain
- **DHCP Networking** - Automatic IP via DHCP with unique MAC from SPI flash
- **TFTP Boot Config** - Per-board YAML layout config fetched at boot via `<mac>.yml`
- **HTTP REST API** - Web status page and JSON API on port 80
- **Bitmap UDP Protocol** - Send RGB images over UDP port 7000
- **Telnet Console** - Remote configuration and management on port 23
- **Double-Buffered Animation** - Tear-free 30fps display updates
- **Multi-Panel Virtual Display** - Configurable grid layout across multiple panels
- **Rust Firmware** - Type-safe embedded development with smoltcp TCP/IP stack

## Hardware Requirements

| Component | Specification |
|-----------|---------------|
| FPGA Board | Colorlight 5A-75E V8.2 (Lattice ECP5-25F) |
| Programmer | USB Blaster, FTDI FT2232, or compatible JTAG |
| LED Panels | HUB75/HUB75E compatible (96x48, 128x64, 64x32, 64x64) |
| Network | 100Mbps Ethernet |

## Quick Start

### Prerequisites

- Docker (for reproducible builds)
- USB Blaster or compatible JTAG programmer
- Network connection to the board

### Using the Build Script (Recommended)

The `build.sh` script simplifies the entire workflow:

```bash
# Build everything (Docker image, bitstream, firmware)
./build.sh

# Build for a specific panel type
./build.sh --panel 128x64 bitstream firmware

# Or step by step
./build.sh docker      # Build Docker environment
./build.sh bitstream   # Build FPGA bitstream
./build.sh firmware    # Build Rust firmware
./build.sh boot        # Program SRAM + boot via TFTP
./build.sh flash       # Program to SPI flash (persistent)

# Show all options
./build.sh --help
```

### Supported Panels

| Panel | Scan Rate | Notes |
|-------|-----------|-------|
| 96x48 | 1/24 | Default configuration |
| 128x64 | 1/32 | Large format |
| 64x32 | 1/16 | Compact |
| 64x64 | 1/32 | Square format |

### Manual Build Steps

If you prefer manual control:

#### 1. Build Docker Environment

```bash
docker build -t litex-hub75 .
```

#### 2. Build Bitstream & Firmware

```bash
# Build FPGA bitstream
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"

# Build Rust firmware
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "cd /project/sw_rust/barsign_disp && cargo build --release"
```

#### 3. Program the FPGA

```bash
# Flash bitstream to SPI (persistent across power cycles)
# NOTE: --board colorlight is required for correct flash chip handling
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --board colorlight --cable usb-blaster \
    -f --unprotect-flash \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"

# Or load to SRAM (temporary, for testing)
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --cable usb-blaster \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

### Test Connection

```bash
# Test ping (IP assigned by DHCP — check your DHCP server for the lease)
ping <board-ip>

# Connect via telnet
telnet <board-ip> 23

# View web status page
curl http://<board-ip>/
```

## Project Structure

```
colorlight/
├── build.sh               # Build script (run ./build.sh --help)
├── colorlight.py          # LiteX SoC definition
├── hub75.py               # HUB75 display driver (gateware)
├── gen_test_image.py      # Test pattern generator
├── Dockerfile             # Build environment
├── sw_rust/               # Rust firmware
│   ├── barsign_disp/      # Main application
│   ├── litex-pac/         # Peripheral Access Crate
│   └── smoltcp-0.8.0/     # Patched smoltcp (DHCP siaddr support)
├── tools/                 # Python tools (send_image, send_animation, etc.)
├── .tftp/                 # TFTP-served config files (<mac>.yml)
├── CLAUDE.md              # Development context
└── CHANGELOG.md           # Version history
```

## Telnet Commands

Connect via `telnet <ip> 23` to access the management console:

| Command | Description |
|---------|-------------|
| `help` | Show available commands |
| `on` / `off` | Enable/disable display output |
| `reboot` | Restart the system |
| `get_image_param` | Show current image dimensions |
| `set_image_param <w> <h>` | Set image dimensions |
| `get_panel_param <out> <chain>` | Get panel configuration |
| `set_panel_param <out> <chain> <x> <y> <rot>` | Configure panel position |
| `load_spi_image` | Load image from flash |
| `save_spi_image` | Save image to flash |

## Memory Map

| Region | Address | Size | Description |
|--------|---------|------|-------------|
| ROM | 0x00000000 | 64KB | BIOS |
| SRAM | 0x10000000 | 8KB | Stack/heap |
| Main RAM | 0x40000000 | 4MB | SDRAM |
| SPI Flash | 0x80200000 | 2MB | Bitstream + firmware |
| CSR | 0xF0000000 | 64KB | Peripheral registers |

## Configuration

### IP Address (DHCP)

The firmware acquires its IP address via DHCP at boot. If no DHCP server responds within 10 seconds, it falls back to `10.11.6.250/24`. Check your DHCP server's lease table to find the board's IP, or use the board's MAC address (`02:xx:xx:xx:xx:xx`) to assign a fixed lease.

### Panel Layout (TFTP Boot Config)

At boot, the firmware fetches a per-board YAML config file from the TFTP server (the DHCP `siaddr`). The filename is the board's MAC address: e.g., `02-78-7b-21-ae-53.yml`.

Example config for two vertically-stacked 96x48 panels:

```yaml
grid: 1x2
panel_width: 96
panel_height: 48
J1: 0,0
J2: 0,1
```

Place config files in your TFTP root directory. The layout is applied automatically at boot.

Panel layout can also be configured at runtime via the HTTP API (`POST /api/layout`) or telnet commands.

## Development

See [CLAUDE.md](CLAUDE.md) for detailed development notes, debugging tips, and architecture documentation.

### Building from Source

The Docker environment includes all dependencies:
- Yosys, nextpnr-ecp5, Trellis (FPGA toolchain)
- LiteX, Migen (SoC framework)
- Rust with riscv32i target (firmware)
- openFPGALoader (programming)

### Running Tests

```bash
# Test network connectivity
ping <board-ip>

# Test telnet
telnet <board-ip> 23
```

## Boot Workflow

1. **Power on** — BIOS loads bitstream from SPI flash
2. **BIOS TFTP** — BIOS fetches `boot.bin` firmware from TFTP server
3. **Firmware starts** — DHCP acquires IP and unique MAC from flash UID
4. **Config fetch** — Firmware fetches `<mac>.yml` from TFTP server
5. **Layout applied** — Panel grid configured and display redrawn

The bitstream is flashed permanently to SPI (`./build.sh flash`). Firmware is loaded via TFTP on each boot.

## Pre-built Binaries

The repo includes pre-built binaries so you can flash and boot without rebuilding:

| File | Description |
|------|-------------|
| `colorlight.bit` | FPGA bitstream for Colorlight 5A-75E v8.2 |
| `barsign-disp.bin` | Rust firmware binary (rename to `boot.bin` for TFTP) |

```bash
# Flash bitstream permanently
./build.sh flash

# Or flash manually (--board colorlight is required)
openFPGALoader --board colorlight --cable usb-blaster -f --unprotect-flash colorlight.bit

# Serve firmware via TFTP
cp barsign-disp.bin /path/to/tftp/boot.bin
```

## Known Issues

- **Art-Net**: Palette updates work, direct pixel writes commented out
- **TFTP server IP**: Currently hardcoded; planned to use DHCP `siaddr` field

See [CHANGELOG.md](CHANGELOG.md) for version history and fixes.

## License

BSD-2-Clause. See individual files for specific attributions.

Based on work by:
- [DerFetzer/colorlight-litex](https://github.com/DerFetzer/colorlight-litex) - Original LiteX implementation
- [q3k/chubby75](https://github.com/q3k/chubby75) - Colorlight reverse engineering
- [enjoy-digital/litex](https://github.com/enjoy-digital/litex) - LiteX SoC framework

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

Please follow existing code style and include tests where applicable.
