use super::{Rect, Widget};
use tiny_skia::Pixmap;

pub struct Emoji {
    pixmap: Option<Pixmap>,
    width: i32,
    height: i32,
}

impl Emoji {
    /// Create an emoji widget from a Unicode emoji string.
    /// Currently renders a colored placeholder square derived from the codepoint.
    /// Use `from_svg` for actual SVG rendering.
    pub fn new(emoji_str: &str, width: i32, height: i32) -> Self {
        let hash = emoji_str
            .chars()
            .fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32));

        let r = ((hash >> 16) & 0xFF) as u8;
        let g = ((hash >> 8) & 0xFF) as u8;
        let b = (hash & 0xFF) as u8;
        let a = 255u8;

        let pixmap = create_solid_pixmap(width as u32, height as u32, r, g, b, a);

        Self {
            pixmap,
            width,
            height,
        }
    }

    /// Create an emoji widget by rendering SVG data via resvg.
    /// Falls back to a magenta placeholder if rendering fails.
    pub fn from_svg(svg_data: &str, width: i32, height: i32) -> Self {
        let pixmap = render_svg(svg_data, width as u32, height as u32)
            .or_else(|| create_solid_pixmap(width as u32, height as u32, 255, 0, 255, 255));

        Self {
            pixmap,
            width,
            height,
        }
    }

    /// Convert an emoji string to a Twemoji-style codepoint filename
    /// (e.g. "😀" -> "1f600", "👨‍👩‍👧" -> "1f468-200d-1f469-200d-1f467").
    pub fn emoji_to_codepoint(emoji: &str) -> String {
        emoji
            .chars()
            .filter(|&c| c != '\u{FE0F}')
            .map(|c| format!("{:x}", c as u32))
            .collect::<Vec<_>>()
            .join("-")
    }
}

fn render_svg(svg_data: &str, width: u32, height: u32) -> Option<Pixmap> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_data, &opt).ok()?;

    let mut pixmap = Pixmap::new(width, height)?;
    let size = tree.size();
    let scale_x = width as f32 / size.width();
    let scale_y = height as f32 / size.height();
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);

    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

fn create_solid_pixmap(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let pa = a as f32 / 255.0;
    let color = tiny_skia::PremultipliedColorU8::from_rgba(
        (r as f32 * pa) as u8,
        (g as f32 * pa) as u8,
        (b as f32 * pa) as u8,
        a,
    )?;

    let pixels = pixmap.pixels_mut();
    for px in pixels.iter_mut() {
        *px = color;
    }
    Some(pixmap)
}

impl Widget for Emoji {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        let src = match &self.pixmap {
            Some(pm) => pm,
            None => return,
        };

        let src_w = src.width() as i32;
        let src_h = src.height() as i32;
        let dst_w = pixmap.width() as i32;
        let dst_h = pixmap.height() as i32;

        let src_pixels = src.pixels();
        let dst_pixels = pixmap.pixels_mut();

        for sy in 0..src_h {
            let dy = bounds.y + sy;
            if dy < 0 || dy >= dst_h {
                continue;
            }
            for sx in 0..src_w {
                let dx = bounds.x + sx;
                if dx < 0 || dx >= dst_w {
                    continue;
                }

                let src_px = src_pixels[(sy * src_w + sx) as usize];
                if src_px.alpha() == 0 {
                    continue;
                }

                let dst_idx = (dy * dst_w + dx) as usize;
                if src_px.alpha() == 255 {
                    dst_pixels[dst_idx] = src_px;
                } else {
                    let sa = src_px.alpha() as u16;
                    let inv_sa = 255 - sa;
                    let dst_px = dst_pixels[dst_idx];

                    let r = src_px.red() as u16 + (dst_px.red() as u16 * inv_sa / 255);
                    let g = src_px.green() as u16 + (dst_px.green() as u16 * inv_sa / 255);
                    let b = src_px.blue() as u16 + (dst_px.blue() as u16 * inv_sa / 255);
                    let a = sa + (dst_px.alpha() as u16 * inv_sa / 255);

                    dst_pixels[dst_idx] = tiny_skia::PremultipliedColorU8::from_rgba(
                        r.min(255) as u8,
                        g.min(255) as u8,
                        b.min(255) as u8,
                        a.min(255) as u8,
                    )
                    .unwrap_or(dst_px);
                }
            }
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }

    fn size(&self) -> Option<(i32, i32)> {
        Some((self.width, self.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CIRCLE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="40" fill="red"/>
    </svg>"#;

    #[test]
    fn from_svg_renders_non_zero_pixels() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 16, 16);
        let pm = widget.pixmap.as_ref().expect("pixmap should exist");
        let non_zero = pm.pixels().iter().any(|p| p.alpha() > 0);
        assert!(non_zero, "rendered SVG should have non-transparent pixels");
    }

    #[test]
    fn from_svg_correct_dimensions() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 12, 8);
        assert_eq!(widget.size(), Some((12, 8)));
        let pm = widget.pixmap.as_ref().unwrap();
        assert_eq!(pm.width(), 12);
        assert_eq!(pm.height(), 8);
    }

    #[test]
    fn invalid_svg_produces_placeholder() {
        let widget = Emoji::from_svg("not valid svg at all", 10, 10);
        let pm = widget.pixmap.as_ref().expect("placeholder should exist");
        assert_eq!(pm.width(), 10);
        assert_eq!(pm.height(), 10);
        // placeholder is solid magenta, all pixels should be opaque
        assert!(pm.pixels().iter().all(|p| p.alpha() == 255));
    }

    #[test]
    fn frame_count_is_one() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 16, 16);
        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn new_creates_colored_placeholder() {
        let widget = Emoji::new("😀", 8, 8);
        assert_eq!(widget.size(), Some((8, 8)));
        let pm = widget.pixmap.as_ref().expect("placeholder should exist");
        assert!(pm.pixels().iter().all(|p| p.alpha() == 255));
    }

    #[test]
    fn emoji_to_codepoint_simple() {
        assert_eq!(Emoji::emoji_to_codepoint("😀"), "1f600");
    }

    #[test]
    fn emoji_to_codepoint_zwj_sequence() {
        // Family emoji with ZWJ
        assert_eq!(
            Emoji::emoji_to_codepoint("👨\u{200D}👩\u{200D}👧"),
            "1f468-200d-1f469-200d-1f467"
        );
    }

    #[test]
    fn emoji_to_codepoint_strips_variation_selector() {
        // U+FE0F (variation selector 16) should be stripped
        assert_eq!(Emoji::emoji_to_codepoint("❤\u{FE0F}"), "2764");
    }

    #[test]
    fn paint_blits_to_pixmap() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 8, 8);
        let mut pixmap = Pixmap::new(16, 16).unwrap();
        widget.paint(&mut pixmap, Rect::new(4, 4, 8, 8), 0);

        // center area should have some non-zero pixels from the circle
        let has_content = (4..12).any(|y| {
            (4..12).any(|x| {
                let idx = (y * 16 + x) as usize;
                pixmap.pixels()[idx].alpha() > 0
            })
        });
        assert!(has_content, "painted region should contain non-zero pixels");

        // top-left corner should be untouched
        assert_eq!(pixmap.pixels()[0].alpha(), 0);
    }
}
