//! Bitmap UDP frame receiver.
//!
//! Wire format, little-endian: magic "BM", frame_id u16, chunk_index u8,
//! total_chunks u8, width u16, height u16, then RGB888 pixel bytes. Chunk N
//! carries pixels `N * PIXELS_PER_CHUNK ..`.
//!
//! Loss handling: the receiver tracks *which* chunks arrived in a bitmask
//! rather than merely counting them, so nothing depends on one specific packet
//! turning up. A frame is presented when its mask is full, when the next frame
//! starts arriving, or when it goes stale -- whichever happens first. Chunks
//! that never arrived are patched from the displayed buffer before the swap, so
//! a lost packet costs one frame of staleness in that band instead of two.

use crate::hub75::Hub75;

const HEADER_SIZE: usize = 10;
const MAX_PAYLOAD: usize = 1462;
pub const PIXELS_PER_CHUNK: usize = MAX_PAYLOAD / 3; // 487

/// chunk_index is a u8, so 256 bits of arrival mask covers the whole index space.
const MAX_CHUNKS: usize = 256;
const MASK_WORDS: usize = MAX_CHUNKS / 64;

/// Missing chunks patched from the front buffer before presenting a frame.
///
/// Each patch is a `PIXELS_PER_CHUNK` SDRAM-to-SDRAM copy -- roughly 0.4 ms on
/// this core -- and it runs in the ISR, which makes this the one place the loss
/// handling spends time exactly when the system is already behind: packets keep
/// arriving during the patch, so an over-generous budget under heavy loss feeds
/// back into more loss. Two caps it below a millisecond.
///
/// Two is also the threshold the pre-v1.10.8 code already used -- it swapped a
/// partial frame when `chunks_count >= total - 2` -- so nothing that used to
/// reach the panel stops reaching it. The difference is that those two chunks
/// are now patched rather than left showing a two-frame-old band. A frame
/// missing more is dropped and the previous frame stays up, as before.
const MAX_REPAIR_CHUNKS: u16 = 2;

/// Present an in-progress frame if nothing has arrived for it in this long.
///
/// This only has to catch one case: the stream *stops* with a frame part-sent.
/// A frame whose tail was lost while the sender keeps going is already flushed
/// promptly by the next frame's first packet. So the deadline wants to be
/// comfortably longer than any legitimate inter-chunk gap rather than tight --
/// a sender pacing at the old `--delay 0.1` puts 100 ms between chunks, and a
/// deadline below that would present after every single packet.
const STALE_MS: i64 = 250;

#[derive(Clone, Copy)]
pub struct BitmapStats {
    pub packets_total: u32,
    pub packets_valid: u32,
    pub packets_bad_magic: u32,
    pub packets_bad_header: u32,
    pub packets_duplicate: u32,
    pub frames_completed: u32,
    pub frames_partial: u32,
    pub frames_dropped: u32,
    pub frames_stale: u32,
    pub chunks_repaired: u32,
    pub last_frame_id: u16,
    pub last_chunk_index: u8,
    pub last_total_chunks: u8,
    pub last_width: u16,
    pub last_height: u16,
    pub last_data_len: u16,
    pub last_missing: u16,
    pub chunks_received: u16,
    pub frame_interval_ms: u32,
    pub avg_interval_ms: u32,
    pub jitter_ms: u32,
}

impl BitmapStats {
    pub const fn new() -> Self {
        Self {
            packets_total: 0,
            packets_valid: 0,
            packets_bad_magic: 0,
            packets_bad_header: 0,
            packets_duplicate: 0,
            frames_completed: 0,
            frames_partial: 0,
            frames_dropped: 0,
            frames_stale: 0,
            chunks_repaired: 0,
            last_frame_id: 0,
            last_chunk_index: 0,
            last_total_chunks: 0,
            last_width: 0,
            last_height: 0,
            last_data_len: 0,
            last_missing: 0,
            chunks_received: 0,
            frame_interval_ms: 0,
            avg_interval_ms: 0,
            jitter_ms: 0,
        }
    }
}

pub struct BitmapReceiver {
    current_frame_id: u16,
    /// Bit N set once chunk N of the current frame has been written.
    chunk_mask: [u64; MASK_WORDS],
    /// Chunks the current frame expects. 0 means no frame in progress.
    total_chunks: u8,
    received: u16,
    last_packet_ms: i64,
    last_complete_ms: i64,
    pub stats: BitmapStats,
}

impl BitmapReceiver {
    pub fn new() -> Self {
        Self {
            current_frame_id: u16::MAX,
            chunk_mask: [0; MASK_WORDS],
            total_chunks: 0,
            received: 0,
            last_packet_ms: 0,
            last_complete_ms: 0,
            stats: BitmapStats::new(),
        }
    }

    fn mask_test(&self, idx: u8) -> bool {
        self.chunk_mask[idx as usize >> 6] & (1u64 << (idx & 63)) != 0
    }

    fn mask_set(&mut self, idx: u8) {
        self.chunk_mask[idx as usize >> 6] |= 1u64 << (idx & 63);
    }

    fn mask_clear(&mut self) {
        self.chunk_mask = [0; MASK_WORDS];
    }

    fn update_timing(&mut self, time_ms: i64) {
        if self.last_complete_ms > 0 {
            let interval = (time_ms - self.last_complete_ms) as u32;
            self.stats.frame_interval_ms = interval;
            if self.stats.avg_interval_ms == 0 {
                self.stats.avg_interval_ms = interval;
            } else {
                // EMA: avg = (avg * 7 + new) / 8
                self.stats.avg_interval_ms = (self.stats.avg_interval_ms * 7 + interval) >> 3;
            }
            let avg = self.stats.avg_interval_ms;
            self.stats.jitter_ms = if interval > avg {
                interval - avg
            } else {
                avg - interval
            };
        }
        self.last_complete_ms = time_ms;
    }

