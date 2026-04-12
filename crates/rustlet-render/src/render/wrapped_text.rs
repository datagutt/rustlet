use super::text_layout::{base_direction_is_rtl, visual_bidi_string};
use super::{Rect, Widget};
use crate::fonts::{self, MAX_TEXT_WIDTH};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};
use unicode_segmentation::UnicodeSegmentation;

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
    auto_align: bool,
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
            auto_align: true,
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
        self.auto_align = false;
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

    fn effective_align(&self) -> WrapAlign {
        if self.auto_align && base_direction_is_rtl(&self.content) {
            WrapAlign::Right
        } else {
            self.align
        }
    }

    fn wrap_lines(&self, max_width: i32) -> Vec<String> {
        let font = fonts::get_font(&self.font);

        if max_width <= 0 {
            return vec![self.content.clone()];
        }

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
                if self.word_break && font.measure_width(word) > max_width {
                    // Force-break long words
                    if !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = String::new();
                    }
                    for chunk in break_word_to_fit(word, max_width, font) {
                        lines.push(chunk);
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

fn break_word_to_fit(word: &str, max_width: i32, font: &fonts::BitmapFont) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for grapheme in word.graphemes(true) {
        let candidate = format!("{current}{grapheme}");
        if current.is_empty() || font.measure_width(&candidate) <= max_width {
            current.push_str(grapheme);
        } else {
            chunks.push(std::mem::take(&mut current));
            current.push_str(grapheme);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
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

        let wrap_width = if self.width > 0 {
            self.width
        } else {
            bounds.width
        };
        let lines = self.wrap_lines(wrap_width);

        // Compute actual content width
        let mut max_w = 0;
        for line in &lines {
            let w = font.measure_width(line).min(MAX_TEXT_WIDTH);
            if w > max_w {
                max_w = w;
            }
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

        let wrap_width = if self.width > 0 {
            self.width
        } else {
            bounds.width
        };
        let clip_width = if self.width > 0 {
            self.width
        } else {
            bounds.width
        };
        let clip_height = if self.height > 0 {
            self.height
        } else {
            bounds.height
        };
        let clip_right = bounds.x + clip_width;
        let clip_bottom = bounds.y + clip_height;
        let lines = self.wrap_lines(wrap_width);

        let premul = premultiply_color(self.color);
        let dst_w = pixmap.width() as usize;
        let dst_h = pixmap.height() as usize;
        let pixels = pixmap.pixels_mut();

        let mut cursor_y = bounds.y;

        for line in &lines {
            if cursor_y >= clip_bottom {
                break;
            }

            // Compute line width for alignment
            let visual_line = visual_bidi_string(line);
            let line_width = font.measure_width(visual_line.as_ref());

            let x_offset = match self.effective_align() {
                WrapAlign::Left => 0,
                WrapAlign::Center => (wrap_width - line_width) / 2,
                WrapAlign::Right => wrap_width - line_width,
            };

            let mut cursor_x = bounds.x + x_offset;

            for ch in visual_line.chars() {
                if let Some(glyph) = font.glyph(ch) {
                    for row in 0..glyph.height as u8 {
                        for col in 0..glyph.width as u8 {
                            if glyph.pixel(col, row) {
                                let px = cursor_x + glyph.x_offset as i32 + col as i32;
                                let py = cursor_y + row as i32;
                                if px >= bounds.x
                                    && px < clip_right
                                    && (px as usize) < dst_w
                                    && py >= bounds.y
                                    && py < clip_bottom
                                    && (py as usize) < dst_h
                                {
                                    pixels[py as usize * dst_w + px as usize] = premul;
                                }
                            }
                        }
                    }
                    cursor_x += glyph.advance as i32;
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

    fn rendered_bounds(wt: &WrappedText, bounds: Rect) -> Rect {
        wt.paint_bounds(bounds, 0)
    }

    fn first_opaque_x(pixmap: &Pixmap, y: u32) -> Option<u32> {
        (0..pixmap.width())
            .find(|&x| pixmap.pixels()[(y * pixmap.width() + x) as usize].alpha() > 0)
    }

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
        let wt = WrappedText::new("abcdef")
            .with_width(15)
            .with_word_break(true);
        let lines = wt.wrap_lines(15);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "abc");
        assert_eq!(lines[1], "def");
    }

    #[test]
    fn wrap_word_break_uses_actual_glyph_width() {
        let wt = WrappedText::new("iiii")
            .with_width(16)
            .with_word_break(true);
        let lines = wt.wrap_lines(16);
        assert_eq!(lines, vec!["iiii"]);
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

    #[test]
    fn width_and_height_override_parent_bounds() {
        let wt = WrappedText::new("AB CD.").with_width(7).with_height(12);
        assert_eq!(
            rendered_bounds(&wt, Rect::new(0, 0, 40, 40)),
            Rect::new(0, 0, 7, 12)
        );
    }

    #[test]
    fn width_override_applies_without_explicit_height() {
        let wt = WrappedText::new("AB CD.").with_width(3);
        assert_eq!(
            rendered_bounds(&wt, Rect::new(0, 0, 9, 5)),
            Rect::new(0, 0, 3, 5)
        );
    }

    #[test]
    fn alignment_shifts_first_line_horizontally() {
        let content = "AB CD.";
        let width = 21;

        let mut left_pm = Pixmap::new(width as u32, 16).unwrap();
        WrappedText::new(content).with_width(width).paint(
            &mut left_pm,
            Rect::new(0, 0, width, 16),
            0,
        );

        let mut center_pm = Pixmap::new(width as u32, 16).unwrap();
        WrappedText::new(content)
            .with_width(width)
            .with_align(WrapAlign::Center)
            .paint(&mut center_pm, Rect::new(0, 0, width, 16), 0);

        let mut right_pm = Pixmap::new(width as u32, 16).unwrap();
        WrappedText::new(content)
            .with_width(width)
            .with_align(WrapAlign::Right)
            .paint(&mut right_pm, Rect::new(0, 0, width, 16), 0);

        let sample_row = 1;
        let left_x = first_opaque_x(&left_pm, sample_row).unwrap();
        let center_x = first_opaque_x(&center_pm, sample_row).unwrap();
        let right_x = first_opaque_x(&right_pm, sample_row).unwrap();

        assert!(left_x < center_x, "center align should shift right");
        assert!(center_x < right_x, "right align should shift farther right");
    }

    #[test]
    fn rtl_content_defaults_to_right_alignment() {
        let content = "שלום abc";
        let width = 40;

        let mut auto_pm = Pixmap::new(width as u32, 16).unwrap();
        WrappedText::new(content).with_width(width).paint(
            &mut auto_pm,
            Rect::new(0, 0, width, 16),
            0,
        );

        let mut left_pm = Pixmap::new(width as u32, 16).unwrap();
        WrappedText::new(content)
            .with_width(width)
            .with_align(WrapAlign::Left)
            .paint(&mut left_pm, Rect::new(0, 0, width, 16), 0);

        let sample_row = 1;
        let auto_x = first_opaque_x(&auto_pm, sample_row).unwrap();
        let left_x = first_opaque_x(&left_pm, sample_row).unwrap();

        assert!(
            auto_x > left_x,
            "rtl auto-align should shift visible text right"
        );
    }
}
