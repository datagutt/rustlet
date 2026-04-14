//! Soft-validate icon identifiers in an applet's schema.
//!
//! Pixlet's `community validate-icons` checks each schema field's `icon`
//! against a hardcoded FontAwesome allowlist shipped with the Go source.
//! Rustlet does not ship that allowlist, so this command does the next-best
//! thing: it confirms every icon value is a non-empty ASCII identifier. It
//! catches typos, empty strings, and whitespace-only values, but does NOT
//! catch "bogusicon" unless someone bolts on a real registry later.

use std::path::Path;

use anyhow::{bail, Context, Result};
use rustlet_runtime::Applet;
use serde_json::Value;

use crate::util::load_applet;

pub fn run(path: &Path) -> Result<()> {
    let loaded = load_applet(path)?;
    let applet = Applet::new();
    let schema_json = applet
        .schema_json(&loaded.id, &loaded.source, loaded.base_dir.as_deref())
        .context("evaluating schema")?;

    let value: Value = serde_json::from_str(&schema_json).context("parsing schema json")?;
    let fields = value
        .get("schema")
        .and_then(|s| s.get("fields"))
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();

    let mut errors = Vec::new();
    for field in &fields {
        let id = field
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)");
        let icon = field.get("icon").and_then(|v| v.as_str()).unwrap_or("");
        if icon.is_empty() {
            // Not every field type requires an icon (generated fields, etc.).
            continue;
        }
        if !is_valid_icon_identifier(icon) {
            errors.push(format!(
                "field `{id}`: icon `{icon}` is not a valid identifier (expected ASCII alphanumerics)"
            ));
        }
    }

    if errors.is_empty() {
        println!("{}: ok ({} icon fields checked)", path.display(), fields.len());
        Ok(())
    } else {
        for err in &errors {
            eprintln!("{err}");
        }
        bail!("{} icon field(s) failed validation", errors.len());
    }
}

fn is_valid_icon_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_identifiers() {
        assert!(is_valid_icon_identifier("clock"));
        assert!(is_valid_icon_identifier("arrowRight"));
        assert!(is_valid_icon_identifier("thumbs-up"));
        assert!(is_valid_icon_identifier("one_two"));
    }

    #[test]
    fn invalid_identifiers() {
        assert!(!is_valid_icon_identifier(""));
        assert!(!is_valid_icon_identifier("has space"));
        assert!(!is_valid_icon_identifier("with.dot"));
        assert!(!is_valid_icon_identifier("emoji-✨"));
    }
}
