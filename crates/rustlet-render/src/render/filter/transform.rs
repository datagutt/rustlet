//! Geometric filter transforms: flip, rotate, shear. Each renders the child
//! to an offscreen pixmap and then composites it through a tiny-skia affine
//! `Transform` applied during `draw_pixmap`.

use super::render_child_to_pixmap;
use crate::render::{Rect, Widget};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform as TsTransform};

fn composite_with_transform(dest: &mut Pixmap, src: &Pixmap, ts: TsTransform) {
    let paint = PixmapPaint {
        opacity: 1.0,
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality: FilterQuality::Nearest,
    };
    dest.draw_pixmap(0, 0, src.as_ref(), &paint, ts, None);
}

pub struct FlipHorizontal {
    pub child: Box<dyn Widget>,
}

impl Widget for FlipHorizontal {
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
        // Mirror across the vertical axis of the src pixmap, then translate to target.
        let ts = TsTransform::from_scale(-1.0, 1.0)
            .post_translate(src.width() as f32, 0.0)
            .post_translate((bounds.x + cb.x) as f32, (bounds.y + cb.y) as f32);
        composite_with_transform(dest, &src, ts);
    }
}

pub struct FlipVertical {
    pub child: Box<dyn Widget>,
}

impl Widget for FlipVertical {
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
        let ts = TsTransform::from_scale(1.0, -1.0)
            .post_translate(0.0, src.height() as f32)
            .post_translate((bounds.x + cb.x) as f32, (bounds.y + cb.y) as f32);
        composite_with_transform(dest, &src, ts);
    }
}

pub struct Rotate {
    pub child: Box<dyn Widget>,
    pub angle: f64,
}

impl Widget for Rotate {
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
        let cx = src.width() as f32 / 2.0;
        let cy = src.height() as f32 / 2.0;
        let ts = TsTransform::from_rotate_at(self.angle as f32, cx, cy).post_translate(
            (bounds.x + cb.x) as f32,
            (bounds.y + cb.y) as f32,
        );
        composite_with_transform(dest, &src, ts);
    }
}

pub struct Shear {
    pub child: Box<dyn Widget>,
    pub x_angle: f64,
    pub y_angle: f64,
}

impl Widget for Shear {
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
        let kx = self.x_angle.to_radians().tan() as f32;
        let ky = self.y_angle.to_radians().tan() as f32;
        let cx = src.width() as f32 / 2.0;
        let cy = src.height() as f32 / 2.0;
        let shear = TsTransform::from_row(1.0, ky, kx, 1.0, 0.0, 0.0);
        let ts = TsTransform::from_translate(-cx, -cy)
            .post_concat(shear)
            .post_translate(cx, cy)
            .post_translate((bounds.x + cb.x) as f32, (bounds.y + cb.y) as f32);
        composite_with_transform(dest, &src, ts);
    }
}
