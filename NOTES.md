# Development Notebook

---

## 2026-02-08

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
