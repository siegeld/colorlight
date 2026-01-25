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
| 0.1.0 | 2025-01-25 | Initial release with telnet support |

---

## Release Process

1. Update version in this file
2. Update version badge in README.md if applicable
3. Commit changes: `git commit -m "Release vX.Y.Z"`
4. Tag release: `git tag -a vX.Y.Z -m "Version X.Y.Z"`
5. Push: `git push && git push --tags`
