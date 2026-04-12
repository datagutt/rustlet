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
    /// Show the version of Rustlet
    Version,

    /// Print the configuration schema for a Rustlet app
    Schema {
        /// Path to the .star file or app directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output path for schema JSON (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

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
        Commands::Version => {
            println!("Rustlet version: {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Schema { path, output } => {
            let (src, base_dir, id) = if path.is_dir() {
                let main = path.join("main.star");
                let src = std::fs::read_to_string(&main)?;
                let id = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app")
                    .to_string();
                (src, Some(path.clone()), id)
            } else {
                let src = std::fs::read_to_string(&path)?;
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app")
                    .to_string();
                (src, path.parent().map(|p| p.to_path_buf()), id)
            };

            let applet = Applet::new();
            let schema_json = applet.schema_json(&id, &src, base_dir.as_deref())?;

            match output {
                Some(path) if path.as_os_str() != "-" => {
                    std::fs::write(&path, schema_json.as_bytes())?;
                }
                _ => {
                    println!("{}", schema_json);
                }
            }
        }
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
            let id = file.file_stem().and_then(|s| s.to_str()).unwrap_or("app");

            let base_dir = file.parent();

            let applet = Applet::new();
            let config = HashMap::new();
            let roots =
                applet.run_with_options(id, &src, &config, width, height, is_2x, base_dir)?;

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
