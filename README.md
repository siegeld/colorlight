# Colorlight HUB75 LED Controller

[![License](https://img.shields.io/badge/license-BSD--2--Clause-blue.svg)](LICENSE)
[![FPGA](https://img.shields.io/badge/FPGA-Lattice%20ECP5-green.svg)](https://www.latticesemi.com/Products/FPGAandCPLD/ECP5)
[![Board](https://img.shields.io/badge/Board-Colorlight%205A--75E-orange.svg)](http://www.colorlight-led.com/)

A complete FPGA-based LED panel controller for **HUB75** displays, built on the **Colorlight 5A-75E** receiver card. Features a LiteX SoC with VexRiscv CPU, Ethernet connectivity, and a Rust-based firmware with telnet management console.

## Features

- **HUB75 LED Panel Driver** - Supports up to 8 output chains, 4 panels per chain
- **Ethernet Connectivity** - Static IP, ARP, ICMP (ping), TCP
- **Telnet Console** - Remote configuration and management on port 23
- **Art-Net Support** - DMX over Ethernet for real-time control (UDP port 6454)
- **Dual Display Modes** - Full-color (24-bit) and indexed (8-bit with palette)
- **Persistent Storage** - Save/load images and configuration to SPI flash
- **Rust Firmware** - Type-safe embedded development with smoltcp TCP/IP stack

## Hardware Requirements

| Component | Specification |
|-----------|---------------|
| FPGA Board | Colorlight 5A-75E V8.2 (Lattice ECP5-25F) |
| Programmer | USB Blaster, FTDI FT2232, or compatible JTAG |
| LED Panels | HUB75/HUB75E compatible (tested with 128x64) |
| Network | 100Mbps Ethernet |

## Quick Start

### Prerequisites

- Docker (for reproducible builds)
- USB Blaster or compatible JTAG programmer
- Network connection to the board

### 1. Build Docker Environment

```bash
docker build -t litex-hub75 .
```

### 2. Build Bitstream & Firmware

```bash
# Build FPGA bitstream
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"

# Build Rust firmware
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "cd /project/sw_rust/barsign_disp && cargo build --release"
```

### 3. Program the FPGA

```bash
# Flash bitstream to SPI (persistent)
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --cable usb-blaster -f --unprotect-flash \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"

# Or load to SRAM (temporary, for testing)
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --cable usb-blaster \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

### 4. Test Connection

```bash
# Test ping
ping 10.11.6.250

# Connect via telnet
telnet 10.11.6.250 23
```

## Project Structure

```
colorlight/
├── colorlight.py          # LiteX SoC definition
├── hub75.py               # HUB75 display driver (gateware)
├── smoleth.py             # Ethernet module
├── Dockerfile             # Build environment
├── sw_rust/               # Rust firmware
│   ├── barsign_disp/      # Main application
│   └── litex-pac/         # Peripheral Access Crate
├── scripts/               # Helper scripts
├── legacy/                # Archived old projects
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

### IP Address

Edit the IP in `sw_rust/barsign_disp/src/main.rs`:

```rust
let ip_data = IpData {
    ip: [10, 11, 6, 250],  // Change this
};
```

### Panel Layout

Configure via telnet or edit defaults in firmware. Each output supports a chain of up to 4 panels with configurable X, Y position and rotation.

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

## Known Issues

- **Flash boot**: Currently requires TFTP boot on rev 8.2 boards (flash chip mismatch)
- **Art-Net**: Palette updates work, direct pixel writes commented out

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
