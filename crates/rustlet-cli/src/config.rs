//! User config for the Tronbyt/Tidbyt API client. Mirrors pixlet's viper-backed
//! yaml at `${UserConfigDir}/tronbyt/config.yaml`. We persist the base URL and
//! API token so users don't need to retype them on every `push`.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const URL_KEY: &str = "url";
pub const TOKEN_KEY: &str = "token";

pub const ENV_URL: &str = "RUSTLET_URL";
pub const ENV_TOKEN: &str = "RUSTLET_TOKEN";

// pixlet's own credential env vars, honored as a fallback so existing pixlet
// setups keep working (.reference/pixlet/cmd/flags/api_credentials.go).
pub const ENV_PIXLET_URL: &str = "PIXLET_URL";
pub const ENV_PIXLET_TOKEN: &str = "PIXLET_TOKEN";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| anyhow!("could not locate user config directory"))?;
    Ok(base.join("rustlet").join("config.yaml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Config::default());
    }
    serde_yaml::from_str::<Config>(&text)
        .with_context(|| format!("parsing {}", path.display()))
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = serde_yaml::to_string(cfg)?;
    fs::write(&path, text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    restrict_permissions(&path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
        .with_context(|| format!("chmod 0600 {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

pub fn set_value(key: &str, value: &str) -> Result<()> {
    let mut cfg = load().unwrap_or_default();
    match key {
        URL_KEY => cfg.url = Some(value.to_string()),
        TOKEN_KEY => cfg.token = Some(value.to_string()),
        other => bail!("unknown config key: {other} (valid keys: url, token)"),
    }
    save(&cfg)
}

pub fn get_value(key: &str) -> Result<Option<String>> {
    let cfg = load()?;
    match key {
        URL_KEY => Ok(cfg.url),
        TOKEN_KEY => Ok(cfg.token),
        other => bail!("unknown config key: {other} (valid keys: url, token)"),
    }
}

/// Apply the credential precedence to candidate values: CLI flag >
/// primary env (`RUSTLET_*`) > pixlet env (`PIXLET_*`) > persisted config file.
/// Pure and side-effect free so the precedence is unit-testable without touching
/// the real environment or config file.
fn pick_credential(
    cli: Option<&str>,
    env_primary: Option<String>,
    env_fallback: Option<String>,
    cfg: Option<String>,
) -> Option<String> {
    cli.map(str::to_string)
        .or(env_primary)
        .or(env_fallback)
        .or(cfg)
}

/// Resolve the base URL and bearer token for API calls. Precedence matches
/// pixlet: CLI flag > `RUSTLET_*` env > `PIXLET_*` env > persisted config file.
/// Returns a user-facing error pointing at `rustlet login` when either is missing.
pub fn resolve_credentials(
    cli_url: Option<&str>,
    cli_token: Option<&str>,
) -> Result<(String, String)> {
    let cfg = load().unwrap_or_default();

    let url = pick_credential(
        cli_url,
        std::env::var(ENV_URL).ok(),
        std::env::var(ENV_PIXLET_URL).ok(),
        cfg.url,
    )
    .ok_or_else(|| {
        anyhow!(
            "API url not set. Run `rustlet login`, use `rustlet config set url <url>`, pass --url, or set {ENV_URL}"
        )
    })?;

    let token = pick_credential(
        cli_token,
        std::env::var(ENV_TOKEN).ok(),
        std::env::var(ENV_PIXLET_TOKEN).ok(),
        cfg.token,
    )
    .ok_or_else(|| {
        anyhow!(
            "API token not set. Run `rustlet login`, use `rustlet config set token <token>`, pass --token, or set {ENV_TOKEN}"
        )
    })?;

    Ok((url, token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_yaml() {
        let cfg = Config {
            url: Some("http://tronbyt.local".into()),
            token: Some("secret".into()),
        };
        let text = serde_yaml::to_string(&cfg).unwrap();
        let back: Config = serde_yaml::from_str(&text).unwrap();
        assert_eq!(back.url.as_deref(), Some("http://tronbyt.local"));
        assert_eq!(back.token.as_deref(), Some("secret"));
    }

    #[test]
    fn config_empty_yaml_parses_as_default() {
        let back: Config = serde_yaml::from_str("").unwrap_or_default();
        assert!(back.url.is_none());
        assert!(back.token.is_none());
    }

    #[test]
    fn credential_precedence_cli_over_rustlet_over_pixlet_over_config() {
        // CLI flag wins over every other source.
        assert_eq!(
            pick_credential(
                Some("cli"),
                Some("rustlet".into()),
                Some("pixlet".into()),
                Some("cfg".into())
            ),
            Some("cli".to_string())
        );
        // RUSTLET_* beats the pixlet fallback and the config file.
        assert_eq!(
            pick_credential(None, Some("rustlet".into()), Some("pixlet".into()), Some("cfg".into())),
            Some("rustlet".to_string())
        );
        // PIXLET_* fallback beats the config file.
        assert_eq!(
            pick_credential(None, None, Some("pixlet".into()), Some("cfg".into())),
            Some("pixlet".to_string())
        );
        // Config file is the last resort.
        assert_eq!(
            pick_credential(None, None, None, Some("cfg".into())),
            Some("cfg".to_string())
        );
        // Nothing set anywhere.
        assert_eq!(pick_credential(None, None, None, None), None);
    }
}
