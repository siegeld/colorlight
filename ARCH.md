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
| `sw_rust/barsign_disp/src/main.rs` | Firmware entry point, ISR setup, main loop (display/animation only) |
| `sw_rust/barsign_disp/src/network.rs` | ISR network handler: packet processing, DHCP, TFTP config, HTTP, telnet |
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

## ISR-Driven Network Architecture

Since v1.10.0, all network processing runs inside the VexRiscv external interrupt handler (`network_handler()` in `network.rs`). The main loop does **zero** network code — only display refresh, animation, and serial menu.

### ISR Packet Processing

When the ETHMAC fires an interrupt (IRQ #2), the assembly trap handler saves all GPRs and calls `network_handler()`. The handler processes up to 64 packets per invocation:

1. **Bitmap UDP (fast path)** — `is_bitmap_udp()` checks dst_port 7000 directly from the raw frame. Matching packets bypass smoltcp entirely and write pixels via `process_raw_bitmap()`.
2. **Multicast drop** — VRRP, mDNS, and other multicast traffic is silently dropped.
3. **Unwanted UDP drop** — UDP packets for ports we don't use (NetBIOS, etc.) are dropped. Allowed ports: 7000 (bitmap), 6454 (Art-Net), 67/68 (DHCP), 69 (TFTP server), 6900 (TFTP client).
4. **Slow path** — Everything else (ARP, TCP, remaining UDP) goes through `iface.poll()` for smoltcp processing. Socket handlers run afterward: telnet, HTTP, DHCP, TFTP config, Art-Net, bitmap fallback.

After processing, the ISR disables `ev_enable` to prevent interrupt storms. The main loop re-enables it via `check_and_reenable_interrupt()`.

### Main Loop

The main loop (`main.rs`) runs continuously and handles only:
- **Timer tracking** — reads `TIME_MS` maintained by the ISR
- **Interrupt re-enable** — calls `check_and_reenable_interrupt()` each iteration
- **Animation** — updates display at ~30fps when not streaming
- **Serial menu** — processes telnet/serial input
- **MAC error counters** — updates diagnostics every 5ms

### Streaming Detection

The ISR tracks `LAST_BITMAP_PACKET_MS` — updated on every bitmap UDP packet. Streaming is active when `TIME_MS - LAST_BITMAP_PACKET_MS < 200`.

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

## Panel Configuration Layers

Panel configuration has three layers that must be consistent:

### Layer 1: Gateware (bitstream)
Built with `--panel` and `--chain-length`. Controls the HUB75 shift register timing:
- **columns**: pixels per shift-register row (e.g., 128 for a 128x64 panel)
- **rows**: total pixel rows (e.g., 64)
- **scan**: address lines = rows/2 (e.g., 32 for 1/32 scan)
- **chain_length_2**: log2 of panels per output (0=1 panel, 1=2 panels)
- **n_outputs**: number of HUB75 outputs (default 6)

These are exposed as read-only CSRs (`hw_columns`, `hw_rows`, `hw_config`)
so the firmware can read them and display them on the web GUI.

**Default build**: `./build.sh` produces columns=128, rows=64, scan=32, chain_length_2=1, n_outputs=6.
This means each output drives two 128-pixel-wide panels via daisy-chain.

### Layer 2: Firmware constants
`hub75.rs` constants must match gateware:
- `CHAIN_LENGTH` must equal `1 << chain_length_2` (e.g., 2 for chain_length_2=1)
- `OUTPUTS` must equal `n_outputs` (e.g., 6)

Mismatch crashes the SoC (firmware accesses nonexistent panel CSRs).

At startup, the firmware reads `hw_columns` and `hw_rows` from the bitstream CSRs
to set the default display size. This is overridden by the TFTP config if one is loaded.

### Layer 3: Runtime layout (TFTP YAML)
Loaded at boot from `.tftp/<mac>.yml` via TFTP (port 6969). Maps logical panels to
physical connectors and defines the virtual display grid:
- `grid`: grid dimensions (e.g., `2x1` = 2 columns, 1 row)
- `panel_width` / `panel_height`: size of each panel in pixels
- `J1`–`J6`: connector-to-grid mapping with chain slot positions

The virtual display size = `panel_width × grid_cols` by `panel_height × grid_rows`.
The firmware sets the DMA `image_width` CSR to this virtual width, which the gateware
uses as the framebuffer row stride. Constraints: `panel_width` must match the bitstream's
`columns`, `panel_height` must match `rows`, and chain slots used per output must not
exceed `chain_length`.

**Example**: For two 128x64 panels on J1 forming a 256x64 display:
```yaml
grid: 2x1
panel_width: 128
panel_height: 64
J1: 0,0 1,0
```
J1's first chain slot `[0]` maps to grid position (0,0), second slot `[1]` to (1,0).

### Connector Map (all 6 outputs)

| Connector | Output Index | Chain Slots | Panel CSRs |
|-----------|-------------|-------------|------------|
| J1 | 0 | [0], [1] | panel0_0, panel0_1 |
| J2 | 1 | [0], [1] | panel1_0, panel1_1 |
| J3 | 2 | [0], [1] | panel2_0, panel2_1 |
| J4 | 3 | [0], [1] | panel3_0, panel3_1 |
| J5 | 4 | [0], [1] | panel4_0, panel4_1 |
| J6 | 5 | [0], [1] | panel5_0, panel5_1 |

Each panel CSR contains x (8-bit, x16), y (8-bit, x16), rot (2-bit).

## Boot Sequence

1. **BIOS loads** — Runs from ROM, initializes SDRAM
2. **BIOS TFTP** — Fetches `boot.bin` from hardcoded server `10.11.6.65:6969` (standard port 69 also supported via dnsmasq)
3. **Firmware starts** — Reads flash UID, derives unique MAC (`02:xx:xx:xx:xx:xx`), initializes HUB75 with default image
4. **Interrupt setup** — Installs trap handler, enables ETHMAC IRQ #2
5. **DHCP** — Acquires IP address; falls back to `10.11.6.250/24` after 10 seconds
6. **TFTP config** — On DHCP completion, fetches `<mac>.yml` from TFTP server (DHCP Option 66, or fallback `10.11.6.65`) on port 6969
7. **Layout applied** — Parses YAML, configures panel CSRs, redraws display at new virtual size
8. **Main loop** — Runs animation/display; all network handled by ISR

Steps 5–7 happen inside the ISR's `network_handler()`. The TFTP config fetch
typically completes within 1–2 seconds of DHCP completion.

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
