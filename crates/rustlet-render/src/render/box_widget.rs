use super::{Rect, Widget};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

pub struct BoxWidget {
    pub child: Option<Box<dyn Widget>>,
    pub width: i32,
    pub height: i32,
    pub padding: i32,
    pub color: Option<Color>,
}

impl BoxWidget {
    pub fn new() -> Self {
        Self {
            child: None,
            width: 0,
            height: 0,
            padding: 0,
            color: None,
        }
    }
}

impl Widget for BoxWidget {
    fn paint_bounds(&self, bounds: Rect, _frame_idx: i32) -> Rect {
        let w = if self.width > 0 {
            self.width
        } else {
            bounds.width
        };
        let h = if self.height > 0 {
            self.height
        } else {
            bounds.height
        };
        Rect::new(0, 0, w, h)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let w = if self.width > 0 {
            self.width
        } else {
            bounds.width
        };
        let h = if self.height > 0 {
            self.height
        } else {
            bounds.height
        };

        // Fill background
        if let Some(color) = self.color {
            if let Some(rect) =
                tiny_skia::Rect::from_xywh(bounds.x as f32, bounds.y as f32, w as f32, h as f32)
            {
                let path = PathBuilder::from_rect(rect);
                let mut paint = Paint::default();
                paint.set_color(color);
                paint.anti_alias = false;
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        // Paint child centered with padding
        if let Some(child) = &self.child {
            let ch_w = w - self.padding * 2;
            let ch_h = h - self.padding * 2;

            if ch_w > 0 && ch_h > 0 {
                let child_available = Rect::new(0, 0, ch_w, ch_h);
                let cb = child.paint_bounds(child_available, frame_idx);

                // Center the child (matching Go's rounding behavior)
                let x = bounds.x + w / 2 - (0.5 * cb.width as f64) as i32;
                let y = bounds.y + h / 2 - (0.5 * cb.height as f64) as i32;

                let child_bounds = Rect::new(x, y, ch_w, ch_h);
                child.paint(pixmap, child_bounds, frame_idx);
            }
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
    fn box_fills_with_color() {
        let b = BoxWidget {
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..BoxWidget::new()
        };
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        b.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        let p = pixel_at(&pixmap, 0, 0);
        assert_eq!(p.red(), 255);
        assert_eq!(p.green(), 0);
        assert_eq!(p.blue(), 0);
    }

    #[test]
    fn box_respects_explicit_size() {
        let b = BoxWidget {
            width: 2,
            height: 2,
            color: Some(Color::from_rgba8(0, 255, 0, 255)),
            ..BoxWidget::new()
        };
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        b.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        // (0,0) should be green
        let p = pixel_at(&pixmap, 0, 0);
        assert_eq!(p.green(), 255);
        // (3,3) should be transparent/black (outside the 2x2 box)
        let p2 = pixel_at(&pixmap, 3, 3);
        assert_eq!(p2.alpha(), 0);
    }

    #[test]
    fn box_paint_bounds() {
        let b = BoxWidget {
            width: 10,
            height: 5,
            ..BoxWidget::new()
        };
        let pb = b.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 10);
        assert_eq!(pb.height, 5);
    }

    #[test]
    fn box_expands_to_bounds_when_no_size() {
        let b = BoxWidget::new();
        let pb = b.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 64);
        assert_eq!(pb.height, 32);
    }
}
