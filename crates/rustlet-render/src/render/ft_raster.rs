//! Direct port of `github.com/golang/freetype/raster` to Rust.
//!
//! This is the same anti-aliasing area-coverage rasterizer pixlet's `gg`
//! backend drives via `freetype/raster.Rasterizer`. Porting it directly gives
//! us pixel-identical AA output with pixlet for curved shapes — prior
//! attempts using `ab_glyph_rasterizer` came close but drifted by ~20/255 at
//! curve edges because of different Bezier decomposition and scanline cell
//! accumulation details.
//!
//! Fixed-point 26.6 arithmetic uses `i32` throughout (no `fixed` crate) and
//! the rasterizer stores cells in a plain `Vec<Cell>` linked list. No
//! `unsafe`, no FFI.
//!
//! Algorithm reference:
//! <http://projects.tuxee.net/cl-vectors/section-the-cl-aa-algorithm>

use tiny_skia::{Color, Pixmap};

use super::Rect;

/// Fixed-point 26.6 value. 64 units per pixel.
pub type Int26_6 = i32;

/// 2D point in 26.6 fixed-point coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point26_6 {
    pub x: Int26_6,
    pub y: Int26_6,
}

impl Point26_6 {
    pub fn new(x: Int26_6, y: Int26_6) -> Self {
        Self { x, y }
    }
    pub fn from_f32(x: f32, y: f32) -> Self {
        Self {
            x: (x * 64.0).round() as Int26_6,
            y: (y * 64.0).round() as Int26_6,
        }
    }
}

/// A cell in the linked list of per-scanline accumulated area/coverage.
#[derive(Clone, Copy, Debug, Default)]
struct Cell {
    xi: i32,
    area: i32,
    cover: i32,
    next: i32,
}

/// Rasterizer: accumulates signed area and coverage for each cell and emits
/// horizontal spans via `for_each_span`. Mirrors `freetype/raster.Rasterizer`
/// one-to-one.
pub struct FtRasterizer {
    use_non_zero_winding: bool,
    width: i32,

    /// Bezier decomposition scale for quadratics, matching freetype's C
    /// heuristic. Cubics use `split_scale_3` but we don't expose cubics yet
    /// since pixlet's renderer only emits quadratics.
    split_scale_2: i32,
    #[allow(dead_code)]
    split_scale_3: i32,

    /// The current pen position.
    a: Point26_6,
    /// Current cell and its area/coverage being accumulated.
    xi: i32,
    yi: i32,
    area: i32,
    cover: i32,

    /// Saved cells.
    cells: Vec<Cell>,
    /// Linked list head per row (-1 = empty).
    cell_index: Vec<i32>,
}

impl FtRasterizer {
    pub fn new(width: i32, height: i32) -> Self {
        let (ss2, ss3) = split_scales(width, height);
        let width = width.max(0);
        let height = height.max(0);
        let mut r = Self {
            use_non_zero_winding: false,
            width,
            split_scale_2: ss2,
            split_scale_3: ss3,
            a: Point26_6::default(),
            xi: 0,
            yi: 0,
            area: 0,
            cover: 0,
            cells: Vec::new(),
            cell_index: vec![-1; height as usize],
        };
        r.clear();
        r
    }

    pub fn set_non_zero_winding(&mut self, v: bool) {
        self.use_non_zero_winding = v;
    }

    /// Cancels any previous `start`/`add_*` calls. Mirrors `Clear()`.
    pub fn clear(&mut self) {
        self.a = Point26_6::default();
        self.xi = 0;
        self.yi = 0;
        self.area = 0;
        self.cover = 0;
        self.cells.clear();
        for slot in self.cell_index.iter_mut() {
            *slot = -1;
        }
    }

    /// `findCell` from freetype/raster.go.
    fn find_cell(&mut self) -> i32 {
        if self.yi < 0 || self.yi as usize >= self.cell_index.len() {
            return -1;
        }
        let mut xi = self.xi;
        if xi < 0 {
            xi = -1;
        } else if xi > self.width {
            xi = self.width;
        }
        let mut i = self.cell_index[self.yi as usize];
        let mut prev: i32 = -1;
        while i != -1 && self.cells[i as usize].xi <= xi {
            if self.cells[i as usize].xi == xi {
                return i;
            }
            prev = i;
            i = self.cells[i as usize].next;
        }
        let c = self.cells.len() as i32;
        self.cells.push(Cell {
            xi,
            area: 0,
            cover: 0,
            next: i,
        });
        if prev == -1 {
            self.cell_index[self.yi as usize] = c;
        } else {
            self.cells[prev as usize].next = c;
        }
        c
    }

