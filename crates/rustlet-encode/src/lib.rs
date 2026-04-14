pub mod filter;

use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{bail, Result};

pub use filter::{apply_filter, magnify, Filter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Gif,
    WebP,
}

/// Sentinel meaning "no level override; use the default encoder preset".
const WEBP_LEVEL_UNSET: u8 = u8::MAX;

static WEBP_LEVEL: AtomicU8 = AtomicU8::new(WEBP_LEVEL_UNSET);

/// Override the WebP lossless preset level (0..=9). A value of 0 is fastest /
/// largest, 9 is slowest / smallest. Mirrors pixlet's `--webp-level/-z` flag.
/// Panics if `level > 9`.
pub fn set_webp_level(level: u8) {
    assert!(level <= 9, "webp level must be 0..=9, got {level}");
    WEBP_LEVEL.store(level, Ordering::Relaxed);
}

/// Clear any previously-set WebP level override.
pub fn clear_webp_level() {
    WEBP_LEVEL.store(WEBP_LEVEL_UNSET, Ordering::Relaxed);
}

fn current_webp_level() -> Option<u8> {
    let v = WEBP_LEVEL.load(Ordering::Relaxed);
    (v != WEBP_LEVEL_UNSET).then_some(v)
}

/// Encode frames in the requested format.
pub fn encode(
    frames: &[tiny_skia::Pixmap],
    delay_ms: u16,
    format: OutputFormat,
) -> Result<Vec<u8>> {
    encode_with_max_duration(frames, delay_ms, format, None)
}

/// Encode frames with an optional animation-length cap. Frames past
/// `max_duration` are dropped. Mirrors pixlet's `--max-duration/-d` behavior.
pub fn encode_with_max_duration(
    frames: &[tiny_skia::Pixmap],
    delay_ms: u16,
    format: OutputFormat,
    max_duration: Option<std::time::Duration>,
) -> Result<Vec<u8>> {
    let frames = truncate_frames(frames, delay_ms, max_duration);
    match format {
        OutputFormat::Gif => encode_gif(&frames, delay_ms),
        OutputFormat::WebP => encode_webp(&frames, delay_ms),
    }
}

fn truncate_frames<'a>(
    frames: &'a [tiny_skia::Pixmap],
    delay_ms: u16,
    max_duration: Option<std::time::Duration>,
) -> std::borrow::Cow<'a, [tiny_skia::Pixmap]> {
    let Some(max) = max_duration else {
        return std::borrow::Cow::Borrowed(frames);
    };
    if delay_ms == 0 {
        return std::borrow::Cow::Borrowed(frames);
    }
    let max_frames = (max.as_millis() as u64 / delay_ms as u64).max(1) as usize;
    if frames.len() <= max_frames {
        return std::borrow::Cow::Borrowed(frames);
    }
    std::borrow::Cow::Owned(frames[..max_frames].to_vec())
}

/// Encode frames as an animated GIF.
///
/// Each frame is quantized to at most 256 colors. Delay is converted from
/// milliseconds to centiseconds (GIF's native unit).
pub fn encode_gif(frames: &[tiny_skia::Pixmap], delay_ms: u16) -> Result<Vec<u8>> {
    if frames.is_empty() {
        bail!("no frames to encode");
    }

    let width = frames[0].width() as u16;
    let height = frames[0].height() as u16;
    let delay_cs = delay_ms / 10;

    let mut buf = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut buf, width, height, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;

        for pixmap in frames {
            let (palette, indices) = quantize(pixmap);
            let frame = gif::Frame {
                width,
                height,
                palette: Some(palette),
                buffer: std::borrow::Cow::Borrowed(&indices),
                delay: delay_cs,
                dispose: gif::DisposalMethod::Background,
                transparent: None,
                needs_user_input: false,
                top: 0,
                left: 0,
                interlaced: false,
            };
            // GIF requires palette size to be a power of 2 (minimum 2 entries)
            // The gif crate handles this internally, but we ensure min 2 in quantize()
            encoder.write_frame(&frame)?;
        }
    }

    Ok(buf)
}

/// Encode frames as an animated lossless WebP.
pub fn encode_webp(frames: &[tiny_skia::Pixmap], delay_ms: u16) -> Result<Vec<u8>> {
    if frames.is_empty() {
        bail!("no frames to encode");
    }

    let width = frames[0].width();
    let height = frames[0].height();

    // quality 0..100 maps to libwebp's lossless preset slider; pixlet passes
    // an int 0..=9 via the -z flag which we spread across 0..=100 by *11.
    // Clamped at 99 to stay inside the (exclusive) upper bound most backends
    // accept without error.
    let (quality, method) = match current_webp_level() {
        Some(level) => (((level as f32) * 11.0).min(99.0), 4),
        None => (75.0, 4),
    };
    let options = webp_animation::EncoderOptions {
        encoding_config: Some(webp_animation::EncodingConfig {
            encoding_type: webp_animation::EncodingType::Lossless,
            quality,
            method,
        }),
        ..Default::default()
    };
    let mut encoder = webp_animation::Encoder::new_with_options((width, height), options)
        .map_err(|e| anyhow::anyhow!("webp encoder init failed: {:?}", e))?;

    for (i, pixmap) in frames.iter().enumerate() {
        // tiny-skia stores premultiplied RGBA; webp wants straight RGBA
        let rgba = unpremultiply(pixmap);
        let timestamp_ms = (i as i32) * (delay_ms as i32);
        encoder
            .add_frame(&rgba, timestamp_ms)
            .map_err(|e| anyhow::anyhow!("webp add_frame failed: {:?}", e))?;
    }

    let final_timestamp = (frames.len() as i32) * (delay_ms as i32);
    let webp_data = encoder
        .finalize(final_timestamp)
        .map_err(|e| anyhow::anyhow!("webp finalize failed: {:?}", e))?;

    Ok(webp_data.to_vec())
}

