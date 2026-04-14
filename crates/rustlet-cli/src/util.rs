//! Shared helpers that straddle the subcommand boundary: applet loading and
//! `.star` file collection. These used to live in main.rs but `serve` needs to
//! reload the applet on every request, so they moved here.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use rustlet_runtime::manifest::Manifest;

/// Resolved applet source: the .star body plus the id we should use when
/// running it, and the optional base directory for `load()` resolution. When
/// loading from a directory we also parse `manifest.yaml` so commands can use
/// the declared supports2x flag and id.
pub struct LoadedApplet {
    pub id: String,
    pub source: String,
    pub base_dir: Option<PathBuf>,
    pub manifest: Option<Manifest>,
}

pub fn load_applet(path: &Path) -> Result<LoadedApplet> {
    if path.is_dir() {
        let main = path.join("main.star");
        let source = std::fs::read_to_string(&main)
            .with_context(|| format!("reading {}", main.display()))?;
        let manifest_path = path.join(rustlet_runtime::manifest::MANIFEST_FILE_NAME);
        let manifest = if manifest_path.exists() {
            Some(Manifest::load_from_path(&manifest_path)?)
        } else {
            None
        };
        let id = manifest
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app")
                    .to_string()
            });
        Ok(LoadedApplet {
            id,
            source,
            base_dir: Some(path.to_path_buf()),
            manifest,
        })
    } else {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("app")
            .to_string();
        Ok(LoadedApplet {
            id,
            source,
            base_dir: path.parent().map(|p| p.to_path_buf()),
            manifest: None,
        })
    }
}

/// Collect every `.star` file implied by the given CLI paths. Directories are
/// scanned for a top-level `main.star`; with `--recursive` every `.star`
/// descendant is included. A bare `.star` argument is kept as-is.
pub fn collect_star_files(paths: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_file() {
            out.push(path.clone());
            continue;
        }
        if path.is_dir() {
            if recursive {
                walk_recursive(path, &mut out)?;
            } else {
                let main = path.join("main.star");
                if main.exists() {
                    out.push(main);
                } else {
                    // Fall back to top-level .star siblings in the directory.
                    for entry in std::fs::read_dir(path)? {
                        let entry = entry?;
                        let p = entry.path();
                        if p.extension().and_then(|e| e.to_str()) == Some("star") {
                            out.push(p);
                        }
                    }
                }
            }
            continue;
        }
        bail!("path does not exist: {}", path.display());
    }
    Ok(out)
}

fn walk_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_recursive(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("star") {
            out.push(p);
        }
    }
    Ok(())
}

/// Resolved render inputs after applying pixlet-compatible positional parsing
/// and JSON config file loading. `path` is None when the caller did not supply
/// one (the caller should default to "." then).
#[derive(Debug)]
pub struct RenderInputs {
    pub path: Option<PathBuf>,
    pub config: HashMap<String, String>,
}

/// Parse pixlet-style positional args for render/profile/check.
///
/// The first arg is the path unless it contains `=` AND the token does not
/// exist on disk as a file or directory. In that case the whole args list is
/// treated as k=v overrides and the path defaults to None (caller supplies
/// ".").
///
/// If `config_file` is provided, its JSON contents seed the config map first
/// so CLI overrides win on collision.
///
/// Mirrors `cmd/render.go:249-290` in the pixlet reference.
pub fn parse_config_args(
    args: &[String],
    config_file: Option<&Path>,
) -> Result<RenderInputs> {
    let mut config = HashMap::new();

    if let Some(path) = config_file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        let parsed: HashMap<String, String> = serde_json::from_str(&text)
            .with_context(|| format!("parsing config file {}", path.display()))?;
        config.extend(parsed);
    }

    let (path, remaining_kv): (Option<PathBuf>, &[String]) = if let Some(first) = args.first() {
        if first.contains('=') && !Path::new(first).exists() {
            (None, args)
        } else {
            (Some(PathBuf::from(first)), &args[1..])
        }
    } else {
        (None, &[][..])
    };

    for raw in remaining_kv {
        let Some((k, v)) = raw.split_once('=') else {
            bail!("parameters must be in form <key>=<value>, found `{raw}`");
        };
        config.insert(k.to_string(), v.to_string());
    }

    Ok(RenderInputs { path, config })
}

/// Run a synchronous closure on a dedicated thread with a wall-clock timeout.
/// The closure must be `Send + 'static`. On timeout the helper returns an
/// error but leaves the worker thread running to detach cleanly — starlark's
/// `Evaluator` is unwind-unsafe so forcibly aborting it would be worse than
/// orphaning the thread until process exit.
pub fn run_with_timeout<F, T>(timeout: Duration, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("rustlet-timeout".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .context("spawning timeout worker")?;

    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(anyhow!("timed out after {}s", timeout.as_secs_f32()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow!("render worker disconnected"))
        }
    }
}

/// Compute the output path when `--output` was not given. Appends `@2x`
/// before the extension when `is_2x`, mirroring `cmd/render.go:185-187`.
pub fn default_output_path(applet_path: &Path, extension: &str, is_2x: bool) -> PathBuf {
    let stem = if applet_path.is_dir() {
        applet_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("app"))
    } else {
        applet_path
            .file_stem()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("app"))
    };
    let parent = applet_path.parent().unwrap_or(Path::new(""));
    let stem_str = stem.as_os_str().to_string_lossy();
    let suffix = if is_2x { "@2x" } else { "" };
    let filename = format!("{stem_str}{suffix}.{extension}");
    parent.join(filename)
}

