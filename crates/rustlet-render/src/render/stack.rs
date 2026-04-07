use super::{max_frame_count, Rect, Widget};
use tiny_skia::Pixmap;

pub struct Stack {
    pub children: Vec<Box<dyn Widget>>,
}

impl Widget for Stack {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        let mut width = 0;
        let mut height = 0;

        for child in &self.children {
            let cb = child.paint_bounds(bounds, frame_idx);
            width = width.max(cb.width);
            height = height.max(cb.height);
        }

        width = width.min(bounds.width);
        height = height.min(bounds.height);

        Rect::new(0, 0, width, height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        for child in &self.children {
            child.paint(pixmap, bounds, frame_idx);
        }
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        max_frame_count(&self.children, bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::{Color, PremultipliedColorU8};

    fn pixel_at(pixmap: &Pixmap, x: u32, y: u32) -> PremultipliedColorU8 {
        pixmap.pixels()[(y * pixmap.width() + x) as usize]
    }

    #[test]
    fn stack_last_child_on_top() {
        let stack = Stack {
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
        stack.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        let p = pixel_at(&pixmap, 0, 0);
        assert_eq!(p.green(), 255);
    }

    #[test]
    fn stack_paint_bounds_max_of_children() {
        let stack = Stack {
            children: vec![
                Box::new(BoxWidget {
                    width: 10,
                    height: 5,
                    ..BoxWidget::new()
                }),
                Box::new(BoxWidget {
                    width: 8,
                    height: 12,
                    ..BoxWidget::new()
                }),
            ],
        };
        let pb = stack.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 10);
        assert_eq!(pb.height, 12);
    }

    #[test]
    fn stack_bounds_clamped() {
        let stack = Stack {
            children: vec![Box::new(BoxWidget {
                width: 100,
                height: 50,
                ..BoxWidget::new()
            })],
        };
        let pb = stack.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 64);
        assert_eq!(pb.height, 32);
    }
}
