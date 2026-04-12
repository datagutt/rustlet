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

/// Convert straight RGB (0..=255) to HSL with h in degrees (0..360), s/l in [0, 1].
/// Matches bild's `util.RGBToHSL`, which is the color model pixlet's hue/saturation
/// filters use.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let rn = r / 255.0;
    let gn = g / 255.0;
    let bn = b / 255.0;
    let max = rn.max(gn.max(bn));
    let min = rn.min(gn.min(bn));
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == rn {
        60.0 * (((gn - bn) / d) + if gn < bn { 6.0 } else { 0.0 })
    } else if max == gn {
        60.0 * (((bn - rn) / d) + 2.0)
    } else {
        60.0 * (((rn - gn) / d) + 4.0)
    };
    (h, s, l)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < f32::EPSILON {
        return (l * 255.0, l * 255.0, l * 255.0);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hn = (h / 360.0).rem_euclid(1.0);
    let r = hue_to_rgb(p, q, hn + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, hn);
    let b = hue_to_rgb(p, q, hn - 1.0 / 3.0);
    (r * 255.0, g * 255.0, b * 255.0)
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
        // bild's Brightness is multiplicative: `c * (1 + change)`. `change` in [-1, 1].
        let factor = 1.0 + self.change.clamp(-1.0, 1.0) as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let adjust = |c: u8| ((c as f32 * factor).clamp(0.0, 255.0)).round() as u8;
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
        // bild formula: `((c/255 - 0.5) * (1 + change) + 0.5) * 255`.
        let change = self.change.clamp(-1.0, 1.0) as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let adj = |c: u8| {
                    let v = c as f32 / 255.0;
                    (((v - 0.5) * (1.0 + change) + 0.5) * 255.0)
                        .clamp(0.0, 255.0)
                        .round() as u8
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
        // bild's Hue takes integer degrees and rotates via HSL.
        let delta = self.change as i32 as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let (h, s, l) = rgb_to_hsl(r as f32, g as f32, b as f32);
                let new_h = (h + delta).rem_euclid(360.0);
                let (r2, g2, b2) = hsl_to_rgb(new_h, s, l);
                (
                    r2.clamp(0.0, 255.0) as u8,
                    g2.clamp(0.0, 255.0) as u8,
                    b2.clamp(0.0, 255.0) as u8,
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
        // bild's Saturation is multiplicative via HSL: `s * (1 + change)`.
        let adj = self.change as f32;
        for_each_pixel(&mut src, |px| {
            with_unpremultiplied(px, |r, g, b| {
                let (h, s, l) = rgb_to_hsl(r as f32, g as f32, b as f32);
                let new_s = (s * (1.0 + adj)).clamp(0.0, 1.0);
                let (r2, g2, b2) = hsl_to_rgb(h, new_s, l);
                (
                    r2.clamp(0.0, 255.0) as u8,
                    g2.clamp(0.0, 255.0) as u8,
                    b2.clamp(0.0, 255.0) as u8,
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
