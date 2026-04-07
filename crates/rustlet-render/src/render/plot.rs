use super::{Rect, Widget};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

/// How the Plot data is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartType {
    #[default]
    Line,
    Scatter,
}

impl ChartType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "scatter" => Self::Scatter,
            _ => Self::Line,
        }
    }
}

/// Dampening factor for fill color when no explicit fill color is provided.
const FILL_DAMP_FACTOR: u8 = 0x55;

pub struct Plot {
    pub data: Vec<[f64; 2]>,
    pub width: i32,
    pub height: i32,
    pub color: Color,
    pub color_inverted: Option<Color>,
    pub x_lim: Option<[f64; 2]>,
    pub y_lim: Option<[f64; 2]>,
    pub fill: bool,
    pub fill_color: Option<Color>,
    pub fill_color_inverted: Option<Color>,
    pub chart_type: ChartType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathPoint {
    x: i32,
    y: i32,
}

impl Plot {
    fn compute_limits(&self) -> (f64, f64, f64, f64) {
        let x_lim_min_set = self.x_lim.map(|l| l[0]);
        let x_lim_max_set = self.x_lim.map(|l| l[1]);
        let y_lim_min_set = self.y_lim.map(|l| l[0]);
        let y_lim_max_set = self.y_lim.map(|l| l[1]);

        // If all limits are provided, just return them
        if let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) =
            (x_lim_min_set, x_lim_max_set, y_lim_min_set, y_lim_max_set)
        {
            return (xmin, xmax, ymin, ymax);
        }

        if self.data.is_empty() {
            return (0.0, 1.0, 0.0, 1.0);
        }

        let pt = self.data[0];
        let mut min_x = pt[0];
        let mut max_x = pt[0];
        let mut min_y = pt[1];
        let mut max_y = pt[1];
        for pt in &self.data[1..] {
            if pt[0] < min_x {
                min_x = pt[0];
            }
            if pt[0] > max_x {
                max_x = pt[0];
            }
            if pt[1] < min_y {
                min_y = pt[1];
            }
            if pt[1] > max_y {
                max_y = pt[1];
            }
        }

        let mut x_lim_min = x_lim_min_set.unwrap_or(min_x);
        let mut x_lim_max = x_lim_max_set.unwrap_or(max_x);
        let mut y_lim_min = y_lim_min_set.unwrap_or(min_y);
        let mut y_lim_max = y_lim_max_set.unwrap_or(max_y);

        // Handle nonsensical inverted limits
        if x_lim_max < x_lim_min {
            if x_lim_min_set.is_none() {
                x_lim_min = x_lim_max - 0.5;
            } else {
                x_lim_max = x_lim_min + 0.5;
            }
        }
        if y_lim_max < y_lim_min {
            if y_lim_min_set.is_none() {
                y_lim_min = y_lim_max - 0.5;
            } else {
                y_lim_max = y_lim_min + 0.5;
            }
        }

        // Handle equal min/max
        if x_lim_min == x_lim_max {
            x_lim_min = min_x;
            x_lim_max = min_x + 0.5;
        }
        if y_lim_min == y_lim_max {
            y_lim_min = min_y - 0.5;
            y_lim_max = min_y + 0.5;
        }

        (x_lim_min, x_lim_max, y_lim_min, y_lim_max)
    }

    fn translate_points(&self) -> (Vec<PathPoint>, i32) {
        let (x_min, x_max, y_min, y_max) = self.compute_limits();

        let points: Vec<PathPoint> = self
            .data
            .iter()
            .map(|pt| {
                let nx = (pt[0] - x_min) / (x_max - x_min);
                let ny = (pt[1] - y_min) / (y_max - y_min);
                PathPoint {
                    x: (nx * (self.width - 1) as f64).round() as i32,
                    y: self.height - 1 - (ny * (self.height - 1) as f64).round() as i32,
                }
            })
            .collect();

        let inv_threshold = self.height
            - 1
            - ((0.0 - y_min) / (y_max - y_min) * (self.height - 1) as f64).round() as i32;

        (points, inv_threshold)
    }
}

fn dampen_color(c: Color, a: u8) -> Color {
    let r = ((c.red() * 255.0) as u32 * a as u32 / 255) as u8;
    let g = ((c.green() * 255.0) as u32 * a as u32 / 255) as u8;
    let b = ((c.blue() * 255.0) as u32 * a as u32 / 255) as u8;
    Color::from_rgba8(r, g, b, 255)
}

fn premultiply(c: Color) -> PremultipliedColorU8 {
    let a = (c.alpha() * 255.0) as u8;
    let r = (c.red() * c.alpha() * 255.0) as u8;
    let g = (c.green() * c.alpha() * 255.0) as u8;
    let b = (c.blue() * c.alpha() * 255.0) as u8;
    PremultipliedColorU8::from_rgba(r, g, b, a).unwrap()
}

