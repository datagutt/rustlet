use std::collections::HashMap;
use std::sync::OnceLock;

use image::RgbaImage;
#[cfg(test)]
use tiny_skia::Pixmap;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphBounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl GlyphBounds {
    fn width(self) -> u32 {
        self.max_x - self.min_x
    }

    fn height(self) -> u32 {
        self.max_y - self.min_y
    }
}

struct EmojiAtlas {
    sheet: RgbaImage,
    index: HashMap<String, GlyphBounds>,
    fallback: GlyphBounds,
    max_height: i32,
}

static ATLAS: OnceLock<EmojiAtlas> = OnceLock::new();

fn atlas() -> &'static EmojiAtlas {
    ATLAS.get_or_init(build_atlas)
}

fn build_atlas() -> EmojiAtlas {
    let sheet = image::load_from_memory_with_format(
        include_bytes!("../../assets/emoji/sprites.png"),
        image::ImageFormat::Png,
    )
    .expect("embedded emoji sprite sheet must decode")
    .to_rgba8();

    let mut index = HashMap::new();
    let mut fallback = None;
    let mut max_height = 0;

    for line in include_str!("../../assets/emoji/index.tsv").lines() {
        let mut parts = line.split('\t');
        let key = parts.next().expect("emoji key");
        let min_x = parts
            .next()
            .expect("min_x")
            .parse::<u32>()
            .expect("min_x must be numeric");
        let min_y = parts
            .next()
            .expect("min_y")
            .parse::<u32>()
            .expect("min_y must be numeric");
        let max_x = parts
            .next()
            .expect("max_x")
            .parse::<u32>()
            .expect("max_x must be numeric");
        let max_y = parts
            .next()
            .expect("max_y")
            .parse::<u32>()
            .expect("max_y must be numeric");
        let bounds = GlyphBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        };
        max_height = max_height.max(bounds.height() as i32);
        if key == "__FALLBACK__" {
            fallback = Some(bounds);
        } else {
            index.insert(key.to_string(), bounds);
        }
    }

    EmojiAtlas {
        sheet,
        index,
        fallback: fallback.expect("emoji fallback glyph must exist"),
        max_height,
    }
}

fn crop_rgba(sheet: &RgbaImage, bounds: GlyphBounds) -> RgbaImage {
    image::imageops::crop_imm(
        sheet,
        bounds.min_x,
        bounds.min_y,
        bounds.width(),
        bounds.height(),
    )
    .to_image()
}

#[cfg(test)]
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

pub(crate) fn max_height() -> i32 {
    atlas().max_height
}

pub(crate) fn contains_exact(seq: &str) -> bool {
    atlas().index.contains_key(seq)
}

pub(crate) fn exact_size(seq: &str) -> Option<(i32, i32)> {
    atlas()
        .index
        .get(seq)
        .map(|bounds| (bounds.width() as i32, bounds.height() as i32))
}

#[cfg(test)]
pub(crate) fn exact_pixmap(seq: &str) -> Option<Pixmap> {
    let atlas = atlas();
    atlas
        .index
        .get(seq)
        .map(|bounds| rgba_to_pixmap(&crop_rgba(&atlas.sheet, *bounds)))
}

pub(crate) fn widget_rgba(seq: &str) -> (RgbaImage, bool) {
    let atlas = atlas();
    if let Some(bounds) = atlas.index.get(seq) {
        return (crop_rgba(&atlas.sheet, *bounds), true);
    }

    let variation = format!("{seq}\u{fe0f}");
    if let Some(bounds) = atlas.index.get(&variation) {
        return (crop_rgba(&atlas.sheet, *bounds), true);
    }

    (crop_rgba(&atlas.sheet, atlas.fallback), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_contains_known_emoji() {
        assert!(contains_exact("😀"));
        assert!(contains_exact("🇺🇸"));
        assert!(!contains_exact("not-a-real-emoji"));
    }

    #[test]
    fn atlas_exact_pixmap_matches_size() {
        let size = exact_size("😀").unwrap();
        let pm = exact_pixmap("😀").unwrap();
        assert_eq!(size, (pm.width() as i32, pm.height() as i32));
    }
}
