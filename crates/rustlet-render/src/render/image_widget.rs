use super::{mod_int, Rect, Widget};
use anyhow::{Context, Result};
use tiny_skia::Pixmap;

pub struct ImageWidget {
    frames: Vec<Pixmap>,
    pub hold_frames: i32,
    pub delay_ms: i32,
}

impl ImageWidget {
    pub fn from_bytes(data: &[u8], width: Option<i32>, height: Option<i32>) -> Result<Self> {
        if let Ok(svg_data) = std::str::from_utf8(data) {
            if looks_like_svg(svg_data) {
                let pixmap = render_svg(svg_data, width, height)?;
                return Ok(Self {
                    frames: vec![pixmap],
                    hold_frames: 1,
                    delay_ms: 0,
                });
            }
        }

        let cursor = std::io::Cursor::new(data);
        let reader = image::ImageReader::new(cursor)
            .with_guessed_format()
            .context("failed to guess image format")?;

        let format = reader.format();
        let is_gif = matches!(format, Some(image::ImageFormat::Gif));

        let (raw_frames, delay_ms) = if is_gif {
            decode_gif_frames(data)?
        } else {
            let img = reader.decode().context("failed to decode image")?;
            (vec![img], 0)
        };

        let mut frames = Vec::with_capacity(raw_frames.len());
        for img in &raw_frames {
            let resized = maybe_resize(img, width, height);
            let pixmap = dynamic_image_to_pixmap(&resized)?;
            frames.push(pixmap);
        }

        if frames.is_empty() {
            anyhow::bail!("image produced no frames");
        }

        Ok(Self {
            frames,
            hold_frames: 1,
            delay_ms,
        })
    }

    fn frame_pixmap(&self, frame_idx: i32) -> &Pixmap {
        let hold_frames = self.hold_frames.max(1);
        let i = mod_int(frame_idx / hold_frames, self.frames.len() as i32);
        &self.frames[i as usize]
    }
}

fn looks_like_svg(data: &str) -> bool {
    let trimmed = data.trim_start();
    trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && trimmed.contains("<svg"))
}

fn render_svg(svg_data: &str, width: Option<i32>, height: Option<i32>) -> Result<Pixmap> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_data, &opt).context("failed to parse SVG")?;
    let size = tree.size();

    let intrinsic_w = size.width().round().max(1.0) as i32;
    let intrinsic_h = size.height().round().max(1.0) as i32;

    let (target_w, target_h) = match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as u32, h as u32),
        (Some(w), _) if w > 0 => {
            let h = ((w as f32) * size.height() / size.width()).round().max(1.0) as u32;
            (w as u32, h)
        }
        (_, Some(h)) if h > 0 => {
            let w = ((h as f32) * size.width() / size.height()).round().max(1.0) as u32;
            (w, h as u32)
        }
        _ => (intrinsic_w as u32, intrinsic_h as u32),
    };

    let mut pixmap = Pixmap::new(target_w, target_h).context("failed to create SVG pixmap")?;
    let transform = tiny_skia::Transform::from_scale(
        target_w as f32 / size.width(),
        target_h as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

fn decode_gif_frames(data: &[u8]) -> Result<(Vec<image::DynamicImage>, i32)> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;

    let decoder =
        GifDecoder::new(std::io::Cursor::new(data)).context("failed to create GIF decoder")?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .context("failed to decode GIF frames")?;

    let delay_ms = frames
        .first()
        .map(|frame| {
            let (numer, denom) = frame.delay().numer_denom_ms();
            if denom == 0 {
                0
            } else {
                (numer / denom) as i32
            }
        })
        .unwrap_or(0);

    Ok((
        frames
            .into_iter()
            .map(|f| image::DynamicImage::ImageRgba8(f.into_buffer()))
            .collect(),
        delay_ms,
    ))
}

