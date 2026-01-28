# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.8.1] - 2026-01-28

### Added
- **Version display in test pattern** — Grid test pattern now shows firmware version (e.g. "v1.8.1") in the second-row left square, avoiding diagonal lines for readability. Version is derived from Cargo.toml at compile time. Both Rust runtime pattern (`patterns::grid()`) and Python-generated default image (`gen_test_image.py`) include the version.

---

## [1.8.0] - 2026-01-27

### Changed
- **Skip slow path and discard non-bitmap packets during streaming** — While bitmap UDP frames are arriving, the firmware skips all slow-path processing (DHCP, telnet, HTTP, Art-Net) and discards non-bitmap packets in the fast-path burst loop via `ack_rx()` instead of calling `iface.poll()`. This eliminates multi-ms smoltcp stalls that were overflowing the 8-slot MAC FIFO. HTTP/telnet are unreachable during streaming. Services resume within 200ms of the last packet. Streaming detection uses `last_bitmap_packet_ms` (any packet, not just completed frames) to avoid getting stuck on partial final frames.
- **Auto chunk-delay in sender tools** — `send_video.py` and `send_youtube.py` now auto-calculate inter-chunk delay from fps and frame size when `--chunk-delay` is omitted: `(0.9 / fps) / total_chunks` (10% headroom). Explicit `--chunk-delay` still overrides. Enables higher panel counts (e.g. 8 panels at 10fps = 135 chunks) without manual tuning.

### Planned
- Re-enable Art-Net direct pixel writes
- Add serial console support documentation

---

## [1.7.0] - 2026-01-27

### Fixed
- **R/G/B color channels** — Physical pin mapping on Colorlight 5A-75E was incorrect (connector pins[0]=Blue, pins[1]=Red, pins[2]=Green). Gateware `Output` class now maps pixel data bits to correct physical pins. Also eliminates a bottom-row display artifact.
- **Bitmap UDP overwriting virtual width** — `process_packet()` called `set_img_param()` on every new frame, overwriting the TFTP-configured virtual width (e.g. 256 for chain_length_2) with the sender's physical width (128). Added dimension validation that rejects frames not matching the configured image size.

### Changed
- **Bitmap chunk tracking** — Replaced bitmask (u32/u64) with u16 counter, supporting up to 255 chunks per frame (scales to 12+ panel virtual displays). Frame completes when last chunk arrives even if earlier chunks were dropped by MAC overflow.
- **Slow-path frequency** — Non-bitmap work (telnet/DHCP/HTTP/ArtNet) runs every 5ms instead of 1ms, with extra `iface.poll()` calls between blocks. Reduces MAC RX overflow during bitmap streaming.
- **UDP receive buffer** — Bitmap socket increased from 32KB/24 metadata to 65KB/48 metadata slots.

### Added
- **256×64 bitstream** — Pre-built bitstream for 2×1 panel chain layout.
- **Debug readbacks** — TFTP handler logs `set_img_param` readback; `panel show` displays raw CTRL register; `layout apply` shows image params after apply.

---

## [1.6.0] - 2026-01-27

### Added
- **Panel chaining** — Each HUB75 output now supports 2 daisy-chained panels (`chain_length_2=1` in gateware), doubling panel capacity from 6 to 12 with zero extra EBRs.
- **YAML chain config syntax** — Space-separated positions per output: `J1: 0,0 1,0` assigns chain slot 0 at (0,0) and chain slot 1 at (1,0).
- **Telnet chain commands** — `panel show` displays all chain slots; `panel J1 0,0 1,0` sets both chain positions at once.
- **Web UI chain display** — Panels card shows `J1[0]`, `J1[1]` etc. per chain slot.
- **HTTP API chain support** — `GET /api/layout` returns arrays per output: `"J1":["0,0","1,0"]`; `POST /api/layout` accepts both array and legacy string format.
- **`--chain-length` build option** — `build.sh -l 2` and gateware `--chain-length 2` configure the chain depth.

