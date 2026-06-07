# AI Development Hints

- Read [README.md](README.md) for project docs, build commands, and usage
- Read [ARCH.md](ARCH.md) for internals: memory map, double buffering, ISR design, key files
- All builds go through `./build.sh` — run `./build.sh --help` for options
- After changing `gateware/colorlight.py` (SoC/gateware), must regenerate PAC before rebuilding firmware: `./build.sh bitstream pac firmware`
- Panel size (columns, rows, scan) is baked into the FPGA bitstream via `--panel` flag — not a runtime setting
- `sw_rust/smoltcp-0.8.0/` is a patched fork — don't replace with upstream
- No serial console available — see ARCH.md "Debugging Without Serial" for alternatives

## Configuration Model

The system has three configuration layers (see ARCH.md for details):

1. **Bitstream** (`--panel` + `--chain-length` + `--outputs`) — baked into FPGA, controls HUB75 shift register timing
2. **Firmware constants** (`hub75.rs`: `OUTPUTS`, `CHAIN_LENGTH`) — must match gateware exactly
3. **Runtime layout** (TFTP YAML: `.tftp/<mac>.yml`) — maps panels to grid positions, applied at boot

The default build (`./build.sh`) produces a 128x64 bitstream with chain_length=2 and 6 outputs.
This supports up to 12 panels (6 connectors × 2 chained). The TFTP YAML config determines
how many panels are actually used and where they appear in the virtual display.

## Current Test Setup

- **Device IP**: 10.11.6.72 (via DHCP)
- **Bitstream**: 128x64, chain_length=2, 6 outputs (default build)
- **Physical panels**: Four 128x64 panels in a 2x2 grid (J1: top row, J2: bottom row)
- **Virtual display**: 256x128 (configured via TFTP YAML)
- **Prebuilt bitstreams**: `bitstreams/` directory (128x64.bit, 96x48.bit, etc.)

## Build Commands

```bash
# Default build — 128x64 panel, chain_length=2, 6 outputs
./build.sh                              # builds bitstream + PAC + firmware
./build.sh firmware boot                # rebuild firmware and boot via TFTP

# Full rebuild after gateware changes
./build.sh bitstream pac firmware

# Boot (loads bitstream to SRAM, starts TFTP server)
./build.sh boot

# Stop TFTP server
./build.sh stop
```

## Test Patterns

```bash
# Single pattern (256 width = two 128-wide panels side by side)
python3 tools/send_test_pattern.py gradient --host 10.11.6.72 --width 256 --height 64 --delay 0.1

# Smoke test (cycles all patterns forever)
python3 tools/send_test_pattern.py --smoke --host 10.11.6.72 --width 256 --height 64 --delay 0.1
```

**Important**: At 40MHz, `--delay 0.1` is required to avoid banding.
