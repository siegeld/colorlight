# Colorlight 5A-75E HUB75 LED Display Driver

Drive HUB75 LED matrix panels from Python using a Colorlight 5A-75E V8.2 FPGA board.

## Overview

```
┌──────────────┐    Ethernet     ┌─────────────┐     HUB75      ┌─────────────┐
│  Your PC     │ ──────────────► │ Colorlight  │ ─────────────► │ LED Panels  │
│  (Python)    │   UDP pixels    │   5A-75E    │                │  (Array)    │
└──────────────┘                 └─────────────┘                └─────────────┘
```

- Python generates pixel data on your PC
- Sends UDP packets over Ethernet to the Colorlight board
- FPGA firmware drives HUB75 panels directly
- 6-bit color depth (64 levels per channel)

## Quick Start

```bash
# 1. Run setup (installs dependencies, builds Docker image)
./setup.sh

# 2. Build the network firmware
./docker_build.sh firmware

# 3. Connect USB Blaster to JTAG pads, power the board

# 4. Flash the firmware
./flash.sh

# 5. Configure network
./network_setup.sh

# 6. Test with Python
source venv/bin/activate
python python/examples/test_connection.py
```

## Requirements

### Host System (your PC)

| Tool | Purpose | Install |
|------|---------|---------|
| Docker | Runs FPGA build tools | `sudo dnf install docker` |
| openFPGALoader | Flashes bitstreams to FPGA | `sudo dnf install openFPGALoader` |
| Python 3 | Runs display control scripts | Usually pre-installed |

The FPGA toolchain (yosys, nextpnr, prjtrellis) runs inside Docker - no need to install these on your system.

### Hardware

| Component | Description |
|-----------|-------------|
| Colorlight 5A-75E V8.2 | FPGA-based LED receiver card (Lattice ECP5-25F) |
| Altera USB Blaster | JTAG programmer for flashing firmware |
| 5V Power Supply | Powers the Colorlight board (2A recommended) |
| HUB75 LED Panel | 64x64 or 64x32 RGB LED matrix |
| Ethernet Cable | Connects PC to Colorlight (Gigabit required) |

## Helper Scripts

| Script | Purpose |
|--------|---------|
| `./setup.sh` | Install dependencies, setup Python venv, build Docker image |
| `./docker_build.sh` | Build firmware inside Docker container |
| `./flash.sh` | Flash firmware to the board |
| `./network_setup.sh` | Configure PC network for board communication |

### Building Firmware

```bash
# Build Docker image (automatic on first use)
./docker_build.sh build-image

# Build LED blink test (verify JTAG works)
./docker_build.sh blink

# Build HUB75 panel test (verify panel wiring)
./docker_build.sh hub75

# Build full network firmware
./docker_build.sh firmware

# Interactive shell in container (for debugging)
./docker_build.sh shell
```

### Flashing Firmware

```bash
# Detect FPGA (verify JTAG connection)
./flash.sh --detect

# Flash network firmware (default)
./flash.sh

# Flash specific firmware
./flash.sh blink
./flash.sh hub75
./flash.sh network

# Flash to SPI (persistent, survives power cycle)
./flash.sh --permanent
./flash.sh network --permanent
```

### Network Setup

```bash
# Configure network interface
./network_setup.sh

# Use specific interface
./network_setup.sh eth0

# Check status
./network_setup.sh --status

# Remove configuration
./network_setup.sh --reset
```

## Hardware Setup

### JTAG Wiring (USB Blaster to Colorlight 5A-75E)

The JTAG pads are located near the ECP5 FPGA chip on the board.

| Colorlight Pad | Signal | USB Blaster Pin |
|----------------|--------|-----------------|
| J30 | TDO | 3 |
| J32 | TDI | 9 |
| J31 | TMS | 5 |
| J27 | TCK | 1 |
| J33 | 3V3 | 7 (optional, can use board power) |
| J34 | GND | 2, 10 |

### HUB75 Panel Connection

Connect your LED panel to the **J1** connector. The firmware currently supports one 64x64 panel.

HUB75 signals on J1:
- R0, G0, B0: Upper half RGB data
- R1, G1, B1: Lower half RGB data
- A, B, C, D, E: Row address (E for 1/32 scan panels)
- CLK: Pixel clock
- LAT: Latch signal
- OE: Output enable

## Network Configuration

| Setting | Value |
|---------|-------|
| Board IP | 192.168.178.50 |
| Host IP | 192.168.178.100 (configured by network_setup.sh) |
| UDP Port | 6000 |
| Protocol | UDP, 5 bytes per pixel |

### Pixel Protocol

Each UDP packet contains pixel data:

```
Byte 0: Y coordinate (0-63)
Byte 1: X coordinate (0-63)
Byte 2: Red (0-63, 6-bit)
Byte 3: Green (0-63, 6-bit)
Byte 4: Blue (0-63, 6-bit)
```

## Testing Sequence

Follow this sequence to verify each component works:

### 1. Blink Test (verify JTAG)

```bash
./docker_build.sh blink
./flash.sh blink
```

**Success:** Onboard LED blinks at ~1Hz

### 2. HUB75 Test (verify panel wiring)

```bash
./docker_build.sh hub75
./flash.sh hub75
```

**Success:** Color bars appear on panel

### 3. Network Test (full functionality)

```bash
./docker_build.sh firmware
./flash.sh
./network_setup.sh
source venv/bin/activate
python python/examples/test_connection.py
```

