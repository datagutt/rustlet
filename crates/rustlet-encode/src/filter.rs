use tiny_skia::Pixmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Filter {
    #[default]
    None,
    Dimmed,
    RedShift,
    Warm,
    Sunset,
    Sepia,
    Vintage,
    Dusk,
    Cool,
    /// Black & White (perceptual luminance grayscale)
    BW,
    Ice,
    Moonlight,
    Neon,
    Pastel,
}

type ColorMatrix = [[f32; 3]; 3];

impl Filter {
    fn matrix(self) -> ColorMatrix {
        match self {
            Filter::None => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Filter::Dimmed => [[0.25, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 0.25]],
            // Chromatic adaptation matrix approximating a D65 -> 3400K whitepoint shift.
            // Computed in XYZ space using the Bradford method and projected into linear sRGB.
            Filter::RedShift => [
                [1.2066, 0.3380, 0.0383],
                [-0.0164, 0.8985, 0.0098],
                [-0.0156, -0.0500, 0.4201],
            ],
            Filter::Warm => [[1.1, 0.05, 0.0], [0.0, 1.0, 0.0], [0.05, 0.0, 0.9]],
            Filter::Sunset => [[1.2, 0.2, 0.0], [0.1, 1.0, 0.1], [0.0, 0.1, 0.6]],
            Filter::Sepia => [
                [0.393, 0.769, 0.189],
                [0.349, 0.686, 0.168],
                [0.272, 0.534, 0.131],
            ],
            Filter::Vintage => [[1.0, 0.6, 0.2], [0.3, 0.9, 0.2], [0.2, 0.4, 0.6]],
            Filter::Dusk => [[1.1, 0.0, 0.2], [0.0, 0.8, 0.1], [0.0, 0.1, 0.6]],
            Filter::Cool => [[0.9, 0.0, 0.2], [0.0, 1.0, 0.0], [-0.1, 0.0, 1.1]],
            Filter::BW => [[0.3, 0.59, 0.11], [0.3, 0.59, 0.11], [0.3, 0.59, 0.11]],
            Filter::Ice => [[0.8, 0.9, 1.0], [0.8, 0.9, 1.0], [1.0, 1.0, 1.2]],
            Filter::Moonlight => [[0.6, 0.2, 0.4], [0.2, 0.7, 0.2], [0.3, 0.3, 0.9]],
            Filter::Neon => [[0.9, 0.0, 1.1], [0.0, 1.0, 0.6], [0.2, 0.5, 1.3]],
            Filter::Pastel => [[1.2, 0.1, 0.1], [0.1, 1.2, 0.1], [0.1, 0.1, 1.2]],
        }
    }
}

fn clamp(f: f32) -> u8 {
    f.clamp(0.0, 255.0) as u8
}

/// Apply a color filter to all frames in-place.
///
/// For each pixel: unpremultiply alpha, apply the 3x3 color matrix, re-premultiply.
/// `Filter::None` is a no-op.
pub fn apply_filter(frames: &mut [Pixmap], filter: Filter) {
    if filter == Filter::None {
        return;
    }

    let m = filter.matrix();

    for pixmap in frames.iter_mut() {
        let data = pixmap.data_mut();

        // data is premultiplied RGBA, 4 bytes per pixel
        let mut i = 0;
        while i < data.len() {
            let a = data[i + 3];
            if a == 0 {
                i += 4;
                continue;
            }

            // Unpremultiply
            let r = (data[i] as f32 * 255.0) / a as f32;
            let g = (data[i + 1] as f32 * 255.0) / a as f32;
            let b = (data[i + 2] as f32 * 255.0) / a as f32;

            // Apply color matrix
            let nr = clamp(m[0][0] * r + m[0][1] * g + m[0][2] * b);
            let ng = clamp(m[1][0] * r + m[1][1] * g + m[1][2] * b);
            let nb = clamp(m[2][0] * r + m[2][1] * g + m[2][2] * b);

            // Re-premultiply
            data[i] = ((nr as u16 * a as u16) / 255) as u8;
            data[i + 1] = ((ng as u16 * a as u16) / 255) as u8;
            data[i + 2] = ((nb as u16 * a as u16) / 255) as u8;
            // alpha unchanged

            i += 4;
        }
    }
}