    /// `saveCell` from freetype/raster.go.
    fn save_cell(&mut self) {
        if self.area != 0 || self.cover != 0 {
            let idx = self.find_cell();
            if idx != -1 {
                let c = &mut self.cells[idx as usize];
                c.area += self.area;
                c.cover += self.cover;
            }
            self.area = 0;
            self.cover = 0;
        }
    }

    fn set_cell(&mut self, xi: i32, yi: i32) {
        if self.xi != xi || self.yi != yi {
            self.save_cell();
            self.xi = xi;
            self.yi = yi;
        }
    }

    /// `scan` from freetype/raster.go. Accumulates area/coverage for the
    /// yi'th scanline from `x0` to `x1` (in 26.6 fixed point) and vertical
    /// fractions `y0f`/`y1f` within that scanline.
    fn scan(&mut self, yi: i32, x0: Int26_6, y0f: Int26_6, x1: Int26_6, y1f: Int26_6) {
        let x0i = x0 / 64;
        let x0f = x0 - 64 * x0i;
        let x1i = x1 / 64;
        let x1f = x1 - 64 * x1i;

        if y0f == y1f {
            self.set_cell(x1i, yi);
            return;
        }
        let dx = x1 - x0;
        let dy = y1f - y0f;

        if x0i == x1i {
            self.area += (x0f + x1f) * dy;
            self.cover += dy;
            return;
        }

        let (mut p, q, edge0, edge1, xi_delta) = if dx > 0 {
            ((64 - x0f) * dy, dx, 0i32, 64i32, 1i32)
        } else {
            (x0f * dy, -dx, 64i32, 0i32, -1i32)
        };
        let mut y_delta = p / q;
        let mut y_rem = p % q;
        if y_rem < 0 {
            y_delta -= 1;
            y_rem += q;
        }
        let mut xi = x0i;
        let mut y = y0f;
        self.area += (x0f + edge1) * y_delta;
        self.cover += y_delta;
        xi += xi_delta;
        y += y_delta;
        self.set_cell(xi, yi);

        if xi != x1i {
            p = 64 * (y1f - y + y_delta);
            let mut full_delta = p / q;
            let mut full_rem = p % q;
            if full_rem < 0 {
                full_delta -= 1;
                full_rem += q;
            }
            y_rem -= q;
            while xi != x1i {
                let mut y_delta = full_delta;
                y_rem += full_rem;
                if y_rem >= 0 {
                    y_delta += 1;
                    y_rem -= q;
                }
                self.area += 64 * y_delta;
                self.cover += y_delta;
                xi += xi_delta;
                y += y_delta;
                self.set_cell(xi, yi);
            }
        }
        // Do the last cell.
        let last_delta = y1f - y;
        self.area += (edge0 + x1f) * last_delta;
        self.cover += last_delta;
    }

    /// Start a new sub-path at `a`.
    pub fn start(&mut self, a: Point26_6) {
        self.set_cell(a.x / 64, a.y / 64);
        self.a = a;
    }

    /// Add a straight line segment ending at `b`.
    pub fn add_line(&mut self, b: Point26_6) {
        let x0 = self.a.x;
        let y0 = self.a.y;
        let x1 = b.x;
        let y1 = b.y;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let y0i = y0 / 64;
        let y0f = y0 - 64 * y0i;
        let y1i = y1 / 64;
        let y1f = y1 - 64 * y1i;

        if y0i == y1i {
            // Single scanline.
            self.scan(y0i, x0, y0f, x1, y1f);
        } else if dx == 0 {
            // Vertical line.
            let (edge0, edge1, yi_delta) = if dy > 0 {
                (0i32, 64i32, 1i32)
            } else {
                (64i32, 0i32, -1i32)
            };
            let x0i = x0 / 64;
            let x0f_times_2 = (x0 - 64 * x0i) * 2;
            let mut yi = y0i;
            // First pixel.
            let mut dcover = edge1 - y0f;
            let mut darea = x0f_times_2 * dcover;
            self.area += darea;
            self.cover += dcover;
            yi += yi_delta;
            self.set_cell(x0i, yi);
            // Intermediate pixels.
            dcover = edge1 - edge0;
            darea = x0f_times_2 * dcover;
            while yi != y1i {
                self.area += darea;
                self.cover += dcover;
                yi += yi_delta;
                self.set_cell(x0i, yi);
            }
            // Last pixel.
            let dcover_last = y1f - edge0;
            let darea_last = x0f_times_2 * dcover_last;
            self.area += darea_last;
            self.cover += dcover_last;
        } else {
            // At least two scanlines.
            let (mut p, q, edge0, edge1, yi_delta) = if dy > 0 {
                ((64 - y0f) * dx, dy, 0i32, 64i32, 1i32)
            } else {
                (y0f * dx, -dy, 64i32, 0i32, -1i32)
            };
            let mut x_delta = p / q;
            let mut x_rem = p % q;
            if x_rem < 0 {
                x_delta -= 1;
                x_rem += q;
            }
            let mut x = x0;
            let mut yi = y0i;
            self.scan(yi, x, y0f, x + x_delta, edge1);
            x += x_delta;
            yi += yi_delta;
            self.set_cell(x / 64, yi);

            if yi != y1i {
                p = 64 * dx;
                let mut full_delta = p / q;
                let mut full_rem = p % q;
                if full_rem < 0 {
                    full_delta -= 1;
                    full_rem += q;
                }
                x_rem -= q;
                while yi != y1i {
                    let mut step = full_delta;
                    x_rem += full_rem;
                    if x_rem >= 0 {
                        step += 1;
                        x_rem -= q;
                    }
                    self.scan(yi, x, edge0, x + step, edge1);
                    x += step;
                    yi += yi_delta;
                    self.set_cell(x / 64, yi);
                }
            }
            self.scan(yi, x, edge0, x1, y1f);
        }
        self.a = b;
    }

