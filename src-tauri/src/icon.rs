//! Tray icons: the stacked-layers glyph, and on Windows a layer badge drawn at runtime (the
//! layer's first letter on the layer's color, with a battery bar).

use tauri::image::Image;

struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Canvas {
        Canvas { width, height, rgba: vec![0; (width * height * 4) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        // "Over" blending, so a translucent color darkens what is below instead of replacing it
        let a = color[3] as u32;
        let dst = &mut self.rgba[i..i + 4];
        for c in 0..3 {
            dst[c] = ((color[c] as u32 * a + dst[c] as u32 * (255 - a)) / 255) as u8;
        }
        dst[3] = (a + dst[3] as u32 * (255 - a) / 255) as u8;
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        for py in y..y + h {
            for px in x..x + w {
                self.set(px, py, color);
            }
        }
    }

    /// Filled rectangle with corners of radius `r` cut off.
    fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 4]) {
        for py in y..y + h {
            for px in x..x + w {
                let dx = (x + r - px).max(px - (x + w - 1 - r)).max(0);
                let dy = (y + r - py).max(py - (y + h - 1 - r)).max(0);
                if dx * dx + dy * dy <= r * r {
                    self.set(px, py, color);
                }
            }
        }
    }

#[cfg_attr(target_os = "macos", allow(dead_code))] // the Windows tray badge
    fn glyph(&mut self, c: char, x: i32, y: i32, scale: i32, color: [u8; 4]) {
        let rows = font_5x7(c);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0b10000 >> col) != 0 {
                    self.rect(x + col * scale, y + row as i32 * scale, scale, scale, color);
                }
            }
        }
    }

    fn into_image(self) -> Image<'static> {
        Image::new_owned(self.rgba, self.width, self.height)
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))] // the Windows tray badge
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The stacked-layers glyph from the design (design/keylay.pen). On macOS it is a template
/// image: only its alpha counts, so it follows the menu bar's light or dark appearance. On
/// Windows it is drawn in the base layer's gray, which reads on light and dark taskbars.
pub fn glyph() -> Image<'static> {
    if cfg!(target_os = "macos") {
        return Image::from_bytes(include_bytes!("../icons/menubar-glyph36.png"))
            .expect("menubar-glyph36.png")
            .to_owned();
    }
    let image = Image::from_bytes(include_bytes!("../icons/menubar-glyph32.png")).expect("menubar-glyph32.png");
    let mut rgba = image.rgba().to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px[..3].copy_from_slice(&LAYER_COLORS[0][..3]);
    }
    Image::new_owned(rgba, image.width(), image.height())
}

/// The layer colors of the design, by layer index (repeating after 7).
pub const LAYER_COLORS: [[u8; 4]; 7] = [
    [138, 143, 152, 255], // slate
    [59, 130, 246, 255],  // blue
    [16, 185, 129, 255],  // green
    [245, 158, 11, 255],  // amber
    [139, 92, 246, 255],  // violet
    [236, 72, 153, 255],  // pink
    [20, 184, 166, 255],  // teal
];

/// Windows and Linux tray badge: the first letter of the layer on the layer's color, and the
/// lowest battery level as a bar along the bottom.
#[cfg_attr(target_os = "macos", allow(dead_code))] // the Windows tray badge
pub fn layer_badge(label: &str, layer: u8, battery: Option<u8>) -> Image<'static> {
    let mut c = Canvas::new(32, 32);
    c.round_rect(0, 0, 32, 32, 7, LAYER_COLORS[layer as usize % LAYER_COLORS.len()]);

    let letter = label.chars().find(|c| c.is_ascii_alphanumeric()).unwrap_or('?');
    let top = if battery.is_some() { 3 } else { 5 };
    c.glyph(letter.to_ascii_uppercase(), 9, top, 3, WHITE);

    if let Some(level) = battery {
        let color = match level {
            0..=19 => [239, 68, 68, 255],
            20..=39 => [245, 158, 11, 255],
            _ => [34, 197, 94, 255],
        };
        c.round_rect(6, 25, 20, 4, 2, [0, 0, 0, 64]);
        c.round_rect(6, 25, ((20 * level.min(100) as i32 + 50) / 100).max(2), 4, 2, color);
    }
    c.into_image()
}

/// 5x7 bitmap font for 0-9 and A-Z; each row's low 5 bits, left pixel first.
#[cfg_attr(target_os = "macos", allow(dead_code))] // the Windows tray badge
fn font_5x7(c: char) -> [u8; 7] {
    match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        _ => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_have_the_right_size() {
        let g = glyph();
        assert_eq!(g.rgba().len(), (g.width() * g.height() * 4) as usize);
        assert!(g.width() >= 32 && g.height() >= 32);
        let b = layer_badge("Lower", 2, Some(55));
        assert_eq!((b.width(), b.height()), (32, 32));
    }
}
