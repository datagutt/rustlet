use super::{Insets, Rect, Widget};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

pub struct Padding {
    pub child: Box<dyn Widget>,
    pub pad: Insets,
    pub expanded: bool,
    pub color: Option<Color>,
}

impl Widget for Padding {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        let inner_w = bounds.width - self.pad.left - self.pad.right;
        let inner_h = bounds.height - self.pad.top - self.pad.bottom;
        let cb = self
            .child
            .paint_bounds(Rect::new(0, 0, inner_w, inner_h), frame_idx);

        let (width, height) = if self.expanded {
            (bounds.width, bounds.height)
        } else {
            (
                cb.width + self.pad.left + self.pad.right,
                cb.height + self.pad.top + self.pad.bottom,
            )
        };

        Rect::new(0, 0, width, height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let inner_w = bounds.width - self.pad.left - self.pad.right;
        let inner_h = bounds.height - self.pad.top - self.pad.bottom;
        let cb = self
            .child
            .paint_bounds(Rect::new(0, 0, inner_w, inner_h), frame_idx);

        let (width, height) = if self.expanded {
            (bounds.width, bounds.height)
        } else {
            (
                cb.width + self.pad.left + self.pad.right,
                cb.height + self.pad.top + self.pad.bottom,
            )
        };

        // Fill background
        if let Some(color) = self.color {
            if let Some(rect) = tiny_skia::Rect::from_xywh(
                bounds.x as f32,
                bounds.y as f32,
                width as f32,
                height as f32,
            ) {
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

        // Paint child offset by padding
        let child_bounds = Rect::new(
            bounds.x + self.pad.left,
            bounds.y + self.pad.top,
            inner_w,
            inner_h,
        );
        self.child.paint(pixmap, child_bounds, frame_idx);
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        self.child.frame_count(bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::PremultipliedColorU8;

    fn pixel_at(pixmap: &Pixmap, x: u32, y: u32) -> PremultipliedColorU8 {
        pixmap.pixels()[(y * pixmap.width() + x) as usize]
    }

    #[test]
    fn padding_offsets_child() {
        let child = BoxWidget {
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            width: 2,
            height: 2,
            ..BoxWidget::new()
        };
        let p = Padding {
            child: Box::new(child),
            pad: Insets::new(1, 1, 0, 0),
            expanded: false,
            color: None,
        };

        let mut pixmap = Pixmap::new(4, 4).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);

        // (0,0) should be transparent (padding area)
        assert_eq!(pixel_at(&pixmap, 0, 0).alpha(), 0);
        // (1,1) should be red (child area)
        let px = pixel_at(&pixmap, 1, 1);
        assert_eq!(px.red(), 255);
        assert_eq!(px.alpha(), 255);
    }

    #[test]
    fn padding_expanded_fills_bounds() {
        let child = BoxWidget::new();
        let p = Padding {
            child: Box::new(child),
            pad: Insets::uniform(2),
            expanded: true,
            color: None,
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 64);
        assert_eq!(pb.height, 32);
    }

    #[test]
    fn padding_shrinks_to_fit() {
        let child = BoxWidget {
            width: 4,
            height: 4,
            ..BoxWidget::new()
        };
        let p = Padding {
            child: Box::new(child),
            pad: Insets::uniform(1),
            expanded: false,
            color: None,
        };
        let pb = p.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 6); // 4 + 1 + 1
        assert_eq!(pb.height, 6);
    }

    #[test]
    fn padding_with_background_color() {
        let child = BoxWidget::new();
        let p = Padding {
            child: Box::new(child),
            pad: Insets::uniform(1),
            expanded: true,
            color: Some(Color::from_rgba8(0, 0, 255, 255)),
        };
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        p.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        let px = pixel_at(&pixmap, 0, 0);
        assert_eq!(px.blue(), 255);
    }
}
