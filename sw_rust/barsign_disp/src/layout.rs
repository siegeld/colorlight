/// Panel layout configuration for multi-panel virtual displays.
/// Maps physical J-connectors (outputs 0–7) to grid positions.

pub const MAX_OUTPUTS: usize = 8;
const POS_UNIT: u16 = 16; // gateware multiplier

pub struct LayoutConfig {
    pub panel_width: u16,  // physical panel width (e.g., 96)
    pub panel_height: u16, // physical panel height (e.g., 48)
    pub grid_cols: u8,     // virtual grid columns
    pub grid_rows: u8,     // virtual grid rows
    /// For each output 0–7: Some((col, row)) if assigned, None if unused
    pub assignments: [Option<(u8, u8)>; MAX_OUTPUTS],
}

impl LayoutConfig {
    pub fn single_panel(width: u16, height: u16) -> Self {
        let mut assignments = [None; MAX_OUTPUTS];
        assignments[0] = Some((0, 0));
        Self {
            panel_width: width,
            panel_height: height,
            grid_cols: 1,
            grid_rows: 1,
            assignments,
        }
    }

    pub fn virtual_width(&self) -> u16 {
        self.panel_width * self.grid_cols as u16
    }

    pub fn virtual_height(&self) -> u16 {
        self.panel_height * self.grid_rows as u16
    }

    pub fn virtual_length(&self) -> u32 {
        self.virtual_width() as u32 * self.virtual_height() as u32
    }

    /// Apply layout to HUB75 hardware: sets image_width and all panel_param CSRs.
    pub fn apply(&self, hub75: &mut crate::hub75::Hub75) {
        hub75.set_img_param(self.virtual_width(), self.virtual_length());
        for (output, assignment) in self.assignments.iter().enumerate() {
            if let Some((col, row)) = assignment {
                let x = (*col as u16 * self.panel_width / POS_UNIT) as u8;
                let y = (*row as u16 * self.panel_height / POS_UNIT) as u8;
                hub75.set_panel_param(output as u8, 0, x, y, 0);
            }
        }
    }

    /// Parse layout from text config (KEY=VALUE format).
    ///
    /// Supported keys:
    /// - `grid=2x1` — set grid dimensions
    /// - `panel_width=96` — physical panel width
    /// - `panel_height=48` — physical panel height
    /// - `J1=0,0` through `J8=7,7` — assign output to grid position
    pub fn parse(text: &str) -> Option<Self> {
        let mut config = Self::single_panel(96, 48);
        // Clear default assignment — config should be explicit
        config.assignments = [None; MAX_OUTPUTS];

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Accept both "key=value" and YAML-style "key: value"
            let sep = line.find('=').or_else(|| line.find(": "));
            if let Some(pos) = sep {
                let skip = if line.as_bytes().get(pos) == Some(&b':') { 2 } else { 1 };
                let key = line[..pos].trim();
                let value = line[pos + skip..].trim();
                match key {
                    "grid" => {
                        if let Some((cols, rows)) = parse_grid(value) {
                            config.grid_cols = cols;
                            config.grid_rows = rows;
                        }
                    }
                    "panel_width" => {
                        if let Ok(w) = parse_u16(value) {
                            config.panel_width = w;
                        }
                    }
                    "panel_height" => {
                        if let Ok(h) = parse_u16(value) {
                            config.panel_height = h;
                        }
                    }
                    _ => {
                        // Try J1–J8
                        if let Some(rest) = key.strip_prefix('J') {
                            if let Ok(n) = parse_u8(rest) {
                                if n >= 1 && n <= 8 {
                                    if let Some((col, row)) = parse_pos(value) {
                                        config.assignments[(n - 1) as usize] = Some((col, row));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(config)
    }
}

fn parse_grid(s: &str) -> Option<(u8, u8)> {
    let pos = s.find('x')?;
    let cols = parse_u8(&s[..pos]).ok()?;
    let rows = parse_u8(&s[pos + 1..]).ok()?;
    Some((cols, rows))
}

fn parse_pos(s: &str) -> Option<(u8, u8)> {
    let pos = s.find(',')?;
    let col = parse_u8(&s[..pos]).ok()?;
    let row = parse_u8(&s[pos + 1..]).ok()?;
    Some((col, row))
}

/// Simple u8 parser for no_std (avoids pulling in core::str::parse for all int types).
fn parse_u8(s: &str) -> Result<u8, ()> {
    let mut result: u8 = 0;
    if s.is_empty() {
        return Err(());
    }
    for b in s.bytes() {
        if b < b'0' || b > b'9' {
            return Err(());
        }
        result = result.checked_mul(10).ok_or(())?.checked_add(b - b'0').ok_or(())?;
    }
    Ok(result)
}

/// Simple u16 parser for no_std.
fn parse_u16(s: &str) -> Result<u16, ()> {
    let mut result: u16 = 0;
    if s.is_empty() {
        return Err(());
    }
    for b in s.bytes() {
        if b < b'0' || b > b'9' {
            return Err(());
        }
        result = result.checked_mul(10).ok_or(())?.checked_add((b - b'0') as u16).ok_or(())?;
    }
    Ok(result)
}

/// Parse a "ColsxRows" grid spec. Public for use by menu commands.
pub fn parse_grid_spec(s: &str) -> Option<(u8, u8)> {
    parse_grid(s)
}

/// Parse a "col,row" position spec. Public for use by menu commands.
pub fn parse_pos_spec(s: &str) -> Option<(u8, u8)> {
    parse_pos(s)
}
