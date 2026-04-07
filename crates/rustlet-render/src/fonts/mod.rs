use std::collections::HashMap;
use std::sync::LazyLock;

pub const DEFAULT_FONT: &str = "tb-8";
pub const MAX_TEXT_WIDTH: i32 = 1000;

pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub x_offset: i8,
    pub y_offset: i8,
    /// Advance width (DWIDTH): how far the cursor moves after this glyph.
    pub advance: u8,
    /// Row-major bitmap. Each row is `ceil(width / 8)` bytes, MSB-left.
    pub bitmap: Vec<u8>,
}

impl Glyph {
    pub fn bytes_per_row(&self) -> usize {
        ((self.width as usize) + 7) / 8
    }

    /// Test whether the pixel at (col, row) is set.
    pub fn pixel(&self, col: u8, row: u8) -> bool {
        if col >= self.width || row >= self.height {
            return false;
        }
        let bpr = self.bytes_per_row();
        let byte_idx = row as usize * bpr + (col as usize / 8);
        let bit = 7 - (col % 8);
        self.bitmap[byte_idx] & (1 << bit) != 0
    }
}

pub struct BitmapFont {
    pub char_width: u8,
    pub char_height: u8,
    glyphs: HashMap<char, Glyph>,
}

impl BitmapFont {
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.get(&ch)
    }

    pub fn measure_width(&self, text: &str) -> i32 {
        text.chars()
            .map(|ch| {
                self.glyphs
                    .get(&ch)
                    .map(|g| g.advance as i32)
                    .unwrap_or(self.char_width as i32)
            })
            .sum()
    }

    pub fn measure_height(&self) -> i32 {
        self.char_height as i32
    }
}

// --- BDF parser ---

fn parse_bdf(src: &str) -> BitmapFont {
    let mut font_bbx_w: u8 = 0;
    let mut font_bbx_h: u8 = 0;
    let mut glyphs = HashMap::new();

    let mut in_char = false;
    let mut in_bitmap = false;

    // Per-glyph state
    let mut encoding: i32 = -1;
    let mut bbx_w: u8 = 0;
    let mut bbx_h: u8 = 0;
    let mut bbx_xoff: i8 = 0;
    let mut bbx_yoff: i8 = 0;
    let mut dwidth: u8 = 0;
    let mut bitmap_rows: Vec<u8> = Vec::new();

    for line in src.lines() {
        let line = line.trim();

        if line.starts_with("FONTBOUNDINGBOX ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                font_bbx_w = parts[1].parse().unwrap_or(0);
                font_bbx_h = parts[2].parse().unwrap_or(0);
            }
            continue;
        }

        if line.starts_with("STARTCHAR ") {
            in_char = true;
            encoding = -1;
            bbx_w = font_bbx_w;
            bbx_h = font_bbx_h;
            bbx_xoff = 0;
            bbx_yoff = 0;
            dwidth = font_bbx_w;
            bitmap_rows.clear();
            continue;
        }

        if !in_char {
            continue;
        }

        if line.starts_with("ENCODING ") {
            encoding = line[9..].trim().parse().unwrap_or(-1);
        } else if line.starts_with("DWIDTH ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                dwidth = parts[1].parse().unwrap_or(font_bbx_w);
            }
        } else if line.starts_with("BBX ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                bbx_w = parts[1].parse().unwrap_or(0);
                bbx_h = parts[2].parse().unwrap_or(0);
                bbx_xoff = parts[3].parse().unwrap_or(0);
                bbx_yoff = parts[4].parse().unwrap_or(0);
            }
        } else if line == "BITMAP" {
            in_bitmap = true;
        } else if line == "ENDCHAR" {
            if encoding >= 0 {
                if let Some(ch) = char::from_u32(encoding as u32) {
                    let expected_bpr = ((bbx_w as usize) + 7) / 8;
                    let expected_len = expected_bpr * bbx_h as usize;

                    // Pad or truncate bitmap to expected size
                    bitmap_rows.resize(expected_len, 0);

                    glyphs.insert(
                        ch,
                        Glyph {
                            width: bbx_w,
                            height: bbx_h,
                            x_offset: bbx_xoff,
                            y_offset: bbx_yoff,
                            advance: dwidth,
                            bitmap: bitmap_rows.clone(),
                        },
                    );
                }
            }
            in_char = false;
            in_bitmap = false;
            bitmap_rows.clear();
        } else if in_bitmap {
            // Parse hex row into bytes
            let hex = line.trim();
            let mut i = 0;
            while i + 1 < hex.len() {
                if let Ok(byte) = u8::from_str_radix(&hex[i..i + 2], 16) {
                    bitmap_rows.push(byte);
                }
                i += 2;
            }
        }
    }

    BitmapFont {
        char_width: font_bbx_w,
        char_height: font_bbx_h,
        glyphs,
    }
}