**Success:** Patterns appear on panel

## Python Library

### Setup

```bash
source venv/bin/activate  # Created by setup.sh
```

### Basic Usage

```python
from colorlight import ColorlightDisplay

# Create display (uses default IP 192.168.178.50, port 6000)
display = ColorlightDisplay()

# Set individual pixels (6-bit color: 0-63)
display.set_pixel(10, 10, 63, 0, 0)   # Red pixel
display.set_pixel(20, 20, 0, 63, 0)   # Green pixel
display.set_pixel(30, 30, 0, 0, 63)   # Blue pixel

# Fill entire display
display.fill(63, 63, 63)  # White

# Clear display
display.clear()

display.close()
```

### Using 8-bit Colors

The library automatically converts 8-bit (0-255) to 6-bit (0-63):

```python
# These are equivalent:
display.set_pixel(0, 0, 255, 128, 0)  # 8-bit values
display.set_pixel(0, 0, 63, 32, 0)    # 6-bit values
```

### Frame Buffer with NumPy

```python
from colorlight import ColorlightFrameBuffer
from PIL import Image
import numpy as np

# Create frame buffer
fb = ColorlightFrameBuffer()

# Load and display an image
fb.load_image("artwork.png")
fb.send()

# Or work with numpy arrays directly
fb.buffer = np.zeros((64, 64, 3), dtype=np.uint8)
fb.buffer[32, 32] = [255, 0, 0]  # Red pixel at center
fb.send()
```

### Example Scripts

```bash
source venv/bin/activate

# Test connection with patterns
python python/examples/test_connection.py

# Animated plasma effect
python python/examples/plasma.py

# Display an image
python python/examples/show_image.py /path/to/image.png
python python/examples/show_image.py image.jpg --loop --interval 0.5
```

## Project Structure

```
colorlight/
├── README.md                 # This file
├── Dockerfile                # FPGA toolchain container
├── docker_build.sh           # Build firmware in Docker
├── flash.sh                  # Flash firmware to board
├── setup.sh                  # Initial setup script
├── network_setup.sh          # Configure network interface
├── requirements.txt          # Python dependencies
├── venv/                     # Python virtual environment
│
├── firmware/
│   ├── build/                # Build outputs
│   │   ├── blink_test/       # blink_test.bit
│   │   ├── hub75_test/       # hub75_test.bit
│   │   └── network/          # top.bit, top.svf
│   ├── src/                  # Test firmware sources
│   ├── constraints/          # Pin mapping files
│   └── colorlight-led-cube/  # Network firmware source
│
├── python/
│   ├── colorlight.py         # Main display driver library
│   └── examples/             # Example scripts
│
└── docs/
    └── chubby75/             # Hardware pinout documentation
```

## Changing the IP Address

The IP address is hardcoded in the firmware. To change it:

1. Edit `firmware/colorlight-led-cube/fpga/liteeth_core.v`
2. Search for IP address bytes (192.168.178.50 = 0xC0, 0xA8, 0xB2, 0x32)
3. Rebuild: `./docker_build.sh firmware`
4. Reflash: `./flash.sh`

Also update the Python library default in `python/colorlight.py`.

## Troubleshooting

### JTAG Not Detected

```bash
# Check USB device
lsusb | grep -i altera

# Detect with flash.sh
./flash.sh --detect
```

- Verify wiring connections (see JTAG table above)
- Ensure 5V power is connected to Colorlight
- Check that USB Blaster LED is on
- Run `./setup.sh` to install udev rules

### No Display Output

- Run blink test first to verify JTAG works
- Check HUB75 ribbon cable orientation (pin 1 marker)
- Verify panel power (often needs separate 5V supply)
- Run hub75 test to verify wiring

### No Network Response

```bash
# Check network status
./network_setup.sh --status
```

- Verify both devices on same subnet (192.168.178.x)
- Check Ethernet cable (use port near power connector on board)
- Board requires Gigabit Ethernet (no 10/100 fallback)
- Use Wireshark to verify packets are being sent
- Make sure network firmware is flashed (not blink/hub75 test)

### Docker Build Fails

```bash
# Check Docker is running
docker info

# Rebuild image from scratch
./docker_build.sh build-image

# Debug interactively
./docker_build.sh shell
```

## Technical Details

### FPGA

- **Chip**: Lattice ECP5-25F (LFE5U-25F-6BG256C)
- **Package**: 256-ball BGA
- **Logic Cells**: ~24K LUTs
- **Block RAM**: 56 x 18Kb = 1008Kb

### Clocks

- 25MHz oscillator on board
- PLL generates:
  - 125MHz system clock
  - 52MHz panel clock

### Ethernet

- RTL8211FD Gigabit PHY
- RGMII interface
- LiteEth UDP/IP stack
- 1000Mbps only

## References

- [chubby75](https://github.com/q3k/chubby75) - Hardware reverse engineering and pinouts
- [colorlight-led-cube](https://github.com/lucysrausch/colorlight-led-cube) - Base firmware project
- [LiteX](https://github.com/enjoy-digital/litex) - SoC builder framework
- [Project Trellis](https://github.com/YosysHQ/prjtrellis) - ECP5 bitstream documentation
- [Yosys](https://github.com/YosysHQ/yosys) - Open source synthesis
- [nextpnr](https://github.com/YosysHQ/nextpnr) - Open source place and route
