//! Image-processing filter widgets that mirror pixlet's `filter` package.
//!
//! Each widget wraps a child, renders it into an offscreen pixmap, applies an
//! image operation (blur, brightness, hue, etc.), and composites the result back
//! into the target pixmap. All filters operate on 32-bit RGBA data using either
//! direct pixel manipulation or the `imageproc` crate for convolution-based ops.

mod blur;
mod color_ops;
mod convolution;
mod transform;

pub use blur::Blur;
pub use color_ops::{
    Brightness, Contrast, Gamma, Grayscale, Hue, Invert, Saturation, Sepia, Threshold,
};
pub use convolution::{EdgeDetection, Emboss, Sharpen};
pub use transform::{FlipHorizontal, FlipVertical, Rotate, Shear};

use crate::render::{Rect, Widget};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform as TsTransform};

/// Render `child` into an offscreen pixmap sized to its paint bounds. Returns the
/// pixmap plus the child's paint bounds so callers can composite it back at the
/// correct location. Returns `None` if the pixmap allocation fails.
pub(crate) fn render_child_to_pixmap(
    child: &dyn Widget,
    bounds: Rect,
    frame_idx: i32,
) -> Option<(Pixmap, Rect)> {
    let cb = child.paint_bounds(bounds, frame_idx);
    let w = (cb.x + cb.width).max(1) as u32;
    let h = (cb.y + cb.height).max(1) as u32;
    let mut pixmap = Pixmap::new(w, h)?;
    child.paint(&mut pixmap, Rect::new(0, 0, w as i32, h as i32), frame_idx);
    Some((pixmap, cb))
}

pub(crate) fn composite_pixmap(
    dest: &mut Pixmap,
    src: &Pixmap,
    x: i32,
    y: i32,
) {
    let paint = PixmapPaint {
        opacity: 1.0,
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality: FilterQuality::Nearest,
    };
    dest.draw_pixmap(x, y, src.as_ref(), &paint, TsTransform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::box_widget::BoxWidget;
    use tiny_skia::Color;

    #[test]
    fn composite_draws_src_at_offset() {
        let mut dest = Pixmap::new(4, 4).unwrap();
        let mut src = Pixmap::new(2, 2).unwrap();
        src.fill(Color::from_rgba8(255, 0, 0, 255));
        composite_pixmap(&mut dest, &src, 1, 1);
        assert_eq!(dest.pixels()[(1 * 4 + 1) as usize].red(), 255);
    }

    #[test]
    fn render_child_returns_bounds() {
        let child = BoxWidget {
            width: 3,
            height: 3,
            ..BoxWidget::new()
        };
        let (pixmap, bounds) = render_child_to_pixmap(&child, Rect::new(0, 0, 10, 10), 0).unwrap();
        assert_eq!(bounds.width, 3);
        assert_eq!(bounds.height, 3);
        assert_eq!(pixmap.width(), 3);
    }
}
