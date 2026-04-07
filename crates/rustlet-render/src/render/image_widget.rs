use super::{Rect, Widget, mod_int};
use anyhow::{Context, Result};
use tiny_skia::Pixmap;

pub struct ImageWidget {
    frames: Vec<Pixmap>,
    pub hold_frames: i32,
}

impl ImageWidget {
    pub fn from_bytes(data: &[u8], width: Option<i32>, height: Option<i32>) -> Result<Self> {
        let cursor = std::io::Cursor::new(data);
        let reader = image::ImageReader::new(cursor)
            .with_guessed_format()
            .context("failed to guess image format")?;

        let format = reader.format();
        let is_gif = matches!(format, Some(image::ImageFormat::Gif));

        let raw_frames = if is_gif {
            decode_gif_frames(data)?
        } else {
            let img = reader.decode().context("failed to decode image")?;
            vec![img]
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
        })
    }

    fn frame_pixmap(&self, frame_idx: i32) -> &Pixmap {
        let i = mod_int(frame_idx / self.hold_frames, self.frames.len() as i32);
        &self.frames[i as usize]
    }
}

fn decode_gif_frames(data: &[u8]) -> Result<Vec<image::DynamicImage>> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;

    let decoder = GifDecoder::new(std::io::Cursor::new(data))
        .context("failed to create GIF decoder")?;

    let frames = decoder
        .into_frames()
        .collect_frames()
        .context("failed to decode GIF frames")?;

    Ok(frames
        .into_iter()
        .map(|f| image::DynamicImage::ImageRgba8(f.into_buffer()))
        .collect())
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
    let mut pixmap = Pixmap::new(w, h)
        .context("failed to create pixmap")?;

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
        (self.frames.len() as i32 * self.hold_frames).max(1)
    }

    fn size(&self) -> Option<(i32, i32)> {
        self.frames.first().map(|pm| (pm.width() as i32, pm.height() as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_1x1_red_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        buf
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
}
