//! Pixel-level color filters: brightness, contrast, gamma, grayscale, hue,
//! invert, saturation, sepia, threshold. Each widget wraps a child, renders it,
//! and applies an in-place pixel transformation before compositing.

use super::{composite_pixmap, render_child_to_pixmap};
use crate::render::{Rect, Widget};
use tiny_skia::Pixmap;

/// Iterate over the pixmap pixels and apply `f` to each premultiplied RGBA quad.
fn for_each_pixel(pixmap: &mut Pixmap, mut f: impl FnMut(&mut [u8; 4])) {
    let data = pixmap.data_mut();
    for chunk in data.chunks_exact_mut(4) {
        let mut px = [chunk[0], chunk[1], chunk[2], chunk[3]];
        f(&mut px);
        chunk.copy_from_slice(&px);
    }
}

/// Unpremultiply a pixel so we can operate on straight RGBA, mutate it, and
/// re-premultiply. Most color filters need straight alpha to produce expected results.
fn with_unpremultiplied(px: &mut [u8; 4], f: impl FnOnce(u8, u8, u8) -> (u8, u8, u8)) {
    let a = px[3];
    if a == 0 {
        return;
    }
    let inv = 255.0 / a as f32;
    let r = (px[0] as f32 * inv).clamp(0.0, 255.0) as u8;
    let g = (px[1] as f32 * inv).clamp(0.0, 255.0) as u8;
    let b = (px[2] as f32 * inv).clamp(0.0, 255.0) as u8;
    let (r, g, b) = f(r, g, b);
    let af = a as f32 / 255.0;
    px[0] = (r as f32 * af).round() as u8;
    px[1] = (g as f32 * af).round() as u8;
    px[2] = (b as f32 * af).round() as u8;
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h_prime as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r1 + m, g1 + m, b1 + m)
}

// ---------- Brightness ----------

pub struct Brightness {
    pub child: Box<dyn Widget>,
    pub change: f64,
}

impl Widget for Brightness {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let change = self.change.clamp(-1.0, 1.0) as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let adjust = |c: u8| {
                    let v = c as f32 / 255.0;
                    ((v + change).clamp(0.0, 1.0) * 255.0).round() as u8
                };
                (adjust(r), adjust(g), adjust(b))
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Contrast ----------

pub struct Contrast {
    pub child: Box<dyn Widget>,
    pub change: f64,
}

impl Widget for Contrast {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let factor = (259.0 * ((self.change as f32).clamp(-1.0, 1.0) * 255.0 + 255.0))
            / (255.0 * (259.0 - ((self.change as f32).clamp(-1.0, 1.0) * 255.0 + 255.0).abs()).max(0.01));
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let adj = |c: u8| {
                    ((factor * (c as f32 - 128.0) + 128.0).clamp(0.0, 255.0)).round() as u8
                };
                (adj(r), adj(g), adj(b))
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Gamma ----------

pub struct Gamma {
    pub child: Box<dyn Widget>,
    pub gamma: f64,
}

impl Widget for Gamma {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let g = self.gamma.max(0.0) as f32;
        let inv = if g == 0.0 { 1.0 } else { 1.0 / g };
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, gr, b| {
                let adj = |c: u8| {
                    let v = (c as f32 / 255.0).powf(inv);
                    (v.clamp(0.0, 1.0) * 255.0).round() as u8
                };
                (adj(r), adj(gr), adj(b))
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Grayscale ----------

pub struct Grayscale {
    pub child: Box<dyn Widget>,
}

impl Widget for Grayscale {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let y = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32)
                    .clamp(0.0, 255.0) as u8;
                (y, y, y)
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Invert ----------

pub struct Invert {
    pub child: Box<dyn Widget>,
}

impl Widget for Invert {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| (255 - r, 255 - g, 255 - b));
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Hue ----------

pub struct Hue {
    pub child: Box<dyn Widget>,
    pub change: f64,
}

impl Widget for Hue {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let delta = self.change as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let (h, s, v) = rgb_to_hsv(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                let new_h = (h + delta).rem_euclid(360.0);
                let (r2, g2, b2) = hsv_to_rgb(new_h, s, v);
                (
                    (r2 * 255.0).clamp(0.0, 255.0) as u8,
                    (g2 * 255.0).clamp(0.0, 255.0) as u8,
                    (b2 * 255.0).clamp(0.0, 255.0) as u8,
                )
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Saturation ----------

pub struct Saturation {
    pub child: Box<dyn Widget>,
    pub change: f64,
}

impl Widget for Saturation {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let adj = self.change as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let (h, s, v) = rgb_to_hsv(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                let new_s = (s + adj).clamp(0.0, 1.0);
                let (r2, g2, b2) = hsv_to_rgb(h, new_s, v);
                (
                    (r2 * 255.0).clamp(0.0, 255.0) as u8,
                    (g2 * 255.0).clamp(0.0, 255.0) as u8,
                    (b2 * 255.0).clamp(0.0, 255.0) as u8,
                )
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Sepia ----------

pub struct Sepia {
    pub child: Box<dyn Widget>,
}

impl Widget for Sepia {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let rf = r as f32;
                let gf = g as f32;
                let bf = b as f32;
                let nr = (0.393 * rf + 0.769 * gf + 0.189 * bf).min(255.0);
                let ng = (0.349 * rf + 0.686 * gf + 0.168 * bf).min(255.0);
                let nb = (0.272 * rf + 0.534 * gf + 0.131 * bf).min(255.0);
                (nr as u8, ng as u8, nb as u8)
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

// ---------- Threshold ----------

pub struct Threshold {
    pub child: Box<dyn Widget>,
    pub level: f64,
}

impl Widget for Threshold {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.child.paint_bounds(bounds, frame_idx)
    }
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
    fn paint(&self, dest: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let Some((mut src, cb)) = render_child_to_pixmap(&*self.child, bounds, frame_idx) else {
            return;
        };
        let threshold = (self.level.clamp(0.0, 1.0) * 255.0) as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let y = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                let v = if y >= threshold { 255 } else { 0 };
                (v, v, v)
            });
        });
        composite_pixmap(dest, &src, bounds.x + cb.x, bounds.y + cb.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn invert_flips_red_to_cyan() {
        let child = BoxWidget {
            width: 2,
            height: 2,
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..BoxWidget::new()
        };
        let w = Invert {
            child: Box::new(child),
        };
        let mut pixmap = Pixmap::new(2, 2).unwrap();
        w.paint(&mut pixmap, Rect::new(0, 0, 2, 2), 0);
        let px = pixmap.pixels()[0];
        assert_eq!(px.red(), 0);
        assert_eq!(px.green(), 255);
        assert_eq!(px.blue(), 255);
    }

    #[test]
    fn grayscale_converts_red() {
        let child = BoxWidget {
            width: 1,
            height: 1,
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..BoxWidget::new()
        };
        let w = Grayscale {
            child: Box::new(child),
        };
        let mut pixmap = Pixmap::new(1, 1).unwrap();
        w.paint(&mut pixmap, Rect::new(0, 0, 1, 1), 0);
        let px = pixmap.pixels()[0];
        // red to gray: 0.299 * 255 = ~76
        assert!(px.red() >= 70 && px.red() <= 80);
        assert_eq!(px.red(), px.green());
        assert_eq!(px.red(), px.blue());
    }
}
