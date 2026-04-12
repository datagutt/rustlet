use super::Curve;
use crate::render::{Rect, Widget};
use tiny_skia::Pixmap;

/// Ports pixlet's `animation.AnimatedPositioned`: animate a child widget between
/// (`x_start`, `y_start`) and (`x_end`, `y_end`) over `duration` frames using the
/// supplied easing curve. `delay` and `hold` add padding frames before and after.
pub struct AnimatedPositioned {
    pub child: Box<dyn Widget>,
    pub x_start: i32,
    pub y_start: i32,
    pub x_end: i32,
    pub y_end: i32,
    pub duration: i32,
    pub curve: Curve,
    pub delay: i32,
    pub hold: i32,
}

impl Widget for AnimatedPositioned {
    fn paint_bounds(&self, bounds: Rect, _frame_idx: i32) -> Rect {
        bounds
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let position = if frame_idx < self.delay {
            0.0
        } else if self.duration <= 0 || frame_idx >= self.delay + self.duration {
            0.9999999999
        } else {
            self.curve
                .transform((frame_idx - self.delay) as f64 / self.duration as f64)
        };

        let dx = if self.x_end < self.x_start { -1 } else { 1 };
        let dy = if self.y_end < self.y_start { -1 } else { 1 };

        let sx = ((self.x_end - self.x_start).abs() as f64 * position).ceil() as i32;
        let sy = ((self.y_end - self.y_start).abs() as f64 * position).ceil() as i32;

        let x = self.x_start + dx * sx;
        let y = self.y_start + dy * sy;

        let child_bounds = Rect::new(bounds.x + x, bounds.y + y, bounds.width, bounds.height);
        self.child.paint(pixmap, child_bounds, frame_idx);
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        (self.duration + self.delay + self.hold).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;

    #[test]
    fn frame_count_sums_parts() {
        let a = AnimatedPositioned {
            child: Box::new(BoxWidget::new()),
            x_start: 0,
            y_start: 0,
            x_end: 10,
            y_end: 0,
            duration: 10,
            curve: Curve::Linear,
            delay: 2,
            hold: 3,
        };
        assert_eq!(a.frame_count(Rect::new(0, 0, 64, 32)), 15);
    }
}
