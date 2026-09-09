# Outstanding work

Tracking file for known defects and planned work. Each item states what is
wrong, why it matters, where it lives, the evidence, and the next concrete step
— enough to pick up cold, without the conversation that produced it.

Status as of 2026-09-09: HEAD `20ac5fe`. Firmware v1.10.11. Tier 0 (packet-loss
handling) and Tier 2 (hardware pixel path) both work on hardware.

**Current wall: SIX 128x64 panels, one per connector J1-J6, no chaining, 2 wide
x 3 tall = 256x192.** Not the four-panel 2x2 the older notes describe.

The panel at 10.11.6.72 boots its firmware over TFTP **from Marquee**
(`/srv/docker/marquee`, port 6969), not from `./build.sh boot` — see item 8. Its
bitstream is loaded over JTAG into SRAM and is **volatile**; it still cannot
survive a power cycle (item 2).

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

**The gateware half is DONE.** `Hub75UdpDma` already exposes the bitmap as
`pixdma.arrival0..7` plus `pixdma.frame_id`, set only as each chunk's write
completes. `bitmap_udp.rs` still ignores them and counts its own packets.

**2026-09-09: the asymmetry also runs the other way, and it is now the single
biggest limit on frame rate.** Streaming video at 256x192, hardware wrote 114,702
chunks while the CPU only saw 94,727 — ~20k chunks that ARE in SDRAM but which
the CPU believes are missing. `present()` drops any frame missing more than
`MAX_REPAIR_CHUNKS` (2), so under sustained load essentially every frame looks
too damaged, the CPU stops swapping, and **the display freezes with correct
pixels already sitting in the framebuffer**. Observed directly: the wall froze
mid-video while the sender kept running at 34 fps.

**Next step:** have `bitmap_udp.rs` read `pixdma.arrival0..7` / `frame_id`
instead of its own packet count. The CPU keeps completion/repair/swap; only the
source of truth changes. Then go further and stop delivering port-7000 traffic
to the CPU's MAC at all (a filter in `smoleth.py`), which removes the per-packet
interrupt cost that item 9 shows is the frame-rate ceiling.

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
~~`/api/reboot` is `soc_rst` — it tests firmware-from-flash with no hands.~~
**`/api/reboot` DOES NOT WORK (verified 2026-09-09).** It returns HTTP 200 and
then nothing happens: no TFTP transfer appears in the boot server's log, the
config is not re-read, and counters keep climbing. The only way to actually cold
-boot the panel is a JTAG reload (`./build.sh --cable tigard sram`). Do not plan
flash day around `/api/reboot` until it is fixed or diagnosed.

**Hazard while flash and SRAM disagree.** The bitstream currently running is
JTAG-loaded into SRAM and is lost on power-off, and flash has never been
written, so it holds an older bitstream. Marquee meanwhile serves the CURRENT
firmware. On a power cycle the FPGA would load the old gateware and TFTP the new
firmware onto it — and in the old gateware `pixdma.base` is a writable CSR that
the current firmware no longer sets (it is derived in gateware now), so it stays
0 and the pixel DMA writes over SDRAM word 0, which is the firmware's own
`.text`. Flashing the matching bitstream closes this.

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

## 4. Panel count — the framebuffer limit recorded here was WRONG

**Corrected 2026-09-09.** This item used to say the framebuffer is "65,536 words
per buffer — exactly 8 panels of 128x64, with no headroom" and that the
bitstream's 12 panels "would not fit". That came from a stale comment, not from
the arithmetic. `FB_TOTAL_WORDS = 0x00400000 / 2 / 4` is **524,288**, so each
buffer is **262,144 words** — the comment on that very line in `hub75.rs` says
131072 and is wrong.

| Panels | Pixels | vs 262,144 |
|---|---|---|
| 6 (current) | 49,152 | fits |
| 9 | 73,728 | fits |
| 12 (bitstream max at chain 2) | 98,304 | fits |

So there is no framebuffer work to do before adding panels, and the "grow it
first" prerequisite that was recorded here does not exist.

**Connectors are not the limit either.** Board revision 8.2 defines 16
connectors (`j1`..`j16`); `--outputs 6` is just the current build flag.

**Frame rate is the real cost, and it scales with pixels, not with wiring.** At
the measured ~2,000 packet/s CPU ceiling: 6 panels = 101 chunks/frame ~ 20 fps;
9 panels = 152 chunks/frame ~ 13 fps. Item 9 is what buys that back.

**Going to 9 panels one-per-connector** needs, all together: rebuild with
`--outputs 9 --chain-length 1`, regenerate the PAC, and change `hub75.rs`
`OUTPUTS` 6->9 and `CHAIN_LENGTH` 2->1, **and `layout.rs` `MAX_OUTPUTS` 6->9**.
That last one fails silently: `parse()` tests `n <= MAX_OUTPUTS`, so `J7`-`J9`
would be dropped with no error and those three panels would show the unassigned
default.