### Changed
- `gateware/colorlight.py`: `BaseSoC` accepts `chain_length_2`, passed to `hub75.Hub75()` constructor.
- `build.sh`: New `CHAIN_LENGTH=2` config variable, `-l|--chain-length` CLI option.
- `sw_rust/barsign_disp/src/hub75.rs`: `CHAIN_LENGTH` = 2.
- `sw_rust/barsign_disp/src/layout.rs`: `assignments` changed from `[Option<(u8,u8)>; 6]` to `[[Option<(u8,u8)>; 2]; 6]`; `MAX_CHAIN = 2` added; `parse()` splits values on whitespace for chain slots.
- `sw_rust/barsign_disp/src/menu.rs`: `layout show/apply` and `panel show/set` display `J#[chain]` notation.
- `sw_rust/barsign_disp/src/http.rs`: Panels table, `api_layout_get/post` updated for chain arrays; new `json_get_array()` helper.

---

## [1.5.0] - 2026-01-27

### Changed
- **HUB75 outputs 4→6** — Expanded from 4 to 6 HUB75 outputs (J1–J6), using 53/56 EBRs on the ECP5-25F. Enables up to 6 independent panels.
- **JTAG retry logic** — SRAM programming now retries up to 5 attempts with a JTAG chain probe before loading, working around unreliable USB Blaster clones.

### Added
- **README: HUB75 Output Count section** — Documents that output count must be set in both `build.sh` and `hub75.rs` to avoid gateware/firmware mismatch.

---

## [1.4.1] - 2026-01-27

### Fixed
- **HTTP server stops responding** — Both TCP sockets on port 80 could get permanently stuck if a client connected without sending a complete request (browser prefetch, half-open connections, remote close). Added idle timeout (5s) and `may_recv()` check to abort stuck sockets so they re-listen.

---

## [1.4.0] - 2026-01-26

### Changed
- **HUB75 outputs reduced from 8 to 4** — Frees BRAM by halving the row buffer allocation (the largest BRAM consumer). Output count is now configurable via `--outputs` flag in gateware build and `build.sh -o|--outputs`.
- **MAC RX slots doubled: 4 → 8** — Uses freed BRAM for deeper ethernet receive buffering, eliminating MAC RX overflow drops during sustained UDP streaming.
- **Firmware matches new defaults** — `OUTPUTS=4`, `MAX_OUTPUTS=4`, `NRXSLOTS=8`; web/telnet UI shows J1–J4 only.
- **`build.sh` new option** — `-o|--outputs N` flag passed through to gateware build (default: 4).

### Technical Details
- `gateware/hub75.py`: All 4 submodule classes (`Hub75`, `RowController`, `RamToBufferReader`, `RamAddressGenerator`, `Output`) parameterized with `n_outputs`; signal widths computed dynamically from `n_outputs`.
- `gateware/helper.py`: Connector allocation parameterized.
- `gateware/colorlight.py`: `BaseSoC` accepts `n_outputs`, `nrxslots` bumped to 8, `--outputs` CLI argument added.
- `sw_rust/barsign_disp/src/ethernet.rs`: `NRXSLOTS` = 8.
- `sw_rust/barsign_disp/src/hub75.rs`: `OUTPUTS` = 4.
- `sw_rust/barsign_disp/src/layout.rs`: `MAX_OUTPUTS` = 4, bounds checks use constant.
- `sw_rust/barsign_disp/src/menu.rs`: Help text and validation updated for J1–J4.
- `sw_rust/barsign_disp/src/http.rs`: Layout API loop bounded by `MAX_OUTPUTS`.

---

## [1.3.1] - 2026-01-26

### Changed
- **Pin LiteX to 2025.12** — Dockerfile now fetches `litex_setup.py` from the `2025.12` tag and passes `--tag=2025.12` to ensure reproducible builds
- **TFTP server listens on 0.0.0.0** — `build.sh` binds TFTP server to all interfaces instead of a specific host IP
- **Rebuild bitstream and firmware** — Full rebuild with pinned LiteX to restore all network optimizations (4 RX slots, dual HTTP sockets, 32KB UDP buffer)