    /// Add a quadratic Bezier segment ending at `c`, with control point `b`.
    /// Matches `Add2` in freetype/raster.go, including the same decomposition
    /// heuristic and two-linear-piece replacement at leaves.
    pub fn add_quad(&mut self, b: Point26_6, c: Point26_6) {
        // Calculate nSplit from the deviation of b from (a+c)/2.
        let dev_x = self.a.x - 2 * b.x + c.x;
        let dev_y = self.a.y - 2 * b.y + c.y;
        let mut dev = max_abs(dev_x, dev_y) / self.split_scale_2;
        let mut nsplit: i32 = 0;
        while dev > 0 {
            dev /= 4;
            nsplit += 1;
        }
        const MAX_NSPLIT: i32 = 16;
        if nsplit > MAX_NSPLIT {
            // Clamp; freetype panics here but we'd rather keep rendering.
            nsplit = MAX_NSPLIT;
        }

        // Recursive decomposition via explicit stacks. Matches freetype's
        // pStack layout: each level occupies 2 slots and the end point is
        // shared with the next level; the very first frame also writes the
        // start point at the top.
        let mut p_stack: Vec<Point26_6> = vec![Point26_6::default(); (2 * MAX_NSPLIT + 3) as usize];
        let mut s_stack: Vec<i32> = vec![0; (MAX_NSPLIT + 1) as usize];
        let mut i: i32 = 0;
        s_stack[0] = nsplit;
        p_stack[0] = c;
        p_stack[1] = b;
        p_stack[2] = self.a;

        while i >= 0 {
            let s = s_stack[i as usize];
            let base = 2 * i as usize;
            if s > 0 {
                let mx = p_stack[base + 1].x;
                p_stack[base + 4].x = p_stack[base + 2].x;
                p_stack[base + 3].x = (p_stack[base + 4].x + mx) / 2;
                p_stack[base + 1].x = (p_stack[base].x + mx) / 2;
                p_stack[base + 2].x = (p_stack[base + 1].x + p_stack[base + 3].x) / 2;
                let my = p_stack[base + 1].y;
                p_stack[base + 4].y = p_stack[base + 2].y;
                p_stack[base + 3].y = (p_stack[base + 4].y + my) / 2;
                p_stack[base + 1].y = (p_stack[base].y + my) / 2;
                p_stack[base + 2].y = (p_stack[base + 1].y + p_stack[base + 3].y) / 2;

                s_stack[i as usize] = s - 1;
                s_stack[(i + 1) as usize] = s - 1;
                i += 1;
            } else {
                let p0 = p_stack[base];
                let p1 = p_stack[base + 1];
                let p2 = p_stack[base + 2];
                let midx = (p0.x + 2 * p1.x + p2.x) / 4;
                let midy = (p0.y + 2 * p1.y + p2.y) / 4;
                // Match freetype ordering: mid first, then p0 (the original
                // start point of this sub-curve).
                self.add_line(Point26_6::new(midx, midy));
                self.add_line(p0);
                i -= 1;
            }
        }
    }

