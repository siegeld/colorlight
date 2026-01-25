use litex_pac as pac;

const CHAIN_LENGTH: u8 = 1;  // Matches chain_length_2=0 in gateware (single 128x64 panel)
const OUTPUTS: u8 = 8;

// Framebuffer layout in SDRAM (addresses in 32-bit words)
const FB_SDRAM_OFFSET_BYTES: u32 = 0x00400000 / 2;
const FB_TOTAL_WORDS: usize = 0x00400000 / 2 / 4;  // 131072 words total
const FB_HALF_WORDS: usize = FB_TOTAL_WORDS / 2;    // 65536 words per buffer
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
        self.display_buffer[0..self.length as usize].iter().copied()
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

    pub fn get_palette(&mut self) -> &'_ [u32] {
        const LENGTH: usize = 256;
        use pac::hub75_palette::Hub75Palette;
        let palette_adr = self.hub75_palette.hub75_palette() as *const Hub75Palette as *const u32;
        let palette_data: &[u32] = unsafe { core::slice::from_raw_parts(palette_adr, LENGTH) };
        palette_data
    }

}