/// Very loose BCP47 validator. Returns the input unchanged on success, or an
/// error message if the tag fails a simple alphanumeric + dash shape check.
/// Pixlet uses Go's `language.Parse` which is much stricter; this is a
/// best-effort placeholder until a real ICU dep lands.
pub fn validate_locale(tag: &str) -> Result<String> {
    if tag.is_empty() {
        bail!("locale is empty");
    }
    for c in tag.chars() {
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            bail!("locale `{tag}` contains invalid characters");
        }
    }
    Ok(tag.to_string())
}

/// Convenience: strip the `-` placeholder from an output path. Returns `None`
/// when the caller wrote `-` explicitly (meaning stdout) or when no output was
/// provided at all.
pub fn explicit_output(path: Option<&Path>) -> Option<&Path> {
    match path {
        Some(p) if p.as_os_str() == OsStr::new("-") => None,
        Some(p) => Some(p),
        None => None,
    }
}

/// Build the User-Agent string for all outgoing HTTP traffic.
///
/// Format mirrors pixlet's `pixlet/<version>[-<git7>] (<goos>/<goarch>)` so
/// server operators who already parse pixlet logs get the same shape. The
/// git sha is baked in at compile time via build.rs; an empty sha (cargo
/// install from a tarball) produces the shorter form.
pub fn user_agent() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let sha = env!("RUSTLET_GIT_SHA");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if sha.is_empty() {
        format!("rustlet/{version} ({os}/{arch})")
    } else {
        format!("rustlet/{version}-{sha} ({os}/{arch})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_args_path_first() {
        let args = ["examples/app.star".to_string(), "who=world".to_string()];
        let got = parse_config_args(&args, None).unwrap();
        assert_eq!(got.path, Some(PathBuf::from("examples/app.star")));
        assert_eq!(got.config.get("who").map(String::as_str), Some("world"));
    }

    #[test]
    fn parse_config_args_first_arg_is_kv_when_not_on_disk() {
        let args = ["definitely_not_a_path=foo".to_string()];
        let got = parse_config_args(&args, None).unwrap();
        assert!(got.path.is_none());
        assert_eq!(
            got.config.get("definitely_not_a_path").map(String::as_str),
            Some("foo")
        );
    }

    #[test]
    fn parse_config_args_error_on_missing_equals() {
        let args = ["app.star".to_string(), "no_equals".to_string()];
        let err = parse_config_args(&args, None).unwrap_err();
        assert!(
            err.to_string().contains("<key>=<value>"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_config_args_cli_overrides_json_file() {
        let dir = std::env::temp_dir();
        let cfg = dir.join("rustlet_cfg_test.json");
        std::fs::write(&cfg, r#"{"who":"from_file","other":"kept"}"#).unwrap();

        let args = ["app.star".to_string(), "who=from_cli".to_string()];
        let got = parse_config_args(&args, Some(&cfg)).unwrap();
        assert_eq!(got.config.get("who").map(String::as_str), Some("from_cli"));
        assert_eq!(got.config.get("other").map(String::as_str), Some("kept"));
        let _ = std::fs::remove_file(cfg);
    }

    #[test]
    fn default_output_path_uses_file_stem() {
        assert_eq!(
            default_output_path(Path::new("foo/bar.star"), "webp", false),
            PathBuf::from("foo/bar.webp")
        );
    }

    #[test]
    fn default_output_path_uses_dir_name() {
        assert_eq!(
            default_output_path(Path::new("foo/bar"), "gif", false),
            PathBuf::from("foo/bar.gif")
        );
    }

    #[test]
    fn default_output_path_appends_2x_suffix() {
        assert_eq!(
            default_output_path(Path::new("foo/bar.star"), "webp", true),
            PathBuf::from("foo/bar@2x.webp")
        );
    }

    #[test]
    fn validate_locale_accepts_bcp47_shapes() {
        assert_eq!(validate_locale("en-US").unwrap(), "en-US");
        assert_eq!(validate_locale("zh-Hant-HK").unwrap(), "zh-Hant-HK");
        assert!(validate_locale("").is_err());
        assert!(validate_locale("en US").is_err());
    }

    #[test]
    fn run_with_timeout_succeeds_before_deadline() {
        let got: i32 = run_with_timeout(Duration::from_secs(2), || Ok(42)).unwrap();
        assert_eq!(got, 42);
    }

    #[test]
    fn user_agent_has_pixlet_compatible_shape() {
        let ua = user_agent();
        assert!(ua.starts_with("rustlet/"), "got: {ua}");
        // Either `rustlet/x.y.z (os/arch)` or `rustlet/x.y.z-abc1234 (os/arch)`.
        assert!(ua.contains(" ("), "got: {ua}");
        assert!(ua.ends_with(')'), "got: {ua}");
    }

    #[test]
    fn run_with_timeout_errors_after_deadline() {
        let err = run_with_timeout::<_, ()>(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(500));
            Ok(())
        })
        .unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
