use super::{Rect, Widget};
use crate::fonts::{self, MAX_TEXT_WIDTH};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl WrapAlign {
    pub fn from_str(s: &str) -> Self {
        match s {
            "center" => Self::Center,
            "right" => Self::Right,
            _ => Self::Left,
        }
    }
}

pub struct WrappedText {
    pub content: String,
    pub font: String,
    pub width: i32,
    pub height: i32,
    pub line_spacing: i32,
    pub color: Color,
    pub align: WrapAlign,
    pub word_break: bool,
}

impl WrappedText {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            font: fonts::DEFAULT_FONT.to_string(),
            width: 0,
            height: 0,
            line_spacing: 0,
            color: Color::from_rgba8(255, 255, 255, 255),
            align: WrapAlign::Left,
            word_break: false,
        }
    }

    pub fn with_width(mut self, width: i32) -> Self {
        self.width = width;
        self
    }

    pub fn with_height(mut self, height: i32) -> Self {
        self.height = height;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn with_font(mut self, font: &str) -> Self {
        self.font = font.to_string();
        self
    }

    pub fn with_align(mut self, align: WrapAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_line_spacing(mut self, spacing: i32) -> Self {
        self.line_spacing = spacing;
        self
    }

    pub fn with_word_break(mut self, word_break: bool) -> Self {
        self.word_break = word_break;
        self
    }

    fn wrap_lines(&self, max_width: i32) -> Vec<String> {
        let font = fonts::get_font(&self.font);
        let char_w = font.char_width as i32;

        if max_width <= 0 || char_w <= 0 {
            return vec![self.content.clone()];
        }

        let max_chars = (max_width / char_w).max(1) as usize;

        let mut lines = Vec::new();

        // Split on explicit newlines first
        for paragraph in self.content.split('\n') {
            let words: Vec<&str> = paragraph.split_whitespace().collect();

            if words.is_empty() {
                lines.push(String::new());
                continue;
            }

            let mut current_line = String::new();

            for word in &words {
                if self.word_break && word.chars().count() > max_chars {
                    // Force-break long words
                    if !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = String::new();
                    }
                    let chars: Vec<char> = word.chars().collect();
                    for chunk in chars.chunks(max_chars) {
                        lines.push(chunk.iter().collect());
                    }
                    continue;
                }

                if current_line.is_empty() {
                    current_line = word.to_string();
                } else {
                    let test = format!("{current_line} {word}");
                    let test_width = font.measure_width(&test);
                    if test_width <= max_width {
                        current_line = test;
                    } else {
                        lines.push(current_line);
                        current_line = word.to_string();
                    }
                }
            }

            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }
}

fn premultiply_color(c: Color) -> PremultipliedColorU8 {
    let a = (c.alpha() * 255.0) as u8;
    let r = (c.red() * c.alpha() * 255.0) as u8;
    let g = (c.green() * c.alpha() * 255.0) as u8;
    let b = (c.blue() * c.alpha() * 255.0) as u8;
    PremultipliedColorU8::from_rgba(r, g, b, a).unwrap()
}

impl Widget for WrappedText {
    fn paint_bounds(&self, bounds: Rect, _frame_idx: i32) -> Rect {
        let font = fonts::get_font(&self.font);
        let line_h = font.measure_height();
        let spacing = self.line_spacing.max(0);

        let wrap_width = if self.width > 0 { self.width } else { bounds.width };
        let lines = self.wrap_lines(wrap_width);

        // Compute actual content width
        let mut max_w = 0;
        for line in &lines {
            let w = font.measure_width(line).min(MAX_TEXT_WIDTH);
            if w > max_w { max_w = w; }
        }

        let total_h = if lines.is_empty() {
            0
        } else {
            lines.len() as i32 * line_h + (lines.len() as i32 - 1) * spacing
        };

        let width = if self.width > 0 {
            self.width
        } else if max_w < bounds.width {
            max_w
        } else {
            bounds.width
        };

        let height = if self.height > 0 {
            self.height
        } else if total_h < bounds.height {
            total_h
        } else {
            bounds.height
        };

        Rect::new(0, 0, width, height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        let font = fonts::get_font(&self.font);
        let line_h = font.measure_height();
        let spacing = self.line_spacing.max(0);

        let wrap_width = if self.width > 0 { self.width } else { bounds.width };
        let lines = self.wrap_lines(wrap_width);

        let premul = premultiply_color(self.color);
        let dst_w = pixmap.width() as usize;
        let dst_h = pixmap.height() as usize;
        let pixels = pixmap.pixels_mut();

        let mut cursor_y = bounds.y;

        for line in &lines {
            // Compute line width for alignment
            let line_width = font.measure_width(line);

            let x_offset = match self.align {
                WrapAlign::Left => 0,
                WrapAlign::Center => (wrap_width - line_width) / 2,
                WrapAlign::Right => wrap_width - line_width,
            };

            let mut cursor_x = bounds.x + x_offset;

            for ch in line.chars() {
                if let Some(glyph) = font.glyph(ch) {
                    for row in 0..glyph.height as i32 {
                        let byte = glyph.bitmap[row as usize];
                        for col in 0..glyph.width as i32 {
                            if byte & (0x80 >> col) != 0 {
                                let px = cursor_x + col;
                                let py = cursor_y + row;
                                if px >= 0 && (px as usize) < dst_w
                                    && py >= 0 && (py as usize) < dst_h
                                {
                                    pixels[py as usize * dst_w + px as usize] = premul;
                                }
                            }
                        }
                    }
                    cursor_x += glyph.width as i32;
                } else {
                    cursor_x += font.char_width as i32;
                }

                if cursor_x >= MAX_TEXT_WIDTH {
                    break;
                }
            }

            cursor_y += line_h + spacing;
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_align_from_str() {
        assert_eq!(WrapAlign::from_str("left"), WrapAlign::Left);
        assert_eq!(WrapAlign::from_str("center"), WrapAlign::Center);
        assert_eq!(WrapAlign::from_str("right"), WrapAlign::Right);
        assert_eq!(WrapAlign::from_str("unknown"), WrapAlign::Left);
    }

    #[test]
    fn wrap_single_word() {
        let wt = WrappedText::new("hello").with_width(50);
        let lines = wt.wrap_lines(50);
        assert_eq!(lines, vec!["hello"]);
    }

    #[test]
    fn wrap_multiple_words() {
        // 5px per char, width=25 fits 5 chars
        let wt = WrappedText::new("hello world").with_width(25);
        let lines = wt.wrap_lines(25);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "world");
    }

    #[test]
    fn wrap_word_break() {
        // 5px per char, width=15 fits 3 chars
        let wt = WrappedText::new("abcdef").with_width(15).with_word_break(true);
        let lines = wt.wrap_lines(15);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "abc");
        assert_eq!(lines[1], "def");
    }

    #[test]
    fn paint_bounds_correct() {
        let wt = WrappedText::new("hi").with_width(50);
        let pb = wt.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 50);
        assert_eq!(pb.height, 8); // single line, 8px tall
    }

    #[test]
    fn paint_bounds_multiline() {
        let wt = WrappedText::new("hello world").with_width(25);
        let pb = wt.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.height, 16); // 2 lines * 8px
    }

    #[test]
    fn paint_renders_pixels() {
        let wt = WrappedText::new("A").with_width(50);
        let mut pixmap = Pixmap::new(50, 10).unwrap();
        wt.paint(&mut pixmap, Rect::new(0, 0, 50, 10), 0);
        let has_pixels = pixmap.pixels().iter().any(|p| p.alpha() > 0);
        assert!(has_pixels, "should render visible pixels");
    }

    #[test]
    fn frame_count_is_one() {
        let wt = WrappedText::new("test");
        assert_eq!(wt.frame_count(Rect::default()), 1);
    }

    #[test]
    fn empty_content() {
        let wt = WrappedText::new("").with_width(50);
        let mut pixmap = Pixmap::new(50, 10).unwrap();
        wt.paint(&mut pixmap, Rect::new(0, 0, 50, 10), 0);
    }

    #[test]
    fn explicit_newlines_preserved() {
        let wt = WrappedText::new("a\nb").with_width(50);
        let lines = wt.wrap_lines(50);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "a");
        assert_eq!(lines[1], "b");
    }
}