fn maybe_resize(
    img: &image::DynamicImage,
    width: Option<i32>,
    height: Option<i32>,
) -> image::DynamicImage {
    match (width, height) {
        (None, None) => img.clone(),
        (w, h) => {
            let orig_w = img.width() as f64;
            let orig_h = img.height() as f64;

            let nw = match w {
                Some(w) if w > 0 => w as u32,
                _ => {
                    // Scale width maintaining aspect ratio
                    let nh = h.unwrap_or(0) as f64;
                    (nh * (orig_w / orig_h)) as u32
                }
            };
            let nh = match h {
                Some(h) if h > 0 => h as u32,
                _ => {
                    // Scale height maintaining aspect ratio
                    let nw_f = nw as f64;
                    (nw_f * (orig_h / orig_w)) as u32
                }
            };

            if nw == 0 || nh == 0 {
                return img.clone();
            }

            img.resize_exact(nw, nh, image::imageops::FilterType::Nearest)
        }
    }
}

fn dynamic_image_to_pixmap(img: &image::DynamicImage) -> Result<Pixmap> {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let mut pixmap = Pixmap::new(w, h).context("failed to create pixmap")?;

    let src = rgba.as_raw();
    let dst = pixmap.data_mut();

    // Both layouts are RGBA, 4 bytes per pixel
    for i in 0..(w * h) as usize {
        let off = i * 4;
        let r = src[off];
        let g = src[off + 1];
        let b = src[off + 2];
        let a = src[off + 3];

        // tiny-skia uses premultiplied alpha
        let pa = a as f32 / 255.0;
        dst[off] = (r as f32 * pa) as u8;
        dst[off + 1] = (g as f32 * pa) as u8;
        dst[off + 2] = (b as f32 * pa) as u8;
        dst[off + 3] = a;
    }

    Ok(pixmap)
}

impl Widget for ImageWidget {
    fn paint_bounds(&self, _bounds: Rect, frame_idx: i32) -> Rect {
        let pm = self.frame_pixmap(frame_idx);
        Rect::new(0, 0, pm.width() as i32, pm.height() as i32)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let src = self.frame_pixmap(frame_idx);
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
                    // Alpha composite (src over dst), both premultiplied
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
        let hold_frames = self.hold_frames.max(1);
        (self.frames.len() as i32 * hold_frames).max(1)
    }

