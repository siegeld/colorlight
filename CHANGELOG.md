# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Re-enable Art-Net direct pixel writes
- Add serial console support documentation

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
