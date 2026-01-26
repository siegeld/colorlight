# AI Development Hints

- Read [README.md](README.md) for project docs, build commands, and usage
- Read [ARCH.md](ARCH.md) for internals: memory map, double buffering, IAC state machine, key files
- All builds go through `./build.sh` — run `./build.sh --help` for options
- After changing `colorlight.py` (SoC/gateware), must regenerate PAC before rebuilding firmware: `./build.sh bitstream pac firmware`
- Panel size (columns, rows, scan) is baked into the FPGA bitstream via `--panel` flag — not a runtime setting
- `sw_rust/smoltcp-0.8.0/` is a patched fork — don't replace with upstream
- No serial console available — see ARCH.md "Debugging Without Serial" for alternatives
