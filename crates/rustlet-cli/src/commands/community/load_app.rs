use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rustlet_runtime::Applet;

use crate::util::load_applet;

pub fn run(path: &Path) -> Result<()> {
    let loaded = load_applet(path)?;
    let applet = Applet::new();
    applet.run_with_options(
        &loaded.id,
        &loaded.source,
        &HashMap::new(),
        64,
        32,
        false,
        loaded.base_dir.as_deref(),
    )?;
    println!("{}: app loaded successfully", path.display());
    Ok(())
}
