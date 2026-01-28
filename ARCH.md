# Architecture

## Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Colorlight 5A-75E                        │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  VexRiscv   │  │   LiteEth   │  │    HUB75 Driver     │  │
│  │    CPU      │◄─┤    MAC      │  │  (6 out × 2 chain)  │  │
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

## Network Stack

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
| `gateware/colorlight.py` | LiteX SoC definition, peripheral instantiation |
| `gateware/hub75.py` | HUB75 display driver gateware (includes `fb_base` CSR) |
| `sw_rust/barsign_disp/src/main.rs` | Firmware entry point, network loop, DHCP, telnet IAC parser |
| `sw_rust/barsign_disp/src/http.rs` | HTTP/1.1 server: status page, REST API for layout/display/patterns |
| `sw_rust/barsign_disp/src/hub75.rs` | HUB75 driver: double-buffered framebuffer, swap_buffers() |
| `sw_rust/barsign_disp/src/menu.rs` | Telnet CLI commands (pattern, quit, animation) |
| `sw_rust/barsign_disp/src/flash_id.rs` | Read SPI flash unique ID, derive MAC address |
| `sw_rust/barsign_disp/src/patterns.rs` | Test pattern generators (grid, rainbow, animated_rainbow) |
| `sw_rust/barsign_disp/src/tftp_config.rs` | TFTP client for fetching MAC-based YAML config at boot |
| `sw_rust/barsign_disp/src/layout.rs` | Panel layout config parser (YAML `key: value` and `key=value`) |
| `sw_rust/barsign_disp/src/ethernet.rs` | smoltcp device driver |
| `sw_rust/smoltcp-0.8.0/` | Patched smoltcp: exposes DHCP Option 66 as `Config.tftp_server_name` |
| `sw_rust/litex-pac/` | Generated peripheral access crate |
| `.tftp/` | TFTP root: `boot.bin` (firmware) + `<mac>.yml` (per-board config) |

## Memory Map

| Region | Address | Size | Description |
|--------|---------|------|-------------|
| ROM | 0x00000000 | 64KB | LiteX BIOS |
| SRAM | 0x10000000 | 8KB | Stack/heap |
| Main RAM | 0x40000000 | 4MB | SDRAM, firmware runs here |
| EthMAC | 0x80000000 | 20KB | 8 RX + 2 TX slots × 2KB each |
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

## Main Loop Architecture

The firmware main loop (`main.rs`) has two processing paths to balance packet throughput against feature responsiveness:

### Fast Path (every iteration)

1. **Raw bitmap drain** — `drain_raw_bitmap!()` reads MAC RX slots directly via `peek_rx()`, bypassing smoltcp entirely. Only bitmap UDP packets (port 7000) are consumed; non-bitmap packets are left in the queue.
2. **Burst loop** — If a non-bitmap packet blocks the RX queue, behavior depends on the streaming state:
   - **Streaming:** The packet is **discarded** via `ack_rx()` and the next raw drain runs immediately. No `iface.poll()` is called — this avoids multi-ms stalls from smoltcp processing TCP/DHCP state machines, which were causing MAC FIFO overflows at high packet rates.
   - **Idle:** `iface.poll()` is called to process the packet normally (ARP, TCP, DHCP), followed by a raw drain pass and a socket-level bitmap drain as fallback. Up to 50 iterations.

### Slow Path (5ms tick, idle only)

Handles DHCP, telnet, HTTP, Art-Net, TFTP config, animation, and MAC error counters. Runs every 5ms when the timer fires.

### Streaming Detection

The firmware tracks `last_bitmap_packet_ms` — updated every time any bitmap UDP packet is received (in both raw drain and socket fallback paths). Streaming is active when `time_ms - last_bitmap_packet_ms < 200`. This timeout-based approach ensures the flag clears reliably even if the last frame was incomplete (a partial frame with nonzero `chunks_count` won't keep the flag stuck).

### Streaming Mode Behavior

While streaming is active:
- **Slow path is skipped entirely** — no DHCP, telnet, HTTP, Art-Net, or animation processing.
- **Non-bitmap packets are discarded** — the fast-path burst loop calls `ack_rx()` instead of `iface.poll()`, preventing smoltcp stalls.
- **HTTP and telnet do not respond** — the board is unreachable via web browser or telnet during active streaming. The sender's ARP entry is already cached so UDP delivery is unaffected.
- **Services resume within 200ms** of the last bitmap packet arriving.

### MAC RX FIFO

The LiteEth MAC has 8 RX slots (`nrxslots=8`), each 2048 bytes. The slot count must be a power of 2 (LiteEth wishbone SRAM decoder constraint). The firmware constant `NRXSLOTS` in `ethernet.rs` must match the gateware value in `colorlight.py`.

## Telnet IAC Handling

The telnet input path in `main.rs` includes a state machine that strips IAC (Interpret As Command) sequences from the byte stream before feeding characters to the menu parser. States:

- **0**: Normal - pass bytes through, enter state 1 on `0xFF`
- **1**: Got IAC - dispatch on command byte (WILL/WONT/DO/DONT -> state 2, SB -> state 3)
- **2**: Got command - consume option byte, return to state 0
- **3**: In subnegotiation - skip until `0xFF`
- **4**: IAC inside subneg - `0xF0` (SE) ends it, return to state 0

Without this parser, telnet option bytes (e.g. `0x22` = `"`) leak through as spurious menu input.

## Hardware Notes

### Colorlight 5A-75E V8.2

- FPGA: Lattice ECP5-25F (LFE5U-25F-6BG256C)
- SDRAM: M12L16161A (2M x 16bit)
- Flash: W25Q32JV (4MB) - **not GD25Q16**
- Ethernet PHY: RTL8211FD (RGMII)
- System clock: 40MHz

## Known Issues & Solutions

### Flash Boot Fails (rev 8.2)

**Symptom:** BIOS sends TFTP requests for `boot.bin` instead of loading from flash.

**Cause:** `gateware/colorlight.py` defines `GD25Q16` flash but rev 8.2 uses W25Q32JV.

**Workaround:** Use TFTP boot (see README).

**Fix:** Update flash chip in `gateware/colorlight.py`: `GD25Q16` -> `W25Q32JV`.

### TCP Connection Timeout

**Symptom:** Ping works but telnet times out.

**Cause:** Socket `listen()` called repeatedly due to `&` vs `&&` operator (bitwise vs short-circuit).

### ARP Shows Wrong MAC

**Symptom:** ARP reply shows `10:e2:d5:00:00:00` instead of `02:xx:xx:xx:xx:xx`.

**Cause:** Response is from BIOS, not firmware. Firmware isn't running yet.

## Debugging Without Serial

Since serial access isn't available:

1. **Check ARP MAC** - `02:xx:xx:xx:xx:xx` = firmware running; `10:e2:d5:00:00:00` = BIOS
2. **Watch DHCP** - `sudo tcpdump -i <iface> udp port 67 or port 68`
3. **tcpdump for TFTP** - TFTP requests on port 69 mean BIOS is running (not firmware)
4. **HUB75 output** - `hub75.on()` is called at startup; display should activate
