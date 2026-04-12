//! Applet manifest parsing and validation. Mirrors pixlet's `manifest` package:
//! parses `manifest.yaml` files and enforces naming conventions for app store
//! listings. The validation rules are intentionally strict because these fields
//! display in the Tidbyt mobile app and are rendered verbatim.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Deserialize;

pub const MANIFEST_FILE_NAME: &str = "manifest.yaml";

/// Maximum characters for an app name. Matches pixlet's `MaxNameLength`.
pub const MAX_NAME_LENGTH: usize = 32;

/// Maximum characters for an app summary. Matches pixlet's `MaxSummaryLength`.
pub const MAX_SUMMARY_LENGTH: usize = 32;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub desc: String,
    pub author: String,
    #[serde(default, rename = "fileName")]
    pub file_name: Option<String>,
    #[serde(default, rename = "packageName")]
    pub package_name: Option<String>,
    #[serde(default)]
    pub supports2x: bool,
    #[serde(default, rename = "minPixletVersion")]
    pub min_pixlet_version: Option<String>,
    #[serde(default)]
    pub broken: bool,
}

impl Manifest {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read manifest at {}: {e}", path.display()))?;
        Self::load_from_str(&text)
    }

    pub fn load_from_str(source: &str) -> Result<Self> {
        serde_yaml::from_str::<Manifest>(source)
            .map_err(|e| anyhow!("could not parse manifest: {e}"))
    }

    /// Run all validation checks and return the first error, if any.
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        validate_name(&self.name)?;
        validate_summary(&self.summary)?;
        validate_desc(&self.desc)?;
        validate_author(&self.author)?;
        Ok(())
    }

    /// Aggregate every validation error so linters can report all issues at once.
    pub fn validate_all(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if let Err(e) = validate_id(&self.id) {
            errors.push(format!("id: {e}"));
        }
        if let Err(e) = validate_name(&self.name) {
            errors.push(format!("name: {e}"));
        }
        if let Err(e) = validate_summary(&self.summary) {
            errors.push(format!("summary: {e}"));
        }
        if let Err(e) = validate_desc(&self.desc) {
            errors.push(format!("desc: {e}"));
        }
        if let Err(e) = validate_author(&self.author) {
            errors.push(format!("author: {e}"));
        }
        errors
    }
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(anyhow!("id cannot be empty"));
    }
    if id != id.to_lowercase() {
        return Err(anyhow!(
            "ids should be lower case, {} != {}",
            id,
            id.to_lowercase()
        ));
    }
    for ch in id.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '-' {
            return Err(anyhow!(
                "ids can only contain letters, numbers, or a dash character"
            ));
        }
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("name cannot be empty"));
    }
    if name != title_case(name) {
        return Err(anyhow!(
            "'{}' should be title case, 'Fuzzy Clock' for example",
            name
        ));
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(anyhow!(
            "app names need to be less than {} characters",
            MAX_NAME_LENGTH
        ));
    }
    Ok(())
}

pub fn validate_summary(summary: &str) -> Result<()> {
    if summary.is_empty() {
        return Err(anyhow!("summary cannot be empty"));
    }
    if summary.len() > MAX_SUMMARY_LENGTH {
        return Err(anyhow!(
            "app summaries need to be less than {} characters",
            MAX_SUMMARY_LENGTH
        ));
    }
    for punct in [".", "!", "?"].iter() {
        if summary.ends_with(punct) {
            return Err(anyhow!("app summaries should not end in punctuation"));
        }
    }
    if let Some(first_word) = summary.split(' ').next() {
        if first_word != capitalize_word(first_word) {
            return Err(anyhow!(
                "app summaries should start with an uppercased character"
            ));
        }
    }
    Ok(())
}

pub fn validate_desc(desc: &str) -> Result<()> {
    if desc.is_empty() {
        return Err(anyhow!("desc cannot be empty"));
    }
    let ends_with_punct = [".", "!", "?"].iter().any(|p| desc.ends_with(p));
    if !ends_with_punct {
        return Err(anyhow!("app descriptions should end in punctuation"));
    }
    if let Some(first_word) = desc.split(' ').next() {
        if first_word != capitalize_word(first_word) {
            return Err(anyhow!(
                "app descriptions should start with an uppercased character"
            ));
        }
    }
    Ok(())
}

pub fn validate_author(author: &str) -> Result<()> {
    if author.is_empty() {
        return Err(anyhow!("author cannot be empty"));
    }
    Ok(())
}

/// Port of pixlet's `titleCase`: capitalize each word except for a short list
/// of articles and prepositions when they appear in the middle of the title.
pub fn title_case(input: &str) -> String {
    const SMALL_WORDS: &[&str] = &["a", "an", "on", "the", "to", "of"];
    input
        .split(' ')
        .enumerate()
        .map(|(i, word)| {
            if i > 0 && SMALL_WORDS.contains(&word) {
                word.to_string()
            } else {
                capitalize_word(word)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_id() {
        assert!(validate_id("fuzzy-clock").is_ok());
        assert!(validate_id("MyApp").is_err());
        assert!(validate_id("has_underscore").is_err());
        assert!(validate_id("").is_err());
    }

    #[test]
    fn valid_name_is_title_case() {
        assert!(validate_name("Fuzzy Clock").is_ok());
        assert!(validate_name("fuzzy clock").is_err());
        // Articles in the middle stay lower-case.
        assert!(validate_name("Time of the Day").is_ok());
    }

    #[test]
    fn valid_summary_no_trailing_punct() {
        assert!(validate_summary("Human readable time").is_ok());
        assert!(validate_summary("Human readable time.").is_err());
        assert!(validate_summary("lowercased start").is_err());
    }

    #[test]
    fn valid_desc_ends_in_punct() {
        assert!(validate_desc("Display the time.").is_ok());
        assert!(validate_desc("Display the time").is_err());
    }

    #[test]
    fn parse_manifest_yaml() {
        let src = r#"
id: fuzzy-clock
name: Fuzzy Clock
summary: Human readable time
desc: Display the time in a groovy way.
author: Max Timkovich
supports2x: true
"#;
        let m = Manifest::load_from_str(src).unwrap();
        assert_eq!(m.id, "fuzzy-clock");
        assert!(m.supports2x);
        m.validate().unwrap();
    }
}