    /// Emit spans as `(y, x0, x1, alpha16)` and invoke the callback for each
    /// non-empty span. Matches `Rasterize` in freetype/raster.go.
    pub fn for_each_span<F: FnMut(i32, i32, i32, u16)>(&mut self, mut f: F) {
        self.save_cell();
        for yi in 0..self.cell_index.len() as i32 {
            let mut xi = 0i32;
            let mut cover: i32 = 0;
            let mut c = self.cell_index[yi as usize];
            while c != -1 {
                let cell = self.cells[c as usize];
                if cover != 0 && cell.xi > xi {
                    let alpha = self.area_to_alpha(cover * 64 * 2);
                    if alpha != 0 {
                        let xi0 = xi.max(0);
                        let xi1 = cell.xi.min(self.width);
                        if xi0 < xi1 {
                            f(yi, xi0, xi1, alpha);
                        }
                    }
                }
                cover += cell.cover;
                let alpha = self.area_to_alpha(cover * 64 * 2 - cell.area);
                xi = cell.xi + 1;
                if alpha != 0 {
                    let xi0 = cell.xi.max(0);
                    let xi1 = xi.min(self.width);
                    if xi0 < xi1 {
                        f(yi, xi0, xi1, alpha);
                    }
                }
                c = cell.next;
            }
        }
    }

    /// `areaToAlpha` from freetype/raster.go. Converts an area value into a
    /// 16-bit alpha using the winding rule. Fully-filled pixel → 0xffff.
    fn area_to_alpha(&self, area: i32) -> u16 {
        let mut a = (area + 1) >> 1;
        if a < 0 {
            a = -a;
        }
        let mut alpha = a as u32;
        if self.use_non_zero_winding {
            if alpha > 0x0fff {
                alpha = 0x0fff;
            }
        } else {
            alpha &= 0x1fff;
            if alpha > 0x1000 {
                alpha = 0x2000 - alpha;
            } else if alpha == 0x1000 {
                alpha = 0x0fff;
            }
        }
        // 12-bit → 16-bit alpha as in freetype.
        ((alpha << 4) | (alpha >> 8)) as u16
    }
}

fn max_abs(a: i32, b: i32) -> i32 {
    let aa = a.abs();
    let bb = b.abs();
    aa.max(bb)
}

/// Pick `splitScale2` and `splitScale3` using freetype's C heuristic.
fn split_scales(width: i32, height: i32) -> (i32, i32) {
    let (mut ss2, mut ss3) = (32i32, 16i32);
    if width > 24 || height > 24 {
        ss2 *= 2;
        ss3 *= 2;
        if width > 120 || height > 120 {
            ss2 *= 2;
            ss3 *= 2;
        }
    }
    (ss2, ss3)
}

/// High-level filled-path builder that defers to `FtRasterizer`. Accepts
/// straight segments and quadratic Beziers in 26.6 fixed point and stores
/// them until `fill_path` blits the path into a pixmap.
pub struct FtPath {
    segments: Vec<Segment>,
    first: Option<Point26_6>,
    current: Option<Point26_6>,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum Segment {
    MoveTo(Point26_6),
    LineTo(Point26_6),
    QuadTo(Point26_6, Point26_6),
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
        let p = Point26_6::from_f32(x, y);
        self.segments.push(Segment::MoveTo(p));
        self.first = Some(p);
        self.current = Some(p);
    }

    #[allow(dead_code)]
    pub fn line_to(&mut self, x: f32, y: f32) {
        let p = Point26_6::from_f32(x, y);
        self.segments.push(Segment::LineTo(p));
        self.current = Some(p);
    }

    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let b = Point26_6::from_f32(cx, cy);
        let p = Point26_6::from_f32(x, y);
        self.segments.push(Segment::QuadTo(b, p));
        self.current = Some(p);
    }

    #[allow(dead_code)]
    pub fn close(&mut self) {
        if let (Some(first), Some(current)) = (self.first, self.current) {
            if first != current {
                self.segments.push(Segment::LineTo(first));
                self.current = Some(first);
            }
        }
    }
}

