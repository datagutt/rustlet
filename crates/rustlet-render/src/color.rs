use anyhow::{bail, Result};

/// Parse a color string into a tiny_skia::Color.
///
/// Supports:
/// - `#rgb` (3 hex chars, each nibble doubled)
/// - `#rgba` (4 hex chars)
/// - `#rrggbb` (6 hex chars)
/// - `#rrggbbaa` (8 hex chars)
/// - All of the above without the `#` prefix
/// - CSS named colors (case-insensitive)
pub fn parse_color(s: &str) -> Result<tiny_skia::Color> {
    let hex = s.strip_prefix('#').unwrap_or(s);

    // Try hex formats first
    if hex.chars().all(|c| c.is_ascii_hexdigit()) {
        match hex.len() {
            3 => {
                let r = u8_from_hex_nibble(hex.as_bytes()[0])? * 17;
                let g = u8_from_hex_nibble(hex.as_bytes()[1])? * 17;
                let b = u8_from_hex_nibble(hex.as_bytes()[2])? * 17;
                return Ok(tiny_skia::Color::from_rgba8(r, g, b, 255));
            }
            4 => {
                let r = u8_from_hex_nibble(hex.as_bytes()[0])? * 17;
                let g = u8_from_hex_nibble(hex.as_bytes()[1])? * 17;
                let b = u8_from_hex_nibble(hex.as_bytes()[2])? * 17;
                let a = u8_from_hex_nibble(hex.as_bytes()[3])? * 17;
                return Ok(tiny_skia::Color::from_rgba8(r, g, b, a));
            }
            6 => {
                let r = u8_from_hex_pair(hex.as_bytes()[0], hex.as_bytes()[1])?;
                let g = u8_from_hex_pair(hex.as_bytes()[2], hex.as_bytes()[3])?;
                let b = u8_from_hex_pair(hex.as_bytes()[4], hex.as_bytes()[5])?;
                return Ok(tiny_skia::Color::from_rgba8(r, g, b, 255));
            }
            8 => {
                let r = u8_from_hex_pair(hex.as_bytes()[0], hex.as_bytes()[1])?;
                let g = u8_from_hex_pair(hex.as_bytes()[2], hex.as_bytes()[3])?;
                let b = u8_from_hex_pair(hex.as_bytes()[4], hex.as_bytes()[5])?;
                let a = u8_from_hex_pair(hex.as_bytes()[6], hex.as_bytes()[7])?;
                return Ok(tiny_skia::Color::from_rgba8(r, g, b, a));
            }
            _ => {}
        }
    }

    // Try CSS named colors
    if let Ok(rgb) = color_name::css::Color::val().by_string(s.to_lowercase()) {
        return Ok(tiny_skia::Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255));
    }

    bail!("invalid color: {s}")
}

fn u8_from_hex_nibble(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => bail!("invalid hex digit: {}", c as char),
    }
}

fn u8_from_hex_pair(hi: u8, lo: u8) -> Result<u8> {
    Ok(u8_from_hex_nibble(hi)? * 16 + u8_from_hex_nibble(lo)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_color(s: &str, r: u8, g: u8, b: u8, a: u8) {
        let c = parse_color(s).unwrap();
        let expected = tiny_skia::Color::from_rgba8(r, g, b, a);
        assert_eq!(c, expected, "color mismatch for {s}");
    }

    #[test]
    fn hex_3() {
        assert_color("#f00", 255, 0, 0, 255);
        assert_color("#abc", 0xaa, 0xbb, 0xcc, 255);
    }

    #[test]
    fn hex_4() {
        assert_color("#f00f", 255, 0, 0, 255);
        assert_color("#f008", 255, 0, 0, 0x88);
    }

    #[test]
    fn hex_6() {
        assert_color("#ff0000", 255, 0, 0, 255);
        assert_color("#1a2b3c", 0x1a, 0x2b, 0x3c, 255);
    }

    #[test]
    fn hex_8() {
        assert_color("#ff000080", 255, 0, 0, 0x80);
    }

    #[test]
    fn hex_no_hash() {
        assert_color("ff0000", 255, 0, 0, 255);
        assert_color("f00", 255, 0, 0, 255);
    }

    #[test]
    fn named_colors() {
        assert_color("red", 255, 0, 0, 255);
        assert_color("white", 255, 255, 255, 255);
        assert_color("black", 0, 0, 0, 255);
    }

    #[test]
    fn case_insensitive_hex() {
        assert_color("#FF0000", 255, 0, 0, 255);
        assert_color("#Ff0000", 255, 0, 0, 255);
    }

    #[test]
    fn invalid_color() {
        assert!(parse_color("notacolor").is_err());
        assert!(parse_color("#xyz").is_err());
        assert!(parse_color("#12345").is_err());
    }
}
