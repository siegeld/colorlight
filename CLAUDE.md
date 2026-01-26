# Development Context

> This file provides context for AI-assisted development. It documents architecture decisions, current state, and debugging knowledge accumulated during development.

## Project Status

| Component | Status | Notes |
|-----------|--------|-------|
| Bitstream | Working | 40MHz, passes timing |
| Firmware | Working | Rust, smoltcp TCP/IP |
| DHCP | Working | Auto IP via smoltcp Dhcpv4Socket, 10s fallback to static |
| Unique MAC | Working | Derived from SPI flash 64-bit unique ID (02:xx:xx:xx:xx:xx) |
| Ping | Working | Via smoltcp ICMP |
| Telnet | Working | Port 23, IAC filtering, quit command |
| Animation | Working | 30fps double-buffered via fb_base CSR |
| HTTP API | Working | Port 80, status page + REST API, dual-socket for fast refresh |
| Bitmap UDP | Working | Port 7000, chunked RGB images, Python sender tools |
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

The firmware uses **smoltcp** (Rust TCP/IP stack, v0.8) for all network handling:

- **LiteEth MAC** provides raw ethernet frame access via Wishbone
- **smoltcp** handles ARP, ICMP, TCP, UDP, DHCP in software
- **DHCPv4** client acquires IP at boot; falls back to `10.11.6.250/24` after 10 seconds
- **Unique MAC** derived from SPI flash factory unique ID (locally-administered `02:xx:xx:xx:xx:xx`)
- **No hardware ARP/ICMP** - different from etherbone approach

This design was chosen to enable TCP (telnet) which hardware-only stacks don't support.

## Key Files

| File | Purpose |
|------|---------|
| `colorlight.py` | LiteX SoC definition, peripheral instantiation |
| `hub75.py` | HUB75 display driver gateware (includes `fb_base` CSR) |
| `smoleth.py` | Custom ethernet module (currently unused, kept for reference) |
| `sw_rust/barsign_disp/src/main.rs` | Firmware entry point, network loop, DHCP, telnet IAC parser |
| `sw_rust/barsign_disp/src/http.rs` | HTTP/1.1 server: status page, REST API for layout/display/patterns |
| `sw_rust/barsign_disp/src/hub75.rs` | HUB75 driver: double-buffered framebuffer, swap_buffers() |
| `sw_rust/barsign_disp/src/menu.rs` | Telnet CLI commands (pattern, quit, animation) |
| `sw_rust/barsign_disp/src/flash_id.rs` | Read SPI flash unique ID, derive MAC address |
| `sw_rust/barsign_disp/src/patterns.rs` | Test pattern generators (grid, rainbow, animated_rainbow) |
| `sw_rust/barsign_disp/src/ethernet.rs` | smoltcp device driver |
| `sw_rust/litex-pac/` | Generated peripheral access crate |
| `tools/send_image.py` | Send image files to panel via UDP port 7000 |
| `tools/send_test_pattern.py` | Generate and send test patterns (gradient, bars, rainbow, heart) |
| `tools/send_animation.py` | Send animated patterns (pulsing heart) at configurable FPS |

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

## HUB75 Double Buffering

The HUB75 gateware has a `fb_base` CSR register (20-bit, at `HUB75 + 0x04`) that controls which SDRAM region the DMA reads from. The firmware splits the SDRAM framebuffer area into two 256KB halves:

- **Buffer 0**: SDRAM word offset `0x80000` (byte addr `0x90200000`)
- **Buffer 1**: SDRAM word offset `0x90000` (byte addr `0x90240000`)

The CPU always writes to the **back buffer** via `write_img_data()`, then calls `swap_buffers()` which swaps the slice references and writes the new front buffer address to `fb_base`. This eliminates tearing from CPU/DMA contention.

### Animation Framework

Animation state is stored in `Context.animation` (enum: `None`, `Rainbow { phase }`). The main loop calls `animation_tick()` every 33ms (~30fps). Each tick writes a new frame to the back buffer and swaps.

Available animated patterns: `rainbow_anim` (via telnet `pattern` command).

## Telnet IAC Handling

The telnet input path in `main.rs` includes a state machine that strips IAC (Interpret As Command) sequences from the byte stream before feeding characters to the menu parser. States:

- **0**: Normal - pass bytes through, enter state 1 on `0xFF`
- **1**: Got IAC - dispatch on command byte (WILL/WONT/DO/DONT → state 2, SB → state 3)
- **2**: Got command - consume option byte, return to state 0
- **3**: In subnegotiation - skip until `0xFF`
- **4**: IAC inside subneg - `0xF0` (SE) ends it, return to state 0

Without this parser, telnet option bytes (e.g. `0x22` = `"`) leak through as spurious menu input.

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

1. **Check ARP MAC** - Firmware uses a `02:xx:xx:xx:xx:xx` MAC (locally administered, derived from flash UID). BIOS uses `10:e2:d5:00:00:00`. If ARP shows `02:...`, firmware is running.
   ```bash
   ping -c1 <ip> && arp -n <ip>
   ```

2. **Watch DHCP** - Firmware sends DHCP Discover at boot. If you see DHCP traffic with a `02:` MAC, firmware is running.
   ```bash
   sudo tcpdump -i <iface> udp port 67 or port 68
   ```

3. **tcpdump for TFTP** - TFTP requests mean BIOS (not firmware) is running
   ```bash
   sudo tcpdump -i <iface> host <ip> and udp port 69
   ```

4. **HUB75 output** - `hub75.on()` is called at startup; display should activate

## Firmware Configuration

### IP Address (DHCP)

The firmware uses DHCP to acquire an IP address at boot. If no DHCP server responds within 10 seconds, it falls back to a static IP:

```rust
// sw_rust/barsign_disp/src/main.rs
// Fallback after 10s with no DHCP:
let fallback = Ipv4Cidr::new(Ipv4Address([10, 11, 6, 250]), 24);
```

### MAC Address (Dynamic)

MAC is derived from the SPI flash's factory-programmed 64-bit unique ID at boot:

```rust
// sw_rust/barsign_disp/src/flash_id.rs
// Command 0x4B reads W25Q32JV unique ID
// Result: 02:xx:xx:xx:xx:xx (locally administered, unicast)
let unique_id = flash_id::read_flash_unique_id(&peripherals.spiflash_mmap);
let mac_bytes = flash_id::derive_mac(&unique_id);
```

Each board gets a deterministic, unique MAC. The `0x02` prefix marks it as locally administered per IEEE 802.

### System Clock
```rust
// Must match bitstream (40MHz default)
sys_clk: 40_000_000,
```

## Tools

Python tools for sending content to the panel over UDP port 7000:

```bash
# Send an image file (auto-resized to panel dimensions)
python3 tools/send_image.py path/to/image.png --host <board-ip> --width 96 --height 48

# Send a test pattern (gradient, bars, rainbow, heart)
python3 tools/send_test_pattern.py heart --host <board-ip> --width 96 --height 48

# Send animated pattern (pulsing heart at 30fps, 3 loops)
python3 tools/send_animation.py heart --host <board-ip> --width 96 --height 48 --fps 30 --loops 3
```

All tools default to `--host 10.11.6.250 --port 7000 --width 96 --height 48`.

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
