use super::{Rect, Widget, max_frame_count};
use tiny_skia::Pixmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainAlign {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl MainAlign {
    pub fn from_str(s: &str) -> Self {
        match s {
            "end" => Self::End,
            "center" => Self::Center,
            "space_between" => Self::SpaceBetween,
            "space_around" => Self::SpaceAround,
            "space_evenly" => Self::SpaceEvenly,
            _ => Self::Start,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossAlign {
    #[default]
    Start,
    Center,
    End,
}

impl CrossAlign {
    pub fn from_str(s: &str) -> Self {
        match s {
            "center" => Self::Center,
            "end" => Self::End,
            _ => Self::Start,
        }
    }
}

pub struct Vector {
    pub children: Vec<Box<dyn Widget>>,
    pub main_align: MainAlign,
    pub cross_align: CrossAlign,
    pub expanded: bool,
    pub vertical: bool,
}

/// Measure children along the vector axis, collecting their paint bounds.
/// Returns (child_bounds, sum_w, sum_h, max_w, max_h).
fn measure_children(
    children: &[Box<dyn Widget>],
    bounds_w: i32,
    bounds_h: i32,
    dx: i32,
    dy: i32,
    frame_idx: i32,
) -> (Vec<Rect>, i32, i32, i32, i32) {
    let mut max_w = 0;
    let mut max_h = 0;
    let mut sum_w = 0;
    let mut sum_h = 0;
    let mut child_bounds = Vec::with_capacity(children.len());

    for child in children {
        let cb = child.paint_bounds(
            Rect::new(0, 0, bounds_w - dx * sum_w, bounds_h - dy * sum_h),
            frame_idx,
        );

        let im_w = cb.width;
        let im_h = cb.height;

        sum_w += im_w;
        max_w = max_w.max(im_w);
        sum_h += im_h;
        max_h = max_h.max(im_h);

        child_bounds.push(cb);

        if sum_w * dx >= bounds_w || sum_h * dy >= bounds_h {
            break;
        }
    }

    (child_bounds, sum_w, sum_h, max_w, max_h)
}

/// Compute final vector dimensions.
fn compute_dimensions(
    sum_w: i32, sum_h: i32,
    max_w: i32, max_h: i32,
    bounds_w: i32, bounds_h: i32,
    dx: i32, dy: i32,
    expanded: bool,
) -> (i32, i32) {
    let mut width = dx * sum_w + dy * max_w;
    let mut height = dx * max_h + dy * sum_h;

    if expanded {
        width = dx * bounds_w + dy * max_w;
        height = dx * max_h + dy * bounds_h;
    }

    width = width.min(bounds_w);
    height = height.min(bounds_h);

    (width, height)
}

impl Widget for Vector {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        let (dx, dy) = if self.vertical { (0, 1) } else { (1, 0) };
        let (_, sum_w, sum_h, max_w, max_h) =
            measure_children(&self.children, bounds.width, bounds.height, dx, dy, frame_idx);
        let (width, height) =
            compute_dimensions(sum_w, sum_h, max_w, max_h, bounds.width, bounds.height, dx, dy, self.expanded);
        Rect::new(0, 0, width, height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let (dx, dy) = if self.vertical { (0, 1) } else { (1, 0) };
        let bounds_w = bounds.width;
        let bounds_h = bounds.height;

        let (child_bounds_vec, sum_w, sum_h, _max_w, _max_h) =
            measure_children(&self.children, bounds_w, bounds_h, dx, dy, frame_idx);

        let (_, _, _, max_w, max_h) =
            measure_children(&self.children, bounds_w, bounds_h, dx, dy, frame_idx);

        let (width, height) =
            compute_dimensions(sum_w, sum_h, max_w, max_h, bounds_w, bounds_h, dx, dy, self.expanded);

        let remaining = (dx * (width - sum_w) + dy * (height - sum_h)).max(0);

        let n = child_bounds_vec.len() as i32;
        let (mut offset, spacing, mut spacing_residual) = match self.main_align {
            MainAlign::Start => (0, 0, 0),
            MainAlign::End => (remaining, 0, 0),
            MainAlign::Center => (remaining / 2, 0, 0),
            MainAlign::SpaceEvenly => {
                let s = remaining / (n + 1);
                let r = remaining % (n + 1);
                (s, s, r)
            }
            MainAlign::SpaceAround => {
                let s = remaining / n;
                let r = remaining % n;
                (s / 2, s, r)
            }
            MainAlign::SpaceBetween => {
                if n > 1 {
                    let s = remaining / (n - 1);
                    let r = remaining % (n - 1);
                    if r > 0 {
                        (-1, s, r + 1)
                    } else {
                        (0, s, 0)
                    }
                } else {
                    (0, 0, 0)
                }
            }
        };

        let mut paint_sum_w = 0;
        let mut paint_sum_h = 0;

        for (i, cb) in child_bounds_vec.iter().enumerate() {
            let im_w = cb.width;
            let im_h = cb.height;
            let child = &self.children[i];

            if spacing_residual > 0 {
                offset += 1;
                spacing_residual -= 1;
            }

            let cross_offset = match self.cross_align {
                CrossAlign::Start => 0,
                CrossAlign::Center => (dx * (height - im_h) + dy * (width - im_w)) / 2,
                CrossAlign::End => dx * (height - im_h) + dy * (width - im_w),
            };

            let child_x = bounds.x + dx * offset + dy * cross_offset;
            let child_y = bounds.y + dx * cross_offset + dy * offset;

            let child_paint_bounds = Rect::new(
                child_x,
                child_y,
                bounds_w - dx * paint_sum_w,
                bounds_h - dy * paint_sum_h,
            );
            child.paint(pixmap, child_paint_bounds, frame_idx);

            paint_sum_w += im_w;
            paint_sum_h += im_h;

            offset += dx * im_w + dy * im_h + spacing;

            if offset >= dx * bounds_w + dy * bounds_h {
                break;
            }
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
    use tiny_skia::Color;

    fn sized_box(w: i32, h: i32) -> Box<dyn Widget> {
        Box::new(BoxWidget {
            width: w,
            height: h,
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            ..BoxWidget::new()
        })
    }

    #[test]
    fn horizontal_start() {
        let v = Vector {
            children: vec![sized_box(10, 5), sized_box(8, 3)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            expanded: false,
            vertical: false,
        };
        let pb = v.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 18); // 10 + 8
        assert_eq!(pb.height, 5); // max
    }

    #[test]
    fn vertical_start() {
        let v = Vector {
            children: vec![sized_box(10, 5), sized_box(8, 3)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            expanded: false,
            vertical: true,
        };
        let pb = v.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 10); // max
        assert_eq!(pb.height, 8); // 5 + 3
    }

    #[test]
    fn expanded_horizontal() {
        let v = Vector {
            children: vec![sized_box(10, 5)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            expanded: true,
            vertical: false,
        };
        let pb = v.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 64);
        assert_eq!(pb.height, 5);
    }

    #[test]
    fn expanded_vertical() {
        let v = Vector {
            children: vec![sized_box(10, 5)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            expanded: true,
            vertical: true,
        };
        let pb = v.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 10);
        assert_eq!(pb.height, 32);
    }

    #[test]
    fn end_alignment_horizontal() {
        let v = Vector {
            children: vec![sized_box(4, 4)],
            main_align: MainAlign::End,
            cross_align: CrossAlign::Start,
            expanded: true,
            vertical: false,
        };
        // Should paint the 4x4 box at x=60 in a 64-wide area
        let mut pixmap = Pixmap::new(64, 32).unwrap();
        v.paint(&mut pixmap, Rect::new(0, 0, 64, 32), 0);
        // pixel at (60,0) should be red, pixel at (59,0) should not
        let p60 = pixmap.pixels()[(0 * 64 + 60) as usize];
        let p59 = pixmap.pixels()[(0 * 64 + 59) as usize];
        assert_eq!(p60.red(), 255);
        assert_eq!(p59.alpha(), 0);
    }

    #[test]
    fn center_alignment_horizontal() {
        let v = Vector {
            children: vec![sized_box(4, 4)],
            main_align: MainAlign::Center,
            cross_align: CrossAlign::Start,
            expanded: true,
            vertical: false,
        };
        let mut pixmap = Pixmap::new(64, 32).unwrap();
        v.paint(&mut pixmap, Rect::new(0, 0, 64, 32), 0);
        // child should be at x=30 (remaining=60, offset=30)
        let p30 = pixmap.pixels()[(0 * 64 + 30) as usize];
        let p29 = pixmap.pixels()[(0 * 64 + 29) as usize];
        assert_eq!(p30.red(), 255);
        assert_eq!(p29.alpha(), 0);
    }

    #[test]
    fn cross_align_center() {
        // Cross axis = max of children heights. With a 4x2 and 4x6 child,
        // cross size = 6. The 4x2 child should be offset by (6-2)/2 = 2.
        let v = Vector {
            children: vec![sized_box(4, 2), sized_box(4, 6)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Center,
            expanded: false,
            vertical: false,
        };
        let mut pixmap = Pixmap::new(64, 32).unwrap();
        v.paint(&mut pixmap, Rect::new(0, 0, 64, 32), 0);
        // First child (4x2) at cross offset 2 → row 2, col 0 should be red
        let p_inside = pixmap.pixels()[(2 * 64 + 0) as usize];
        assert_eq!(p_inside.red(), 255);
        // Row 0 should be empty (offset not reached yet)
        let p_outside = pixmap.pixels()[(0 * 64 + 0) as usize];
        assert_eq!(p_outside.alpha(), 0);
    }

    #[test]
    fn cross_align_end() {
        // With 4x2 and 4x6, cross size = 6. The 4x2 child at end → offset 4.
        let v = Vector {
            children: vec![sized_box(4, 2), sized_box(4, 6)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::End,
            expanded: false,
            vertical: false,
        };
        let mut pixmap = Pixmap::new(64, 32).unwrap();
        v.paint(&mut pixmap, Rect::new(0, 0, 64, 32), 0);
        // First child at cross offset 6-2=4, so row 4 col 0 should be red
        let p_inside = pixmap.pixels()[(4 * 64 + 0) as usize];
        assert_eq!(p_inside.red(), 255);
        // Row 3 col 0 should be empty
        let p_outside = pixmap.pixels()[(3 * 64 + 0) as usize];
        assert_eq!(p_outside.alpha(), 0);
    }

    #[test]
    fn overflow_clips() {
        let v = Vector {
            children: vec![sized_box(40, 5), sized_box(40, 5)],
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
            expanded: false,
            vertical: false,
        };
        let pb = v.paint_bounds(Rect::new(0, 0, 64, 32), 0);
        assert_eq!(pb.width, 64); // clamped to bounds
    }
}
