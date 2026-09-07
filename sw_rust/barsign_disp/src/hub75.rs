use litex_pac as pac;

const CHAIN_LENGTH: u8 = 2;  // Matches chain_length_2=1 in gateware (2 panels per output)
const OUTPUTS: u8 = 6;

// Framebuffer layout in SDRAM (addresses in 32-bit words)
const FB_SDRAM_OFFSET_BYTES: u32 = 0x00400000 / 2;
const FB_TOTAL_WORDS: usize = 0x00400000 / 2 / 4;  // 131072 words total
const FB_HALF_WORDS: usize = FB_TOTAL_WORDS / 2;    // 65536 words per buffer

/// Largest image one framebuffer can hold, in pixels. An image header
/// claiming more than this is not valid -- see img::load_image().
pub const MAX_IMG_PIXELS: usize = FB_HALF_WORDS;
const FB_BASE_WORDS: u32 = FB_SDRAM_OFFSET_BYTES / 4; // 0x80000 - gateware word address

pub struct Hub75 {
    hub75: pac::Hub75,
    /// Back buffer: CPU writes here via write_img_data()
    hub75_data: &'static mut [u32],
    /// Front buffer: HW reads from here for display
    display_buffer: &'static mut [u32],
    hub75_palette: pac::Hub75Palette,
    length: u32,
    /// Which half the HW is currently reading (0 or 1)
    active_buf: u8,
}

pub enum OutputMode {
    FullColor,
    Indexed,
}

impl Hub75 {
    pub fn new(hub75: pac::Hub75, hub75_palette: pac::Hub75Palette) -> Self {
        let base = (0x90000000u32 + FB_SDRAM_OFFSET_BYTES) as *mut u32;
        let buf0 = unsafe {
            core::slice::from_raw_parts_mut(base, FB_HALF_WORDS)
        };
        let buf1 = unsafe {
            core::slice::from_raw_parts_mut(base.add(FB_HALF_WORDS), FB_HALF_WORDS)
        };

        // HW starts reading from buffer 0 (gateware reset value = FB_BASE_WORDS)
        unsafe { hub75.fb_base().write(|w| w.offset().bits(FB_BASE_WORDS)) };

        Self {
            hub75,
            hub75_data: buf1,       // back buffer (CPU writes here)
            display_buffer: buf0,   // front buffer (HW reads here)
            hub75_palette,
            length: 0,
            active_buf: 0,
        }
    }

    pub fn on(&mut self) {
        self.hub75.ctrl().modify(|_, w| w.enabled().set_bit());
    }

    pub fn off(&mut self) {
        self.hub75.ctrl().modify(|_, w| w.enabled().clear_bit());
    }

    pub fn set_mode(&mut self, mode: OutputMode) {
        self.hub75.ctrl().modify(|_, w| match mode {
            OutputMode::FullColor => w.indexed().clear_bit(),
            OutputMode::Indexed => w.indexed().set_bit(),
        });
    }

    pub fn get_mode(&mut self) -> OutputMode {
        match self.hub75.ctrl().read().indexed().bit() {
            false => OutputMode::FullColor,
            true => OutputMode::Indexed,
        }
    }

    /// Write pixel data to the back buffer (not yet displayed)
    pub fn write_img_data(&mut self, offset: usize, data: impl Iterator<Item = u32>) {
        let sdram = self.hub75_data[offset..].iter_mut();
        for (sdram, data) in sdram.zip(data).take(self.length as usize - offset) {
            *sdram = data;
        }
    }