    fn size(&self) -> Option<(i32, i32)> {
        self.frames
            .first()
            .map(|pm| (pm.width() as i32, pm.height() as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_GIF: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x05, 0x00, 0x04, 0x00, 0xf0, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x7b, 0x00, 0x00, 0x00, 0x21, 0xff, 0x0b,
        0x4e, 0x45, 0x54, 0x53, 0x43, 0x41, 0x50, 0x45, 0x32, 0x2e, 0x30, 0x03, 0x01, 0x00, 0x00,
        0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x04, 0x00, 0x00, 0x02, 0x06, 0x04, 0x62,
        0x68, 0xb9, 0x8b, 0x05, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x7b, 0x00, 0x00, 0x00, 0x2c, 0x00,
        0x00, 0x00, 0x00, 0x05, 0x00, 0x04, 0x00, 0x00, 0x02, 0x05, 0x84, 0x73, 0xa6, 0xa8, 0x57,
        0x00, 0x21, 0xf9, 0x04, 0x01, 0x7b, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x05,
        0x00, 0x04, 0x00, 0x00, 0x02, 0x06, 0x0c, 0x6e, 0x90, 0xa7, 0xcc, 0x05, 0x00, 0x21, 0xf9,
        0x04, 0x01, 0x7b, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x04, 0x00,
        0x00, 0x02, 0x06, 0x44, 0x80, 0x67, 0xc8, 0xca, 0x05, 0x00, 0x3b,
    ];

    fn make_1x1_red_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        buf
    }

    fn frame_mask(widget: &ImageWidget, frame_idx: i32) -> Vec<String> {
        let frame = widget.frame_pixmap(frame_idx);
        (0..frame.height())
            .map(|y| {
                (0..frame.width())
                    .map(|x| {
                        let px = frame.pixels()[(y * frame.width() + x) as usize];
                        if px.alpha() > 0 {
                            'x'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn decode_png() {
        let data = make_1x1_red_png();
        let widget = ImageWidget::from_bytes(&data, None, None).unwrap();
        assert_eq!(widget.frames.len(), 1);
        assert_eq!(widget.size(), Some((1, 1)));
    }

    #[test]
    fn frame_count_single_frame() {
        let data = make_1x1_red_png();
        let widget = ImageWidget::from_bytes(&data, None, None).unwrap();
        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn hold_frames_multiplies_count() {
        let data = make_1x1_red_png();
        let mut widget = ImageWidget::from_bytes(&data, None, None).unwrap();
        widget.hold_frames = 3;
        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 3);
    }

    #[test]
    fn resize_scales_image() {
        let data = make_1x1_red_png();
        let widget = ImageWidget::from_bytes(&data, Some(4), Some(4)).unwrap();
        assert_eq!(widget.size(), Some((4, 4)));
    }

    #[test]
    fn paint_copies_pixels() {
        let data = make_1x1_red_png();
        let widget = ImageWidget::from_bytes(&data, None, None).unwrap();
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        widget.paint(&mut pixmap, Rect::new(1, 1, 4, 4), 0);
        let px = pixmap.pixels()[(1 * 4 + 1) as usize];
        assert_eq!(px.red(), 255);
        assert_eq!(px.alpha(), 255);
        // (0,0) should still be empty
        let corner = pixmap.pixels()[0];
        assert_eq!(corner.alpha(), 0);
    }

    #[test]
    fn decode_svg() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1" viewBox="0 0 2 1"><rect width="2" height="1" fill="#00ff00"/></svg>"##;
        let widget = ImageWidget::from_bytes(svg, None, None).unwrap();
        assert_eq!(widget.size(), Some((2, 1)));
    }

    #[test]
    fn decode_animated_gif_matches_pixlet_frames() {
        let widget = ImageWidget::from_bytes(TEST_GIF, None, None).unwrap();

        assert_eq!(widget.size(), Some((5, 4)));
        assert_eq!(widget.delay_ms, 1230);
        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 4);

        assert_eq!(
            frame_mask(&widget, 0),
            vec!["..x..", "x....", ".x...", "...x."]
        );
        assert_eq!(
            frame_mask(&widget, 1),
            vec!["..xx.", "xx...", ".xx..", "...xx"]
        );
        assert_eq!(
            frame_mask(&widget, 2),
            vec!["x.xxx", "xxx..", ".xxx.", "...xx"]
        );
        assert_eq!(
            frame_mask(&widget, 3),
            vec!["xxxxx", "xxxx.", ".xxxx", "...xx"]
        );
        assert_eq!(frame_mask(&widget, 4), frame_mask(&widget, 0));
    }

    #[test]
    fn hold_frames_repeats_gif_frames() {
        let mut widget = ImageWidget::from_bytes(TEST_GIF, None, None).unwrap();
        widget.hold_frames = 2;

        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 8);
        assert_eq!(frame_mask(&widget, 0), frame_mask(&widget, 1));
        assert_ne!(frame_mask(&widget, 1), frame_mask(&widget, 2));
        assert_eq!(frame_mask(&widget, 2), frame_mask(&widget, 3));
        assert_eq!(frame_mask(&widget, 4), frame_mask(&widget, 5));
        assert_eq!(frame_mask(&widget, 6), frame_mask(&widget, 7));
    }

    #[test]
    fn hold_frames_zero_falls_back_to_one() {
        let data = make_1x1_red_png();
        let mut widget = ImageWidget::from_bytes(&data, None, None).unwrap();
        widget.hold_frames = 0;

        assert_eq!(widget.frame_count(Rect::new(0, 0, 64, 32)), 1);
        assert_eq!(
            widget.paint_bounds(Rect::new(0, 0, 64, 32), 3),
            Rect::new(0, 0, 1, 1)
        );
    }
}
