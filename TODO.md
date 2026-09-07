# Outstanding work

Tracking file for known defects and planned work. Each item states what is
wrong, why it matters, where it lives, the evidence, and the next concrete step
— enough to pick up cold, without the conversation that produced it.

Status as of 2026-09-07: HEAD `9d597e4`. Firmware v1.10.10 (tagged). Tier 0
(packet-loss handling) and Tier 2 (hardware pixel path) both work on hardware.
The panel at 10.11.6.72 boots over TFTP from `jupiter` and **cannot yet survive a
power cycle** — see item 2.

---

## 1. Hardware arrival tracking — the CPU's "frame complete" can lie

**Priority: high. This is a correctness bug, not a feature.**

The CPU builds its chunk arrival mask from packets *it* receives. The pixels are
written by the gateware DMA, and those are not the same set: under continuously
sustained peak load the `AlwaysReady` gate drops whole packets to avoid
backpressuring the CPU's ethernet (`gateware/smoleth.py`), and the DMA's
watchdog abandons a payload if the DRAM writer stalls (`gateware/dma_writer.py`).
Neither is visible to the CPU, so it marks the frame complete and presents it
with stale bands.

This also undermines Tier 0: `present()` repairs the chunks its mask says are
missing, which is the wrong set.

**Evidence:** cumulative `dma_chunks` 35,046 against `n_udp` 54,085 after
high-rate sweeps, with `dma_bad_magic = 0` — payloads that passed the UDP filter
but were never written. At non-saturating rates the two track exactly (10 frames
= 680 chunks = 327,680 pixels, verified).

**Next step:** expose a per-chunk arrival bitmap from `Hub75UdpDma` as CSRs (256
bits = 8 words), set as each chunk's write completes, cleared by the CPU when it
starts a frame. Have `bitmap_udp.rs` read that instead of counting its own
packets. The CPU keeps completion/repair/swap; only the source of truth changes.

## 2. Flash day — standalone boot has never been verified

**Priority: high. Needs David present.**

The panel depends on a TFTP server running on `jupiter`. It has never booted
standalone. `build.sh flash`, `flash-firmware` and `flash-all` **have never been
run by anyone** — the flags were verified against openFPGALoader 0.11.0 by
inspection, not execution. Treat the first run as a test.

Order matters: `flash-all` writes bitstream then firmware, because writing the
bitstream can erase sectors past its own length and would take the firmware at
chip offset `0x100000` with it.

Now unblocked because JTAG works (item 3 below is the fix that made it possible).
`/api/reboot` is `soc_rst` — it re-runs the BIOS without reconfiguring the FPGA,
so it tests firmware-from-flash with no hands. A real power cycle additionally
tests the ECP5 loading its bitstream from flash.

**Clear these two first:**
- The gateware declares a Winbond `W25Q32JV`. The silicon reports JEDEC
  `c8 40 16` = **GigaDevice GD25Q32**. Both are 4MB/256-byte-page/`READ_1_1_1`
  so it very likely works either way, but the declaration does not match the
  part. Fixing it needs a bitstream rebuild, so do it before flash day, not
  after.
- `ARCH.md` presents v1.10.7 as the fix for flash boot. That is unproven. The
  simpler untested explanation is that nothing ever wrote firmware to
  `FLASH_BOOT_ADDRESS`, because `build.sh` had no target that did until
  2026-09-06.

## 3. JTAG — fixed, recorded here so it is not rediscovered

The Waveshare USB Blaster corrupts bulk JTAG transfers: two dumps of the same
1MB of flash differed in **29% of bytes** (BER ~9.7e-2), while `--detect` passed
every time because the first ~1037 bytes are clean. It also silently ignores
`--freq`. This had blocked all flashing for months.

**Use the Tigard** (`--cable tigard`): byte-identical dumps, 4.6s vs 36.3s, and
it honours `--freq`. Board pads are documented at `README.md:31-45`. Acceptance
test before trusting any programmer here: dump the same 1MB twice and compare
hashes — `--detect` passing proves nothing.

## 4. Eight panels at 30 fps — the actual goal

Demonstrated: 49 fps clean at 4 panels (256x128 = 32,768 px = 1.6 Mpx/s), with
100% of chunks landing at every rate from 7.4 to 49 fps.

Eight panels at 30 fps is 65,536 px = 2.0 Mpx/s. The framebuffer is **exactly**
65,536 words per buffer — 8 panels of 128x64 with zero headroom — so it fits,
but only just. Needs the panels physically wired.

**Beyond 8 panels, the framebuffer must grow first.** The bitstream drives 6
outputs x 2 chain = 12 panels, needing 98,304 words, and the display DMA already
reads that span every refresh. There is spare SDRAM in the region (2MB reserved,
512KB used), so raising it is cheap.

## 5. RGB565 — deferred, on its own merits

The framebuffer is 32bpp with a wasted byte (`0x00GGRRBB`). Packing to RGB565
halves both memory and display read bandwidth.

Originally planned as a prerequisite for Tier 2 and **dropped after measurement**:
the display read measures 78.9 MB/s but adding writes costs it nothing
observable, and Tier 2 needs only 3.9-7.9 MB/s. It was not needed to make room.

It remains the right lever for *panel count* and *refresh headroom* — worth doing
when item 4 pushes past 8 panels, not before.

## 6. Smaller known defects, none urgent

- **Art-Net colour order is wrong.** `artnet.rs` still packs `0x00BBGGRR`; the
  v1.10.1 colour fix corrected `patterns.rs` and `bitmap_udp.rs` and missed this
  file. Art-Net only sets the palette here and runs on the slow path.
- **`RX_RING` in `ethernet.rs` appears to be dead code** — a 32-slot 64KB ring
  the trap handler never touches, since `network_handler` reads MAC slots
  directly via `peek_rx()`. `poll_rx_to_ring()`/`ethmac()` look orphaned. Not
  removed; verify before deleting.
- **Single HTTP socket.** The panel refuses back-to-back connects while the
  previous one drains, which is why `bench_stream.py` retries with 0.7s gaps.
  Not a bug, but it makes any tool that polls status while streaming unreliable —
  it produced at least one bogus measurement during Tier 2 work.
- **Stale PTR, outside this repo.** Reverse-resolving 10.11.6.65 answers
  `dogwood.siegel.com`, but `dogwood` forward-resolves to 10.1.1.149 at another
  site. The flash host is `jupiter`. Recorded in `CLAUDE.md`; the DNS record
  itself still wants fixing.

---

## Method notes worth keeping

Two habits that cost real time when skipped:

- **The failure was intermittent (~1 run in 3).** Three runs of the *same* binary
  gave 0, 6, 0 resets. Never bisect this system on a single run, and verify the
  board is alive *before* each measurement — a dead board silently reports zero
  errors because nothing is running to fail.
- **Assume a fourth stale artifact.** Three separate ones appeared in one day: a
  bitstream older than its gateware, a `boot.bin` older than its sources, and a
  210-day-old TFTP daemon serving February firmware out of the deprecated
  `/u/siegeld/colorlight` tree. All failed the same way — silently serving
  something plausible. `build.sh` now guards the first two. Before believing any
  measurement, prove which binary is running; the version is on the panel at boot
  for exactly this reason.
