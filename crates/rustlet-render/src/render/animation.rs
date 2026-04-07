use super::{Rect, Widget, mod_int};
use tiny_skia::Pixmap;

pub struct Animation {
    pub children: Vec<Box<dyn Widget>>,
}

impl Widget for Animation {
    fn frame_count(&self, _bounds: Rect) -> i32 {
        self.children.len() as i32
    }

    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        if self.children.is_empty() {
            return Rect::default();
        }
        let idx = mod_int(frame_idx, self.children.len() as i32) as usize;
        self.children[idx].paint_bounds(bounds, frame_idx)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        if self.children.is_empty() {
            return;
        }
        let idx = mod_int(frame_idx, self.children.len() as i32) as usize;
        self.children[idx].paint(pixmap, bounds, frame_idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn animation_frame_count() {
        let a = Animation {
            children: vec![
                Box::new(BoxWidget::new()),
                Box::new(BoxWidget::new()),
                Box::new(BoxWidget::new()),
            ],
        };
        assert_eq!(a.frame_count(Rect::new(0, 0, 64, 32)), 3);
    }

    #[test]
    fn animation_cycles() {
        let a = Animation {
            children: vec![
                Box::new(BoxWidget { color: Some(Color::from_rgba8(255, 0, 0, 255)), ..BoxWidget::new() }),
                Box::new(BoxWidget { color: Some(Color::from_rgba8(0, 255, 0, 255)), ..BoxWidget::new() }),
            ],
        };

        let mut p = Pixmap::new(2, 2).unwrap();
        a.paint(&mut p, Rect::new(0, 0, 2, 2), 0);
        assert_eq!(p.pixels()[0].red(), 255);

        let mut p = Pixmap::new(2, 2).unwrap();
        a.paint(&mut p, Rect::new(0, 0, 2, 2), 1);
        assert_eq!(p.pixels()[0].green(), 255);

        // Wraps around
        let mut p = Pixmap::new(2, 2).unwrap();
        a.paint(&mut p, Rect::new(0, 0, 2, 2), 2);
        assert_eq!(p.pixels()[0].red(), 255);
    }

    #[test]
    fn animation_empty() {
        let a = Animation { children: vec![] };
        assert_eq!(a.frame_count(Rect::default()), 0);
    }
}
