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
- Read [TODO.md](TODO.md) for known defects and outstanding work — check it
  before starting anything, it says what is already known to be broken
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
- **Physical panels**: Six 128x64 panels, **one per connector J1-J6, no chaining**,
  arranged 2 wide x 3 tall:

  | | col 0 | col 1 |
  |---|---|---|
  | **row 0** (top) | J1 | J2 |
  | **row 1** | J3 | J4 |
  | **row 2** (bottom) | J5 | J6 |

- **Virtual display**: 256x192 (configured via TFTP YAML). 49,152 px, so it fits
  the 65,536-word framebuffer half with 16,384 words spare.
  The bitstream still carries chain_length=2, so chain slot 1 simply goes
  unassigned -- 6 of the 12 available slots are used.
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
# Single pattern — the virtual display is 256x192 (2x3 grid of 128x64 panels)
python3 tools/send_test_pattern.py gradient --host 10.11.6.72 --width 256 --height 192

# Smoke test (cycles all patterns forever)
python3 tools/send_test_pattern.py --smoke --host 10.11.6.72 --width 256 --height 192

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
  256x128 that is 18 fps absolute.
- **The "~15.6 fps clean" figure does NOT reproduce.** It was measured on the
  mismatched flash gateware. Re-measured 2026-09-07 on matched gateware, both
  v1.10.6 and v1.10.10 are clean (0% loss) only to ~2 ms spacing = **7.35 fps**,
  with loss appearing by 1.20 ms. Trust a fresh sweep over any number here.
- **Cause**: VexRiscv_Lite has an I-cache but *no D-cache*
  (`cpu_variant="lite"`), so every access is a bare bus round-trip. The old
  byte-at-a-time RGB unpack cost 4 of them per pixel (3 loads + 1 store) at
  ~17 cycles each. v1.10.8 reads the payload 32 bits at a time — 3 loads + 4
  stores per 4 pixels instead of 12 + 4. **This did not deliver the expected
  ~2x.** Measured head-to-head on identical gateware, v1.10.10 beats v1.10.6 at
  moderate rates (3.5% vs 11.1% loss at 1.20 ms) and ties at 0.80 ms (9.8% vs
  11.0%), but the clean ceiling is unchanged. Treat the 2x as unproven.
- Going meaningfully past ~40 fps needs the pixels off the CPU entirely: a
  `LiteDRAMDMAWriter` fed from a LiteEth UDP port. Note the display DMA already
  reads the whole framebuffer every refresh and is close to SDRAM-bandwidth
  bound, so write DMA trades refresh rate for frame rate.
- **Landmines that cost a full night (2026-09-07), both now fixed — do not
  reintroduce the pattern:** the assembly trap vector incremented a counter at a
  *hardcoded* `0x40020000`, which is inside `.text`, so every interrupt
  overwrote an instruction in the running firmware. And `panic.rs` spun
  unbounded on the UART TX FIFO, which nothing drains on a board with no serial
  console, so panics hung instead of rebooting. Anything writing to a fixed
  absolute address in RAM, or waiting forever on a peripheral, is suspect here.
- **Debugging without serial**: `src/breadcrumb.rs` records how far execution got
  before a crash in uncached SDRAM that survives `soc_rst`; read `prev_mark`,
  `prev_mcause` and `prev_mepc` from `/api/status` after a reboot. The trap
  vector records CPU exceptions. This found the bug above after bisection failed.
- **The failure was intermittent (~1 run in 3).** Never bisect this system on a
  single run, and verify the board is alive *before* each measurement — a dead
  board silently reports zero errors because nothing is running to fail.
- The framebuffer is 65,536 words per buffer — **exactly** 8 panels of 128x64,
  with no headroom. The bitstream can drive 12, which would not fit. There is
  spare SDRAM in the framebuffer region if that needs raising.
