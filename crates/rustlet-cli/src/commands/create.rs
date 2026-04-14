//! `rustlet create` — interactive applet scaffolder. Mirrors pixlet's
//! community manifest prompt flow: asks for name/summary/desc/author with the
//! same validation rules that `lint` enforces, then writes `manifest.yaml` and
//! `<slug>.star` to the current working directory.
//!
//! The command does not create a subdirectory. Users run it after
//! `mkdir myapp && cd myapp` (or equivalent).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dialoguer::{theme::ColorfulTheme, Input};
use rustlet_runtime::manifest::{
    generate_dir_name, generate_file_name, generate_id, validate_author, validate_desc,
    validate_name, validate_summary, MANIFEST_FILE_NAME,
};

const MANIFEST_TEMPLATE: &str = include_str!("../../templates/manifest.yaml.tmpl");
const MAIN_STAR_TEMPLATE: &str = include_str!("../../templates/main.star.tmpl");

/// Inputs resolved from interactive prompts. Exposed separately from `run()`
/// so integration tests can exercise the file-writing path without a TTY.
pub struct Inputs {
    pub name: String,
    pub summary: String,
    pub desc: String,
    pub author: String,
}

pub fn run() -> Result<()> {
    let cwd = env::current_dir().context("reading current directory")?;
    let inputs = prompt_inputs()?;
    let written = scaffold(&cwd, &inputs)?;

    println!();
    println!("Created:");
    for path in &written {
        println!("  {}", path.display());
    }
    let star_name = written
        .last()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("main.star");
    println!();
    println!("Next: `rustlet serve {star_name}` to preview.");
    Ok(())
}

fn prompt_inputs() -> Result<Inputs> {
    let name = prompt("App name", |s| validate_name(s).map_err(|e| e.to_string()))?;
    let summary = prompt("Summary", |s| {
        validate_summary(s).map_err(|e| e.to_string())
    })?;
    let desc = prompt("Description", |s| {
        validate_desc(s).map_err(|e| e.to_string())
    })?;
    let author = prompt("Author", |s| validate_author(s).map_err(|e| e.to_string()))?;
    Ok(Inputs {
        name,
        summary,
        desc,
        author,
    })
}

/// Write `manifest.yaml` and `<slug>.star` under `dir`. Refuses to overwrite
/// existing files. Returns the created paths in the order they were written.
pub fn scaffold(dir: &Path, inputs: &Inputs) -> Result<Vec<PathBuf>> {
    validate_name(&inputs.name)?;
    validate_summary(&inputs.summary)?;
    validate_desc(&inputs.desc)?;
    validate_author(&inputs.author)?;

    let id = generate_id(&inputs.name);
    let dir_name = generate_dir_name(&inputs.name);
    let file_name = generate_file_name(&inputs.name);

    let manifest_path = dir.join(MANIFEST_FILE_NAME);
    let star_path = dir.join(&file_name);

    refuse_overwrite(&manifest_path)?;
    refuse_overwrite(&star_path)?;

    let manifest_body = render_template(
        MANIFEST_TEMPLATE,
        &[
            ("id", id.as_str()),
            ("name", inputs.name.as_str()),
            ("summary", inputs.summary.as_str()),
            ("desc", inputs.desc.as_str()),
            ("author", inputs.author.as_str()),
            ("file_name", file_name.as_str()),
            ("package_name", dir_name.as_str()),
        ],
    );
    let star_body = render_template(
        MAIN_STAR_TEMPLATE,
        &[
            ("name", inputs.name.as_str()),
            ("summary", inputs.summary.as_str()),
            ("desc", inputs.desc.as_str()),
            ("author", inputs.author.as_str()),
        ],
    );

    fs::write(&manifest_path, manifest_body)
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    fs::write(&star_path, star_body)
        .with_context(|| format!("writing {}", star_path.display()))?;

    Ok(vec![manifest_path, star_path])
}

fn prompt<F>(label: &str, validator: F) -> Result<String>
where
    F: Fn(&str) -> std::result::Result<(), String> + 'static,
{
    let theme = ColorfulTheme::default();
    let value: String = Input::with_theme(&theme)
        .with_prompt(label)
        .validate_with(move |s: &String| validator(s))
        .interact_text()
        .with_context(|| format!("reading {label}"))?;
    Ok(value.trim().to_string())
}

fn refuse_overwrite(path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            "refusing to overwrite existing {}. Move it aside or `cd` into an empty directory and retry.",
            path.display()
        );
    }
    Ok(())
}

fn render_template(src: &str, substitutions: &[(&str, &str)]) -> String {
    let mut out = src.to_string();
    for (key, value) in substitutions {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn inputs() -> Inputs {
        Inputs {
            name: "Test App".into(),
            summary: "A test app".into(),
            desc: "This is a test.".into(),
            author: "Tom".into(),
        }
    }

    #[test]
    fn scaffold_writes_manifest_and_star() {
        let dir = tempdir().unwrap();
        let written = scaffold(dir.path(), &inputs()).unwrap();
        assert_eq!(written.len(), 2);
        assert!(written[0].ends_with("manifest.yaml"));
        assert!(written[1].ends_with("test_app.star"));

        let manifest = fs::read_to_string(&written[0]).unwrap();
        assert!(manifest.contains("id: test-app"));
        assert!(manifest.contains("name: Test App"));
        assert!(manifest.contains("fileName: test_app.star"));
        assert!(manifest.contains("packageName: testapp"));

        let star = fs::read_to_string(&written[1]).unwrap();
        assert!(star.contains("Applet: Test App"));
        assert!(star.contains("def main(config)"));
    }

    #[test]
    fn scaffold_refuses_overwrite() {
        let dir = tempdir().unwrap();
        scaffold(dir.path(), &inputs()).unwrap();
        let err = scaffold(dir.path(), &inputs()).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn scaffold_rejects_invalid_name() {
        let dir = tempdir().unwrap();
        let bad = Inputs {
            name: "lowercase app".into(),
            ..inputs()
        };
        let err = scaffold(dir.path(), &bad).unwrap_err();
        assert!(err.to_string().contains("title case"));
    }

    #[test]
    fn render_template_substitutes_placeholders() {
        let src = "hello {{who}}, meet {{other}}";
        let out = render_template(src, &[("who", "world"), ("other", "moon")]);
        assert_eq!(out, "hello world, meet moon");
    }

    #[test]
    fn scaffolded_applet_is_loadable() {
        use rustlet_runtime::manifest::Manifest;
        use rustlet_runtime::Applet;
        let dir = tempdir().unwrap();
        let written = scaffold(dir.path(), &inputs()).unwrap();
        // Manifest parses and validates.
        let manifest = Manifest::load_from_path(&written[0]).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.id, "test-app");

        // Starlark source lints cleanly (parse + sandbox eval).
        let src = fs::read_to_string(&written[1]).unwrap();
        let applet = Applet::new();
        let issues = applet.lint_source("test-app", &src, Some(dir.path())).unwrap();
        assert!(issues.is_empty(), "lint issues: {issues:?}");
    }
}
