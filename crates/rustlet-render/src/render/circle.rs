use super::ft_raster::{fill_path as ft_fill_path, FtPath};
use super::{Rect, Widget};
use tiny_skia::{Color, Pixmap};

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

/// Build gg's 16-segment quadratic-Bézier circle and recursively subdivide
/// each quad into straight lines using freetype/raster's `Add2` splitting
/// rule. `ab_glyph_rasterizer` has its own quad subdivision heuristic that
/// deviates from freetype's at the small-radius circles pixlet renders for
/// analog clocks; feeding pre-flattened lines keeps both rasterizers reading
/// the same input geometry.
fn build_circle_path(cx: f32, cy: f32, r: f32) -> FtPath {
    const N: i32 = 16;
    let mut path = FtPath::new();
    let two_pi = std::f64::consts::TAU;
    let rd = r as f64;
    let cxd = cx as f64;
    let cyd = cy as f64;

    let mut first = None;
    for i in 0..N {
        let p1 = i as f64 / N as f64;
        let p2 = (i + 1) as f64 / N as f64;
        let a1 = two_pi * p1;
        let a2 = two_pi * p2;
        let x0 = cxd + rd * a1.cos();
        let y0 = cyd + rd * a1.sin();
        let x1 = cxd + rd * ((a1 + a2) / 2.0).cos();
        let y1 = cyd + rd * ((a1 + a2) / 2.0).sin();
        let x2 = cxd + rd * a2.cos();
        let y2 = cyd + rd * a2.sin();
        let ctrl_x = 2.0 * x1 - x0 / 2.0 - x2 / 2.0;
        let ctrl_y = 2.0 * y1 - y0 / 2.0 - y2 / 2.0;

        if i == 0 {
            path.move_to(x0 as f32, y0 as f32);
            first = Some((x0, y0));
        }
        flatten_quad(
            &mut path,
            (x0, y0),
            (ctrl_x, ctrl_y),
            (x2, y2),
        );
    }
    if let Some((fx, fy)) = first {
        path.line_to(fx as f32, fy as f32);
    }
    path
}

/// Recursively subdivide a quadratic Bézier until the deviation from linear
/// drops below `tol`, then emit straight segments. This matches freetype's
/// approach where quadratic curves are decomposed into fine polylines before
/// scanline rasterization. The tolerance of 0.25 pixels gives the rasterizer
/// enough precision to produce pixel-identical coverage with pixlet.
fn flatten_quad(
    path: &mut FtPath,
    p0: (f64, f64),
    c: (f64, f64),
    p1: (f64, f64),
) {
    const TOL: f64 = 0.25;
    let dx = p0.0 - 2.0 * c.0 + p1.0;
    let dy = p0.1 - 2.0 * c.1 + p1.1;
    let dev = (dx * dx + dy * dy).sqrt();
    if dev <= TOL {
        path.line_to(p1.0 as f32, p1.1 as f32);
        return;
    }
    // de Casteljau subdivision at t=0.5
    let m01 = ((p0.0 + c.0) * 0.5, (p0.1 + c.1) * 0.5);
    let m12 = ((c.0 + p1.0) * 0.5, (c.1 + p1.1) * 0.5);
    let mid = ((m01.0 + m12.0) * 0.5, (m01.1 + m12.1) * 0.5);
    flatten_quad(path, p0, m01, mid);
    flatten_quad(path, mid, m12, p1);
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
            let path = build_circle_path(cx, cy, r);
            let raster_bounds = Rect::new(bounds.x, bounds.y, self.diameter, self.diameter);
            ft_fill_path(pixmap, raster_bounds, &path, color);
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
