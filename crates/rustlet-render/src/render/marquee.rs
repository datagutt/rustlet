use super::{Rect, Widget};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollDirection {
    #[default]
    Horizontal,
    Vertical,
}

impl ScrollDirection {
    pub fn from_str(s: &str) -> Self {
        if s == "vertical" {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarqueeAlign {
    #[default]
    Start,
    Center,
    End,
}

impl MarqueeAlign {
    pub fn from_str(s: &str) -> Self {
        match s {
            "center" => Self::Center,
            "end" => Self::End,
            _ => Self::Start,
        }
    }
}

pub struct Marquee {
    pub child: Box<dyn Widget>,
    pub width: i32,
    pub height: i32,
    pub offset_start: i32,
    pub offset_end: i32,
    pub scroll_direction: ScrollDirection,
    pub align: MarqueeAlign,
    pub delay: i32,
}

impl Marquee {
    fn is_vertical(&self) -> bool {
        self.scroll_direction == ScrollDirection::Vertical
    }

    /// Measure child in a generous bounding box (10x the marquee size along scroll axis).
    fn child_bounds_and_metrics(&self, bounds: Rect) -> (Rect, i32, i32) {
        let (cb, cw, size) = if self.is_vertical() {
            let cb = self
                .child
                .paint_bounds(Rect::new(0, 0, bounds.width, self.height * 10), 0);
            (cb, cb.height, self.height)
        } else {
            let cb = self
                .child
                .paint_bounds(Rect::new(0, 0, self.width * 10, bounds.height), 0);
            (cb, cb.width, self.width)
        };
        (cb, cw, size)
    }
}

impl Widget for Marquee {
    fn paint_bounds(&self, bounds: Rect, _frame_idx: i32) -> Rect {
        let (cb, _, _) = self.child_bounds_and_metrics(bounds);
        if self.is_vertical() {
            Rect::new(0, 0, cb.width, self.height)
        } else {
            Rect::new(0, 0, self.width, cb.height)
        }
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        let (_, cw, size) = self.child_bounds_and_metrics(bounds);

        if cw <= size {
            return 1;
        }

        let offstart = self.offset_start.max(-cw);
        let offend = self.offset_end.max(-cw);
        let delay = self.delay;

        if offstart == offend {
            cw + offstart + size - offend + delay
        } else {
            cw + offstart + size - offend + 1 + delay
        }
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let (cb, cw, size) = self.child_bounds_and_metrics(bounds);

        let offstart = self.offset_start.max(-cw);
        let offend = self.offset_end.max(-cw);
        let delay = self.delay;
        let loop_idx = cw + offstart + delay;
        let end_idx = cw + offstart + size - offend + delay;

        let mut align_f = 0.0_f64;
        let offset;

        if cw <= size {
            // Child fits, no scrolling
            let mut off = 0;
            match self.align {
                MarqueeAlign::Start => {}
                MarqueeAlign::Center => {
                    align_f = 0.5;
                    off = size / 2;
                }
                MarqueeAlign::End => {
                    align_f = 1.0;
                    off = size;
                }
            }
            offset = off;
        } else if frame_idx <= delay {
            offset = offstart;
        } else if frame_idx <= loop_idx {
            offset = offstart - frame_idx + delay;
        } else if frame_idx <= end_idx {
            offset = offend + (end_idx - frame_idx);
        } else {
            offset = offend;
        };

        let pb = self.paint_bounds(bounds, frame_idx);
        if pb.width <= 0 || pb.height <= 0 {
            return;
        }

        let mut clipped =
            Pixmap::new(pb.width as u32, pb.height as u32).expect("marquee viewport must be valid");

        if self.is_vertical() {
            let final_offset = offset - (align_f * cb.height as f64) as i32;
            let child_bounds = Rect::new(0, final_offset, pb.width, self.height * 10);
            self.child.paint(&mut clipped, child_bounds, 0);
        } else {
            let final_offset = offset - (align_f * cb.width as f64) as i32;
            let child_bounds = Rect::new(final_offset, 0, self.width * 10, pb.height);
            self.child.paint(&mut clipped, child_bounds, 0);
        }

        pixmap.draw_pixmap(
            bounds.x,
            bounds.y,
            clipped.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use crate::render::text::Text;
    use tiny_skia::Color;

    #[test]
    fn no_scroll_when_fits() {
        let m = Marquee {
            child: Box::new(BoxWidget {
                width: 10,
                height: 8,
                ..BoxWidget::new()
            }),
            width: 64,
            height: 0,
            offset_start: 0,
            offset_end: 0,
            scroll_direction: ScrollDirection::Horizontal,
            align: MarqueeAlign::Start,
            delay: 0,
        };
        assert_eq!(m.frame_count(Rect::new(0, 0, 64, 32)), 1);
    }

    #[test]
    fn scrolls_when_child_wider() {
        let m = Marquee {
            child: Box::new(Text::new("this is a long text that won't fit")),
            width: 20,
            height: 0,
            offset_start: 0,
            offset_end: 0,
            scroll_direction: ScrollDirection::Horizontal,
            align: MarqueeAlign::Start,
            delay: 0,
        };
        let fc = m.frame_count(Rect::new(0, 0, 64, 32));
        assert!(fc > 1, "expected scrolling, got frame_count={fc}");
    }

    #[test]
    fn delay_phase() {
        let m = Marquee {
            child: Box::new(Text::new("this is a long text that won't fit")),
            width: 20,
            height: 0,
            offset_start: 5,
            offset_end: 0,
            scroll_direction: ScrollDirection::Horizontal,
            align: MarqueeAlign::Start,
            delay: 10,
        };
        let fc = m.frame_count(Rect::new(0, 0, 64, 32));
        assert!(fc > 10, "frame count should include delay");
    }

    #[test]
    fn vertical_mode() {
        let m = Marquee {
            child: Box::new(BoxWidget {
                width: 10,
                height: 50,
                color: Some(Color::from_rgba8(255, 0, 0, 255)),
                ..BoxWidget::new()
            }),
            width: 0,
            height: 10,
            offset_start: 0,
            offset_end: 0,
            scroll_direction: ScrollDirection::Vertical,
            align: MarqueeAlign::Start,
            delay: 0,
        };
        let fc = m.frame_count(Rect::new(0, 0, 64, 32));
        assert!(fc > 1, "vertical should scroll, got frame_count={fc}");
    }
}
