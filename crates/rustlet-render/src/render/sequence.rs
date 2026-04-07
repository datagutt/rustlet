use super::{Rect, Widget};
use tiny_skia::Pixmap;

pub struct Sequence {
    pub children: Vec<Box<dyn Widget>>,
}

impl Widget for Sequence {
    fn frame_count(&self, bounds: Rect) -> i32 {
        self.children.iter().map(|c| c.frame_count(bounds)).sum()
    }

    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        let mut fc = 0;
        for child in &self.children {
            let child_fc = child.frame_count(bounds);
            if frame_idx < fc + child_fc {
                return child.paint_bounds(bounds, frame_idx - fc);
            }
            fc += child_fc;
        }
        Rect::default()
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let mut fc = 0;
        for child in &self.children {
            let child_fc = child.frame_count(bounds);
            if frame_idx < fc + child_fc {
                child.paint(pixmap, bounds, frame_idx - fc);
                break;
            }
            fc += child_fc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn sequence_frame_count() {
        let s = Sequence {
            children: vec![
                Box::new(BoxWidget {
                    color: Some(Color::from_rgba8(255, 0, 0, 255)),
                    ..BoxWidget::new()
                }),
                Box::new(BoxWidget {
                    color: Some(Color::from_rgba8(0, 255, 0, 255)),
                    ..BoxWidget::new()
                }),
            ],
        };
        // Each box has 1 frame
        assert_eq!(s.frame_count(Rect::new(0, 0, 64, 32)), 2);
    }

    #[test]
    fn sequence_paints_correct_child() {
        let s = Sequence {
            children: vec![
                Box::new(BoxWidget {
                    color: Some(Color::from_rgba8(255, 0, 0, 255)),
                    ..BoxWidget::new()
                }),
                Box::new(BoxWidget {
                    color: Some(Color::from_rgba8(0, 255, 0, 255)),
                    ..BoxWidget::new()
                }),
            ],
        };

        let mut pixmap = Pixmap::new(4, 4).unwrap();
        s.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        assert_eq!(pixmap.pixels()[0].red(), 255);

        let mut pixmap = Pixmap::new(4, 4).unwrap();
        s.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 1);
        assert_eq!(pixmap.pixels()[0].green(), 255);
    }
}
