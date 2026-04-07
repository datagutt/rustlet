mod root;

pub mod animation;
pub mod arc;
pub mod box_widget;
pub mod circle;
pub mod column;
pub mod emoji;
pub mod image_widget;
pub mod line;
pub mod marquee;
pub mod padding;
pub mod pie_chart;
pub mod plot;
pub mod polygon;
pub mod row;
pub mod sequence;
pub mod stack;
pub mod starfield;
pub mod text;
pub mod vector;
pub mod wrapped_text;

pub use root::Root;

/// Axis-aligned rectangle used for layout bounds and paint regions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// Shrink the rect by the given insets.
    pub fn inset(&self, insets: Insets) -> Self {
        Self {
            x: self.x + insets.left,
            y: self.y + insets.top,
            width: self.width - insets.left - insets.right,
            height: self.height - insets.top - insets.bottom,
        }
    }

    /// Return the intersection of two rects, or an empty rect if they don't overlap.
    pub fn intersection(&self, other: &Rect) -> Self {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        if x2 > x1 && y2 > y1 {
            Self::new(x1, y1, x2 - x1, y2 - y1)
        } else {
            Self::default()
        }
    }
}

/// Insets for padding around a widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn uniform(value: i32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

/// Core rendering trait matching pixlet's Widget interface.
///
/// All widgets implement this to participate in the layout and paint pipeline.
/// `frame_idx` indexes into the widget's animation frames.
pub trait Widget: Send + Sync {
    /// Returns the rectangle this widget will actually paint into,
    /// given parent `bounds` and the current `frame_idx`.
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect;

    /// Render this widget onto `pixmap` within `bounds` at `frame_idx`.
    fn paint(&self, pixmap: &mut tiny_skia::Pixmap, bounds: Rect, frame_idx: i32);

    /// How many animation frames this widget produces within `bounds`.
    fn frame_count(&self, bounds: Rect) -> i32;

    /// Optional: returns intrinsic (width, height) if known without layout.
    fn size(&self) -> Option<(i32, i32)> {
        None
    }
}

/// Modular integer that handles negative values correctly.
/// Matches Go's `ModInt(a, m)`.
pub fn mod_int(a: i32, m: i32) -> i32 {
    if m == 0 {
        return 0;
    }
    ((a % m) + m) % m
}

/// Returns the maximum frame count among a slice of widgets.
pub fn max_frame_count(widgets: &[Box<dyn Widget>], bounds: Rect) -> i32 {
    widgets
        .iter()
        .map(|w| w.frame_count(bounds))
        .max()
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_default_is_empty() {
        assert!(Rect::default().is_empty());
    }

    #[test]
    fn rect_new() {
        let r = Rect::new(1, 2, 64, 32);
        assert_eq!(r.x, 1);
        assert_eq!(r.y, 2);
        assert_eq!(r.width, 64);
        assert_eq!(r.height, 32);
        assert!(!r.is_empty());
    }

    #[test]
    fn rect_inset() {
        let r = Rect::new(0, 0, 64, 32);
        let insets = Insets::new(2, 3, 4, 5);
        let inner = r.inset(insets);
        assert_eq!(inner, Rect::new(2, 3, 58, 24));
    }

    #[test]
    fn rect_intersection_overlap() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(&b), Rect::new(5, 5, 5, 5));
    }

    #[test]
    fn rect_intersection_no_overlap() {
        let a = Rect::new(0, 0, 5, 5);
        let b = Rect::new(10, 10, 5, 5);
        assert!(a.intersection(&b).is_empty());
    }

    #[test]
    fn insets_uniform() {
        let i = Insets::uniform(4);
        assert_eq!(i, Insets::new(4, 4, 4, 4));
    }

    #[test]
    fn mod_int_positive() {
        assert_eq!(mod_int(7, 3), 1);
        assert_eq!(mod_int(6, 3), 0);
    }

    #[test]
    fn mod_int_negative() {
        assert_eq!(mod_int(-1, 3), 2);
        assert_eq!(mod_int(-4, 3), 2);
    }

    #[test]
    fn mod_int_zero_modulus() {
        assert_eq!(mod_int(5, 0), 0);
    }

    #[test]
    fn widget_is_object_safe() {
        struct Dummy;
        impl Widget for Dummy {
            fn paint_bounds(&self, bounds: Rect, _: i32) -> Rect {
                bounds
            }
            fn paint(&self, _: &mut tiny_skia::Pixmap, _: Rect, _: i32) {}
            fn frame_count(&self, _: Rect) -> i32 {
                1
            }
        }
        let _: Box<dyn Widget> = Box::new(Dummy);
    }
}