## 5. RGB565 — deferred, on its own merits

The framebuffer is 32bpp with a wasted byte (`0x00GGRRBB`). Packing to RGB565
halves both memory and display read bandwidth.

Originally planned as a prerequisite for Tier 2 and **dropped after measurement**:
the display read measures 78.9 MB/s but adding writes costs it nothing
observable, and Tier 2 needs only 3.9-7.9 MB/s. It was not needed to make room.

It remains the right lever for *panel count* and *refresh headroom* — worth doing
when item 4 pushes past 8 panels, not before.

## 6. Dedicated panel VLAN

The panels sit on a general-purpose /24 and drown in broadcast traffic. Measured
on the bench panel: `mcast_dropped` 1.4M, `slow_arp` 15.5k, `mac_overflow` 806k.
Multicast is discarded cheaply in the fast path, but ARP went through smoltcp on
the SLOW path -- inside the same interrupt handler that consumes the pixel
stream -- so every broadcast ARP on the segment was a short gap in frame
processing, visible as a periodic stutter in scrolling text.

Mitigated in firmware (`is_foreign_arp`, v1.10.11): ARP not addressed to this
panel is acked and discarded without waking the network stack. ARP *for* the
panel is still handled -- dropping it would let the sender's cache expire and
kill the stream.

**Prerequisite, now fixed (v1.10.11).** The Timer0 interrupt was never unmasked,
so `TIME_MS` only advanced when an ethernet packet arrived and any whole second
with no packet was lost outright (`ev_pending` is a latched bit, not a counter).
Broadcast traffic hid it. Moving to a quiet VLAN would have frozen the clock and
silently killed the stale-frame flush — the one thing that presents the last
frame when a stream stops — exactly when the quiet network made it necessary.
Fixed in `f0b5c2b`; do not un-fix it.

**That is the tactical fix. The real one is a dedicated panel VLAN**, which
removes the broadcast domain rather than filtering it, and also lets the panels
be addressed independently of the general estate. Marquee stores each panel's
address in the database and hardcodes nothing, so the move is a re-address plus
DHCP option 66 pointing at whichever host runs Marquee.

## 7. Smaller known defects, none urgent

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
- **Layout YAML over 512 bytes is silently truncated.** `tftp_config.rs` has
  `MAX_CONFIG_SIZE = 512`. A config longer than that is cut mid-file with no
  error, `parse()` sees whatever survived, and the panel falls back to its
  `single_panel(96, 48)` default — a 1x1 96x48 display and no panel assignments.
  Cost a boot on 2026-09-09 after a comment block pushed the file to 1061 bytes.
  Either raise the buffer or keep configs terse; the current one notes the limit.

- **Chain slot 1 is the near panel, and assigning only slot 0 looks like a
  working config.** With `chain_length=2` in the bitstream the gateware shifts
  two panels per connector: slot 0 goes out first and lands on the FAR panel,
  slot 1 stays on the near, directly-connected one. With a single panel per
  connector the panel IS the near one, so `J1: 0,0` leaves it reading an
  unassigned slot and **every connector displays the same default region** — it
  reads as "the test pattern just repeats". Current workaround is to write the
  position twice (`J1: 1,0 1,0`). Building with `--chain-length 1` removes the
  trap entirely and is the right answer once the wall stops chaining.

- **Stale PTR, outside this repo.** Reverse-resolving 10.11.6.65 answers
  `dogwood.siegel.com`, but `dogwood` forward-resolves to 10.1.1.149 at another
  site. The flash host is `jupiter`. Recorded in `CLAUDE.md`; the DNS record
  itself still wants fixing.

## 8. Marquee owns panel TFTP now — and cannot do per-panel firmware

`build.sh`'s own TFTP server is an anachronism on the flash host. Marquee
(`/srv/docker/marquee`) serves the fleet: `services/player/main.py:tftp_resolve`
answers `boot.bin` and `<mac>.yml` on the same port 6969, from its database, so
rollout is a fleet operation with a record of what each panel booted. `build.sh`
now detects the port is held and refuses rather than starting a second server
(`cd34efb`) — do not "fix" that by killing Marquee.

**The `firmware` field on a panel record does not work.** `tftp_resolve` looks
the panel up with `Panel.host == client_ip`, but the BIOS fetches `boot.bin`
before it has its DHCP lease — the request arrives from **10.11.6.250**, so the
lookup misses and it silently falls back to the default `boot.bin`. Verified
2026-09-09: the record said `boot-v1.10.11-22b0a86.bin` and Marquee served the
225,640-byte default instead. `<mac>.yml` works because that branch has a MAC
fallback; the `boot.bin` branch has none.

