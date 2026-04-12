use super::{Rect, Widget};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

const KAPPA: f32 = 0.5522848;

pub struct Circle {
    pub child: Option<Box<dyn Widget>>,
    pub color: Option<Color>,
    pub diameter: i32,
}

impl Circle {
    pub fn new(diameter: i32) -> Self {
        Self {
            child: None,
            color: None,
            diameter,
        }
    }
}

/// Build a circle path from 4 cubic bezier curves, matching the Go
/// `gg.DrawCircle` approach.
fn circle_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + r * KAPPA, cx + r * KAPPA, cy + r, cx, cy + r);
    pb.cubic_to(cx - r * KAPPA, cy + r, cx - r, cy + r * KAPPA, cx - r, cy);
    pb.cubic_to(cx - r, cy - r * KAPPA, cx - r * KAPPA, cy - r, cx, cy - r);
    pb.cubic_to(cx + r * KAPPA, cy - r, cx + r, cy - r * KAPPA, cx + r, cy);
    pb.close();
    pb.finish()
}

impl Widget for Circle {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        Rect::new(0, 0, self.diameter, self.diameter)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let r = self.diameter as f32 / 2.0;
        let cx = bounds.x as f32 + r;
        let cy = bounds.y as f32 + r;

        if let Some(color) = self.color {
            if let Some(path) = circle_path(cx, cy, r) {
                let mut paint = Paint::default();
                paint.set_color(color);
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        if let Some(child) = &self.child {
            let child_bounds_rect = Rect::new(0, 0, self.diameter, self.diameter);
            let cb = child.paint_bounds(child_bounds_rect, frame_idx);

            // Match Go rounding: math.Ceil(diameter / 2.0)
            let center = (self.diameter as f64 / 2.0).ceil() as i32;
            let x = center - (0.5 * cb.width as f64) as i32;
            let y = center - (0.5 * cb.height as f64) as i32;

            let child_draw_bounds =
                Rect::new(bounds.x + x, bounds.y + y, self.diameter, self.diameter);
            child.paint(pixmap, child_draw_bounds, frame_idx);
        }
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child
            .as_ref()
            .map(|c| c.frame_count(bounds))
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::PremultipliedColorU8;

    fn pixel_at(pixmap: &Pixmap, x: u32, y: u32) -> PremultipliedColorU8 {
        pixmap.pixels()[(y * pixmap.width() + x) as usize]
    }

    #[test]
    fn paint_bounds_returns_diameter() {
        let c = Circle::new(10);
        let pb = c.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 10, 10));
    }

    #[test]
    fn circle_fills_center_pixel() {
        let c = Circle {
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..Circle::new(6)
        };
        let mut pixmap = Pixmap::new(6, 6).unwrap();
        c.paint(&mut pixmap, Rect::new(0, 0, 6, 6), 0);
        let center = pixel_at(&pixmap, 3, 3);
        assert_eq!(center.red(), 255);
    }

    #[test]
    fn circle_does_not_fill_corner() {
        let c = Circle {
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..Circle::new(10)
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        c.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        let corner = pixel_at(&pixmap, 0, 0);
        assert_eq!(corner.alpha(), 0);
    }

    #[test]
    fn frame_count_defaults_to_one() {
        let c = Circle::new(4);
        assert_eq!(c.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn frame_count_delegates_to_child() {
        struct MultiFrame;
        impl Widget for MultiFrame {
            fn paint_bounds(&self, _: Rect, _: i32) -> Rect {
                Rect::new(0, 0, 2, 2)
            }
            fn paint(&self, _: &mut Pixmap, _: Rect, _: i32) {}
            fn frame_count(&self, _: Rect) -> i32 {
                5
            }
        }
        let c = Circle {
            child: Some(Box::new(MultiFrame)),
            ..Circle::new(10)
        };
        assert_eq!(c.frame_count(Rect::new(0, 0, 64, 32)), 5);
    }
}
