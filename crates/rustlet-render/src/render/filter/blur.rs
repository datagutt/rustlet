//! Gaussian blur filter using imageproc. The child is rendered into an offscreen
//! pixmap padded by `ceil(radius * 3)` so the blur does not clip at edges, then
//! `imageproc::filter::gaussian_blur_f32` is applied and the result composited back.

use super::composite_pixmap;
use crate::render::{Rect, Widget};
use image::{ImageBuffer, Rgba};
use tiny_skia::Pixmap;

pub struct Blur {
    pub child: Box<dyn Widget>,
    pub radius: f64,
}

impl Widget for Blur {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        let cb = self.child.paint_bounds(bounds, frame_idx);
        let padding = (self.radius * 3.0).ceil() as i32;
        Rect::new(0, 0, cb.width + 2 * padding, cb.height + 2 * padding)
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }

    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let padding = (self.radius * 3.0).ceil() as i32;
        let cb = self.child.paint_bounds(bounds, frame_idx);

        // Render child into a padded pixmap.
        let tw = (cb.width + 2 * padding).max(1) as u32;
        let th = (cb.height + 2 * padding).max(1) as u32;
        let Some(mut tmp) = Pixmap::new(tw, th) else {
            return;
        };
        // Paint child offset by padding via bounds.
        self.child.paint(
            &mut tmp,
            Rect::new(padding, padding, cb.width, cb.height),
            frame_idx,
        );

        // Convert the pixmap to an image::ImageBuffer and run the blur.
        let buf = pixmap_to_rgba_unpremul(&tmp);
        let blurred = imageproc::filter::gaussian_blur_f32(&buf, self.radius as f32);
        rgba_unpremul_to_pixmap(&blurred, &mut tmp);

        composite_pixmap(dest, &tmp, bounds.x, bounds.y);
    }
}

pub(crate) fn pixmap_to_rgba_unpremul(pixmap: &Pixmap) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let w = pixmap.width();
    let h = pixmap.height();
    let mut out: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for chunk in pixmap.data().chunks_exact(4) {
        let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let inv = 255.0 / a as f32;
        let rr = (r as f32 * inv).clamp(0.0, 255.0) as u8;
        let gg = (g as f32 * inv).clamp(0.0, 255.0) as u8;
        let bb = (b as f32 * inv).clamp(0.0, 255.0) as u8;
        out.extend_from_slice(&[rr, gg, bb, a]);
    }
    ImageBuffer::from_raw(w, h, out).expect("pixmap rgba conversion")
}

pub(crate) fn rgba_unpremul_to_pixmap(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, pixmap: &mut Pixmap) {
    let data = pixmap.data_mut();
    for (chunk, px) in data.chunks_exact_mut(4).zip(buf.pixels()) {
        let [r, g, b, a] = px.0;
        if a == 0 {
            chunk.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let af = a as f32 / 255.0;
        chunk[0] = (r as f32 * af).round() as u8;
        chunk[1] = (g as f32 * af).round() as u8;
        chunk[2] = (b as f32 * af).round() as u8;
        chunk[3] = a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn blur_enlarges_paint_bounds() {
        let child = BoxWidget {
            width: 4,
            height: 4,
            color: Some(Color::from_rgba8(255, 255, 255, 255)),
            ..BoxWidget::new()
        };
        let w = Blur {
            child: Box::new(child),
            radius: 2.0,
        };
        let pb = w.paint_bounds(Rect::new(0, 0, 16, 16), 0);
        assert_eq!(pb.width, 4 + 2 * 6);
        assert_eq!(pb.height, 4 + 2 * 6);
    }
}

