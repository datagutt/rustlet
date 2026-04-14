use std::path::Path;

use anyhow::{bail, Context, Result};
use rustlet_runtime::manifest::{Manifest, MANIFEST_FILE_NAME};

pub fn run(path: &Path) -> Result<()> {
    let manifest_path = if path.is_dir() {
        path.join(MANIFEST_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    if !manifest_path.exists() {
        bail!("manifest not found: {}", manifest_path.display());
    }
    let manifest = Manifest::load_from_path(&manifest_path)
        .with_context(|| format!("loading {}", manifest_path.display()))?;
    manifest.validate()?;
    println!("{}: ok", manifest_path.display());
    Ok(())
}
