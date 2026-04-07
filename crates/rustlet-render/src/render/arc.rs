use super::{Rect, Widget};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

const ARC_SEGMENTS: i32 = 32;

pub struct Arc {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub start_angle: f32,
    pub end_angle: f32,
    pub color: Color,
    pub width: f32,
}

impl Arc {
    fn get_bounds(&self) -> (f32, f32, f32, f32) {
        let x1 = self.x + self.radius * self.start_angle.cos();
        let y1 = self.y + self.radius * self.start_angle.sin();
        let x2 = self.x + self.radius * self.end_angle.cos();
        let y2 = self.y + self.radius * self.end_angle.sin();

        let mut min_x = x1.min(x2);
        let mut max_x = x1.max(x2);
        let mut min_y = y1.min(y2);
        let mut max_y = y1.max(y2);

        // Check cardinal angles to see if the arc spans them
        let norm = |angle: f32| {
            let a = angle % (2.0 * std::f32::consts::PI);
            if a < 0.0 { a + 2.0 * std::f32::consts::PI } else { a }
        };

        let norm_start = norm(self.start_angle);
        let norm_end = norm(self.end_angle);

        let cardinals = [
            0.0_f32,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            3.0 * std::f32::consts::FRAC_PI_2,
        ];

        for &angle in &cardinals {
            let in_arc = if norm_start <= norm_end {
                angle >= norm_start && angle <= norm_end
            } else {
                angle >= norm_start || angle <= norm_end
            };

            if in_arc {
                let px = self.x + self.radius * angle.cos();
                let py = self.y + self.radius * angle.sin();
                min_x = min_x.min(px);
                max_x = max_x.max(px);
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
        }

        let half_w = self.width / 2.0;
        (min_x - half_w, max_x + half_w, min_y - half_w, max_y + half_w)
    }

    fn build_path(&self, offset_x: f32, offset_y: f32) -> Option<tiny_skia::Path> {
        let sweep = self.end_angle - self.start_angle;
        let steps = ((sweep.abs() * self.radius.max(1.0) * 2.0).ceil() as i32).max(ARC_SEGMENTS);
        let mut pb = PathBuilder::new();

        let start_x = self.x + self.radius * self.start_angle.cos() + offset_x;
        let start_y = self.y + self.radius * self.start_angle.sin() + offset_y;
        pb.move_to(start_x, start_y);

        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let angle = self.start_angle + sweep * t;
            let px = self.x + self.radius * angle.cos() + offset_x;
            let py = self.y + self.radius * angle.sin() + offset_y;
            pb.line_to(px, py);
        }

        pb.finish()
    }
}

impl Widget for Arc {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        let (min_x, max_x, min_y, max_y) = self.get_bounds();
        Rect::new(
            0,
            0,
            (max_x - min_x).ceil() as i32,
            (max_y - min_y).ceil() as i32,
        )
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        let (min_x, _, min_y, _) = self.get_bounds();
        let offset_x = bounds.x as f32 - min_x;
        let offset_y = bounds.y as f32 - min_y;

        if let Some(path) = self.build_path(offset_x, offset_y) {
            let mut paint = Paint::default();
            paint.set_color(self.color);
            paint.anti_alias = false;

            let mut stroke = Stroke::default();
            stroke.width = self.width;

            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_bounds_covers_arc() {
        let arc = Arc {
            x: 10.0,
            y: 10.0,
            radius: 10.0,
            start_angle: 0.0,
            end_angle: std::f32::consts::PI * 1.5,
            color: Color::from_rgba8(0, 255, 255, 255),
            width: 3.0,
        };
        let pb = arc.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert!(pb.width > 0);
        assert!(pb.height > 0);
    }

    #[test]
    fn frame_count_is_one() {
        let arc = Arc {
            x: 5.0,
            y: 5.0,
            radius: 5.0,
            start_angle: 0.0,
            end_angle: std::f32::consts::PI,
            color: Color::from_rgba8(255, 0, 0, 255),
            width: 1.0,
        };
        assert_eq!(arc.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn paint_does_not_panic() {
        let arc = Arc {
            x: 10.0,
            y: 10.0,
            radius: 8.0,
            start_angle: 0.0,
            end_angle: std::f32::consts::PI,
            color: Color::from_rgba8(255, 255, 0, 255),
            width: 2.0,
        };
        let mut pixmap = Pixmap::new(24, 24).unwrap();
        arc.paint(&mut pixmap, Rect::new(0, 0, 24, 24), 0);
    }

    #[test]
    fn arc_paints_pixels() {
        let arc = Arc {
            x: 10.0,
            y: 10.0,
            radius: 8.0,
            start_angle: 0.0,
            end_angle: 2.0 * std::f32::consts::PI,
            color: Color::from_rgba8(255, 0, 0, 255),
            width: 2.0,
        };
        let mut pixmap = Pixmap::new(24, 24).unwrap();
        arc.paint(&mut pixmap, Rect::new(0, 0, 24, 24), 0);
        let has_colored_pixel = pixmap.pixels().iter().any(|p| p.red() > 0);
        assert!(has_colored_pixel, "arc should paint at least one pixel");
    }
}