/// Convert premultiplied RGBA (tiny-skia) to straight RGBA.
fn unpremultiply(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let a = pixel.alpha();
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = ((pixel.red() as u16 * 255) / a as u16) as u8;
            let g = ((pixel.green() as u16 * 255) / a as u16) as u8;
            let b = ((pixel.blue() as u16 * 255) / a as u16) as u8;
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    out
}

/// Simple color quantization: collect unique colors up to 256, nearest-neighbor for overflow.
/// Returns (palette, indices) where palette is a flat Vec of RGB triplets.
fn quantize(pixmap: &tiny_skia::Pixmap) -> (Vec<u8>, Vec<u8>) {
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut index_map: std::collections::HashMap<[u8; 3], u8> = std::collections::HashMap::new();
    let mut indices = Vec::with_capacity(pixmap.pixels().len());

    for pixel in pixmap.pixels() {
        let a = pixel.alpha();
        let (r, g, b) = if a == 0 {
            (0, 0, 0)
        } else {
            (
                ((pixel.red() as u16 * 255) / a as u16) as u8,
                ((pixel.green() as u16 * 255) / a as u16) as u8,
                ((pixel.blue() as u16 * 255) / a as u16) as u8,
            )
        };
        let rgb = [r, g, b];

        let idx = if let Some(&existing) = index_map.get(&rgb) {
            existing
        } else if colors.len() < 256 {
            let idx = colors.len() as u8;
            colors.push(rgb);
            index_map.insert(rgb, idx);
            idx
        } else {
            nearest_color(&colors, &rgb)
        };
        indices.push(idx);
    }

    // GIF requires at least 2 palette entries
    while colors.len() < 2 {
        colors.push([0, 0, 0]);
    }

    let palette: Vec<u8> = colors.iter().flat_map(|c| c.iter().copied()).collect();
    (palette, indices)
}

fn nearest_color(palette: &[[u8; 3]], target: &[u8; 3]) -> u8 {
    palette
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let dr = c[0] as i32 - target[0] as i32;
            let dg = c[1] as i32 - target[1] as i32;
            let db = c[2] as i32 - target[2] as i32;
            (i, dr * dr + dg * dg + db * db)
        })
        .min_by_key(|(_, dist)| *dist)
        .map(|(i, _)| i as u8)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_frame(width: u32, height: u32, r: u8, g: u8, b: u8) -> tiny_skia::Pixmap {
        let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
        for pixel in pixmap.pixels_mut() {
            *pixel = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255).unwrap();
        }
        pixmap
    }

    #[test]
    fn gif_single_frame_magic_bytes() {
        let frame = solid_frame(64, 32, 255, 0, 0);
        let data = encode_gif(&[frame], 50).unwrap();
        assert_eq!(&data[..3], b"GIF");
        assert_eq!(&data[3..6], b"89a");
    }

    #[test]
    fn gif_multi_frame() {
        let frames = vec![
            solid_frame(64, 32, 255, 0, 0),
            solid_frame(64, 32, 0, 255, 0),
        ];
        let data = encode_gif(&frames, 50).unwrap();
        assert_eq!(&data[..3], b"GIF");
        assert!(data.len() > 100);
    }

    #[test]
    fn gif_empty_frames_error() {
        assert!(encode_gif(&[], 50).is_err());
    }

    #[test]
    fn webp_single_frame_magic_bytes() {
        let frame = solid_frame(64, 32, 255, 0, 0);
        let data = encode_webp(&[frame], 50).unwrap();
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
    }

    #[test]
    fn webp_empty_frames_error() {
        assert!(encode_webp(&[], 50).is_err());
    }

    #[test]
    fn encode_dispatch() {
        let frame = solid_frame(64, 32, 0, 0, 255);
        let gif = encode(&[frame.clone()], 50, OutputFormat::Gif).unwrap();
        assert_eq!(&gif[..3], b"GIF");

        let frame = solid_frame(64, 32, 0, 0, 255);
        let webp = encode(&[frame], 50, OutputFormat::WebP).unwrap();
        assert_eq!(&webp[..4], b"RIFF");
    }

    #[test]
    fn truncate_frames_respects_max_duration() {
        let frame = solid_frame(2, 2, 0, 0, 0);
        let frames: Vec<_> = (0..10).map(|_| frame.clone()).collect();
        let got = truncate_frames(&frames, 100, Some(std::time::Duration::from_millis(350)));
        assert_eq!(got.len(), 3);

        let got = truncate_frames(&frames, 100, None);
        assert_eq!(got.len(), 10);

        let got = truncate_frames(&frames, 100, Some(std::time::Duration::from_millis(0)));
        assert_eq!(got.len(), 1, "zero duration produces one frame minimum");
    }

    #[test]
    fn webp_level_setter_clamps_to_9() {
        set_webp_level(9);
        assert_eq!(current_webp_level(), Some(9));
        set_webp_level(0);
        assert_eq!(current_webp_level(), Some(0));
        clear_webp_level();
        assert_eq!(current_webp_level(), None);
    }

    #[test]
    #[should_panic(expected = "webp level must be")]
    fn webp_level_panics_above_9() {
        set_webp_level(10);
    }

    #[test]
    fn quantize_basic() {
        let pixmap = solid_frame(2, 2, 128, 64, 32);
        let (palette, indices) = quantize(&pixmap);
        assert!(palette.len() >= 6); // at least 2 colors * 3 bytes
        assert_eq!(indices.len(), 4);
        // all pixels same color → all same index
        assert!(indices.iter().all(|&i| i == indices[0]));
    }
}
