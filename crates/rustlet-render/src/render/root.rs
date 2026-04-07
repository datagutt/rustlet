use super::{Rect, Widget};

#[allow(dead_code)]
pub const DEFAULT_FRAME_WIDTH: u32 = 64;
#[allow(dead_code)]
pub const DEFAULT_FRAME_HEIGHT: u32 = 32;
pub const DEFAULT_MAX_FRAME_COUNT: i32 = 2000;
pub const DEFAULT_DELAY_MS: i32 = 50;

/// Top-level container for a rendered app. NOT a Widget itself.
pub struct Root {
    pub child: Box<dyn Widget>,
    pub delay: i32,
    pub max_age: i32,
    pub show_full_animation: bool,
}

impl Root {
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            child,
            delay: DEFAULT_DELAY_MS,
            max_age: 0,
            show_full_animation: false,
        }
    }

    /// Render all animation frames as RGBA pixmaps.
    pub fn paint_frames(&self, width: u32, height: u32) -> Vec<tiny_skia::Pixmap> {
        let bounds = Rect::new(0, 0, width as i32, height as i32);
        let num_frames = self
            .child
            .frame_count(bounds)
            .min(DEFAULT_MAX_FRAME_COUNT)
            .max(1);

        (0..num_frames)
            .map(|frame_idx| {
                let mut pixmap = tiny_skia::Pixmap::new(width, height)
                    .expect("pixmap dimensions must be non-zero");
                // Fill with solid black background (matching Go's solidBackground)
                for pixel in pixmap.pixels_mut() {
                    *pixel =
                        tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
                }
                self.child.paint(&mut pixmap, bounds, frame_idx);
                pixmap
            })
            .collect()
    }
}