    /// Write RGB888 bytes into the back buffer starting at pixel `offset`.
    ///
    /// Reads the source 32 bits at a time. `src` points into the Ethernet MAC's
    /// slot buffer, and this SoC's VexRiscv_Lite has no data cache (only
    /// CONFIG_CPU_HAS_ICACHE), so every load is a full bus round-trip: reading
    /// byte-at-a-time costs three of them per pixel. Four pixels are twelve
    /// bytes -- exactly three aligned words -- so this form issues 3 loads + 4
    /// stores per 4 pixels instead of 12 loads + 4 stores.
    ///
    /// The word path needs 4-byte alignment. It always holds for a bitmap UDP
    /// packet read in place out of a 2048-aligned MAC slot: eth(14) + ip(4*IHL)
    /// + udp(8) + bitmap header(10) is a multiple of 4 for every legal IHL.
    /// The check is kept anyway so a caller passing anything else stays correct.
    ///
    /// Returns the number of pixels written.
    pub fn write_img_rgb888(&mut self, offset: usize, src: &[u8]) -> usize {
        let limit = (self.length as usize).min(self.hub75_data.len());
        if offset >= limit {
            return 0;
        }
        let n_px = (src.len() / 3).min(limit - offset);
        if n_px == 0 {
            return 0;
        }
        let dst = &mut self.hub75_data[offset..offset + n_px];
        if src.as_ptr() as usize % 4 == 0 {
            let words = src.as_ptr() as *const u32;
            let groups = n_px / 4;
            for g in 0..groups {
                // payload bytes, little-endian:
                //   w0 = R0 G0 B0 R1   w1 = G1 B1 R2 G2   w2 = B2 R3 G3 B3
                let (w0, w1, w2) = unsafe {
                    (
                        core::ptr::read_volatile(words.add(g * 3)),
                        core::ptr::read_volatile(words.add(g * 3 + 1)),
                        core::ptr::read_volatile(words.add(g * 3 + 2)),
                    )
                };
                // HUB75 wants 0x00GGRRBB
                let i = g * 4;
                dst[i] = ((w0 >> 8) & 0xFF) << 16 | (w0 & 0xFF) << 8 | ((w0 >> 16) & 0xFF);
                dst[i + 1] = (w1 & 0xFF) << 16 | ((w0 >> 24) & 0xFF) << 8 | ((w1 >> 8) & 0xFF);
                dst[i + 2] = ((w1 >> 24) & 0xFF) << 16 | ((w1 >> 16) & 0xFF) << 8 | (w2 & 0xFF);
                dst[i + 3] = ((w2 >> 16) & 0xFF) << 16 | ((w2 >> 8) & 0xFF) << 8 | ((w2 >> 24) & 0xFF);
            }
            for i in (groups * 4)..n_px {
                let c = &src[i * 3..i * 3 + 3];
                dst[i] = (c[1] as u32) << 16 | (c[0] as u32) << 8 | (c[2] as u32);
            }
        } else {
            for i in 0..n_px {
                let c = &src[i * 3..i * 3 + 3];
                dst[i] = (c[1] as u32) << 16 | (c[0] as u32) << 8 | (c[2] as u32);
            }
        }
        n_px
    }

    /// Copy a pixel range from the front (displayed) buffer into the back buffer.
    ///
    /// Used to patch chunks that never arrived before presenting a frame. Without
    /// this a lost packet leaves that band holding whatever the back buffer had,
    /// which under double buffering is the frame from *two* frames ago; after it,
    /// the band is one frame old and effectively invisible in motion.
    pub fn repair_from_front(&mut self, offset: usize, len: usize) {
        let limit = (self.length as usize)
            .min(self.hub75_data.len())
            .min(self.display_buffer.len());
        if offset >= limit || len == 0 {
            return;
        }
        let end = (offset + len).min(limit);
        self.hub75_data[offset..end].copy_from_slice(&self.display_buffer[offset..end]);
    }

    /// Configured image length in pixels.
    pub fn img_len(&self) -> usize {
        (self.length as usize).min(self.hub75_data.len())
    }

    /// Swap front and back buffers. The back buffer becomes visible and
    /// the old front buffer becomes available for writing.
    pub fn swap_buffers(&mut self) {
        core::mem::swap(&mut self.hub75_data, &mut self.display_buffer);
        self.active_buf ^= 1;
        let base = if self.active_buf == 1 {
            FB_BASE_WORDS + FB_HALF_WORDS as u32
        } else {
            FB_BASE_WORDS
        };
        unsafe { self.hub75.fb_base().write(|w| w.offset().bits(base)) };
    }

    /// Read pixel data from the front buffer (what's currently displayed)
    pub fn read_img_data(&'_ self) -> impl Iterator<Item = u32> + '_ {
        // Clamp like every other consumer of self.length; unguarded this
        // panics on a bad image header, and a panic reboots the SoC.
        let limit = (self.length as usize).min(self.display_buffer.len());
        self.display_buffer[0..limit].iter().copied()
    }

    /// Overlay the firmware version in the top-left of the back buffer.
    ///
    /// Called at boot so the panel itself reports which build is running.
    pub fn draw_version_banner(&mut self, width: usize) {
        if width == 0 {
            return;
        }
        let limit = (self.length as usize).min(self.hub75_data.len());
        let text_w = (crate::patterns::VERSION_TEXT.len() * 6 + 4).min(width);
        for row in 0..11usize {
            for col in 0..text_w {
                let idx = row * width + col;
                if idx >= limit {
                    return;
                }
                if crate::patterns::boot_version_pixel(col, row) {
                    self.hub75_data[idx] = 0x00FF_FFFF;
                } else {
                    self.hub75_data[idx] = 0;
                }
            }
        }
    }

    pub fn set_img_param(&mut self, width: u16, length: u32) {
        unsafe { self.hub75.ctrl().modify(|_, w| w.width().bits(width)) };
        self.length = length;
    }

    pub fn get_img_param(&self) -> (u16, u32) {
        let width = self.hub75.ctrl().read().width().bits();
        (width, self.length)
    }

    pub fn get_panel_params(&self) -> impl Iterator<Item = u32> + '_ {
        use pac::hub75::Panel0_0;
        let panel_adr = self.hub75.panel0_0() as *const Panel0_0 as *const u32;
        let panel_reg: &[u32] =
            unsafe { core::slice::from_raw_parts(panel_adr, (OUTPUTS * CHAIN_LENGTH) as usize) };
        panel_reg.iter().copied()
    }

