use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use rustlet_encode::{Filter, OutputFormat};
use rustlet_runtime::{manifest::Manifest, Applet};

mod api;
mod commands;
mod config;
mod util;

use commands::config_cmd::ConfigAction;
use util::{collect_star_files, load_applet};

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

    /// Lint a .star file or app directory. Parses the file, evaluates it in a
    /// sandbox, checks for required callables, and validates manifest.yaml when
    /// present. Returns a non-zero exit code if any issues are found.
    Lint {
        /// Paths to lint. Accepts .star files and directories. Defaults to the
        /// current directory.
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
    },

    /// Format .star files. Requires `buildifier` on $PATH (same tool pixlet
    /// uses). If buildifier is missing, prints an error with install
    /// instructions and exits non-zero.
    Format {
        /// Paths to format. Defaults to the current directory.
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Preview changes without modifying files.
        #[arg(short = 'd', long)]
        dry_run: bool,

        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
    },

    /// Render a .star file to an image
    Render {
        /// Path to the .star file or app directory
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

        /// Double the canvas size (128x64) and use terminus-16 default font.
        /// Auto-enabled when the manifest declares `supports2x: true` and the
        /// applet is loaded from a directory.
        #[arg(long = "2x")]
        double: bool,

        /// Directory containing Twemoji SVG files (named by codepoint, e.g. 1f600.svg)
        #[arg(long)]
        twemoji_dir: Option<PathBuf>,
    },

    /// Push a WebP image to a Tronbyt or Tidbyt device.
    ///
    /// Reads the image from the given file, or from stdin when the path is `-`.
    /// Credentials resolve in this order: CLI flag > environment
    /// (`RUSTLET_URL`, `RUSTLET_TOKEN`) > config file.
    Push {
        /// Device ID to push to.
        device_id: String,

        /// WebP file to push. Use `-` to read from stdin.
        image: PathBuf,

        /// Keeps the image in rotation under this installation identifier.
        #[arg(short = 'i', long)]
        installation_id: Option<String>,

        /// Don't interrupt the current display; just save to the slot.
        /// Requires --installation-id.
        #[arg(short = 'b', long)]
        background: bool,

        /// Base URL of the API (default: from config or RUSTLET_URL).
        #[arg(long)]
        url: Option<String>,

        /// API token (default: from config or RUSTLET_TOKEN).
        #[arg(long, env = config::ENV_TOKEN, hide_env_values = true)]
        token: Option<String>,
    },

    /// Delete an installation from a device.
    Delete {
        /// Device ID.
        device_id: String,

        /// Installation ID to remove.
        installation_id: String,

        #[arg(long)]
        url: Option<String>,

        #[arg(long, env = config::ENV_TOKEN, hide_env_values = true)]
        token: Option<String>,
    },

    /// List devices registered to the configured account.
    Devices {
        #[arg(long)]
        url: Option<String>,

        #[arg(long, env = config::ENV_TOKEN, hide_env_values = true)]
        token: Option<String>,
    },

    /// List installations currently on a device.
    List {
        /// Device ID.
        device_id: String,

        #[arg(long)]
        url: Option<String>,

        #[arg(long, env = config::ENV_TOKEN, hide_env_values = true)]
        token: Option<String>,
    },

    /// Read or write the persisted API config.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Create a new applet in the current working directory.
    ///
    /// Prompts for the app name, summary, description and author, then writes
    /// `manifest.yaml` and a `<slug>.star` stub alongside. Run this after
    /// `mkdir myapp && cd myapp`.
    Create,

    /// Run a dev server with live reload for a .star file or app directory.
    ///
    /// Watches the applet for changes and pushes a Server-Sent Event to the
    /// browser whenever a `.star`, `.yaml` or `.yml` file in the watched
    /// directory is modified. The browser reloads the preview image on each
    /// event.
    Serve {
        /// Path to the .star file or app directory.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Host interface to bind.
        #[arg(short = 'i', long, default_value = "127.0.0.1")]
        host: String,

        /// TCP port to listen on.
        #[arg(short = 'p', long, default_value_t = 8080)]
        port: u16,

        /// Display width in pixels.
        #[arg(long, default_value_t = 64)]
        width: u32,

        /// Display height in pixels.
        #[arg(long, default_value_t = 32)]
        height: u32,

        /// Don't open a browser window on start.
        #[arg(long)]
        no_browser: bool,
    },
}

#[derive(Clone, ValueEnum)]
enum Format {
    Gif,
    Webp,
}

