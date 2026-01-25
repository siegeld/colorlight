//! Test pattern generators for HUB75 LED panels

/// Pack RGB into little-endian u32 (0x00BBGGRR format)
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (b as u32) << 16 | (g as u32) << 8 | (r as u32)
}

/// Convert HSV to RGB. h in [0,360), s,v in [0,255]
fn hsv_to_rgb(h: u16, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }

    let region = h / 60;
    let remainder = ((h % 60) as u32 * 255) / 60;

    let p = ((v as u32) * (255 - s as u32)) / 255;
    let q = ((v as u32) * (255 - (s as u32 * remainder) / 255)) / 255;
    let t = ((v as u32) * (255 - (s as u32 * (255 - remainder)) / 255)) / 255;

    match region {
        0 => (v, t as u8, p as u8),
        1 => (q as u8, v, p as u8),
        2 => (p as u8, v, t as u8),
        3 => (p as u8, q as u8, v),
        4 => (t as u8, p as u8, v),
        _ => (v, p as u8, q as u8),
    }
}

/// Generate grid pattern - white lines with colored verticals and diagonal X
pub fn grid(width: u16, height: u16) -> impl Iterator<Item = u32> {
    let w = width as usize;
    let h = height as usize;

    (0..h).flat_map(move |row| {
        (0..w).map(move |col| {
            // Horizontal lines at 0, 1/4, 1/2, 3/4, h-1
            let is_h_line = row == 0
                || row == h / 4
                || row == h / 2
                || row == 3 * h / 4
                || row == h - 1;

            // Vertical lines at 0, 1/4, 1/2, 3/4, w-1
            let v_pos = if col == 0 {
                Some(0)
            } else if col == w / 4 {
                Some(1)
            } else if col == w / 2 {
                Some(2)
            } else if col == 3 * w / 4 {
                Some(3)
            } else if col == w - 1 {
                Some(4)
            } else {
                None
            };

            // Diagonal lines
            let diag1 = col == row * w / h;
            let diag2 = col == w - 1 - row * w / h;

            // Priority: diagonals > verticals > horizontals > black
            if diag1 {
                rgb(0, 255, 255) // CYAN
            } else if diag2 {
                rgb(255, 0, 255) // MAGENTA
            } else if let Some(v) = v_pos {
                match v {
                    0 => rgb(255, 0, 0),   // RED
                    1 => rgb(0, 255, 0),   // GREEN
                    2 => rgb(0, 0, 255),   // BLUE
                    3 => rgb(255, 255, 0), // YELLOW
                    _ => rgb(255, 0, 255), // MAGENTA
                }
            } else if is_h_line {
                rgb(255, 255, 255) // WHITE
            } else {
                rgb(0, 0, 0) // BLACK
            }
        })
    })
}

/// Generate rainbow diagonal wave pattern
pub fn rainbow(width: u16, height: u16) -> impl Iterator<Item = u32> {
    let w = width as u32;
    let h = height as u32;

    (0..h).flat_map(move |row| {
        (0..w).map(move |col| {
            // Diagonal wave - hue varies with position
            let hue = ((col + row) * 360 * 2 / (w + h)) % 360;
            let (r, g, b) = hsv_to_rgb(hue as u16, 255, 255);
            rgb(r, g, b)
        })
    })
}

/// Generate solid color pattern
pub fn solid(width: u16, height: u16, r: u8, g: u8, b: u8) -> impl Iterator<Item = u32> {
    let total = (width as usize) * (height as usize);
    let color = rgb(r, g, b);
    core::iter::repeat(color).take(total)
}

/// Generate solid white pattern
pub fn solid_white(width: u16, height: u16) -> impl Iterator<Item = u32> {
    solid(width, height, 255, 255, 255)
}

/// Generate solid red pattern
pub fn solid_red(width: u16, height: u16) -> impl Iterator<Item = u32> {
    solid(width, height, 255, 0, 0)
}

/// Generate solid green pattern
pub fn solid_green(width: u16, height: u16) -> impl Iterator<Item = u32> {
    solid(width, height, 0, 255, 0)
}

/// Generate solid blue pattern
pub fn solid_blue(width: u16, height: u16) -> impl Iterator<Item = u32> {
    solid(width, height, 0, 0, 255)
}
