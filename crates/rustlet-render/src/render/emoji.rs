use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;

use super::emoji_atlas;
use super::{Rect, Widget};
use anyhow::Context;
use image::RgbaImage;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

static TWEMOJI_DIR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub struct Emoji {
    pixmap: Option<Pixmap>,
    width: i32,
    height: i32,
}

impl Emoji {
    /// Set the directory where Twemoji SVG files are stored.
    /// Files should be named by codepoint (e.g. "1f600.svg").
    pub fn set_twemoji_dir(dir: &str) {
        let lock = TWEMOJI_DIR.get_or_init(|| Mutex::new(None));
        *lock.lock().unwrap() = Some(dir.to_string());
    }

    /// Get the configured Twemoji directory, if any.
    pub fn twemoji_dir() -> Option<String> {
        TWEMOJI_DIR
            .get()
            .and_then(|m| m.lock().ok())
            .and_then(|guard| guard.clone())
    }

    /// Create an emoji widget from a Unicode emoji string.
    /// Uses the built-in Pixlet-style emoji atlas by default and optional SVG overrides
    /// from `twemoji_dir` when configured.
    pub fn new(emoji_str: &str, width: i32, height: i32) -> anyhow::Result<Self> {
        if height < 0 {
            anyhow::bail!("emoji height must not be negative, got {height}");
        }

        if width < 0 {
            anyhow::bail!("emoji width must not be negative, got {width}");
        }

        if emoji_str.is_empty() {
            anyhow::bail!("emoji string cannot be empty");
        }

        let pixmap = if let Some(dir) = Self::twemoji_dir() {
            let codepoint = Self::emoji_to_codepoint(emoji_str);
            let svg_path = Path::new(&dir).join(format!("{codepoint}.svg"));
            if let Ok(svg_data) = std::fs::read_to_string(&svg_path) {
                render_svg_scaled(&svg_data, width, height)?
            } else {
                let (src, _) = emoji_atlas::widget_rgba(emoji_str);
                scale_emoji_image(&src, width, height)?
            }
        } else {
            let (src, _) = emoji_atlas::widget_rgba(emoji_str);
            scale_emoji_image(&src, width, height)?
        };
        let widget = Self {
            width: pixmap.width() as i32,
            height: pixmap.height() as i32,
            pixmap: Some(pixmap),
        };
        Ok(widget)
    }

    /// Create an emoji widget by rendering SVG data via resvg.
    pub fn from_svg(svg_data: &str, width: i32, height: i32) -> anyhow::Result<Self> {
        let pixmap = render_svg_scaled(svg_data, width, height)?;
        Ok(Self {
            width: pixmap.width() as i32,
            height: pixmap.height() as i32,
            pixmap: Some(pixmap),
        })
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

fn render_svg_scaled(svg_data: &str, width: i32, height: i32) -> anyhow::Result<Pixmap> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_data, &opt).context("failed to parse SVG")?;
    let size = tree.size();

    let intrinsic_w = size.width().round().max(1.0) as i32;
    let intrinsic_h = size.height().round().max(1.0) as i32;
    let (target_w, target_h) = resolve_target_dimensions(intrinsic_w, intrinsic_h, width, height)?;

