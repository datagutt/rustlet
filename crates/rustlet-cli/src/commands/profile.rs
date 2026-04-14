//! `rustlet profile` — runs an applet under starlark-rust's profiler and
//! writes the collected profile data.
//!
//! Unlike pixlet, which emits a Go pprof protobuf, rustlet emits whatever
//! format starlark-rust's `ProfileData::gen()` produces for the selected
//! mode. That's CSV for heap summaries, bytecode, and typecheck; a folded
//! flamegraph for heap/time flame; line-by-line for statement; and so on.
//! The `--mode` flag defaults to `heap-summary-allocated`, the closest
//! equivalent to pixlet's default.

use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rustlet_runtime::{Applet, AppletRunOptions, ProfileMode};

use crate::util::{load_applet, parse_config_args, run_with_timeout, validate_locale};

pub struct Args {
    pub positional: Vec<String>,
    pub config_file: Option<PathBuf>,
    pub mode: String,
    pub output: Option<PathBuf>,
    pub silent: bool,
    pub timeout: Duration,
    pub locale: Option<String>,
    pub width: u32,
    pub height: u32,
    pub is_2x: bool,
}

pub fn run(args: Args) -> Result<()> {
    let mode = ProfileMode::from_str(&args.mode)
        .map_err(|e| anyhow::anyhow!("invalid --mode `{}`: {e}", args.mode))?;
    if matches!(mode, ProfileMode::None) {
        bail!("`none` is not a runnable profile mode");
    }

    if let Some(ref tag) = args.locale {
        validate_locale(tag)?;
    }

    let inputs = parse_config_args(&args.positional, args.config_file.as_deref())?;
    let file = inputs.path.unwrap_or_else(|| PathBuf::from("."));
    let config = inputs.config;

    let loaded = load_applet(&file)?;
    let manifest_supports_2x = loaded
        .manifest
        .as_ref()
        .map(|m| m.supports2x)
        .unwrap_or(false);
    let is_2x = args.is_2x || manifest_supports_2x;
    let (width, height) = if is_2x {
        (128, 64)
    } else {
        (args.width, args.height)
    };

    let id = loaded.id.clone();
    let source = loaded.source.clone();
    let base_dir = loaded.base_dir.clone();
    let silent = args.silent;
    let locale = args.locale.clone();
    let profile_text = run_with_timeout(args.timeout, move || {
        let applet = Applet::new();
        let opts = AppletRunOptions {
            width,
            height,
            is_2x,
            base_dir: base_dir.as_deref(),
            secret_decryption_key: None,
            silent,
            locale,
        };
        applet.profile(&id, &source, &config, opts, &mode)
    })?;

    match args.output.as_deref() {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::write(path, profile_text.as_bytes())
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("profile written to {}", path.display());
        }
        _ => {
            std::io::stdout()
                .write_all(profile_text.as_bytes())
                .context("writing profile to stdout")?;
        }
    }
    Ok(())
}
