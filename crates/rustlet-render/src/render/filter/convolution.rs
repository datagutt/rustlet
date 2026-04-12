//! Convolution-based filters: sharpen, edge detection, emboss. Each runs a
//! 3x3 kernel over the RGBA pixels of the rendered child. imageproc provides
//! generic convolution helpers, but we implement 3x3 directly because the
//! kernels are small and avoid allocating separate image buffers.

use super::{composite_pixmap, render_child_to_pixmap};
use crate::render::{Rect, Widget};
use tiny_skia::Pixmap;

const SHARPEN_KERNEL: [[f32; 3]; 3] = [
    [0.0, -1.0, 0.0],
    [-1.0, 5.0, -1.0],
    [0.0, -1.0, 0.0],
];

const EDGE_KERNEL: [[f32; 3]; 3] = [
    [-1.0, -1.0, -1.0],
    [-1.0, 8.0, -1.0],
    [-1.0, -1.0, -1.0],
];

const EMBOSS_KERNEL: [[f32; 3]; 3] = [
    [-2.0, -1.0, 0.0],
    [-1.0, 1.0, 1.0],
    [0.0, 1.0, 2.0],
];

fn apply_kernel(src: &Pixmap, kernel: &[[f32; 3]; 3], offset: f32) -> Option<Pixmap> {
    let w = src.width() as i32;
    let h = src.height() as i32;
    let mut dst = Pixmap::new(w as u32, h as u32)?;
    let src_data = src.data();
    let dst_data = dst.data_mut();

    for y in 0..h {
        for x in 0..w {
            let mut r = 0.0_f32;
            let mut g = 0.0_f32;
            let mut b = 0.0_f32;
            let alpha;

            for ky in -1..=1 {
                for kx in -1..=1 {
                    let sx = (x + kx).clamp(0, w - 1);
                    let sy = (y + ky).clamp(0, h - 1);
                    let idx = ((sy * w + sx) * 4) as usize;
                    let k = kernel[(ky + 1) as usize][(kx + 1) as usize];
                    // The filter operates on unpremultiplied data but since we
                    // clamp to [0, 255] anyway, running on premultiplied values
                    // is good enough for our pixel displays.
                    r += src_data[idx] as f32 * k;
                    g += src_data[idx + 1] as f32 * k;
                    b += src_data[idx + 2] as f32 * k;
                }
            }

            let src_idx = ((y * w + x) * 4) as usize;
            alpha = src_data[src_idx + 3];

            let dst_idx = src_idx;
            dst_data[dst_idx] = (r + offset).clamp(0.0, 255.0) as u8;
            dst_data[dst_idx + 1] = (g + offset).clamp(0.0, 255.0) as u8;
            dst_data[dst_idx + 2] = (b + offset).clamp(0.0, 255.0) as u8;
            dst_data[dst_idx + 3] = alpha;
        }
    }

    Some(dst)
}

pub struct Sharpen {
    pub child: Box<dyn Widget>,
}

impl Widget for Sharpen {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let Some(out) = apply_kernel(&src, &SHARPEN_KERNEL, 0.0) else {
            return;
        };
        composite_pixmap(dest, &out, bounds.x + cb.x, bounds.y + cb.y);
    }
}

pub struct EdgeDetection {
    pub child: Box<dyn Widget>,
}

impl Widget for EdgeDetection {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let Some(out) = apply_kernel(&src, &EDGE_KERNEL, 0.0) else {
            return;
        };
        composite_pixmap(dest, &out, bounds.x + cb.x, bounds.y + cb.y);
    }
}

pub struct Emboss {
    pub child: Box<dyn Widget>,
}

impl Widget for Emboss {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        // Emboss typically offsets by 128 to bring the midpoint into visible range.
        let Some(out) = apply_kernel(&src, &EMBOSS_KERNEL, 128.0) else {
            return;
        };
        composite_pixmap(dest, &out, bounds.x + cb.x, bounds.y + cb.y);
    }
}
