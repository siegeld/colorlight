# Claude Context File - hub75_sawatzke Project

## Current State (2026-01-25)

### What Works
- **Bitstream builds** at 40MHz, passes timing
- **FPGA programming** via USB Blaster with openFPGALoader
- **Rust firmware builds** for riscv32i-unknown-none-elf
- **SPI Flash programming** for persistent storage

### What Was Fixed (2026-01-25)
- **Timing bug** - smoltcp was getting fake timestamps, causing ARP/TCP timeouts to fail
- **MAC address consistency** - Now using fixed MAC (10:e2:d5:00:00:01) matching hardware default
- **Hardware deadlock** - Art-Net packets could block all ethernet; added null sink for udp.source

### What Needs Testing
- **Network (ARP/ping/telnet)** - Should work after rebuild with fixes above
- Rebuild both bitstream (smoleth.py changed) and firmware (main.rs changed)

## Architecture Overview

The project uses **SmolEth** instead of standard LiteEth:
- SmolEth provides raw MAC access to the CPU
- ARP/ICMP/TCP handled by **Rust firmware using smoltcp**
- Hardware only handles: MAC layer, UDP depacketizer for Art-Net port 6454

This is different from standard LiteX etherbone which handles ARP/ICMP in hardware.

## Key Files

| File | Purpose |
|------|---------|
| `colorlight.py` | LiteX SoC definition with SmolEth |
| `smoleth.py` | Custom ethernet module (raw MAC + UDP depacketizer) |
| `sw_rust/barsign_disp/` | Rust firmware (telnet, ARP via smoltcp) |
| `sw_rust/litex-pac/` | Rust PAC generated from SVD |

## Build Commands

### 1. Build Bitstream
```bash
cd hub75_sawatzke
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"
```

### 2. Build Rust Firmware
```bash
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "cd /project/sw_rust/barsign_disp && cargo build --release"
```

### 3. Convert ELF to Binary
```bash
docker run --rm -v "$(pwd):/project" litex-hub75 \
    "/opt/xpack-riscv/bin/riscv-none-elf-objcopy \
    /project/sw_rust/barsign_disp/target/riscv32i-unknown-none-elf/release/barsign-disp \
    -O binary \
    /project/sw_rust/barsign_disp/target/riscv32i-unknown-none-elf/release/barsign-disp.bin"
```

### 4. Create Boot Image with Header
The LiteX BIOS expects: 4 bytes length + 4 bytes CRC32 + binary data

```python
import struct, zlib
with open('barsign-disp.bin', 'rb') as f:
    data = f.read()
crc = zlib.crc32(data) & 0xffffffff
header = struct.pack('<II', len(data), crc)
with open('barsign-disp-flash.bin', 'wb') as f:
    f.write(header + data)
```

### 5. Flash Everything to SPI
```bash
# Flash bitstream at offset 0
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged litex-hub75 \
    "openFPGALoader --cable usb-blaster -f --unprotect-flash /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"

# Flash firmware at offset 0x100000 (FLASH_BOOT_ADDRESS)
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged litex-hub75 \
    "openFPGALoader --cable usb-blaster -f --unprotect-flash -o 0x100000 /project/barsign-disp-flash.bin"

# Load bitstream to SRAM for immediate test
docker run --rm -v "$(pwd):/project" -v /dev/bus/usb:/dev/bus/usb --privileged litex-hub75 \
    "openFPGALoader --cable usb-blaster /project/build/colorlight_5a_75e/gateware/colorlight_5a_75e.bit"
```

## Memory Map

| Region | CPU Address | SPI Offset | Size | Content |
|--------|-------------|------------|------|---------|
| ROM | 0x00000000 | - | 64KB | BIOS |
| SRAM | 0x10000000 | - | 8KB | Stack/heap |
| Main RAM | 0x40000000 | - | 4MB | Firmware runs here |
| EthMAC RX | 0x80000000 | - | 4KB | Ethernet RX buffers |
| EthMAC TX | 0x80001000 | - | 4KB | Ethernet TX buffers |
| SPI Flash | 0x80200000 | 0x000000 | 2MB | Bitstream + firmware |
| Flash Boot | 0x80300000 | 0x100000 | - | Firmware location |
| CSR | 0xF0000000 | - | 64KB | Peripheral registers |

## Rust Firmware Details

### IP Address Configuration
Edit `sw_rust/barsign_disp/src/main.rs`:
```rust
let ip_data = IpData {
    ip: [10, 11, 6, 250],  // Change this
};
```

### System Clock
```rust
let mut delay = TIMER {
    registers: peripherals.timer0,
    sys_clk: 40_000_000,  // Must match bitstream
};
```

### Memory Layout
Edit `sw_rust/barsign_disp/regions.ld` if memory map changes:
```
MEMORY {
    rom : ORIGIN = 0x00000000, LENGTH = 0x00010000
    sram : ORIGIN = 0x10000000, LENGTH = 0x00002000
    spiflash : ORIGIN = 0x80200000, LENGTH = 0x00200000
    main_ram : ORIGIN = 0x40000000, LENGTH = 0x00400000
    ethmac : ORIGIN = 0x80000000, LENGTH = 0x00001000
    ethmac_tx : ORIGIN = 0x80001000, LENGTH = 0x00001000
    csr : ORIGIN = 0xf0000000, LENGTH = 0x00010000
}
```

## Debugging Issues

### Problem: Firmware doesn't respond to network
**Symptoms:**
- ARP requests get no response
- Ping shows "Destination Host Unreachable"

**Possible Causes:**
1. BIOS not loading firmware from flash
2. Firmware crashes during initialization
3. smoltcp network loop not running

**Debug Options (require serial cable):**
- Connect serial (115200 baud) to see BIOS output
- BIOS should print "Booting from flash..." if it finds the image
- Firmware prints "Hello world!" on startup

### Alternative: Use Etherbone (Hardware ARP)
If SmolEth+Rust is problematic, switch back to etherbone in `colorlight.py`:
```python
# Replace SmolEth section with:
self.add_etherbone(
    phy=phy,
    ip_address=ip_address,
    mac_address=0x10e2d5000000,
    with_ethmac=False,  # True breaks ARP
)
```
This gives hardware ARP/ICMP but no telnet.

## Files Modified from Original

1. `colorlight.py` - Changed from standard LiteEth to SmolEth
2. `sw_rust/barsign_disp/src/main.rs` - IP address, sys_clk
3. `sw_rust/barsign_disp/regions.ld` - Memory layout (spiflash moved)

## Next Steps to Debug

1. **Get serial access** - Essential for debugging firmware issues
2. **Check BIOS output** - Verify it attempts flash boot
3. **Add debug prints** - In Rust firmware to trace execution
4. **Try simpler test** - Blink LED or similar to verify firmware runs

## Quick Reference

```bash
# Build everything
cd hub75_sawatzke
docker run --rm -v "$(pwd):/project" litex-hub75 "./colorlight.py --revision 8.2 --ip-address 10.11.6.250 --build"
docker run --rm -v "$(pwd):/project" litex-hub75 "cd /project/sw_rust/barsign_disp && cargo build --release"

# Test ping (after flashing)
ping 10.11.6.250

# Test telnet (after flashing)
telnet 10.11.6.250 23
```
