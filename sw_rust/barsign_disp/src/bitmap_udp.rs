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
    pub last_frame_id: u16,
    pub last_chunk_index: u8,
    pub last_total_chunks: u8,
    pub last_width: u16,
    pub last_height: u16,
    pub last_data_len: u16,
    pub chunks_received: u16,
}

impl BitmapStats {
    pub const fn new() -> Self {
        Self {
            packets_total: 0,
            packets_valid: 0,
            packets_bad_magic: 0,
            packets_bad_header: 0,
            frames_completed: 0,
            last_frame_id: 0,
            last_chunk_index: 0,
            last_total_chunks: 0,
            last_width: 0,
            last_height: 0,
            last_data_len: 0,
            chunks_received: 0,
        }
    }
}

pub struct BitmapReceiver {
    current_frame_id: u16,
    chunks_received: u16, // bitmask, supports up to 16 chunks
    total_chunks: u8,
    width: u16,
    height: u16,
    pub stats: BitmapStats,
}

impl BitmapReceiver {
    pub fn new() -> Self {
        Self {
            current_frame_id: u16::MAX, // sentinel: ensures first packet triggers reset
            chunks_received: 0,
            total_chunks: 0,
            width: 0,
            height: 0,
            stats: BitmapStats::new(),
        }
    }

    /// Process one UDP packet. Returns true if the frame is now complete.
    pub fn process_packet(&mut self, data: &[u8], hub75: &mut Hub75) -> bool {
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

        if total_chunks == 0 || chunk_index >= total_chunks || total_chunks > 16 {
            self.stats.packets_bad_header += 1;
            return false;
        }

        self.stats.packets_valid += 1;

        // New frame: reset state and configure image params
        if frame_id != self.current_frame_id {
            self.current_frame_id = frame_id;
            self.chunks_received = 0;
            self.total_chunks = total_chunks;
            self.width = width;
            self.height = height;
            let length = width as u32 * height as u32;
            hub75.set_img_param(width, length);
        }

        // Write pixel data to back buffer
        let pixel_data = &data[HEADER_SIZE..];
        let pixel_offset = chunk_index as usize * PIXELS_PER_CHUNK;

        // Convert RGB byte triples to u32 pixels (0x00BBGGRR format)
        let pixels = pixel_data.chunks_exact(3).map(|c| {
            (c[2] as u32) << 16 | (c[1] as u32) << 8 | (c[0] as u32)
        });
        hub75.write_img_data(pixel_offset, pixels);

        // Mark chunk as received
        self.chunks_received |= 1 << chunk_index;
        self.stats.chunks_received = self.chunks_received;

        // Check if all chunks received
        let expected_mask = (1u16 << self.total_chunks) - 1;
        let complete = self.chunks_received == expected_mask;
        if complete {
            self.stats.frames_completed += 1;
            self.chunks_received = 0; // ready for next frame
        }
        complete
    }
}