/// Fill `path` into `pixmap` using the freetype-compatible rasterizer. Pixels
/// outside `bounds` are clipped.
pub fn fill_path(pixmap: &mut Pixmap, bounds: Rect, path: &FtPath, color: Color) {
    if bounds.is_empty() {
        return;
    }
    let w = bounds.width;
    let h = bounds.height;
    if w <= 0 || h <= 0 {
        return;
    }

    let mut rast = FtRasterizer::new(w, h);
    rast.set_non_zero_winding(true);

    // Offset path points into the rasterizer's local (0..w, 0..h) space.
    let ox = bounds.x * 64;
    let oy = bounds.y * 64;
    let offset = |p: Point26_6| Point26_6::new(p.x - ox, p.y - oy);

    for seg in &path.segments {
        match *seg {
            Segment::MoveTo(p) => rast.start(offset(p)),
            Segment::LineTo(p) => rast.add_line(offset(p)),
            Segment::QuadTo(b, p) => rast.add_quad(offset(b), offset(p)),
        }
    }

    // Port of `RGBAPainter.Paint` (draw.Over branch) from freetype/raster.
    // pixlet uses Go's image.RGBA which is premultiplied, so the fill color
    // is expanded to 16-bit premultiplied components and the blend formula
    // is `dst = dst*(m - ca*ma/m)*0x101/m>>8 + cr*ma/m>>8`, where `ma` is
    // the 16-bit span coverage emitted by the rasterizer.
    // Port of Go's `color.NRGBA.RGBA()`: expand 8-bit straight components to
    // 16-bit (`c << 8 | c`) then premultiply by alpha. Matches the values
    // pixlet's `RGBAPainter.SetColor` stores as `cr, cg, cb, ca`.
    let rgba = color.to_color_u8();
    let ca8 = rgba.alpha() as u32;
    let to16_premul = |c8: u32| -> u32 {
        let c16 = (c8 << 8) | c8;
        c16 * ca8 / 0xff
    };
    let ca = (ca8 << 8) | ca8;
    let cr = to16_premul(rgba.red() as u32);
    let cg = to16_premul(rgba.green() as u32);
    let cb = to16_premul(rgba.blue() as u32);
    const M: u32 = (1 << 16) - 1; // 65535

    let pixmap_w = pixmap.width() as i32;
    let pixmap_h = pixmap.height() as i32;
    let data = pixmap.data_mut();

    rast.for_each_span(|y, x0, x1, alpha16| {
        if alpha16 == 0 {
            return;
        }
        let ma = alpha16 as u32;
        let dy = bounds.y + y;
        if dy < 0 || dy >= pixmap_h {
            return;
        }
        let a = (M - (ca * ma / M)) * 0x101;
        for dx_local in x0..x1 {
            let dx = bounds.x + dx_local;
            if dx < 0 || dx >= pixmap_w {
                continue;
            }
            let idx = ((dy * pixmap_w + dx) * 4) as usize;
            let dr = data[idx] as u32;
            let dg = data[idx + 1] as u32;
            let db = data[idx + 2] as u32;
            let da = data[idx + 3] as u32;
            data[idx] = (((dr * a + cr * ma) / M) >> 8) as u8;
            data[idx + 1] = (((dg * a + cg * ma) / M) >> 8) as u8;
            data[idx + 2] = (((db * a + cb * ma) / M) >> 8) as u8;
            data[idx + 3] = (((da * a + ca * ma) / M) >> 8) as u8;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    #[test]
    fn rasterize_small_square() {
        // A 2x2 axis-aligned filled square at integer coordinates should
        // produce two fully-covered spans.
        let mut path = FtPath::new();
        path.move_to(0.0, 0.0);
        path.line_to(2.0, 0.0);
        path.line_to(2.0, 2.0);
        path.line_to(0.0, 2.0);
        path.close();

        let mut pixmap = Pixmap::new(4, 4).unwrap();
        fill_path(
            &mut pixmap,
            Rect::new(0, 0, 4, 4),
            &path,
            Color::from_rgba8(255, 0, 0, 255),
        );
        // The 2x2 top-left square should be fully red.
        assert_eq!(pixmap.pixels()[0].red(), 255);
        assert_eq!(pixmap.pixels()[1].red(), 255);
        assert_eq!(pixmap.pixels()[4].red(), 255);
        assert_eq!(pixmap.pixels()[5].red(), 255);
        // The 2x2 bottom-right square should be black (unfilled).
        assert_eq!(pixmap.pixels()[10].red(), 0);
    }

    #[test]
    fn fractional_square_is_partially_covered() {
        let mut path = FtPath::new();
        // Square from (0.5, 0.5) to (1.5, 1.5) — overlaps 4 pixels by 25% each.
        path.move_to(0.5, 0.5);
        path.line_to(1.5, 0.5);
        path.line_to(1.5, 1.5);
        path.line_to(0.5, 1.5);
        path.close();

        let mut pixmap = Pixmap::new(2, 2).unwrap();
        fill_path(
            &mut pixmap,
            Rect::new(0, 0, 2, 2),
            &path,
            Color::from_rgba8(255, 0, 0, 255),
        );
        // Each of the 4 pixels should get roughly 25% red coverage (~63/255).
        for px in pixmap.pixels() {
            let r = px.red();
            assert!(r > 50 && r < 80, "expected ~64 got {r}");
        }
    }
}
