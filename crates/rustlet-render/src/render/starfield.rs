use super::{Rect, Widget};
use tiny_skia::Pixmap;

const FRAME_COUNT: i32 = 300;
const STARS_PER_LAYER: usize = 20;
const NUM_LAYERS: usize = 3;

struct Star {
    x: f64,
    y: f64,
    speed: f64,
    brightness: u8,
}

pub struct Starfield {
    stars: Vec<Star>,
    width: i32,
    height: i32,
}

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

impl Starfield {
    pub fn new(width: i32, height: i32) -> Self {
        let mut seed: u64 = 42;
        let mut stars = Vec::with_capacity(NUM_LAYERS * STARS_PER_LAYER);

        let layers: [(f64, u8); NUM_LAYERS] = [
            (0.5, 80),
            (1.0, 160),
            (2.0, 255),
        ];

        for &(speed, brightness) in &layers {
            for _ in 0..STARS_PER_LAYER {
                let x = (xorshift64(&mut seed) % width as u64) as f64;
                let y = (xorshift64(&mut seed) % height as u64) as f64;
                stars.push(Star { x, y, speed, brightness });
            }
        }

        Self { stars, width, height }
    }
}

impl Widget for Starfield {
    fn paint_bounds(&self, _bounds: Rect, _frame_idx: i32) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    fn paint(&self, pixmap: &mut Pixmap, bounds: Rect, frame_idx: i32) {
        let w = self.width as f64;

        for star in &self.stars {
            let shifted_x = (star.x + star.speed * frame_idx as f64) % w;
            // Handle negative modulo (shouldn't happen with positive values, but be safe)
            let px = if shifted_x < 0.0 { shifted_x + w } else { shifted_x } as i32;
            let py = star.y as i32;

            let abs_x = bounds.x + px;
            let abs_y = bounds.y + py;

            if abs_x < 0 || abs_y < 0 {
                continue;
            }
            let ux = abs_x as u32;
            let uy = abs_y as u32;
            if ux >= pixmap.width() || uy >= pixmap.height() {
                continue;
            }

            let idx = (uy * pixmap.width() + ux) as usize;
            let pixels = pixmap.data_mut();
            let base = idx * 4;
            if base + 3 < pixels.len() {
                // Premultiply: white (255,255,255) with alpha = brightness
                let a = star.brightness;
                pixels[base] = a;     // R (premultiplied)
                pixels[base + 1] = a; // G (premultiplied)
                pixels[base + 2] = a; // B (premultiplied)
                pixels[base + 3] = a; // A
            }
        }
    }

    fn frame_count(&self, _bounds: Rect) -> i32 {
        FRAME_COUNT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_is_300() {
        let sf = Starfield::new(64, 32);
        assert_eq!(sf.frame_count(Rect::new(0, 0, 64, 32)), 300);
    }

    #[test]
    fn different_frames_produce_different_pixels() {
        let sf = Starfield::new(64, 32);
        let bounds = Rect::new(0, 0, 64, 32);

        let mut pm0 = Pixmap::new(64, 32).unwrap();
        sf.paint(&mut pm0, bounds, 0);

        let mut pm1 = Pixmap::new(64, 32).unwrap();
        sf.paint(&mut pm1, bounds, 10);

        assert_ne!(pm0.data(), pm1.data());
    }

    #[test]
    fn deterministic_output() {
        let sf1 = Starfield::new(64, 32);
        let sf2 = Starfield::new(64, 32);
        let bounds = Rect::new(0, 0, 64, 32);

        let mut pm1 = Pixmap::new(64, 32).unwrap();
        sf1.paint(&mut pm1, bounds, 5);

        let mut pm2 = Pixmap::new(64, 32).unwrap();
        sf2.paint(&mut pm2, bounds, 5);

        assert_eq!(pm1.data(), pm2.data());
    }

    #[test]
    fn paint_produces_nonzero_pixels() {
        let sf = Starfield::new(64, 32);
        let bounds = Rect::new(0, 0, 64, 32);
        let mut pm = Pixmap::new(64, 32).unwrap();
        sf.paint(&mut pm, bounds, 0);

        let has_nonzero = pm.data().iter().any(|&b| b != 0);
        assert!(has_nonzero, "expected some non-zero pixels after painting starfield");
    }

    #[test]
    fn paint_bounds_returns_widget_size() {
        let sf = Starfield::new(64, 32);
        let pb = sf.paint_bounds(Rect::new(10, 10, 100, 100), 0);
        assert_eq!(pb, Rect::new(0, 0, 64, 32));
    }

    #[test]
    fn stars_wrap_horizontally() {
        let sf = Starfield::new(64, 32);
        let bounds = Rect::new(0, 0, 64, 32);

        // Frame 0 and frame 64 should look similar for the slowest layer
        // (speed 0.5, so full wrap at frame 128), but painting at high frame
        // should still produce output without panics.
        let mut pm = Pixmap::new(64, 32).unwrap();
        sf.paint(&mut pm, bounds, 299);
        let has_nonzero = pm.data().iter().any(|&b| b != 0);
        assert!(has_nonzero);
    }
}
