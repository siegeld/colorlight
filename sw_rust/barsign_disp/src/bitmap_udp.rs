use crate::hub75::Hub75;

const HEADER_SIZE: usize = 10;
const MAX_PAYLOAD: usize = 1462;
const PIXELS_PER_CHUNK: usize = MAX_PAYLOAD / 3; // 487

#[derive(Clone, Copy)]
pub struct BitmapStats {
    pub packets_total: u32,
    pub packets_valid: u32,
    pub packets_bad_magic: u32,
    pub packets_bad_header: u32,
    pub frames_completed: u32,
    pub frames_partial: u32,
    pub frames_dropped: u32,
    pub last_frame_id: u16,
    pub last_chunk_index: u8,
    pub last_total_chunks: u8,
    pub last_width: u16,
    pub last_height: u16,
    pub last_data_len: u16,
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
            frames_completed: 0,
            frames_partial: 0,
            frames_dropped: 0,
            last_frame_id: 0,
            last_chunk_index: 0,
            last_total_chunks: 0,
            last_width: 0,
            last_height: 0,
            last_data_len: 0,
            chunks_received: 0,
            frame_interval_ms: 0,
            avg_interval_ms: 0,
            jitter_ms: 0,
        }
    }
}

pub struct BitmapReceiver {
    current_frame_id: u16,
    pub chunks_count: u16, // number of chunks received for current frame
    total_chunks: u8,
    width: u16,
    height: u16,
    last_complete_ms: i64,
    pub stats: BitmapStats,
}

impl BitmapReceiver {
    pub fn new() -> Self {
        Self {
            current_frame_id: u16::MAX, // sentinel: ensures first packet triggers reset
            chunks_count: 0,
            total_chunks: 0,
            width: 0,
            height: 0,
            last_complete_ms: 0,
            stats: BitmapStats::new(),
        }
    }

    fn update_timing(&mut self, time_ms: i64) {
        if self.last_complete_ms > 0 {
            let interval = (time_ms - self.last_complete_ms) as u32;
            self.stats.frame_interval_ms = interval;
            if self.stats.avg_interval_ms == 0 {
                self.stats.avg_interval_ms = interval;
            } else {
                // EMA: avg = (avg * 7 + new) / 8
                self.stats.avg_interval_ms =
                    (self.stats.avg_interval_ms * 7 + interval) >> 3;
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

    /// Process one UDP packet. Returns true if the frame should be displayed
    /// (complete, or partial frame was swapped before starting new frame).
    pub fn process_packet(&mut self, data: &[u8], hub75: &mut Hub75, time_ms: i64) -> bool {
        self.stats.packets_total += 1;
        self.stats.last_data_len = data.len() as u16;

        if data.len() < HEADER_SIZE {
            self.stats.packets_bad_header += 1;
            return false;
        }

        // Validate magic
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

        self.stats.packets_valid += 1;

        // New frame: handle incomplete previous frame, then reset
        let mut swapped_partial = false;
        if frame_id != self.current_frame_id {
            // Reject frames whose dimensions don't match the configured image size.
            // With chain_length_2 layouts the image width is the virtual width (e.g. 256),
            // and the sender must match it — otherwise SDRAM row addressing breaks.
            let (cur_w, cur_len) = hub75.get_img_param();
            let incoming_len = width as u32 * height as u32;
            if cur_w != 0 && (width != cur_w || incoming_len != cur_len) {
                self.stats.packets_bad_header += 1;
                return false;
            }

            if self.chunks_count > 0 {
                let total = self.total_chunks as u16;
                if total > 0 && self.chunks_count >= total.saturating_sub(2) {
                    // Close enough — swap the partial frame
                    hub75.swap_buffers();
                    self.stats.frames_partial += 1;
                    self.update_timing(time_ms);
                    swapped_partial = true;
                } else {
                    self.stats.frames_dropped += 1;
                }
            }
            self.current_frame_id = frame_id;
            self.chunks_count = 0;
            self.total_chunks = total_chunks;
            self.width = width;
            self.height = height;
        }

        // Write pixel data to back buffer
        let pixel_data = &data[HEADER_SIZE..];
        let pixel_offset = chunk_index as usize * PIXELS_PER_CHUNK;

        // Convert RGB byte triples to u32 pixels (0x00BBGGRR format)
        let pixels = pixel_data.chunks_exact(3).map(|c| {
            (c[2] as u32) << 16 | (c[1] as u32) << 8 | (c[0] as u32)
        });
        hub75.write_img_data(pixel_offset, pixels);

        // Track received count
        self.chunks_count += 1;
        self.stats.chunks_received = self.chunks_count;

        // Complete when last chunk arrives (sender sends sequentially)
        // or when count reaches total (all chunks received)
        let complete = chunk_index == total_chunks - 1
            || self.chunks_count >= self.total_chunks as u16;
        if complete {
            self.stats.frames_completed += 1;
            self.chunks_count = 0; // ready for next frame
            self.update_timing(time_ms);
        }
        complete || swapped_partial
    }
}
