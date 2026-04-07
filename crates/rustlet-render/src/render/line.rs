use super::{Rect, Widget};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct Line {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub color: Color,
    pub width: f32,
}

impl Line {
    fn get_bounds(&self) -> (f32, f32, f32, f32) {
        let min_x = self.x1.min(self.x2);
        let max_x = self.x1.max(self.x2);
        let min_y = self.y1.min(self.y2);
        let max_y = self.y1.max(self.y2);

        let half_w = self.width / 2.0;
        (min_x - half_w, max_x + half_w, min_y - half_w, max_y + half_w)
    }
}

impl Widget for Line {
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

        let mut pb = PathBuilder::new();
        pb.move_to(self.x1 + offset_x, self.y1 + offset_y);
        pb.line_to(self.x2 + offset_x, self.y2 + offset_y);

        if let Some(path) = pb.finish() {
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
    use tiny_skia::PremultipliedColorU8;

    fn pixel_at(pixmap: &Pixmap, x: u32, y: u32) -> PremultipliedColorU8 {
        pixmap.pixels()[(y * pixmap.width() + x) as usize]
    }

    #[test]
    fn paint_bounds_horizontal_line() {
        let line = Line {
            x1: 0.0,
            y1: 5.0,
            x2: 10.0,
            y2: 5.0,
            color: Color::from_rgba8(255, 255, 255, 255),
            width: 1.0,
        };
        let pb = line.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 11);
        assert_eq!(pb.height, 1);
    }

    #[test]
    fn paint_bounds_diagonal_line() {
        let line = Line {
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 10.0,
            color: Color::from_rgba8(255, 255, 255, 255),
            width: 1.0,
        };
        let pb = line.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 11);
        assert_eq!(pb.height, 11);
    }

    #[test]
    fn frame_count_is_one() {
        let line = Line {
            x1: 0.0,
            y1: 0.0,
            x2: 5.0,
            y2: 5.0,
            color: Color::from_rgba8(255, 0, 0, 255),
            width: 1.0,
        };
        assert_eq!(line.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn line_paints_pixels() {
        let line = Line {
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 0.0,
            color: Color::from_rgba8(255, 0, 0, 255),
            width: 1.0,
        };
        let mut pixmap = Pixmap::new(12, 2).unwrap();
        line.paint(&mut pixmap, Rect::new(0, 0, 12, 2), 0);
        let has_colored_pixel = pixmap.pixels().iter().any(|p| p.red() > 0);
        assert!(has_colored_pixel, "line should paint at least one pixel");
    }

    #[test]
    fn line_respects_bounds_offset() {
        let line = Line {
            x1: 2.0,
            y1: 2.0,
            x2: 8.0,
            y2: 2.0,
            color: Color::from_rgba8(0, 255, 0, 255),
            width: 1.0,
        };
        let mut pixmap = Pixmap::new(12, 6).unwrap();
        line.paint(&mut pixmap, Rect::new(0, 0, 12, 6), 0);
        // Corner should be empty
        let corner = pixel_at(&pixmap, 0, 0);
        assert_eq!(corner.alpha(), 0);
    }
}
