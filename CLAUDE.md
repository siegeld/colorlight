# AI Development Hints

- Read [README.md](README.md) for project docs, build commands, and usage
- Read [ARCH.md](ARCH.md) for internals: memory map, double buffering, IAC state machine, key files
- All builds go through `./build.sh` — run `./build.sh --help` for options
- After changing `gateware/colorlight.py` (SoC/gateware), must regenerate PAC before rebuilding firmware: `./build.sh bitstream pac firmware`
- Panel size (columns, rows, scan) is baked into the FPGA bitstream via `--panel` flag — not a runtime setting
- `sw_rust/smoltcp-0.8.0/` is a patched fork — don't replace with upstream
- No serial console available — see ARCH.md "Debugging Without Serial" for alternatives

## Current Test Setup

- **Device IP**: 10.11.6.72
- **Panel size**: 256x64 (two daisy-chained 128x64 panels)
- **Prebuilt bitstreams**: `bitstreams/` directory (256x64.bit, 128x64.bit, etc.)

## Build Commands

```bash
# Build bitstream for specific panel (MUST specify -p for correct panel!)
./build.sh -p 256x64 bitstream

# Boot with specific panel bitstream (uses prebuilt from bitstreams/)
./build.sh -p 256x64 boot

# Full rebuild with PAC regeneration
./build.sh -p 256x64 bitstream pac firmware
```

## Test Patterns

```bash
# Single pattern
python3 tools/send_test_pattern.py gradient --host 10.11.6.72 --width 256 --height 64 --delay 0.1

# Smoke test (cycles all patterns forever)
python3 tools/send_test_pattern.py --smoke --host 10.11.6.72 --width 256 --height 64 --delay 0.1
```

**Important**: At 40MHz, `--delay 0.1` is required to avoid banding.