---

## [1.3.0] - 2026-01-26

### Added
- **BIOS broadcast TFTP on custom port** — BIOS now broadcasts its TFTP request to `255.255.255.255` on port 6969 instead of unicasting to a hardcoded IP on port 69. Any TFTP server on the subnet listening on port 6969 will respond. Eliminates the last hardcoded server IP.

### Changed
- **TFTP port 69 → 6969** — All three layers (BIOS bitstream, Rust firmware, dnsmasq server) now use port 6969 to avoid conflicts with other TFTP servers on the network

---

## [1.2.0] - 2026-01-26

### Added
- **Dynamic TFTP server discovery** — Firmware now discovers the TFTP server address from DHCP instead of using a hardcoded IP. Priority: `siaddr` header field → DHCP Option 66 → fallback `10.11.6.65`
- **DHCP Option 66 parsing** — Patched smoltcp to parse DHCP Option 66 (TFTP Server Name) as a dotted-decimal IP address; the DHCP client now requests Option 66 in its parameter request list
- **Boot server in web GUI** — Status page shows the active TFTP server IP and how it was discovered (siaddr, option 66, or fallback)

### Changed
- **smoltcp DHCP `Config`** — Now exposes both `server_ip` (siaddr) and `tftp_server_name` (Option 66) separately so firmware can distinguish the source
- **DHCP parameter request list** — Added Option 66 so DHCP servers (especially Windows DHCP Server) include it in responses

---

## [1.1.0] - 2026-01-26

### Added
- **Web reboot button** — New "System" section on status page with a Reboot button that triggers a full SoC reset via `POST /api/reboot`; response is sent before reset fires
- **Modern dark theme** — Status page restyled with dark navy background, system font, styled tables/buttons, responsive viewport meta tag
- **Higher contrast text** — Body text `#eee`, label column `#aaa`, 16px font for readability

### Changed
- **`./build.sh firmware` no longer starts TFTP server** — Build and serve are now separate concerns; use `./build.sh start` or `./build.sh boot` for TFTP
- **HTTP response buffer** — Increased from 2048 to 2560 bytes to accommodate the new CSS

---

## [1.0.0] - 2026-01-26

First stable release. All core features working and tested.

### Added
- **Multi-panel build system** — `./build.sh build-all` builds bitstreams for all 4 panel sizes (128x64, 96x48, 64x32, 64x64) in one command, with pre-built bitstreams committed to `bitstreams/`
- **Universal firmware** — Single firmware binary works with all panel sizes; only bitstreams differ per panel
- **TFTP auto-start** — `./build.sh firmware` and `./build.sh boot` automatically start the TFTP server if not already running; idempotent and non-blocking
- **128x64 default panel** — Default panel changed from 96x48 to 128x64
- **Architecture docs** — New `ARCH.md` with internals: memory map, double buffering, IAC state machine, hardware notes, debugging tips
- **Video streaming tool** — `tools/send_video.py` streams video files to the panel via UDP using ffmpeg

### Changed
- **Repository cleanup** — Legacy scripts, old notes, and unused code moved to `legacy/`
- **README rewritten** — Concise build instructions via `build.sh`, removed incorrect manual docker commands, added multi-panel workflow
- **Pre-built binaries** — All 4 panel bitstreams included in `bitstreams/` directory
- **`build.sh` improvements** — `build-all` target, `get_bitstream_path` for panel-aware flash/sram, `--panel` flag selects bitstream without rebuilding
- **Python tools** — All tools default to 128x64; `send_video.py` added with `--layout`, `--fps`, `--loop` options
- **TFTP configs tracked** — `.tftp/*.yml` board configs committed to git

### Fixed
- **TFTP not attempted during batch builds** — `ensure_tftp` only runs on explicit `firmware`/`boot` targets, not when called internally from `build-all`
- **TFTP failure non-fatal** — Warns instead of aborting the build if dnsmasq can't start