    let mut pixmap = Pixmap::new(target_w as u32, target_h as u32)
        .context("failed to allocate emoji SVG pixmap")?;
    let scale_x = target_w as f32 / size.width();
    let scale_y = target_h as f32 / size.height();
    let transform = tiny_skia::Transform::from_scale(scale_x, scale_y);

    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

fn resolve_target_dimensions(
    source_w: i32,
    source_h: i32,
    width: i32,
    height: i32,
) -> anyhow::Result<(i32, i32)> {
    if source_w <= 0 || source_h <= 0 {
        anyhow::bail!("emoji source dimensions must be positive");
    }

    let (mut target_w, mut target_h) = (width, height);
    if target_w == 0 && target_h == 0 {
        target_w = source_w;
        target_h = source_h;
    } else if target_w == 0 {
        target_w = ((target_h as f64) * (source_w as f64) / (source_h as f64)).round() as i32;
    } else if target_h == 0 {
        target_h = ((target_w as f64) * (source_h as f64) / (source_w as f64)).round() as i32;
    }

    Ok((target_w.max(1), target_h.max(1)))
}

fn scale_emoji_image(src: &RgbaImage, width: i32, height: i32) -> anyhow::Result<Pixmap> {
    let (source_w, source_h) = (src.width() as i32, src.height() as i32);
    let (target_w, target_h) = resolve_target_dimensions(source_w, source_h, width, height)?;

    if target_w == source_w && target_h == source_h {
        return Ok(rgba_to_pixmap(src));
    } else if target_w % source_w == 0 && target_h % source_h == 0 {
        let scaled = image::imageops::resize(
            src,
            target_w as u32,
            target_h as u32,
            image::imageops::FilterType::Nearest,
        );
        return Ok(rgba_to_pixmap(&scaled));
    } else {
        let sx = target_w as f64 / source_w as f64;
        let sy = target_h as f64 / source_h as f64;
        let up_factor = sx.max(sy).ceil().max(2.0).min(10.0) as u32;
        let upscaled = image::imageops::resize(
            src,
            src.width() * up_factor,
            src.height() * up_factor,
            image::imageops::FilterType::Nearest,
        );
        let upscaled_pixmap = rgba_to_pixmap(&upscaled);
        let mut pixmap = Pixmap::new(target_w as u32, target_h as u32)
            .context("failed to allocate scaled emoji pixmap")?;
        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: FilterQuality::Bicubic,
        };
        let transform = Transform::from_scale(
            target_w as f32 / upscaled.width() as f32,
            target_h as f32 / upscaled.height() as f32,
        );
        pixmap.draw_pixmap(0, 0, upscaled_pixmap.as_ref(), &paint, transform, None);
        return Ok(pixmap);
    }
}

fn rgba_to_pixmap(img: &RgbaImage) -> Pixmap {
    let (w, h) = img.dimensions();
    let mut pixmap = Pixmap::new(w, h).expect("emoji pixmap dimensions must be positive");
    let src = img.as_raw();
    let dst = pixmap.data_mut();
    for i in 0..(w * h) as usize {
        let off = i * 4;
        let r = src[off];
        let g = src[off + 1];
        let b = src[off + 2];
        let a = src[off + 3];
        let pa = a as f32 / 255.0;
        dst[off] = (r as f32 * pa) as u8;
        dst[off + 1] = (g as f32 * pa) as u8;
        dst[off + 2] = (b as f32 * pa) as u8;
        dst[off + 3] = a;
    }
    pixmap
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
        pixmap.draw_pixmap(
            bounds.x,
            bounds.y,
            src.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
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
        let widget = Emoji::from_svg(CIRCLE_SVG, 16, 16).unwrap();
        let pm = widget.pixmap.as_ref().expect("pixmap should exist");
        let non_zero = pm.pixels().iter().any(|p| p.alpha() > 0);
        assert!(non_zero, "rendered SVG should have non-transparent pixels");
    }

    #[test]
    fn from_svg_correct_dimensions() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 12, 8).unwrap();
        assert_eq!(widget.size(), Some((12, 8)));
        let pm = widget.pixmap.as_ref().unwrap();
        assert_eq!(pm.width(), 12);
        assert_eq!(pm.height(), 8);
    }

    #[test]
    fn invalid_svg_errors() {
        assert!(Emoji::from_svg("not valid svg at all", 10, 10).is_err());
    }

    #[test]
    fn frame_count_is_one() {
        let widget = Emoji::from_svg(CIRCLE_SVG, 16, 16).unwrap();
        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn new_uses_atlas_default_size() {
        let widget = Emoji::new("😀", 0, 0).unwrap();
        let (w, h) = widget.size().unwrap();
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn new_scales_height_with_aspect_ratio() {
        let widget = Emoji::new("😀", 0, 16).unwrap();
        let (w, h) = widget.size().unwrap();
        assert_eq!(h, 16);
        assert!(w > 0);
    }

    #[test]
    fn new_errors_on_empty_or_negative_height() {
        assert!(Emoji::new("", 0, 16).is_err());
        assert!(Emoji::new("😀", 0, -1).is_err());
    }

    #[test]
    fn new_uses_fallback_for_unknown_sequence() {
        let widget = Emoji::new("not-a-real-emoji", 0, 16).unwrap();
        let (w, h) = widget.size().unwrap();
        assert_eq!(h, 16);
        assert!(w > 0);
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
        let widget = Emoji::from_svg(CIRCLE_SVG, 8, 8).unwrap();
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