fn run_lint(paths: &[PathBuf], recursive: bool) -> Result<bool> {
    let files = collect_star_files(paths, recursive)?;
    if files.is_empty() {
        eprintln!("no .star files found");
        return Ok(false);
    }

    let mut had_issue = false;
    let applet = Applet::new();

    // Lint manifest files next to each app, deduplicated.
    let mut seen_manifests = std::collections::HashSet::new();
    for file in &files {
        if let Some(parent) = file.parent() {
            let manifest_path = parent.join(rustlet_runtime::manifest::MANIFEST_FILE_NAME);
            if manifest_path.exists() && seen_manifests.insert(manifest_path.clone()) {
                match Manifest::load_from_path(&manifest_path) {
                    Ok(m) => {
                        let errors = m.validate_all();
                        if !errors.is_empty() {
                            had_issue = true;
                            println!("{}: manifest issues:", manifest_path.display());
                            for err in errors {
                                println!("  - {err}");
                            }
                        }
                    }
                    Err(e) => {
                        had_issue = true;
                        println!("{}: {}", manifest_path.display(), e);
                    }
                }
            }
        }

        let src = std::fs::read_to_string(file)
            .with_context(|| format!("reading {}", file.display()))?;
        let id = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("app");
        let base_dir = file.parent();
        match applet.lint_source(id, &src, base_dir) {
            Ok(issues) => {
                if issues.is_empty() {
                    println!("{}: ok", file.display());
                } else {
                    had_issue = true;
                    println!("{}:", file.display());
                    for issue in issues {
                        println!("  - {issue}");
                    }
                }
            }
            Err(e) => {
                had_issue = true;
                println!("{}: {}", file.display(), e);
            }
        }
    }

    Ok(!had_issue)
}

fn run_format(paths: &[PathBuf], dry_run: bool, recursive: bool) -> Result<bool> {
    let buildifier = std::env::var("BUILDIFIER").unwrap_or_else(|_| "buildifier".to_string());
    // Verify buildifier is on PATH.
    if Command::new(&buildifier)
        .arg("--version")
        .output()
        .is_err()
    {
        bail!(
            "`{buildifier}` not found on PATH. Install it from https://github.com/bazelbuild/buildtools \
             (e.g. `go install github.com/bazelbuild/buildtools/buildifier@latest`) and re-run."
        );
    }

    let files = collect_star_files(paths, recursive)?;
    if files.is_empty() {
        eprintln!("no .star files found");
        return Ok(false);
    }

    let mode = if dry_run { "diff" } else { "fix" };
    let mut ok = true;
    for file in &files {
        let status = Command::new(&buildifier)
            .arg("--type=default")
            .arg(format!("--mode={mode}"))
            .arg("--lint=off")
            .arg(file)
            .status()
            .with_context(|| format!("running buildifier on {}", file.display()))?;
        if !status.success() {
            ok = false;
        }
    }
    Ok(ok)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version => {
            println!("Rustlet version: {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Schema { path, output } => {
            let loaded = load_applet(&path)?;
            let applet = Applet::new();
            let schema_json =
                applet.schema_json(&loaded.id, &loaded.source, loaded.base_dir.as_deref())?;
            match output {
                Some(path) if path.as_os_str() != "-" => {
                    std::fs::write(&path, schema_json.as_bytes())?;
                }
                _ => {
                    println!("{}", schema_json);
                }
            }
        }
        Commands::Lint { paths, recursive } => {
            let ok = run_lint(&paths, recursive)?;
            return Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            });
        }
        Commands::Format {
            paths,
            dry_run,
            recursive,
        } => {
            let ok = run_format(&paths, dry_run, recursive)?;
            return Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            });
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

            let loaded = load_applet(&file)?;
            // `--2x` on the CLI takes precedence; otherwise auto-enable when the
            // manifest opts in with `supports2x: true`, matching pixlet.
            let is_2x =
                double || loaded.manifest.as_ref().map(|m| m.supports2x).unwrap_or(false);
            let (width, height) = if is_2x { (128, 64) } else { (width, height) };

            let applet = Applet::new();
            let config = HashMap::new();
            let roots = applet.run_with_options(
                &loaded.id,
                &loaded.source,
                &config,
                width,
                height,
                is_2x,
                loaded.base_dir.as_deref(),
            )?;

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
                None => match output
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|e| e.to_str())
                {
                    Some("webp") => OutputFormat::WebP,
                    _ => OutputFormat::Gif,
                },
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
        Commands::Push {
            device_id,
            image,
            installation_id,
            background,
            url,
            token,
        } => {
            commands::push::run(commands::push::Args {
                device_id: &device_id,
                image: &image,
                installation_id: installation_id.as_deref(),
                background,
                url: url.as_deref(),
                token: token.as_deref(),
            })?;
        }
        Commands::Delete {
            device_id,
            installation_id,
            url,
            token,
        } => {
            commands::delete::run(commands::delete::Args {
                device_id: &device_id,
                installation_id: &installation_id,
                url: url.as_deref(),
                token: token.as_deref(),
            })?;
        }
        Commands::Devices { url, token } => {
            commands::devices::run(commands::devices::Args {
                url: url.as_deref(),
                token: token.as_deref(),
            })?;
        }
        Commands::List {
            device_id,
            url,
            token,
        } => {
            commands::list::run(commands::list::Args {
                device_id: &device_id,
                url: url.as_deref(),
                token: token.as_deref(),
            })?;
        }
        Commands::Config { action } => {
            commands::config_cmd::run(action)?;
        }
        Commands::Create => {
            commands::create::run()?;
        }
        Commands::Serve {
            path,
            host,
            port,
            width,
            height,
            no_browser,
        } => {
            commands::serve::run(commands::serve::Args {
                path,
                host,
                port,
                width,
                height,
                no_browser,
            })?;
        }
    }

    Ok(ExitCode::SUCCESS)
}

