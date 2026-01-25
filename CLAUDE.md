# Development Context

> This file provides context for AI-assisted development. It documents architecture decisions, current state, and debugging knowledge accumulated during development.

## Project Status

| Component | Status | Notes |
|-----------|--------|-------|
| Bitstream | Working | 40MHz, passes timing |
| Firmware | Working | Rust, smoltcp TCP/IP |
| Ping | Working | Via smoltcp ICMP |
| Telnet | Working | Port 23, via TFTP boot |
| Flash Boot | **Broken** | Needs flash chip update for rev 8.2 |
| Art-Net | Partial | Palette works, pixels disabled |

## Architecture

### Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Colorlight 5A-75E                        │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  VexRiscv   │  │   LiteEth   │  │    HUB75 Driver     │  │
│  │    CPU      │◄─┤    MAC      │  │   (8 outputs)       │  │
│  │   40MHz     │  │             │  │                     │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│         ▼                ▼                     ▼             │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    Wishbone Bus                         ││
│  └─────────────────────────────────────────────────────────┘│
│         │                │                     │             │
│         ▼                ▼                     ▼             │
│  ┌───────────┐    ┌───────────┐         ┌───────────┐       │
│  │   SDRAM   │    │ SPI Flash │         │   CSRs    │       │
│  │   4MB     │    │    2MB    │         │           │       │
│  └───────────┘    └───────────┘         └───────────┘       │
└─────────────────────────────────────────────────────────────┘
```

### Network Stack

The firmware uses **smoltcp** (Rust TCP/IP stack) for all network handling:

- **LiteEth MAC** provides raw ethernet frame access via Wishbone
- **smoltcp** handles ARP, ICMP, TCP, UDP in software
- **No hardware ARP/ICMP** - different from etherbone approach

This design was chosen to enable TCP (telnet) which hardware-only stacks don't support.

## Key Files

| File | Purpose |
|------|---------|
| `colorlight.py` | LiteX SoC definition, peripheral instantiation |
| `hub75.py` | HUB75 display driver gateware |
| `smoleth.py` | Custom ethernet module (currently unused, kept for reference) |
| `sw_rust/barsign_disp/src/main.rs` | Firmware entry point, network loop |
| `sw_rust/barsign_disp/src/ethernet.rs` | smoltcp device driver |
| `sw_rust/litex-pac/` | Generated peripheral access crate |

## Build Commands

All builds use Docker for reproducibility:

```bash
# Build bitstream
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"

# Build firmware
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "cd /project/sw_rust/barsign_disp && cargo build --release"

# Regenerate PAC (after changing SoC)
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "cd /project/sw_rust/litex-pac && \
     svd2rust -i colorlight.svd --target riscv && \
     rm -rf src && form -i lib.rs -o src && rm lib.rs"

# Flash bitstream
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --cable usb-blaster -f --unprotect-flash \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"

# Load to SRAM (temporary)
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged \
    litex-hub75 "openFPGALoader --cable usb-blaster \
    /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

## Memory Map

| Region | Address | Size | Description |
|--------|---------|------|-------------|
| ROM | 0x00000000 | 64KB | LiteX BIOS |
| SRAM | 0x10000000 | 8KB | Stack/heap |
| Main RAM | 0x40000000 | 4MB | SDRAM, firmware runs here |
| EthMAC | 0x80000000 | 8KB | RX/TX buffers |
| SPI Flash | 0x80200000 | 2MB | Memory-mapped flash |
| Flash Boot | 0x80300000 | - | Firmware load address |
| CSR | 0xF0000000 | 64KB | Peripheral registers |

## Known Issues & Solutions

### Flash Boot Fails (rev 8.2)

**Symptom:** BIOS sends TFTP requests for `boot.bin` instead of loading from flash.

**Cause:** `colorlight.py` defines `GD25Q16` flash but rev 8.2 uses a different chip (likely W25Q32JV).

**Workaround:** Use TFTP boot:
```bash
# Start TFTP server
sudo dnsmasq --no-daemon --port=0 --enable-tftp \
    --tftp-root=/path/to/firmware --listen-address=<host-ip>

# Place firmware binary as boot.bin
cp sw_rust/barsign_disp/target/.../barsign-disp.bin /path/to/firmware/boot.bin
```

**Fix:** Update flash chip in `colorlight.py`:
```python
# Change from:
flash = GD25Q16(Codes.READ_1_1_1)
# To:
flash = W25Q32JV(Codes.READ_1_1_1)
```

### TCP Connection Timeout

**Symptom:** Ping works but telnet times out.

**Cause:** Socket `listen()` called repeatedly due to `&` vs `&&` operator.

**Fix:** In `main.rs`, change:
```rust
// Wrong - both sides always evaluated
if !socket.is_open() & socket.listen(23).is_err()

// Correct - short-circuit evaluation
if !socket.is_open() && socket.listen(23).is_err()
```

### ARP Shows Wrong MAC

**Symptom:** ARP reply shows `10:e2:d5:00:00:00` instead of configured MAC.

**Cause:** Response is from BIOS, not firmware. Firmware isn't running.

**Debug:** Check if firmware is actually loaded (see flash boot issue above).

## Debugging Without Serial

Since serial access isn't available, use these techniques:

1. **Check ARP MAC** - If MAC matches firmware config, firmware is running
   ```bash
   ping -c1 <ip> && arp -n <ip>
   ```

2. **tcpdump** - Watch for TFTP requests (means BIOS, not firmware)
   ```bash
   sudo tcpdump -i <iface> host <ip> and udp port 69
   ```

3. **HUB75 output** - `hub75.on()` is called at startup; display should activate

## Firmware Configuration

### IP Address
```rust
// sw_rust/barsign_disp/src/main.rs
let ip_data = IpData {
    ip: [10, 11, 6, 250],
};
```

### MAC Address
```rust
// sw_rust/barsign_disp/src/main.rs
let mac_bytes: [u8; 6] = [0x10, 0xe2, 0xd5, 0x00, 0x00, 0x01];
```

### System Clock
```rust
// Must match bitstream (40MHz default)
sys_clk: 40_000_000,
```

## Hardware Notes

### Colorlight 5A-75E V8.2

- FPGA: Lattice ECP5-25F (LFE5U-25F-6BG256C)
- SDRAM: M12L16161A (2M x 16bit)
- Flash: W25Q32JV (4MB) - **not GD25Q16**
- Ethernet PHY: RTL8211FD (RGMII)
- 2x RJ45 ports (active low accent)

### JTAG Programming

USB Blaster connected to J27:
- TDI, TDO, TCK, TMS standard JTAG

### HUB75 Outputs

8 outputs (active accent accent accent accent), accent chain of 4 panels each.
accent accent accent: J1-J8

## Version History

See [CHANGELOG.md](CHANGELOG.md) for detailed version history.
