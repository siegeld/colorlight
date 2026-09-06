# AI Development Hints

- **Canonical checkout is `/share/src/colorlight`** (NFS, same path on every host —
  this is device firmware, it does not ship as a container image). The old
  `/u/siegeld/colorlight` tree is superseded; don't edit there.
- **Bitstream builds and flashing happen on `jupiter`** — it holds the USB-Blaster
  (Altera 09fb:6001) and the `litex-hub75` image, and its second NIC is the
  10.11.6.65 leg on the panel's segment, so it is also where `./build.sh boot`
  serves TFTP (matching the panel's DHCP option 66).
  **DNS trap:** a reverse lookup of 10.11.6.65 answers `dogwood.siegel.com`, but
  that PTR is stale — `dogwood` forward-resolves to 10.1.1.149, a different host
  at another site. jupiter has no A record for its 10.11.6.65 leg; `jupiter`
  resolves to its *other* NIC, 10.11.7.60. Don't name the flash host from the
  reverse lookup.
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
# Single pattern — the virtual display is 256x128 (2x2 grid of 128x64 panels)
python3 tools/send_test_pattern.py gradient --host 10.11.6.72 --width 256 --height 128

# Smoke test (cycles all patterns forever)
python3 tools/send_test_pattern.py --smoke --host 10.11.6.72 --width 256 --height 128

# Throughput / loss measurement — see the baseline table in the script header
python3 tools/bench_stream.py --host 10.11.6.72 sweep
```

**Send rate**: the sender paces itself and defaults to 0.8 ms between packets
(~1250 pkt/s), which is the measured limit. Don't pass `--delay` unless you are
deliberately testing — the old `--delay 0.1` advice was 125x more conservative
than the hardware needs and drops you to ~0.15 fps.

## Streaming performance — measured, not assumed

The bottleneck is the CPU pixel loop, not the network. Numbers from
`tools/bench_stream.py` on v1.10.6, worth re-checking after any change here:

- **~600,000 px/s hard ceiling** — flat regardless of how hard you push. At
  256x128 that is 18 fps absolute, ~15.6 fps clean.
- **Cause**: VexRiscv_Lite has an I-cache but *no D-cache*
  (`cpu_variant="lite"`), so every access is a bare bus round-trip. The old
  byte-at-a-time RGB unpack cost 4 of them per pixel (3 loads + 1 store) at
  ~17 cycles each. v1.10.8 reads the payload 32 bits at a time — 3 loads + 4
  stores per 4 pixels instead of 12 + 4.
- Going meaningfully past ~40 fps needs the pixels off the CPU entirely: a
  `LiteDRAMDMAWriter` fed from a LiteEth UDP port. Note the display DMA already
  reads the whole framebuffer every refresh and is close to SDRAM-bandwidth
  bound, so write DMA trades refresh rate for frame rate.
- The framebuffer is 65,536 words per buffer — **exactly** 8 panels of 128x64,
  with no headroom. The bitstream can drive 12, which would not fit. There is
  spare SDRAM in the framebuffer region if that needs raising.