    pub fn set_panel_params(&mut self, params: impl Iterator<Item = u32>) {
        use pac::hub75::Panel0_0;
        let panel_adr = self.hub75.panel0_0() as *const Panel0_0;
        let panel_reg: &[Panel0_0] =
            unsafe { core::slice::from_raw_parts(panel_adr, (OUTPUTS * CHAIN_LENGTH) as usize) };
        for (reg, data) in panel_reg.iter().zip(params) {
            unsafe { reg.write(|w| w.bits(data)) };
        }
    }

    pub fn set_panel_param(&mut self, output: u8, chain_num: u8, x: u8, y: u8, rot: u8) {
        if output >= OUTPUTS || chain_num >= CHAIN_LENGTH {
            return;
        }
        use pac::hub75::Panel0_0;
        let chain_offset = (output * CHAIN_LENGTH + chain_num) as usize;
        let panel_adr = self.hub75.panel0_0() as *const Panel0_0;
        let panel_reg: &[Panel0_0] =
            unsafe { core::slice::from_raw_parts(panel_adr, (OUTPUTS * CHAIN_LENGTH) as usize) };
        unsafe { panel_reg[chain_offset].write(|w| w.x().bits(x).y().bits(y).rot().bits(rot)) };
    }

    pub fn get_panel_param(&mut self, output: u8, chain_num: u8) -> (u8, u8, u8) {
        if output >= OUTPUTS || chain_num >= CHAIN_LENGTH {
            return (255, 255, 255);
        }
        use pac::hub75::Panel0_0;
        let chain_offset = (output * CHAIN_LENGTH + chain_num) as usize;
        let panel_adr = self.hub75.panel0_0() as *const Panel0_0;
        let panel_reg: &[Panel0_0] =
            unsafe { core::slice::from_raw_parts(panel_adr, (OUTPUTS * CHAIN_LENGTH) as usize) };
        let data = panel_reg[chain_offset].read();
        (data.x().bits(), data.y().bits(), data.rot().bits())
    }

    pub fn set_palette(&mut self, offset: u8, data: impl Iterator<Item = u32>) {
        const LENGTH: usize = 256;
        use pac::hub75_palette::Hub75Palette;
        let palette_adr = self.hub75_palette.hub75_palette() as *const Hub75Palette;
        let palette_data: &[Hub75Palette] =
            unsafe { core::slice::from_raw_parts(palette_adr, LENGTH) };
        for (index, data) in data.take(LENGTH - (offset as usize)).enumerate() {
            unsafe { palette_data[index + (offset as usize)].write(|w| w.bits(data)) };
        }
    }

    /// Read bitstream hardware parameters from CSRStatus registers.
    /// Returns (columns, rows, scan, chain_length_2, n_outputs).
    pub fn get_hw_info(&self) -> (u16, u16, u8, u8, u8) {
        let columns = self.hub75.hw_columns().read().bits() as u16;
        let rows = self.hub75.hw_rows().read().bits() as u16;
        let config = self.hub75.hw_config().read().bits() as u16;
        let scan = (config & 0xFF) as u8;
        let chain_length_2 = ((config >> 8) & 0xF) as u8;
        let n_outputs = ((config >> 12) & 0xF) as u8;
        (columns, rows, scan, chain_length_2, n_outputs)
    }

    pub fn get_palette(&mut self) -> &'_ [u32] {
        const LENGTH: usize = 256;
        use pac::hub75_palette::Hub75Palette;
        let palette_adr = self.hub75_palette.hub75_palette() as *const Hub75Palette as *const u32;
        let palette_data: &[u32] = unsafe { core::slice::from_raw_parts(palette_adr, LENGTH) };
        palette_data
    }

}
