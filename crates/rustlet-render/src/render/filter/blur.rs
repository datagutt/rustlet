//! Gaussian blur matching bild's implementation exactly.
//!
//! bild (pixlet's upstream) uses a 1D Gaussian of length `ceil(2*radius + 1)`
//! with coefficients `exp(-x^2 / (4 * radius))` for x in `[-radius, radius]`,
//! normalized. It runs this kernel separably (horizontal then vertical) over
//! edge-extended padding. imageproc's `gaussian_blur_f32` uses a wider kernel
//! derived from sigma, which produces noticeably more spread at small radii;
//! to match pixlet exactly we implement bild's kernel directly.

use super::composite_pixmap;
use crate::render::{Rect, Widget};
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
        if self.radius <= 0.0 {
            // No-op blur: just draw the child normally.
            let Some((src, cb)) = super::render_child_to_pixmap(&*self.child, bounds, frame_idx)
            else {
                return;
            };
            composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
            return;
        }

        let padding = (self.radius * 3.0).ceil() as i32;
        let cb = self.child.paint_bounds(bounds, frame_idx);

        let tw = (cb.width + 2 * padding).max(1) as u32;
        let th = (cb.height + 2 * padding).max(1) as u32;
        let Some(mut tmp) = Pixmap::new(tw, th) else {
            return;
        };
        self.child.paint(
            &mut tmp,
            Rect::new(padding, padding, cb.width, cb.height),
            frame_idx,
        );

        // Blur the premultiplied bytes directly. bild (via Go's `image.RGBA`)
        // blurs premultiplied channels, which is what pixlet compares against.
        // Unpremultiplying first would darken the result after re-premultiply.
        gaussian_blur_bild(tmp.data_mut(), tw as usize, th as usize, self.radius as f32);

        composite_pixmap(dest, &tmp, bounds.x, bounds.y);
    }
}

/// Build bild's 1D Gaussian kernel: `exp(-x^2 / (4*radius))` over
/// `x = -radius..radius` with `ceil(2*radius + 1)` samples, then normalize.
fn build_kernel(radius: f32) -> Vec<f32> {
    let length = (2.0 * radius + 1.0).ceil() as usize;
    let mut kernel = Vec::with_capacity(length);
    let mut x = -radius;
    for _ in 0..length {
        kernel.push((-(x * x) / (4.0 * radius)).exp());
        x += 1.0;
    }
    let sum: f32 = kernel.iter().sum();
    for v in kernel.iter_mut() {
        *v /= sum;
    }
    kernel
}

/// In-place separable Gaussian blur on a straight-RGBA byte buffer. Uses edge
/// extension for out-of-bounds samples to match bild's `EdgeExtend` padding.
fn gaussian_blur_bild(buf: &mut [u8], w: usize, h: usize, radius: f32) {
    let kernel = build_kernel(radius);
    let k_radius = (kernel.len() / 2) as isize;

    let mut temp = vec![0u8; buf.len()];

    // Horizontal pass: buf -> temp
    for y in 0..h {
        for x in 0..w {
            let mut r = 0.0_f32;
            let mut g = 0.0_f32;
            let mut b = 0.0_f32;
            let mut a = 0.0_f32;
            for (ki, kv) in kernel.iter().enumerate() {
                let sx = (x as isize + ki as isize - k_radius).clamp(0, w as isize - 1) as usize;
                let idx = (y * w + sx) * 4;
                r += buf[idx] as f32 * kv;
                g += buf[idx + 1] as f32 * kv;
                b += buf[idx + 2] as f32 * kv;
                a += buf[idx + 3] as f32 * kv;
            }
            let di = (y * w + x) * 4;
            temp[di] = r.clamp(0.0, 255.0) as u8;
            temp[di + 1] = g.clamp(0.0, 255.0) as u8;
            temp[di + 2] = b.clamp(0.0, 255.0) as u8;
            temp[di + 3] = a.clamp(0.0, 255.0) as u8;
        }
    }

    // Vertical pass: temp -> buf
    for y in 0..h {
        for x in 0..w {
            let mut r = 0.0_f32;
            let mut g = 0.0_f32;
            let mut b = 0.0_f32;
            let mut a = 0.0_f32;
            for (ki, kv) in kernel.iter().enumerate() {
                let sy = (y as isize + ki as isize - k_radius).clamp(0, h as isize - 1) as usize;
                let idx = (sy * w + x) * 4;
                r += temp[idx] as f32 * kv;
                g += temp[idx + 1] as f32 * kv;
                b += temp[idx + 2] as f32 * kv;
                a += temp[idx + 3] as f32 * kv;
            }
            let di = (y * w + x) * 4;
            buf[di] = r.clamp(0.0, 255.0) as u8;
            buf[di + 1] = g.clamp(0.0, 255.0) as u8;
            buf[di + 2] = b.clamp(0.0, 255.0) as u8;
            buf[di + 3] = a.clamp(0.0, 255.0) as u8;
        }
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

    #[test]
    fn kernel_normalizes_to_one() {
        let k = build_kernel(2.0);
        let sum: f32 = k.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        // bild's length = ceil(2*2 + 1) = 5
        assert_eq!(k.len(), 5);
    }
}