// --- Font registry ---

macro_rules! register_fonts {
    ($($name:literal),+ $(,)?) => {
        static FONT_REGISTRY: LazyLock<HashMap<String, BitmapFont>> = LazyLock::new(|| {
            let mut map = HashMap::new();
            $(
                let src = include_str!(concat!("../../fonts/", $name, ".bdf"));
                map.insert($name.to_string(), parse_bdf(src));
            )+
            map
        });

        static FONT_NAMES: &[&str] = &[$($name),+];
    };
}

register_fonts!(
    "10x20",
    "5x8",
    "6x10",
    "6x10-rounded",
    "6x13",
    "CG-pixel-3x5-mono",
    "CG-pixel-4x5-mono",
    "Dina_r400-6",
    "tb-8",
    "terminus-12",
    "terminus-14",
    "terminus-14-light",
    "terminus-16",
    "terminus-16-light",
    "terminus-18",
    "terminus-18-light",
    "terminus-20",
    "terminus-20-light",
    "terminus-22",
    "terminus-22-light",
    "terminus-24",
    "terminus-24-light",
    "terminus-28",
    "terminus-28-light",
    "terminus-32",
    "terminus-32-light",
    "tom-thumb",
);

pub fn get_font(name: &str) -> &'static BitmapFont {
    FONT_REGISTRY
        .get(name)
        .or_else(|| FONT_REGISTRY.get(DEFAULT_FONT))
        .expect("default font must exist")
}

pub fn get_font_list() -> Vec<&'static str> {
    FONT_NAMES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_has_ascii() {
        let font = get_font(DEFAULT_FONT);
        assert!(font.glyph('A').is_some());
        assert!(font.glyph(' ').is_some());
        assert!(font.glyph('~').is_some());
    }

    #[test]
    fn tb8_has_expected_dimensions() {
        let font = get_font("tb-8");
        assert_eq!(font.char_width, 5);
        assert_eq!(font.char_height, 8);
    }

    #[test]
    fn measure_known_glyph() {
        let font = get_font("tb-8");
        let g = font.glyph('A').unwrap();
        // tb-8 'A' has DWIDTH 5, BBX 5x8
        assert_eq!(g.advance, 5);
        assert_eq!(g.width, 5);
        assert_eq!(g.height, 8);
    }

    #[test]
    fn fallback_to_default() {
        let font = get_font("nonexistent");
        let default = get_font(DEFAULT_FONT);
        // Both should point to the same font (tb-8)
        assert_eq!(font.char_width, default.char_width);
        assert_eq!(font.char_height, default.char_height);
    }

    #[test]
    fn font_list_contains_all() {
        let list = get_font_list();
        assert_eq!(list.len(), 27);
        assert!(list.contains(&"tb-8"));
        assert!(list.contains(&"tom-thumb"));
        assert!(list.contains(&"terminus-32"));
    }

    #[test]
    fn glyph_a_has_pixels() {
        let font = get_font("tb-8");
        let g = font.glyph('A').unwrap();
        // tb-8 'A' bitmap row 1 (0-indexed) is 0x60 = 0110_0000
        assert!(!g.pixel(0, 1)); // col 0 off
        assert!(g.pixel(1, 1)); // col 1 on
        assert!(g.pixel(2, 1)); // col 2 on
        assert!(!g.pixel(3, 1)); // col 3 off
    }

    #[test]
    fn render_a_nonzero() {
        let font = get_font("tb-8");
        let g = font.glyph('A').unwrap();
        let has_pixel = (0..g.height).any(|r| (0..g.width).any(|c| g.pixel(c, r)));
        assert!(has_pixel, "'A' must have at least one set pixel");
    }

    #[test]
    fn all_fonts_parse() {
        for name in get_font_list() {
            let font = get_font(name);
            assert!(
                font.glyph(' ').is_some() || font.char_width > 0,
                "font {name} must have space glyph or positive char_width"
            );
        }
    }

    #[test]
    fn measure_width_tb8() {
        let font = get_font("tb-8");
        // Space in tb-8 has DWIDTH 3, so "A " = 5 + 3 = 8
        let a_advance = font.glyph('A').map(|g| g.advance).unwrap_or(0);
        let sp_advance = font.glyph(' ').map(|g| g.advance).unwrap_or(0);
        assert_eq!(font.measure_width("A "), (a_advance + sp_advance) as i32);
    }

    #[test]
    fn wide_font_glyphs() {
        let font = get_font("terminus-32");
        let g = font.glyph('A').unwrap();
        // terminus-32 'A': BBX 16x32, DWIDTH 15
        assert_eq!(g.width, 16);
        assert_eq!(g.height, 32);
        assert_eq!(g.advance, 15);
        assert_eq!(g.bytes_per_row(), 2);
    }
}
