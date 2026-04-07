use super::{Rect, Widget};
use crate::fonts::{self, MAX_TEXT_WIDTH};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

pub struct Text {
    pub content: String,
    pub font: String,
    pub height: i32,
    pub offset: i32,
    pub color: Color,
    rendered: Option<Pixmap>,
    rendered_width: i32,
    rendered_height: i32,
}

impl Text {
    pub fn new(content: &str) -> Self {
        let mut t = Self {
            content: content.to_string(),
            font: fonts::DEFAULT_FONT.to_string(),
            height: 0,
            offset: 0,
            color: Color::from_rgba8(255, 255, 255, 255),
            rendered: None,
            rendered_width: 0,
            rendered_height: 0,
        };
        t.render_text();
        t
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self.render_text();
        self
    }

    pub fn with_font(mut self, font: &str) -> Self {
        self.font = font.to_string();
        self.render_text();
        self
    }

    pub fn with_height(mut self, height: i32) -> Self {
        self.height = height;
        self
    }

    pub fn with_offset(mut self, offset: i32) -> Self {
        self.offset = offset;
        self
    }

    fn render_text(&mut self) {
        if self.content.is_empty() {
            self.rendered = None;
            self.rendered_width = 0;
            self.rendered_height = 0;
            return;
        }

        let font = fonts::get_font(&self.font);
        let text_w = font.measure_width(&self.content).min(MAX_TEXT_WIDTH);
        let text_h = font.measure_height();

        if text_w <= 0 || text_h <= 0 {
            self.rendered = None;
            self.rendered_width = 0;
            self.rendered_height = 0;
            return;
        }

        let mut pixmap = Pixmap::new(text_w as u32, text_h as u32)
            .expect("text pixmap dimensions must be positive");

        let premul = premultiply_color(self.color);

        let dst_w = pixmap.width() as usize;
        let dst_h = pixmap.height() as usize;
        let pixels = pixmap.pixels_mut();

        let mut cursor_x: i32 = 0;
        for ch in self.content.chars() {
            if let Some(glyph) = font.glyph(ch) {
                for row in 0..glyph.height as u8 {
                    for col in 0..glyph.width as u8 {
                        if glyph.pixel(col, row) {
                            let px = cursor_x + glyph.x_offset as i32 + col as i32;
                            let py = row as i32;
                            if px >= 0 && (px as usize) < dst_w && py >= 0 && (py as usize) < dst_h {
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

        self.rendered_width = text_w;
        self.rendered_height = text_h;
        self.rendered = Some(pixmap);
    }
}

fn premultiply_color(c: Color) -> PremultipliedColorU8 {
    let a = (c.alpha() * 255.0) as u8;
    let r = (c.red() * c.alpha() * 255.0) as u8;
    let g = (c.green() * c.alpha() * 255.0) as u8;
    let b = (c.blue() * c.alpha() * 255.0) as u8;
    PremultipliedColorU8::from_rgba(r, g, b, a).unwrap()
}

impl Widget for Text {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        let h = if self.height > 0 { self.height } else { self.rendered_height };
        Rect::new(0, 0, self.rendered_width, h)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        let src = match &self.rendered {
            Some(p) => p,
            None => return,
        };

        let src_pixels = src.pixels();
        let src_w = src.width() as i32;
        let src_h = src.height() as i32;

        let dst_w = pixmap.width() as i32;
        let dst_h = pixmap.height() as i32;
        let dst_pixels = pixmap.pixels_mut();

        let offset_y = self.offset;

        for sy in 0..src_h {
            let dy = bounds.y + sy + offset_y;
            if dy < 0 || dy >= dst_h { continue; }
            for sx in 0..src_w {
                let dx = bounds.x + sx;
                if dx < 0 || dx >= dst_w { continue; }

                let src_pixel = src_pixels[(sy * src_w + sx) as usize];
                if src_pixel.alpha() == 0 { continue; }

                dst_pixels[(dy * dst_w + dx) as usize] = src_pixel;
            }
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }

    fn size(&self) -> Option<(i32, i32)> {
        Some((self.rendered_width, self.rendered_height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_hello_dimensions() {
        let t = Text::new("Hello");
        // tb-8 BDF: H=5 + e=5 + l=4 + l=4 + o=5 = 23
        assert_eq!(t.rendered_width, 23);
        assert_eq!(t.rendered_height, 8);
    }

    #[test]
    fn text_empty() {
        let t = Text::new("");
        assert!(t.rendered.is_none());
        assert_eq!(t.rendered_width, 0);
    }

    #[test]
    fn text_paints_pixels() {
        let t = Text::new("A");
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        t.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        // 'A' should have some non-zero pixels
        let has_pixels = pixmap.pixels().iter().any(|p| p.alpha() > 0);
        assert!(has_pixels);
    }

    #[test]
    fn text_with_color() {
        let t = Text::new("A").with_color(Color::from_rgba8(255, 0, 0, 255));
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        t.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        let red_pixels = pixmap.pixels().iter().filter(|p| p.red() == 255 && p.alpha() > 0).count();
        assert!(red_pixels > 0);
    }

    #[test]
    fn text_size() {
        let t = Text::new("Hi");
        // tb-8 BDF: H=5 + i=4 = 9
        assert_eq!(t.size(), Some((9, 8)));
    }

    #[test]
    fn text_paint_bounds() {
        let t = Text::new("Test");
        let pb = t.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        // tb-8 BDF: T=4 + e=5 + s=4 + t=5 = 18
        assert_eq!(pb.width, 18);
        assert_eq!(pb.height, 8);
    }
}
