# Colorlight 5A-75E LED Panel Projects

This repository contains two different FPGA projects for driving HUB75 LED panels using a **Colorlight 5A-75E V8.2** board.

## Projects Overview

| Project | Location | Status | Features |
|---------|----------|--------|----------|
| **hub75_sawatzke** | `hub75_sawatzke/` | **WORKING** | LiteX SoC, VexRiscv CPU, Ethernet, ICMP ping, Etherbone |
| **colorlight-led-cube** | `firmware/colorlight-led-cube/` | Timing issues | Verilog-only, simpler, but 125MHz fails timing |

**Recommendation:** Use the `hub75_sawatzke` project - it works reliably at 40MHz and passes timing.

---

## Quick Start (hub75_sawatzke)

### 1. Build the Bitstream

```bash
cd hub75_sawatzke

# Build Docker image (first time only)
docker build -t litex-hub75 .

# Build bitstream with your desired IP address
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"
```

### 2. Program the FPGA

```bash
# Using USB Blaster (Altera) programmer:
docker run --rm \
    -v "$(pwd):/project" \
    -v /dev/bus/usb:/dev/bus/usb \
    --privileged \
    litex-hub75 \
    "openFPGALoader --cable usb-blaster /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

### 3. Test Network

```bash
# Clear any stale ARP entries
sudo arp -d 10.11.6.250 2>/dev/null

# Test ping
ping 10.11.6.250

# Expected: 0% packet loss, ~0.3ms latency
```

### 4. Test Etherbone (CSR Access)

```bash
docker run --rm --network host -v "$(pwd):/project" litex-hub75 \
    "litex_server --udp --udp-ip 10.11.6.250 &
     sleep 2
     litex_cli --csr-csv /project/build/colorlight_5a_75e/csr.csv --regs | head -20"
```

---

## Hardware Requirements

| Component | Description |
|-----------|-------------|
| **Colorlight 5A-75E V8.2** | FPGA board with Lattice ECP5-25F |
| **Altera USB Blaster** | JTAG programmer (09fb:6001) |
| **5V Power Supply** | Powers the board (2A minimum) |
| **HUB75 LED Panel** | 64x64 RGB panel (1/32 scan, ABCDE addressing) |
| **Ethernet Cable** | Gigabit connection to your PC |

### V8.2 Specifications

- **FPGA:** Lattice LFE5U-25F-7BG256I
- **Ethernet PHY:** Broadcom B50612D (not RTL8211!)
- **Clock:** 25MHz oscillator
- **SDRAM:** 4MB

---

## JTAG Wiring (USB Blaster to Colorlight)

The JTAG pads are near the ECP5 chip:

| Colorlight Pad | Signal | USB Blaster Pin |
|----------------|--------|-----------------|
| J30 | TDO | 3 |
| J32 | TDI | 9 |
| J31 | TMS | 5 |
| J27 | TCK | 1 |
| J33 | 3V3 | 7 (optional) |
| J34 | GND | 2, 10 |

---

## Network Configuration

### Default Settings (hub75_sawatzke)

| Parameter | Value |
|-----------|-------|
| Board IP | Set via `--ip-address` (e.g., 10.11.6.250) |
| Etherbone MAC | 10:e2:d5:00:00:00 |
| Etherbone Port | 1234 (UDP) |

**Note:** Telnet is not currently working. The `with_ethmac=True` option (needed for CPU network access) breaks ARP. This requires further debugging.

### Changing IP Address

Edit the build command:
```bash
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address YOUR.IP.HERE --build"
```

---

## hub75_sawatzke Project Details

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     LiteX SoC (40MHz)                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ VexRiscv │  │  SDRAM   │  │  SPI     │  │   HUB75     │ │
│  │   CPU    │  │  4MB     │  │  Flash   │  │   Driver    │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────────┘ │
│        │              │             │              │        │
│        └──────────────┼─────────────┼──────────────┘        │
│                       │             │                        │
│              ┌────────┴─────────────┴────────┐              │
│              │      Wishbone Bus             │              │
│              └───────────────────────────────┘              │
│                           │                                  │
│              ┌────────────┴────────────┐                    │
│              │   LiteEth UDP/IP Stack  │                    │
│              │   - ICMP (ping)         │                    │
│              │   - Etherbone           │                    │
│              │   - EthMAC (optional)   │                    │
│              └────────────┬────────────┘                    │
│                           │                                  │
│              ┌────────────┴────────────┐                    │
│              │  RGMII PHY (B50612D)    │                    │
│              │  tx_delay=0, rx_delay=2ns│                    │
│              └─────────────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

### Key Files

| File | Purpose |
|------|---------|
| `colorlight.py` | Main LiteX SoC definition |
| `hub75.py` | HUB75 panel driver module |
| `helper.py` | Pin definitions for HUB75 connectors |
| `Dockerfile` | Build environment with LiteX toolchain |
| `build/colorlight_5a_75e/` | Build outputs (bitstream, CSR files) |

### Memory Map

| Region | Address | Size | Description |
|--------|---------|------|-------------|
| ROM | 0x00000000 | 64KB | Boot ROM |
| SRAM | 0x10000000 | 8KB | Fast internal RAM |
| Main RAM | 0x40000000 | 4MB | SDRAM |
| SPI Flash | 0x80200000 | 2MB | External flash |
| EthMAC RX | 0x80000000 | 4KB | Ethernet RX buffers |
| EthMAC TX | 0x80001000 | 4KB | Ethernet TX buffers |
| Uncached RAM | 0x90000000 | 4MB | SDRAM (uncached mirror) |
| CSR | 0xF0000000 | 64KB | Control/Status registers |

### Building from Source

```bash
cd hub75_sawatzke

