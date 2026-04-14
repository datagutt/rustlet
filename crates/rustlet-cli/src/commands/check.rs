//! `rustlet check` — composed validation suite for community apps.
//!
//! Ports pixlet's `cmd/check.go`. For each discovered app directory we run:
//!
//!   1. `manifest.yaml` load + `Manifest::validate()`
//!   2. Skip if manifest marks the app broken (with `--skip-broken`)
//!   3. Load applet with empty config (equivalent to `community load-app`)
//!   4. Soft icon validation (same check as `community validate-icons`)
//!   5. Render once with silent print, output discarded, timing measured
//!   6. Fail if render wall-clock exceeds `--max-render-time`
//!   7. Format dry-run via the existing `run_format` helper in main.rs
//!   8. Lint via the existing `run_lint` helper in main.rs
//!
//! Unlike pixlet we do not gate on a profile-reported duration because
//! starlark-rust's profile output format is not pprof. Wall-clock timing is
//! simpler and still catches runaway apps.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use rustlet_encode::OutputFormat;
use rustlet_runtime::manifest::{Manifest, MANIFEST_FILE_NAME};

use crate::util::{load_applet, render_bytes, RenderBytesOptions};

pub struct Args {
    pub paths: Vec<PathBuf>,
    pub recursive: bool,
    pub skip_broken: bool,
    pub max_render_time: Duration,
}

pub fn run(args: Args) -> Result<bool> {
    let apps = discover_apps(&args.paths, args.recursive)?;
    if apps.is_empty() {
        eprintln!("no apps found");
        return Ok(false);
    }

    let mut had_failure = false;
    for app in apps {
        match check_app(&app, args.skip_broken, args.max_render_time) {
            Ok(CheckOutcome::Passed) => println!("{} ok", tick(&app)),
            Ok(CheckOutcome::Skipped) => println!("{} skipped (broken)", dash(&app)),
            Err(e) => {
                println!("{} {e:#}", cross(&app));
                had_failure = true;
            }
        }
    }
    Ok(!had_failure)
}

enum CheckOutcome {
    Passed,
    Skipped,
}

fn check_app(app: &Path, skip_broken: bool, max_render_time: Duration) -> Result<CheckOutcome> {
    let manifest_path = if app.is_dir() {
        app.join(MANIFEST_FILE_NAME)
    } else {
        app.parent()
            .map(|p| p.join(MANIFEST_FILE_NAME))
            .unwrap_or_else(|| PathBuf::from(MANIFEST_FILE_NAME))
    };
    if !manifest_path.exists() {
        bail!("manifest.yaml missing at {}", manifest_path.display());
    }

    let manifest = Manifest::load_from_path(&manifest_path)
        .with_context(|| format!("loading {}", manifest_path.display()))?;
    manifest.validate().context("manifest invalid")?;

    if skip_broken && manifest.broken {
        return Ok(CheckOutcome::Skipped);
    }

    // Load step: evaluate the applet with empty config.
    let loaded = load_applet(app).context("loading applet")?;
    let applet = rustlet_runtime::Applet::new();
    applet
        .run_with_options(
            &loaded.id,
            &loaded.source,
            &HashMap::new(),
            64,
            32,
            false,
            loaded.base_dir.as_deref(),
        )
        .context("evaluating applet")?;

    // Icon validation: soft check (non-empty, ASCII identifier).
    validate_schema_icons(&applet, &loaded)?;

    // Render step, silent, output discarded, wall-clock timed.
    let started = Instant::now();
    let _bytes = render_bytes(
        app,
        &HashMap::new(),
        &RenderBytesOptions {
            silent: true,
            format: OutputFormat::WebP,
            ..Default::default()
        },
    )
    .context("render step")?;
    let elapsed = started.elapsed();
    if elapsed > max_render_time {
        bail!(
            "render took {:?}, exceeding --max-render-time {:?}",
            elapsed,
            max_render_time
        );
    }

    // Format dry-run + lint reuse the existing main.rs helpers via their
    // public wrappers. Each expects a list of paths.
    let app_paths = vec![app.to_path_buf()];
    let format_ok = crate::run_format(&app_paths, true, false).context("format step")?;
    if !format_ok {
        bail!("format check failed");
    }
    let lint_ok = crate::run_lint(&app_paths, false).context("lint step")?;
    if !lint_ok {
        bail!("lint check failed");
    }

    Ok(CheckOutcome::Passed)
}

fn validate_schema_icons(
    applet: &rustlet_runtime::Applet,
    loaded: &crate::util::LoadedApplet,
) -> Result<()> {
    let schema_json = match applet.schema_json(
        &loaded.id,
        &loaded.source,
        loaded.base_dir.as_deref(),
    ) {
        Ok(s) => s,
        Err(e) => {
            // No get_schema() is fine; pixlet's check also tolerates schema-less apps.
            if e.to_string().contains("get_schema") {
                return Ok(());
            }
            return Err(e);
        }
    };
    let value: serde_json::Value =
        serde_json::from_str(&schema_json).context("parsing schema json")?;
    let fields = value
        .get("schema")
        .and_then(|s| s.get("fields"))
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();
    for field in &fields {
        let id = field
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)");
        let icon = field.get("icon").and_then(|v| v.as_str()).unwrap_or("");
        if icon.is_empty() {
            continue;
        }
        if !icon
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("field `{id}` icon `{icon}` is not a valid identifier");
        }
    }
    Ok(())
}

fn discover_apps(paths: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in paths {
        if !path.exists() {
            bail!("path does not exist: {}", path.display());
        }
        if path.is_file() {
            out.push(path.clone());
            continue;
        }
        if !recursive {
            out.push(path.clone());
            continue;
        }
        walk(path, &mut out)?;
    }
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if dir.join(MANIFEST_FILE_NAME).exists() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir()
            && !p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
        {
            walk(&p, out)?;
        }
    }
    Ok(())
}

fn tick(path: &Path) -> String {
    format!("\u{2714} {}", path.display())
}

fn cross(path: &Path) -> String {
    format!("\u{2717} {}", path.display())
}

fn dash(path: &Path) -> String {
    format!("- {}", path.display())
}
