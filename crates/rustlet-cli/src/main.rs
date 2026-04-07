use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};

use rustlet_encode::{Filter, OutputFormat};
use rustlet_runtime::Applet;

#[derive(Parser)]
#[command(name = "rustlet", about = "build apps for pixel-based displays")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Render a .star file to an image
    Render {
        /// Path to the .star file
        file: PathBuf,

        /// Output file path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Display width in pixels
        #[arg(long, default_value_t = 64)]
        width: u32,

        /// Display height in pixels
        #[arg(long, default_value_t = 32)]
        height: u32,

        /// Output format (auto-detected from extension if not specified)
        #[arg(long, value_enum)]
        format: Option<Format>,

        /// Color filter to apply before encoding
        #[arg(long, value_enum, default_value_t = Filter::None)]
        filter: Filter,

        /// Integer magnification factor
        #[arg(long, default_value_t = 1)]
        magnify: u32,

        /// Double the canvas size (128x64) and use terminus-16 default font
        #[arg(long = "2x")]
        double: bool,

        /// Directory containing Twemoji SVG files (named by codepoint, e.g. 1f600.svg)
        #[arg(long)]
        twemoji_dir: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum Format {
    Gif,
    Webp,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render {
            file,
            output,
            width,
            height,
            format,
            filter,
            magnify,
            double,
            twemoji_dir,
        } => {
            if let Some(ref dir) = twemoji_dir {
                rustlet_render::Emoji::set_twemoji_dir(&dir.to_string_lossy());
            }

            let (width, height, is_2x) = if double {
                (128, 64, true)
            } else {
                (width, height, false)
            };

            let src = std::fs::read_to_string(&file)?;
            let id = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("app");

            let applet = Applet::new();
            let config = HashMap::new();
            let roots = applet.run_with_options(id, &src, &config, width, height, is_2x)?;

            if roots.is_empty() {
                bail!("main() returned no roots");
            }

            let root = roots.into_iter().next().unwrap();
            let mut frames = root.paint_frames(width, height);
            let delay_ms = root.delay as u16;

            rustlet_encode::apply_filter(&mut frames, filter);
            let frames = rustlet_encode::magnify(&frames, magnify);

            let out_format = match format {
                Some(Format::Gif) => OutputFormat::Gif,
                Some(Format::Webp) => OutputFormat::WebP,
                None => {
                    // Auto-detect from output extension
                    match output
                        .as_ref()
                        .and_then(|p| p.extension())
                        .and_then(|e| e.to_str())
                    {
                        Some("webp") => OutputFormat::WebP,
                        _ => OutputFormat::Gif,
                    }
                }
            };

            let data = rustlet_encode::encode(&frames, delay_ms, out_format)?;

            match output {
                Some(path) => std::fs::write(&path, &data)?,
                None => {
                    use std::io::Write;
                    std::io::stdout().write_all(&data)?;
                }
            }
        }
    }

    Ok(())
}