---

## [0.2.9] - 2026-01-25

### Added
- **Video streaming tool** — `tools/send_video.py` streams video files to the LED panel via UDP using ffmpeg for real-time decoding
  - Supports `--layout`, `--fps`, `--loop`, `--chunk-delay` options
  - Auto-detects video FPS via ffprobe
- **Fast bitmap receive loop** — Firmware drains all queued UDP packets per main loop iteration instead of one-at-a-time, reducing packet loss during video streaming

### Changed
- **LiteEth RX slots: 2 → 4** — Gateware change doubles hardware ethernet receive buffering (8KB), allowing higher sustained packet rates
- **Ethernet driver rewrite** — Direct buffer address computation bypasses broken LiteX SVD generator (which only describes 2 RX buffers regardless of `nrxslots`)
- **smoltcp burst size: 1 → 4** — Firmware processes up to 4 packets per `iface.poll()` call, matching the hardware RX slot count
- **Bitmap UDP RX buffer: 16KB → 32KB** — Socket buffer increased with 24 metadata slots (was 12) to hold full frames

### Performance
- 96×96 (2 panels): **19 fps** at 2.8ms chunk delay (was 10.8 fps before — 76% improvement)
- Reliable frame delivery at 3ms chunk delay (was 5ms)

---

## [0.2.8] - 2026-01-25

### Added
- **Web pattern selector** — Dropdown on HTTP status page to load test patterns (grid, rainbow, rainbow_anim, white, red, green, blue) directly from the browser
- **JTAG pinout in README** — Documented J27-J34 programmer connection pins

---

## [0.2.7] - 2026-01-25

### Added
- **TFTP boot config** — Firmware fetches `<mac-address>.yml` (e.g., `02-78-7b-21-ae-53.yml`) from TFTP server at boot
  - Simple YAML layout config: `grid`, `panel_width`, `panel_height`, `J1`..`J8` mappings
  - Automatically applies layout and redraws display at new virtual size after config load
- **Patched smoltcp** — Local patch adds `server_ip` (DHCP `siaddr`) to `Dhcpv4Config` for TFTP server discovery
- **Persistent bitstream flash** — `./build.sh flash` now uses `--board colorlight` flag for reliable SPI flash writes (seconds instead of hours)

### Changed
- `build.sh` — TFTP server stays running after `boot` (firmware needs it for config fetch); `flash` uses `--board colorlight` for correct flash chip handling
- `tftp_config.rs` — Accepts dynamic filename instead of hardcoded `layout.cfg`
- `layout.rs` — Parses YAML-style `key: value` separators in addition to `key=value`
- `http.rs` — Web page title renamed from "Barsign" to "Colorlight"

### Fixed
- **Flash programming speed** — `--board colorlight` flag tells openFPGALoader the correct flash chip, eliminating timeout/retry on every sector write

---

## [0.2.6] - 2026-01-25

### Added
- **HTTP REST API** - Web status page and JSON API on port 80
  - `GET /` — HTML status page with MAC, IP, display config, panel layout
  - `GET /api/status` — JSON system status
  - `GET /api/layout` / `POST /api/layout` — Get/set panel grid layout
  - `POST /api/layout/apply` — Apply layout to HUB75 hardware
  - `GET /api/display` / `POST /api/display/on` / `POST /api/display/off` — Display control
  - `POST /api/display/pattern` — Load test patterns (grid, rainbow, rainbow_anim, solid colors)
  - `GET /api/bitmap/stats` — Bitmap UDP receiver statistics
- **Dual HTTP sockets** — Two TCP sockets on port 80 so one can accept new connections while the other completes graceful TCP close, eliminating "site cannot be reached" on browser refresh

### Changed
- `http.rs` — New module with minimal HTTP/1.1 request parser, response writer, and route dispatcher

---

## [0.2.5] - 2026-01-25