# Full build with custom IP
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"

# Build outputs:
#   build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit  (bitstream)
#   build/colorlight_5a_75e/gateware/colorlight_5a_75e.svf  (SVF format)
#   build/colorlight_5a_75e/csr.csv                        (register map)
#   sw_rust/litex-pac/colorlight.svd                       (SVD for Rust)
```

---

## Programming Methods

### Method 1: openFPGALoader with USB Blaster (Recommended)

```bash
# Detect FPGA
docker run --rm -v /dev/bus/usb:/dev/bus/usb --privileged litex-hub75 \
    "openFPGALoader --cable usb-blaster --detect"

# Program SRAM (temporary, lost on power cycle)
docker run --rm \
    -v "$(pwd):/project" \
    -v /dev/bus/usb:/dev/bus/usb \
    --privileged \
    litex-hub75 \
    "openFPGALoader --cable usb-blaster /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"

# Program SPI Flash (permanent)
docker run --rm \
    -v "$(pwd):/project" \
    -v /dev/bus/usb:/dev/bus/usb \
    --privileged \
    litex-hub75 \
    "openFPGALoader --cable usb-blaster -f /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

### Method 2: ecpprog with FTDI Programmer

If you have an FTDI-based programmer (not USB Blaster):

```bash
docker run --rm \
    -v "$(pwd):/project" \
    -v /dev/bus/usb:/dev/bus/usb \
    --privileged \
    litex-hub75 \
    "ecpprog -S /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

---

## Troubleshooting

### Ping Doesn't Work

1. **Check programmer is connected:** `lsusb | grep -i "altera\|ftdi"`
2. **Reload bitstream** (SRAM is lost on power cycle)
3. **Clear stale ARP:** `sudo arp -d <board-ip>`
4. **Verify network interface:** `ip addr show` - must be on same subnet
5. **Check ARP table:** `arp -a | grep <board-ip>` - should show board MAC

### FPGA Not Detected

```bash
# Check USB device
lsusb | grep -i "altera\|09fb"

# Expected: Bus 001 Device 006: ID 09fb:6001 Altera Blaster

# Try detect with openFPGALoader
docker run --rm -v /dev/bus/usb:/dev/bus/usb --privileged litex-hub75 \
    "openFPGALoader --cable usb-blaster --detect"
```

### Build Fails with Memory Overlap

If you see "Region overlap between spiflash and ethmac":
- This is fixed in current code - spiflash is at 0x80200000
- If using old code, set spiflash origin to 0x80200000 (2MB aligned)

### Timing Fails

- Ensure `--revision 8.2` matches your board
- System runs at 40MHz (passes timing)
- If using 125MHz, expect failures (~58MHz achievable)

---

## Original colorlight-led-cube Project

Located in `firmware/colorlight-led-cube/`. This is a simpler Verilog-only project but has timing issues at 125MHz.

**Status:** Not recommended - use hub75_sawatzke instead.

---

## Directory Structure

```
colorlight/
├── README.md                    # This file
├── hub75_sawatzke/              # RECOMMENDED PROJECT
│   ├── colorlight.py            # LiteX SoC definition
│   ├── hub75.py                 # HUB75 driver
│   ├── Dockerfile               # Build environment
│   ├── docker_build.sh          # Build script
│   └── build/                   # Build outputs
│
├── firmware/                    # Original project (timing issues)
│   └── colorlight-led-cube/     # Verilog sources
│
├── litex/                       # LiteX framework
├── liteeth/                     # Ethernet stack
├── litedram/                    # SDRAM controller
├── migen/                       # HDL generator
└── [other litex deps]/          # Various LiteX dependencies
```

---

## References

- [hub75_colorlight75_stuff](https://github.com/david-sawatzke/hub75_colorlight75_stuff) - Original project by David Sawatzke
- [colorlight-led-cube](https://github.com/lucysrausch/colorlight-led-cube) - Original Verilog project
- [chubby75](https://github.com/q3k/chubby75) - Hardware reverse engineering
- [LiteX](https://github.com/enjoy-digital/litex) - SoC builder
- [openFPGALoader](https://github.com/trabucayre/openFPGALoader) - FPGA programming tool
