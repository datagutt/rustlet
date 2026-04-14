use std::io::{self, Write};

use anyhow::Result;
use rustlet_render::fonts::{get_font, get_font_list};

pub fn run() -> Result<()> {
    let names = get_font_list();
    let longest = names.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut out = io::stdout().lock();
    let _ = writeln!(
        out,
        "{:<width$}  WIDTH  HEIGHT  ASCENT  DESCENT",
        "NAME",
        width = longest
    );
    for name in &names {
        let font = get_font(name);
        if writeln!(
            out,
            "{:<width$}  {:>5}  {:>6}  {:>6}  {:>7}",
            name,
            font.char_width,
            font.char_height,
            font.ascent,
            font.descent,
            width = longest
        )
        .is_err()
        {
            // Downstream pipe closed (`| head`), exit cleanly.
            break;
        }
    }
    Ok(())
}
