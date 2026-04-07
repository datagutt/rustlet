use super::{Rect, Widget};
use super::vector::{CrossAlign, MainAlign, Vector};
use tiny_skia::Pixmap;

pub struct Row {
    inner: Vector,
}

impl Row {
    pub fn new(
        children: Vec<Box<dyn Widget>>,
        main_align: MainAlign,
        cross_align: CrossAlign,
        expanded: bool,
    ) -> Self {
        Self {
            inner: Vector {
                children,
                main_align,
                cross_align,
                expanded,
                vertical: false,
            },
        }
    }
}

impl Widget for Row {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.inner.paint_bounds(bounds, frame_idx)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        self.inner.paint(pixmap, bounds, frame_idx)
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        self.inner.frame_count(bounds)
    }
}