    /// Present the frame currently in the back buffer.
    ///
    /// Patches any chunks that never arrived from the displayed buffer, then
    /// swaps. Returns true if a swap happened. Because every presented frame
    /// has each chunk either freshly written or patched, no partial frame can
    /// leave stale data behind for a later frame to inherit.
    fn present(&mut self, hub75: &mut Hub75, time_ms: i64) -> bool {
        if self.total_chunks == 0 {
            return false;
        }
        let expected = self.total_chunks as u16;
        let missing = expected.saturating_sub(self.received);
        self.stats.last_missing = missing;

        if missing > MAX_REPAIR_CHUNKS {
            // Too damaged to show. Leave the previous frame up; the next frame
            // will overwrite this buffer.
            self.stats.frames_dropped += 1;
            self.reset_frame();
            return false;
        }

        if missing > 0 {
            let img_len = hub75.img_len();
            for idx in 0..expected {
                if self.mask_test(idx as u8) {
                    continue;
                }
                let offset = idx as usize * PIXELS_PER_CHUNK;
                if offset >= img_len {
                    continue;
                }
                let len = PIXELS_PER_CHUNK.min(img_len - offset);
                hub75.repair_from_front(offset, len);
                self.stats.chunks_repaired += 1;
            }
            self.stats.frames_partial += 1;
        } else {
            self.stats.frames_completed += 1;
        }

        hub75.swap_buffers();
        self.update_timing(time_ms);
        self.reset_frame();
        true
    }

    fn reset_frame(&mut self) {
        self.mask_clear();
        self.total_chunks = 0;
        self.received = 0;
        self.stats.chunks_received = 0;
    }

    /// Present an in-progress frame that has gone quiet. Called from the main
    /// loop; without it, a frame whose tail was lost -- or the last frame of a
    /// stream -- would never reach the panel.
    pub fn tick(&mut self, hub75: &mut Hub75, time_ms: i64) -> bool {
        if self.total_chunks == 0 || self.received == 0 {
            return false;
        }
        if time_ms - self.last_packet_ms < STALE_MS {
            return false;
        }
        self.stats.frames_stale += 1;
        self.present(hub75, time_ms)
    }

    /// Process one UDP payload. Returns true if a frame was presented.
    pub fn process_packet(&mut self, data: &[u8], hub75: &mut Hub75, time_ms: i64) -> bool {
        self.stats.packets_total += 1;
        self.stats.last_data_len = data.len() as u16;

        if data.len() < HEADER_SIZE {
            self.stats.packets_bad_header += 1;
            return false;
        }
        if data[0] != 0x42 || data[1] != 0x4D {
            self.stats.packets_bad_magic += 1;
            return false;
        }

        let frame_id = u16::from_le_bytes([data[2], data[3]]);
        let chunk_index = data[4];
        let total_chunks = data[5];
        let width = u16::from_le_bytes([data[6], data[7]]);
        let height = u16::from_le_bytes([data[8], data[9]]);

        self.stats.last_frame_id = frame_id;
        self.stats.last_chunk_index = chunk_index;
        self.stats.last_total_chunks = total_chunks;
        self.stats.last_width = width;
        self.stats.last_height = height;

        if total_chunks == 0 || chunk_index >= total_chunks {
            self.stats.packets_bad_header += 1;
            return false;
        }

        // Bound the write offset against the framebuffer. Without this a packet
        // claiming a large total_chunks drives chunk_index * PIXELS_PER_CHUNK
        // past the buffer, and the resulting panic resets the SoC.
        let img_len = hub75.img_len();
        let max_chunks = (img_len + PIXELS_PER_CHUNK - 1) / PIXELS_PER_CHUNK;
        if img_len == 0 || total_chunks as usize > max_chunks {
            self.stats.packets_bad_header += 1;
            return false;
        }

        self.stats.packets_valid += 1;

        let mut presented = false;
        if frame_id != self.current_frame_id || total_chunks != self.total_chunks {
            // Reject frames whose dimensions don't match the configured image.
            // With chain_length 2 layouts the image width is the virtual width
            // (e.g. 256) and the sender must match it, or SDRAM row addressing
            // breaks.
            let (cur_w, cur_len) = hub75.get_img_param();
            let incoming_len = width as u32 * height as u32;
            if cur_w != 0 && (width != cur_w || incoming_len != cur_len) {
                self.stats.packets_bad_header += 1;
                return false;
            }

            // Flush the previous frame before reusing the back buffer.
            presented = self.present(hub75, time_ms);

            self.current_frame_id = frame_id;
            self.total_chunks = total_chunks;
        }

        if self.mask_test(chunk_index) {
            // Duplicate or reordered retransmit. Writing it again would only
            // burn bus cycles, and counting it would corrupt the arrival count.
            self.stats.packets_duplicate += 1;
            self.last_packet_ms = time_ms;
            return presented;
        }

        let pixel_offset = chunk_index as usize * PIXELS_PER_CHUNK;
        hub75.write_img_rgb888(pixel_offset, &data[HEADER_SIZE..]);

        self.mask_set(chunk_index);
        self.received += 1;
        self.last_packet_ms = time_ms;
        self.stats.chunks_received = self.received;

        if self.received >= self.total_chunks as u16 {
            return self.present(hub75, time_ms) || presented;
        }
        presented
    }
}