### Added
- **DHCP client** - Firmware now acquires IP address via DHCP at boot using smoltcp's `Dhcpv4Socket`
  - Falls back to static IP `10.11.6.250/24` after 10 seconds if no DHCP server responds
  - Applies gateway route from DHCP server when provided
  - Logs IP assignment and lease loss to serial and telnet output
- **Unique MAC from SPI flash** - Reads the W25Q32JV 64-bit factory unique ID at boot and derives a locally-administered MAC address (`02:xx:xx:xx:xx:xx`)
  - Each board gets a deterministic, unique MAC without manual configuration
  - XOR-folds the 8-byte UID into 5 bytes, prepends `0x02` (locally administered, unicast)
- **`flash_id.rs` module** - New firmware module for reading flash unique ID via SPI master CSRs

### Changed
- **Gateware**: Added `with_master=True` to LiteSPI instantiation (explicit raw SPI command support)
- **Network init**: Interface starts with unspecified IP and routing table; DHCP configures both
- **Context**: Replaced `IpMacData` with plain `mac: [u8; 6]` (IP is now dynamic)
- Removed unused `IpData`, `IpMacData` types from `ethernet.rs`

---

## [0.2.4] - 2026-01-25

### Added
- **Bitmap UDP protocol** - Send RGB images to the panel over UDP port 7000
  - 10-byte little-endian header: magic "BM", frame_id, chunk_index, total_chunks, width, height
  - Pixel-aligned chunking (487 pixels / 1461 bytes per chunk, 10 chunks for 96x48)
  - Bitmask-based frame assembly with automatic buffer swap on completion
  - 16KB receive buffer with drain loop for reliable multi-packet reception
- **`bitmap_status` telnet command** - Shows packet counters, frame completion stats, and last packet details
- **`debug` telnet command** - Toggle live per-packet logging to telnet console
- **`tools/send_image.py`** - Send any image file (resized to panel dimensions) via UDP
- **`tools/send_test_pattern.py`** - Generate and send test patterns (gradient, bars, rainbow) without needing an image file

---

## [0.2.3] - 2026-01-25

### Added
- **Telnet IAC filtering** - State machine parser strips telnet negotiation sequences from input, preventing binary option bytes from corrupting the menu
- **`quit` command** - Closes the telnet connection cleanly
- **Prompt on connect** - Welcome message now includes `> ` prompt so users can type immediately

---

## [0.2.2] - 2026-01-25

### Added
- **Framebuffer double buffering** - New `fb_base` CSR register in HUB75 gateware allows software-controlled framebuffer base address
  - Two 256KB framebuffer regions in SDRAM (front and back)
  - CPU writes to back buffer, then flips `fb_base` to swap atomically
  - Eliminates tearing from CPU/DMA contention during animation
- `swap_buffers()` method on Hub75 driver

### Changed
- Animation tick restored to 30fps (33ms) now that double buffering prevents tearing
- All image-writing paths (patterns, default image, SPI image) now use write-then-swap

---

## [0.2.1] - 2026-01-25

### Added
- **Animated rainbow pattern** - `pattern rainbow_anim` telnet command displays a smoothly scrolling diagonal rainbow
- `Animation` enum in Context for tracking active animation state
- `animated_rainbow()` pattern generator with phase offset
- `animation_tick()` method called from main loop

---

## [0.2.0] - 2026-01-25

### Added
- **General-purpose panel configuration** - Support for multiple panel types via `--panel` argument
  - 96x48 (default), 128x64, 64x32, 64x64 panels supported
  - Configurable scan rates (1/16, 1/24, 1/32)
- **build.sh enhancements**
  - `--panel` argument to select panel type at build time
  - Automatic test image generation with correct panel dimensions
- **gen_test_image.py** - New script to generate panel-specific test patterns
  - Horizontal lines at key rows (top, middle, bottom)
  - Vertical lines at evenly spaced columns (colored: RED, GREEN, BLUE, YELLOW, MAGENTA)
  - Diagonal X pattern (CYAN and MAGENTA) for visual alignment verification

