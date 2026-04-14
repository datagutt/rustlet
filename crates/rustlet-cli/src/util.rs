//! Shared helpers that straddle the subcommand boundary: applet loading and
//! `.star` file collection. These used to live in main.rs but `serve` needs to
//! reload the applet on every request, so they moved here.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
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
