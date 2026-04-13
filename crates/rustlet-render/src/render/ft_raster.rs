//! Freetype-compatible anti-aliased rasterization helpers.
//!
//! `gg` (pixlet's renderer) uses the same coverage-accumulation AA algorithm
//! as FreeType's "smooth" rasterizer via `github.com/golang/freetype/raster`.
//! tiny-skia uses a different AA path with subtly different coverage at curve
//! edges, which drifts per-pixel values at the anti-aliased border by up to
//! ~30/255 on small shapes. To match pixlet exactly, curved primitives go
//! through this module which drives `ab_glyph_rasterizer` (same underlying
//! algorithm as FreeType, just ported to Rust) and blits the resulting
//! coverage alpha onto the pixmap directly.

use ab_glyph_rasterizer::{point, Point, Rasterizer};
use tiny_skia::{Color, Pixmap};

use super::Rect;

/// Filled-path builder that accumulates straight and quadratic bezier
/// segments, then rasterizes them into per-pixel coverage values using the
/// freetype-compatible `ab_glyph_rasterizer` crate.
pub struct FtPath {
    segments: Vec<Segment>,
    first: Option<Point>,
    current: Option<Point>,
}

enum Segment {
    Line(Point, Point),
    Quad(Point, Point, Point),
}

impl FtPath {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            first: None,
            current: None,
        }
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        let p = point(x, y);
        if self.first.is_none() {
            self.first = Some(p);
        }
        self.current = Some(p);
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        if let Some(start) = self.current {
            let p = point(x, y);
            self.segments.push(Segment::Line(start, p));
            self.current = Some(p);
            if self.first.is_none() {
                self.first = Some(start);
            }
        }
    }

    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if let Some(start) = self.current {
            let c = point(cx, cy);
            let p = point(x, y);
            self.segments.push(Segment::Quad(start, c, p));
            self.current = Some(p);
            if self.first.is_none() {
                self.first = Some(start);
            }
        }
    }

    pub fn close(&mut self) {
        if let (Some(first), Some(current)) = (self.first, self.current) {
            if first != current {
                self.segments.push(Segment::Line(current, first));
            }
        }
    }
}

/// Fill the path into the pixmap using freetype-style coverage AA. Only the
/// pixels inside `bounds` are affected; samples outside are clipped.
pub fn fill_path(pixmap: &mut Pixmap, bounds: Rect, path: &FtPath, color: Color) {
    if bounds.is_empty() {
        return;
    }
    let w = bounds.width as usize;
    let h = bounds.height as usize;
    if w == 0 || h == 0 {
        return;
    }

    let mut rasterizer = Rasterizer::new(w, h);
    let offset_x = bounds.x as f32;
    let offset_y = bounds.y as f32;
    let offset = |p: Point| point(p.x - offset_x, p.y - offset_y);

    for seg in &path.segments {
        match *seg {
            Segment::Line(p0, p1) => {
                rasterizer.draw_line(offset(p0), offset(p1));
            }
            Segment::Quad(p0, c, p1) => {
                rasterizer.draw_quad(offset(p0), offset(c), offset(p1));
            }
        }
    }

    // Extract straight RGB and premultiply with the computed coverage alpha
    // before blending over the destination pixmap.
    let rgba = color.to_color_u8();
    let cr = rgba.red() as f32;
    let cg = rgba.green() as f32;
    let cb = rgba.blue() as f32;
    let ca = rgba.alpha() as f32 / 255.0;

    let pixmap_w = pixmap.width() as i32;
    let pixmap_h = pixmap.height() as i32;
    let data = pixmap.data_mut();

    rasterizer.for_each_pixel_2d(|lx, ly, coverage| {
        if coverage <= 0.0 {
            return;
        }
        let dx = bounds.x + lx as i32;
        let dy = bounds.y + ly as i32;
        if dx < 0 || dy < 0 || dx >= pixmap_w || dy >= pixmap_h {
            return;
        }
        let idx = ((dy * pixmap_w + dx) * 4) as usize;
        let a = (coverage.min(1.0) * ca * 255.0).round();
        let inv = 1.0 - a / 255.0;
        // SourceOver: dest = src + dest * (1 - src.a). Source is straight RGB
        // from the fill color, premultiplied here by the coverage alpha.
        let src_r = cr * (a / 255.0);
        let src_g = cg * (a / 255.0);
        let src_b = cb * (a / 255.0);
        data[idx] = (src_r + data[idx] as f32 * inv).round() as u8;
        data[idx + 1] = (src_g + data[idx + 1] as f32 * inv).round() as u8;
        data[idx + 2] = (src_b + data[idx + 2] as f32 * inv).round() as u8;
        data[idx + 3] = (a + data[idx + 3] as f32 * inv).round() as u8;
    });
}