/// Bresenham line rasterization between two points. Returns all points on the line
/// segment including endpoints.
fn bresenham_line(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<PathPoint> {
    let mut result = Vec::new();

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };

    let mut cx = x0;
    let mut cy = y0;

    // Handle purely vertical
    if dx == 0 {
        loop {
            result.push(PathPoint { x: cx, y: cy });
            if cy == y1 {
                break;
            }
            cy += sy;
        }
        return result;
    }

    // Handle purely horizontal
    if dy == 0 {
        loop {
            result.push(PathPoint { x: cx, y: cy });
            if cx == x1 {
                break;
            }
            cx += sx;
        }
        return result;
    }

    let mut err = dx + dy;
    result.push(PathPoint { x: cx, y: cy });
    while cx != x1 || cy != y1 {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
        result.push(PathPoint { x: cx, y: cy });
    }

    result
}

/// Build the polyline path through all translated vertices using Bresenham segments.
fn build_polyline(vertices: &[PathPoint]) -> Vec<PathPoint> {
    let mut path = Vec::new();
    for i in 0..vertices.len().saturating_sub(1) {
        let segment = bresenham_line(
            vertices[i].x,
            vertices[i].y,
            vertices[i + 1].x,
            vertices[i + 1].y,
        );
        path.extend_from_slice(&segment);
    }
    path
}

impl Widget for Plot {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        if self.data.is_empty() {
            return;
        }

        let col = self.color;
        let col_inv = self.color_inverted.unwrap_or(col);

        let fill_col = self
            .fill_color
            .unwrap_or_else(|| dampen_color(col, FILL_DAMP_FACTOR));
        let fill_col_inv = self
            .fill_color_inverted
            .unwrap_or_else(|| dampen_color(col_inv, FILL_DAMP_FACTOR));

        let (vertices, inv_threshold) = self.translate_points();

        let dst_w = pixmap.width() as i32;
        let dst_h = pixmap.height() as i32;

        // Surface fill
        if self.fill {
            let polyline = build_polyline(&vertices);
            let premul_fill = premultiply(fill_col);
            let premul_fill_inv = premultiply(fill_col_inv);
            let pixels = pixmap.pixels_mut();

            for pt in &polyline {
                let px = bounds.x + pt.x;
                let py = bounds.y + pt.y;
                if px < 0 || px >= dst_w || py < 0 || py >= dst_h {
                    continue;
                }
                if pt.y > inv_threshold {
                    // Below baseline, fill upward to threshold
                    let mut y = pt.y;
                    while y != inv_threshold && y >= 0 {
                        let draw_y = bounds.y + y;
                        if draw_y >= 0 && draw_y < dst_h && px >= 0 && px < dst_w {
                            pixels[(draw_y * dst_w + px) as usize] = premul_fill_inv;
                        }
                        y -= 1;
                    }
                } else {
                    // Above baseline, fill downward to threshold
                    let mut y = pt.y;
                    while y <= inv_threshold && y <= self.height {
                        let draw_y = bounds.y + y;
                        if draw_y >= 0 && draw_y < dst_h && px >= 0 && px < dst_w {
                            pixels[(draw_y * dst_w + px) as usize] = premul_fill;
                        }
                        y += 1;
                    }
                }
            }
        }

        let pixels = pixmap.pixels_mut();

        match self.chart_type {
            ChartType::Scatter => {
                for pt in &vertices {
                    let premul = if pt.y > inv_threshold {
                        premultiply(col_inv)
                    } else {
                        premultiply(col)
                    };
                    let px = bounds.x + pt.x;
                    let py = bounds.y + pt.y;
                    if px >= 0 && px < dst_w && py >= 0 && py < dst_h {
                        pixels[(py * dst_w + px) as usize] = premul;
                    }
                }
            }
            ChartType::Line => {
                let polyline = build_polyline(&vertices);
                for pt in &polyline {
                    let premul = if pt.y > inv_threshold {
                        premultiply(col_inv)
                    } else {
                        premultiply(col)
                    };
                    let px = bounds.x + pt.x;
                    let py = bounds.y + pt.y;
                    if px >= 0 && px < dst_w && py >= 0 && py < dst_h {
                        pixels[(py * dst_w + px) as usize] = premul;
                    }
                }
            }
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }

    fn size(&self) -> Option<(i32, i32)> {
        Some((self.width, self.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_plot() -> Plot {
        Plot {
            data: Vec::new(),
            width: 10,
            height: 10,
            color: Color::from_rgba8(255, 255, 255, 255),
            color_inverted: None,
            x_lim: None,
            y_lim: None,
            fill: false,
            fill_color: None,
            fill_color_inverted: None,
            chart_type: ChartType::Line,
        }
    }

    #[test]
    fn compute_limits_from_data() {
        let p = Plot {
            data: vec![[3.14, 1.62], [3.56, 2.7], [3.9, 2.9]],
            ..default_plot()
        };
        let (xmin, xmax, ymin, ymax) = p.compute_limits();
        assert_eq!(xmin, 3.14);
        assert_eq!(xmax, 3.9);
        assert_eq!(ymin, 1.62);
        assert_eq!(ymax, 2.9);
    }

    #[test]
    fn compute_limits_explicit() {
        let p = Plot {
            data: vec![[3.14, 1.62], [3.9, 2.9]],
            x_lim: Some([3.0, 4.0]),
            y_lim: Some([1.0, 3.0]),
            ..default_plot()
        };
        let (xmin, xmax, ymin, ymax) = p.compute_limits();
        assert_eq!(xmin, 3.0);
        assert_eq!(xmax, 4.0);
        assert_eq!(ymin, 1.0);
        assert_eq!(ymax, 3.0);
    }

    #[test]
    fn compute_limits_empty_data() {
        let p = default_plot();
        let (xmin, xmax, ymin, ymax) = p.compute_limits();
        assert_eq!((xmin, xmax, ymin, ymax), (0.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn compute_limits_equal_y() {
        let p = Plot {
            data: vec![[3.14, 3.14], [3.56, 3.14], [3.9, 3.14]],
            ..default_plot()
        };
        let (_, _, ymin, ymax) = p.compute_limits();
        assert_eq!(ymin, 3.14 - 0.5);
        assert_eq!(ymax, 3.14 + 0.5);
    }

    #[test]
    fn compute_limits_equal_x() {
        let p = Plot {
            data: vec![[2.0, 3.14], [2.0, 3.14], [2.0, 3.14]],
            ..default_plot()
        };
        let (xmin, xmax, _, _) = p.compute_limits();
        assert_eq!(xmin, 2.0);
        assert_eq!(xmax, 2.5);
    }

    #[test]
    fn translate_linear_data() {
        let p = Plot {
            data: vec![
                [0.0, 0.0],
                [1.0, 2.0],
                [2.0, 4.0],
                [3.0, 6.0],
                [4.0, 8.0],
                [5.0, 10.0],
                [6.0, 12.0],
                [7.0, 14.0],
                [8.0, 16.0],
                [9.0, 18.0],
            ],
            ..default_plot()
        };
        let (points, inv_threshold) = p.translate_points();
        assert_eq!(points.len(), 10);
        assert_eq!(points[0], PathPoint { x: 0, y: 9 });
        assert_eq!(points[9], PathPoint { x: 9, y: 0 });
        assert_eq!(inv_threshold, 9);
    }

    #[test]
    fn paint_bounds_correct() {
        let p = Plot {
            width: 20,
            height: 10,
            ..default_plot()
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 20, 10));
    }

    #[test]
    fn frame_count_is_one() {
        let p = default_plot();
        assert_eq!(p.frame_count(Rect::default()), 1);
    }

    #[test]
    fn empty_plot_no_panic() {
        let p = default_plot();
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        // All pixels should be transparent
        assert!(pixmap.pixels().iter().all(|px| px.alpha() == 0));
    }

    #[test]
    fn flat_line_paints() {
        let p = Plot {
            data: vec![[0.0, 47.0], [9.0, 47.0]],
            width: 10,
            height: 5,
            ..default_plot()
        };
        let mut pixmap = Pixmap::new(10, 5).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 5), 0);
        // Middle row (y=2) should have all white pixels
        for x in 0..10 {
            let px = pixmap.pixels()[2 * 10 + x];
            assert!(px.alpha() > 0, "pixel at ({x}, 2) should be visible");
        }
    }

    #[test]
    fn scatter_only_data_points() {
        let p = Plot {
            data: vec![[0.0, 47.0], [9.0, 47.0]],
            width: 10,
            height: 5,
            chart_type: ChartType::Scatter,
            ..default_plot()
        };
        let mut pixmap = Pixmap::new(10, 5).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 5), 0);
        let visible_count = pixmap.pixels().iter().filter(|px| px.alpha() > 0).count();
        assert_eq!(visible_count, 2);
    }

    #[test]
    fn chart_type_from_str() {
        assert_eq!(ChartType::from_str("scatter"), ChartType::Scatter);
        assert_eq!(ChartType::from_str("line"), ChartType::Line);
        assert_eq!(ChartType::from_str(""), ChartType::Line);
    }
}