### Changed
- hub75.py now accepts `columns`, `rows`, `scan` parameters instead of hardcoded values
- colorlight.py uses PANELS configuration dictionary for panel definitions
- Row addressing properly wraps using Migen conditional logic instead of Python modulo

### Fixed
- Row wrap-around calculation using proper Migen signal expressions

---

## [0.1.0] - 2025-01-25

### Added
- Initial working release with telnet support
- LiteX SoC with VexRiscv CPU at 40MHz
- Standard LiteEth MAC with smoltcp TCP/IP stack in firmware
- Telnet management console on port 23
- ICMP ping support
- Art-Net UDP receiver (palette updates)
- HUB75 display driver supporting 8 outputs, 4 panels per chain
- Full-color (24-bit) and indexed (8-bit) display modes
- SPI flash image storage
- Docker-based build environment
- Support for Colorlight 5A-75E rev 8.2

### Fixed
- TCP socket state machine - changed `&` to `&&` in listen/bind conditions
- Timing in main loop - added proper delay for smoltcp timestamp handling
- MAC address consistency - use fixed MAC matching hardware default
- PAC imports for new svd2rust structure

### Changed
- Switched from custom SmolEth to standard LiteEth for better compatibility
- Reorganized repository structure, moved old projects to `legacy/`

### Known Issues
- Flash boot requires TFTP on rev 8.2 (flash chip definition mismatch)
- Art-Net direct pixel writes disabled pending testing

---

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 1.8.0 | 2026-01-27 | Skip slow path during streaming, auto chunk-delay, zero MAC overflows |
| 1.7.0 | 2026-01-27 | Fix color channels, bitmap dimension validation, streaming improvements |
| 1.6.0 | 2026-01-27 | Panel chaining: 2 panels per output, up to 12 total |
| 1.5.0 | 2026-01-27 | Expand HUB75 outputs 4→6, JTAG retry logic |
| 1.4.1 | 2026-01-27 | Fix HTTP server stuck sockets with idle timeout |
| 1.4.0 | 2026-01-26 | Parameterize HUB75 outputs (8→4), nrxslots 4→8 |
| 1.3.1 | 2026-01-26 | Pin LiteX to 2025.12, reproducible Docker builds |
| 1.3.0 | 2026-01-26 | BIOS broadcast TFTP on custom port 6969 |
| 1.2.0 | 2026-01-26 | Dynamic TFTP server via DHCP siaddr/Option 66 |
| 1.1.0 | 2026-01-26 | Web reboot button, modern dark theme, build.sh TFTP fix |
| 1.0.0 | 2026-01-26 | First stable release: multi-panel build, auto TFTP, repo cleanup |
| 0.2.9 | 2026-01-25 | Video streaming, 4 RX slots, fast bitmap receive |
| 0.2.8 | 2026-01-25 | Web pattern selector, JTAG pinout docs |
| 0.2.7 | 2026-01-25 | TFTP boot config, persistent flash, YAML layout |
| 0.2.6 | 2026-01-25 | HTTP REST API with dual-socket refresh fix |
| 0.2.5 | 2026-01-25 | DHCP client with unique MAC from SPI flash |
| 0.2.4 | 2026-01-25 | Bitmap UDP protocol for sending images |
| 0.2.3 | 2026-01-25 | Telnet IAC filtering and quit command |
| 0.2.2 | 2026-01-25 | Framebuffer double buffering via fb_base CSR |
| 0.2.1 | 2026-01-25 | Animated rainbow pattern |
| 0.2.0 | 2026-01-25 | General-purpose panel configuration |
| 0.1.0 | 2025-01-25 | Initial release with telnet support |

---

## Release Process

1. Update version in this file
2. Update version badge in README.md if applicable
3. Commit changes: `git commit -m "Release vX.Y.Z"`
4. Tag release: `git tag -a vX.Y.Z -m "Version X.Y.Z"`
5. Push: `git push && git push --tags`
