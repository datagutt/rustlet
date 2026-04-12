use super::{Rect, Widget};
use std::f32::consts::PI;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

pub struct PieChart {
    pub colors: Vec<Color>,
    pub weights: Vec<f64>,
    pub diameter: i32,
}

/// Arc approximation: number of line segments per full 2π circle.
const SEGMENTS_PER_CIRCLE: i32 = 32;

impl Widget for PieChart {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        Rect::new(0, 0, self.diameter, self.diameter)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        if self.weights.is_empty() || self.colors.is_empty() || self.diameter <= 0 {
            return;
        }

        let total: f64 = self.weights.iter().sum();
        if total <= 0.0 {
            return;
        }

        let r = self.diameter as f32 / 2.0;
        let cx = bounds.x as f32 + r;
        let cy = bounds.y as f32 + r;

        let mut start = 0.0_f64;
        for (i, &weight) in self.weights.iter().enumerate() {
            let end = start + weight / total;
            let color = self.colors[i % self.colors.len()];

            let start_angle = (start * 2.0 * std::f64::consts::PI) as f32;
            let end_angle = (end * 2.0 * std::f64::consts::PI) as f32;
            let sweep = end_angle - start_angle;

            // Number of segments proportional to arc size
            let num_segments = ((sweep.abs() / (2.0 * PI)) * SEGMENTS_PER_CIRCLE as f32)
                .ceil()
                .max(1.0) as i32;

            let mut pb = PathBuilder::new();

            // Arc points
            let first_x = cx + r * start_angle.cos();
            let first_y = cy + r * start_angle.sin();
            pb.move_to(first_x, first_y);

            for seg in 1..=num_segments {
                let t = seg as f32 / num_segments as f32;
                let angle = start_angle + sweep * t;
                pb.line_to(cx + r * angle.cos(), cy + r * angle.sin());
            }

            // Close through center
            pb.line_to(cx, cy);
            pb.line_to(first_x, first_y);
            pb.close();

            if let Some(path) = pb.finish() {
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

            start = end;
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }

    fn size(&self) -> Option<(i32, i32)> {
        Some((self.diameter, self.diameter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_bounds_returns_diameter() {
        let pc = PieChart {
            colors: vec![Color::from_rgba8(255, 0, 0, 255)],
            weights: vec![1.0],
            diameter: 30,
        };
        let pb = pc.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 30, 30));
    }

    #[test]
    fn frame_count_is_one() {
        let pc = PieChart {
            colors: vec![],
            weights: vec![],
            diameter: 10,
        };
        assert_eq!(pc.frame_count(Rect::default()), 1);
    }

    #[test]
    fn empty_weights_no_panic() {
        let pc = PieChart {
            colors: vec![],
            weights: vec![],
            diameter: 10,
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        pc.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
    }

    #[test]
    fn single_slice_fills_circle() {
        let pc = PieChart {
            colors: vec![Color::from_rgba8(255, 0, 0, 255)],
            weights: vec![1.0],
            diameter: 10,
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        pc.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        let filled = pixmap.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(filled > 0, "single slice should fill some pixels");
    }

    #[test]
    fn two_slices_paint() {
        let pc = PieChart {
            colors: vec![
                Color::from_rgba8(255, 0, 0, 255),
                Color::from_rgba8(0, 255, 0, 255),
            ],
            weights: vec![1.0, 1.0],
            diameter: 20,
        };
        let mut pixmap = Pixmap::new(20, 20).unwrap();
        pc.paint(&mut pixmap, Rect::new(0, 0, 20, 20), 0);
        let red = pixmap
            .pixels()
            .iter()
            .filter(|p| p.red() > 0 && p.green() == 0)
            .count();
        let green = pixmap
            .pixels()
            .iter()
            .filter(|p| p.green() > 0 && p.red() == 0)
            .count();
        assert!(red > 0, "should have red pixels");
        assert!(green > 0, "should have green pixels");
    }

    #[test]
    fn size_returns_diameter() {
        let pc = PieChart {
            colors: vec![],
            weights: vec![],
            diameter: 15,
        };
        assert_eq!(pc.size(), Some((15, 15)));
    }
}
