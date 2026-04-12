use super::{Rect, Widget};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct Polygon {
    pub vertices: Vec<(f64, f64)>,
    pub fill_color: Option<Color>,
    pub stroke_color: Option<Color>,
    pub stroke_width: f32,
}

impl Polygon {
    fn get_bounds(&self) -> (f64, f64, f64, f64) {
        if self.vertices.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for &(x, y) in &self.vertices {
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }

        if self.stroke_color.is_some() && self.stroke_width > 0.0 {
            let half = self.stroke_width as f64 / 2.0;
            min_x -= half;
            max_x += half;
            min_y -= half;
            max_y += half;
        }

        (min_x, max_x, min_y, max_y)
    }

    fn build_path(&self, offset_x: f32, offset_y: f32) -> Option<tiny_skia::Path> {
        if self.vertices.len() < 3 {
            return None;
        }

        let mut pb = PathBuilder::new();
        for (i, &(x, y)) in self.vertices.iter().enumerate() {
            let px = x as f32 + offset_x;
            let py = y as f32 + offset_y;
            if i == 0 {
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        pb.close();
        pb.finish()
    }
}

impl Widget for Polygon {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        if self.vertices.is_empty() {
            return Rect::new(0, 0, 0, 0);
        }
        let (min_x, max_x, min_y, max_y) = self.get_bounds();
        if min_x.is_infinite() {
            return Rect::new(0, 0, 0, 0);
        }
        Rect::new(
            0,
            0,
            (max_x - min_x).ceil() as i32,
            (max_y - min_y).ceil() as i32,
        )
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, _frame_idx: i32) {
        if self.vertices.len() < 3 {
            return;
        }

        let (min_x, _, min_y, _) = self.get_bounds();

        // Translate vertices so the polygon's bounding box origin aligns with the
        // widget's paint position. Matches the Go implementation's dc.Translate(-minX, -minY).
        let offset_x = bounds.x as f32 - min_x as f32;
        let offset_y = bounds.y as f32 - min_y as f32;

        let path = match self.build_path(offset_x, offset_y) {
            Some(p) => p,
            None => return,
        };

        if let Some(fill_color) = self.fill_color {
            let mut paint = Paint::default();
            paint.set_color(fill_color);
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        if let Some(stroke_color) = self.stroke_color {
            if self.stroke_width > 0.0 {
                let mut paint = Paint::default();
                paint.set_color(stroke_color);
                paint.anti_alias = true;

                let mut stroke = Stroke::default();
                stroke.width = self.stroke_width;
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
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
    fn paint_bounds_triangle() {
        let p = Polygon {
            vertices: vec![(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)],
            fill_color: Some(Color::from_rgba8(255, 0, 0, 255)),
            stroke_color: None,
            stroke_width: 0.0,
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 10, 10));
    }

    #[test]
    fn paint_bounds_with_stroke() {
        let p = Polygon {
            vertices: vec![(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)],
            fill_color: None,
            stroke_color: Some(Color::from_rgba8(255, 255, 255, 255)),
            stroke_width: 2.0,
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 12, 12));
    }

    #[test]
    fn paint_bounds_empty() {
        let p = Polygon {
            vertices: vec![],
            fill_color: None,
            stroke_color: None,
            stroke_width: 0.0,
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb, Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn frame_count_is_one() {
        let p = Polygon {
            vertices: vec![],
            fill_color: None,
            stroke_color: None,
            stroke_width: 0.0,
        };
        assert_eq!(p.frame_count(Rect::default()), 1);
    }

    #[test]
    fn too_few_vertices_no_panic() {
        let p = Polygon {
            vertices: vec![(0.0, 0.0), (1.0, 1.0)],
            fill_color: Some(Color::from_rgba8(255, 0, 0, 255)),
            stroke_color: None,
            stroke_width: 0.0,
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        assert!(pixmap.pixels().iter().all(|px| px.alpha() == 0));
    }

    #[test]
    fn fill_rectangle_paints() {
        let p = Polygon {
            vertices: vec![(0.0, 0.0), (9.0, 0.0), (9.0, 9.0), (0.0, 9.0)],
            fill_color: Some(Color::from_rgba8(255, 0, 0, 255)),
            stroke_color: None,
            stroke_width: 0.0,
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        let filled = pixmap.pixels().iter().filter(|px| px.alpha() > 0).count();
        assert!(filled > 0, "rectangle should have filled pixels");
    }

    #[test]
    fn stroke_only_paints() {
        let p = Polygon {
            vertices: vec![(1.0, 1.0), (8.0, 1.0), (8.0, 8.0), (1.0, 8.0)],
            fill_color: None,
            stroke_color: Some(Color::from_rgba8(0, 255, 0, 255)),
            stroke_width: 1.0,
        };
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 10, 10), 0);
        let stroked = pixmap.pixels().iter().filter(|px| px.alpha() > 0).count();
        assert!(stroked > 0, "stroke should produce visible pixels");
    }
}
