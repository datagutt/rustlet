use super::keyframe::{find_keyframes, process_keyframes, Keyframe};
use super::transform::interpolate_transforms;
use super::{rescale, Direction, FillMode, Rounding};
use crate::render::{Rect, Widget};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform as TsTransform};

/// Origin is a relative anchor point in [0, 1] used for rotation, scale and shear.
#[derive(Clone, Copy, Debug)]
pub struct Origin {
    pub x: f64,
    pub y: f64,
}

impl Default for Origin {
    fn default() -> Self {
        Origin { x: 0.5, y: 0.5 }
    }
}

impl Origin {
    pub fn transform(&self, width: i32, height: i32) -> (f64, f64) {
        (self.x * width as f64, self.y * height as f64)
    }
}

/// `animation.Transformation` widget: renders a child through a CSS-style keyframe
/// animation of translate/rotate/scale/shear transforms. The child is first rendered
/// to an offscreen pixmap, then blitted into the target pixmap using the interpolated
/// affine transform built from the current frame's keyframe bracket.
pub struct Transformation {
    pub child: Box<dyn Widget>,
    pub keyframes: Vec<Keyframe>,
    pub duration: i32,
    pub delay: i32,
    pub width: i32,
    pub height: i32,
    pub origin: Origin,
    pub direction: Direction,
    pub fill_mode: FillMode,
    pub rounding: Rounding,
    pub wait_for_child: bool,
}

impl Transformation {
    pub fn new(child: Box<dyn Widget>, keyframes: Vec<Keyframe>, duration: i32) -> Self {
        Transformation {
            child,
            keyframes: process_keyframes(keyframes),
            duration,
            delay: 0,
            width: 0,
            height: 0,
            origin: Origin::default(),
            direction: Direction::NORMAL,
            fill_mode: FillMode::Forwards,
            rounding: Rounding::Round,
            wait_for_child: false,
        }
    }

    fn resolved_bounds(&self, bounds: Rect) -> Rect {
        let w = if self.width == 0 {
            bounds.width
        } else {
            self.width
        };
        let h = if self.height == 0 {
            bounds.height
        } else {
            self.height
        };
        Rect::new(0, 0, w, h)
    }
}

impl Widget for Transformation {
    fn paint_bounds(&self, bounds: Rect, _frame_idx: i32) -> Rect {
        let rb = self.resolved_bounds(bounds);
        Rect::new(0, 0, rb.width, rb.height)
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        let own = self.direction.frame_count(self.delay, self.duration);
        let child_bounds = self.resolved_bounds(bounds);
        let child = self.child.frame_count(child_bounds);
        if self.wait_for_child && child > own {
            child
        } else {
            own.max(1)
        }
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let inner = self.resolved_bounds(bounds);
        let child_bounds = self.child.paint_bounds(inner, frame_idx);

        // Render child into an offscreen pixmap large enough for its bounds.
        // The child's paint_bounds.x/y are treated as offsets within that pixmap.
        let cw = (child_bounds.x + child_bounds.width).max(1) as u32;
        let ch = (child_bounds.y + child_bounds.height).max(1) as u32;
        let Some(mut child_pixmap) = Pixmap::new(cw, ch) else {
            return;
        };
        self.child.paint(
            &mut child_pixmap,
            Rect::new(0, 0, child_bounds.x + child_bounds.width, child_bounds.y + child_bounds.height),
            frame_idx,
        );

        // Origin is resolved against the child's paint bounds, not the total pixmap.
        let origin = (
            self.origin.x * child_bounds.width as f64 + child_bounds.x as f64,
            self.origin.y * child_bounds.height as f64 + child_bounds.y as f64,
        );

        let mut progress = self.direction.progress(
            self.delay,
            self.duration,
            self.fill_mode.value(),
            frame_idx,
        );

        // Start with identity and translate by the target bounds' x/y so the child lands
        // at the right spot in the output pixmap.
        let mut ts = TsTransform::from_translate(bounds.x as f32, bounds.y as f32);

        if let Some((from, to)) = find_keyframes(&self.keyframes, progress) {
            // Rescale progress into the local [0, 1] between from and to, then apply easing.
            progress = rescale(from.percentage, to.percentage, 0.0, 1.0, progress);
            progress = from.curve.transform(progress);

            let (transforms, ok) = interpolate_transforms(&from.transforms, &to.transforms, progress);
            if ok {
                for t in transforms {
                    ts = t.apply(ts, origin, self.rounding);
                }
            }
        }

        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: FilterQuality::Nearest,
        };
        pixmap.draw_pixmap(0, 0, child_pixmap.as_ref(), &paint, ts, None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Curve, Transform};
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn identity_transformation_paints_child() {
        let child = BoxWidget {
            color: Some(Color::from_rgba8(255, 0, 0, 255)),
            width: 2,
            height: 2,
            ..BoxWidget::new()
        };
        let t = Transformation::new(Box::new(child), Vec::new(), 10);
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        t.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 0);
        // Empty keyframes -> identity transforms -> child painted as-is
        let px = pixmap.pixels()[0];
        assert_eq!(px.red(), 255);
    }

    #[test]
    fn translate_keyframe_moves_child() {
        let child = BoxWidget {
            color: Some(Color::from_rgba8(0, 255, 0, 255)),
            width: 1,
            height: 1,
            ..BoxWidget::new()
        };
        let kfs = vec![
            Keyframe::new(
                0.0,
                vec![Transform::Translate { x: 0.0, y: 0.0 }],
                Curve::Linear,
            ),
            Keyframe::new(
                1.0,
                vec![Transform::Translate { x: 2.0, y: 0.0 }],
                Curve::Linear,
            ),
        ];
        let mut t = Transformation::new(Box::new(child), kfs, 4);
        t.origin = Origin { x: 0.0, y: 0.0 };
        let mut pixmap = Pixmap::new(4, 4).unwrap();
        // At end of animation, child should be at x=2
        t.paint(&mut pixmap, Rect::new(0, 0, 4, 4), 10);
        let px = pixmap.pixels()[(0 * 4 + 2) as usize];
        assert_eq!(px.green(), 255, "expected green pixel at (2,0)");
    }
}
