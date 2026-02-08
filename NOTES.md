# Development Notebook

---

## 2026-02-08

### 18:30 - ETHMAC Interrupt Investigation

Attempted to implement interrupt-driven packet reception to reduce MAC FIFO overflow.

**Key Findings:**

1. **VexRiscv "minimal" variant doesn't support external interrupts** — Changed to "lite" variant which adds ~25% CPU size but enables full interrupt support.

2. **mtvec is WRITE_ONLY in both variants** — Writes work but reads return 0. This is expected VexRiscv behavior.

3. **LiteX EventManager is level-triggered** — The `ev_status` register shows real-time packet availability. If you clear `ev_pending` while `ev_status=1`, the pending bit immediately re-sets and interrupt fires again.

4. **Calling Rust code from trap handler crashes** — Even with proper register save/restore, calling `poll_rx_to_ring()` from assembly trap handler crashes the device. Root cause unknown (stack alignment? atomics? race condition?).

5. **Interrupt storm from rapid re-enable** — If you re-enable the interrupt while a packet is waiting (`ev_status=1`), it fires immediately causing rapid oscillation. Solution: only re-enable when `ev_status=0`.

**Current Implementation (partial success):**
- Minimal trap handler: just disables `ev_enable` and returns via `mret`
- Main loop checks if ISR fired (`ev_enable=0`) and re-enables when idle
- Device is stable with interrupts enabled
- ~15% packet loss at max speed (vs polling-only baseline)

**Why it doesn't actually help much:**
- The interrupt only serves as a "wake-up signal" — it doesn't drain packets
- Main loop is already polling constantly via `drain_raw_bitmap`
- To truly help, the ISR would need to drain to ring buffer, but that crashes

**Relevant addresses:**
- ETHMAC base: `0xF0001800`
- `sram_writer_ev_pending`: `0xF0001810` (W1C - read then write back to clear)
- `sram_writer_ev_enable`: `0xF0001814` (write 0 to disable, 1 to enable)
- `sram_writer_ev_status`: `0xF000180C` (read-only, shows raw packet availability)

**VexRiscv interrupt CSRs:**
- `mtvec`: trap handler address (WRITE_ONLY)
- `0xBC0` (IRQ_MASK): which interrupts are enabled (ETHMAC = bit 2)
- `0xFC0` (IRQ_PENDING): which interrupts are pending
- `mie`: machine interrupt enable (bit 11 = MEIE for external)
- `mstatus`: global interrupt enable (bit 3 = MIE)

---

### 10:53 - Test Pattern Delay Requirement

`--delay 0.002` (2ms) works - same pacing as video streaming. This matches the auto-calculated delay in `send_youtube.py`:
```
chunk_delay = 0.9 / fps / total_chunks  # ≈1.8ms for 15fps, 34 chunks
```

With `--delay 0` all 34 packets arrive instantly → MAC FIFO overflow (8 slots).
With `--delay 0.1` too slow for streaming mode detection (200ms timeout).

Test command: `python3 tools/send_test_pattern.py --smoke --host 10.11.6.72 --width 256 --height 64 --delay 0.002`

Note: May need to send twice occasionally - timing-sensitive.

---

### 09:30 - Post-CDC Revert Analysis

Reverted dual clock domain CDC implementation back to v1.9.0 (40MHz baseline). The CDC approach caused display banding.

**What was tried:**
- Added `cd_hub75` at 20MHz separate from `cd_sys`
- Changed FrameController, RowModule, RowColorOutput to `sync.hub75`
- Added MultiReg/PulseSynchronizer for control signals
- Set row buffer Memory ports to different clock domains

**Why it failed:**
1. Row buffer dual-clock memory may not work correctly with Migen's Memory primitive on ECP5
2. PulseSynchronizer timing may drop pulses at the clock ratio difference
3. FSM state machine coordination between domains too complex
4. The `shifting_buffer` signal selection of Array elements crosses domains unsafely

**Key discovery:** The CPU speed limit (~60MHz) is due to SDRAM PHY critical path, not CPU logic:
- Achieved timing: 60.22 MHz (16.6ns critical path)
- Bottleneck: SDRAM address decode → data valid path through `GENSDRPHY`

**Next approach:** Try HalfRate SDRAM mode (1:2) instead of CDC. The code already supports it - just need to enable `sdram_rate="1:2"` and target 80MHz sys clock.

---