/// Integer upscaling: each pixel becomes a `factor x factor` block.
///
/// Returns new pixmaps with dimensions `width * factor` by `height * factor`.
/// Factor <= 1 returns clones of the input frames.
pub fn magnify(frames: &[Pixmap], factor: u32) -> Vec<Pixmap> {
    if factor <= 1 {
        return frames.to_vec();
    }

    frames
        .iter()
        .map(|pixmap| {
            let w = pixmap.width();
            let h = pixmap.height();
            let new_w = w * factor;
            let new_h = h * factor;
            let mut out = Pixmap::new(new_w, new_h).expect("magnified pixmap dimensions overflow");

            let src = pixmap.data();
            let dst = out.data_mut();
            let src_stride = (w * 4) as usize;
            let dst_stride = (new_w * 4) as usize;

            for y in 0..h {
                for x in 0..w {
                    let si = (y as usize * src_stride) + (x as usize * 4);
                    let pixel = &src[si..si + 4];

                    for dy in 0..factor {
                        for dx in 0..factor {
                            let out_x = x * factor + dx;
                            let out_y = y * factor + dy;
                            let di = (out_y as usize * dst_stride) + (out_x as usize * 4);
                            dst[di..di + 4].copy_from_slice(pixel);
                        }
                    }
                }
            }

            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::PremultipliedColorU8;

    fn solid_frame(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).unwrap();
        if a > 0 {
            for pixel in pixmap.pixels_mut() {
                *pixel = PremultipliedColorU8::from_rgba(
                    ((r as u16 * a as u16) / 255) as u8,
                    ((g as u16 * a as u16) / 255) as u8,
                    ((b as u16 * a as u16) / 255) as u8,
                    a,
                )
                .unwrap();
            }
        }
        pixmap
    }

    fn opaque_frame(width: u32, height: u32, r: u8, g: u8, b: u8) -> Pixmap {
        solid_frame(width, height, r, g, b, 255)
    }

    #[test]
    fn filter_none_is_identity() {
        let mut frames = vec![opaque_frame(4, 4, 100, 150, 200)];
        let original_data = frames[0].data().to_vec();
        apply_filter(&mut frames, Filter::None);
        assert_eq!(frames[0].data(), &original_data[..]);
    }

    #[test]
    fn filter_bw_produces_grayscale() {
        let mut frames = vec![opaque_frame(4, 4, 200, 100, 50)];
        apply_filter(&mut frames, Filter::BW);

        for pixel in frames[0].pixels() {
            assert_eq!(pixel.red(), pixel.green());
            assert_eq!(pixel.green(), pixel.blue());
        }
    }

    #[test]
    fn filter_sepia_warm_tone() {
        let mut frames = vec![opaque_frame(4, 4, 100, 100, 100)];
        apply_filter(&mut frames, Filter::Sepia);

        let px = frames[0].pixels()[0];
        // Sepia: R channel gets the largest weight sum, B the smallest
        assert!(
            px.red() > px.blue(),
            "sepia should produce warm tones (r > b)"
        );
    }

    #[test]
    fn magnify_doubles_dimensions() {
        let frames = vec![opaque_frame(4, 3, 255, 0, 0)];
        let result = magnify(&frames, 2);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].width(), 8);
        assert_eq!(result[0].height(), 6);
    }

    #[test]
    fn magnify_factor_one_unchanged() {
        let frames = vec![opaque_frame(4, 3, 128, 64, 32)];
        let result = magnify(&frames, 1);
        assert_eq!(result[0].width(), 4);
        assert_eq!(result[0].height(), 3);
        assert_eq!(result[0].data(), frames[0].data());
    }

    #[test]
    fn magnify_preserves_pixel_values() {
        let frames = vec![opaque_frame(2, 2, 200, 100, 50)];
        let result = magnify(&frames, 3);

        // Every pixel in the magnified output should match the source
        let expected = frames[0].pixels()[0];
        for pixel in result[0].pixels() {
            assert_eq!(*pixel, expected);
        }
    }

    #[test]
    fn filter_preserves_alpha() {
        let mut frames = vec![solid_frame(4, 4, 200, 100, 50, 128)];

        // Grab all original alpha values
        let original_alphas: Vec<u8> = frames[0].pixels().iter().map(|p| p.alpha()).collect();

        apply_filter(&mut frames, Filter::Sepia);

        let new_alphas: Vec<u8> = frames[0].pixels().iter().map(|p| p.alpha()).collect();
        assert_eq!(original_alphas, new_alphas);
    }

    #[test]
    fn filter_transparent_pixels_unchanged() {
        let mut frames = vec![solid_frame(4, 4, 0, 0, 0, 0)];
        let original_data = frames[0].data().to_vec();
        apply_filter(&mut frames, Filter::Neon);
        assert_eq!(frames[0].data(), &original_data[..]);
    }
}
