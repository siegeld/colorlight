# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Fix SPI flash boot for rev 8.2 boards (update flash chip from GD25Q16 to W25Q32JV)
- Re-enable Art-Net direct pixel writes
- Add serial console support documentation

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