**Deploying firmware today therefore means replacing `/data/firmware/boot.bin`.**
The previous build is kept beside it as `boot-prev-<md5>.bin`; rollback is one
`cp`. **Next step:** give the `boot.bin` branch the same MAC fallback, or key it
on the BIOS's source address, so per-panel firmware and staged rollout work.

## 9. Frame rate is bounded by per-interrupt cost, not by pixels

**Measured 2026-09-09** at 256x192 with the hardware DMA on. Stable ceiling is
**~20.6 fps** (22 requested: 20.6 / 20.5 / 20.7 across three runs, `mac_overflow`
+16 / +29 / +28). It is bistable above that — 30 requested gave 26.8, then 8.8,
then 14.4 — because once `mac_overflow` starts, the CPU's arrival mask degrades,
frames exceed the 2-chunk repair limit, and everything is dropped (item 1).

The DMA is not the limit: it sustained **2.23 Mpx/s (36 fps)** and wrote 114,702
of 114,703 chunks. The limit is that the CPU is in the per-packet path at all.

**`fast_path / isr_count` = 1.13 packets per ISR entry.** Despite
`MAX_PACKETS_PER_ISR = 64`, the handler drains until empty and self-balances at
the point where ISR duration equals the inter-packet gap, so full interrupt
overhead is paid once per packet. At ~2,000 packets/s that is ~19,800 cycles per
packet at 40 MHz — roughly thirty times the visible work.

Already removed (`20ac5fe`, worth ~7%): the unconditional `handle_telnet()` /
`handle_http()` on every ISR entry, and the per-packet `BitmapStats` copy into
uncached SDRAM. Packets per ISR stayed at 1.00, so what remains is **the trap
entry/exit itself** — 31 GPRs saved and restored through uncached SDRAM on every
interrupt.

**Next step, in order of leverage:** item 1's arrival bitmap plus a `smoleth.py`
filter so port-7000 never reaches the CPU's MAC (removes the cost entirely);
then RGB565 (item 5, 1.5x fewer packets per frame); then a payload above 1471
bytes toward the 2048-byte slot size, which needs the dedicated VLAN (item 6).

## 10. sys_clk cannot be raised without decoupling the HUB75 clock

**Tried and reverted 2026-09-09.** 60 MHz was expected to give 1.5x on
everything. Two problems, both real:

- Timing closes only barely: `61.15 MHz` against a 60 MHz constraint, and the
  first placement attempt failed outright at **51.56 MHz**. Not a margin to ship.
- The HUB75 shift clock is `clk.eq(buffer_counter[0])` in `hub75.py` — literally
  `sys_clk/2`. 60 MHz drives the panels at 30 MHz, past the 20 MHz limit the
  `--sys-clk-freq` comment in `colorlight.py` warns about. **Confirmed on
  hardware: the image showed artifacts.** That comment is accurate, not stale.

So the clock cannot move until the HUB75 shift is decoupled from `sys_clk` —
which is exactly what that comment proposes ("maybe add a CDC?"). Note the
divider cannot simply be widened either: the pixel data path is indexed on
`buffer_counter[0]`, so a /4 clock needs the pipeline reworked to match, and at
60/4 = 15 MHz the shift rate would be *lower* than today's 20 MHz.

`SYS_CLK_HZ` in `network.rs` is now the single constant that `TIMER_RELOAD`,
`CYCLES_PER_MS` and the Timer0 reload all derive from. It **must** match
`sys_clk_freq` in `colorlight.py`; a mismatch silently rescales every deadline in
the firmware.

---

## Method notes worth keeping

Two habits that cost real time when skipped:

- **The failure was intermittent (~1 run in 3).** Three runs of the *same* binary
  gave 0, 6, 0 resets. Never bisect this system on a single run, and verify the
  board is alive *before* each measurement — a dead board silently reports zero
  errors because nothing is running to fail.
- **A fifth one turned up.** 2026-09-09: **eighteen** `dnsmasq` TFTP daemons had
  been running on the flash host since 25-26 January — seven months — all serving
  the deprecated `/u/siegeld/colorlight` tree whose `boot.bin` is dated
  2026-02-09. They accumulated because `build.sh` only ever tracked servers it
  started itself, through a pid file inside its own tree, so anything started by
  another checkout was invisible to both `stop` and `ensure`. Fixed in `cd34efb`;
  the lesson is that "is it running?" must be asked of the PORT, not of a pid
  file you wrote.

- **Assume a fourth stale artifact.** Three separate ones appeared in one day: a
  bitstream older than its gateware, a `boot.bin` older than its sources, and a
  210-day-old TFTP daemon serving February firmware out of the deprecated
  `/u/siegeld/colorlight` tree. All failed the same way — silently serving
  something plausible. `build.sh` now guards the first two. Before believing any
  measurement, prove which binary is running; the version is on the panel at boot
  for exactly this reason.
