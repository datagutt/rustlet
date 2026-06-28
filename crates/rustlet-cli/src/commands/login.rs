//! `rustlet login` — interactive credential setup. Mirrors pixlet's `login`
//! (.reference/pixlet/cmd/login.go): prompt for the server URL and API token,
//! verify them by listing devices, then persist them. The token is never
//! printed, and `config::save` chmods the file 0600 on unix.

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Input, Password};

use crate::api::Client;
use crate::config::{self, Config};

pub fn run() -> Result<()> {
    let existing = config::load().unwrap_or_default();
    let theme = ColorfulTheme::default();

    // Default the URL to whatever is already configured so re-running `login`
    // only requires re-entering the token.
    let mut url_prompt = Input::<String>::with_theme(&theme).with_prompt("Tronbyt URL");
    if let Some(url) = existing.url.as_deref() {
        url_prompt = url_prompt.default(url.to_string());
    }
    let url = url_prompt
        .interact_text()
        .context("reading URL")?
        .trim()
        .to_string();

    let token = Password::with_theme(&theme)
        .with_prompt("API token")
        .interact()
        .context("reading API token")?;

    verify_and_save(&url, &token)?;

    let path = config::config_path()?;
    println!("credentials verified and saved to {}", path.display());
    Ok(())
}

/// Verify the credentials by listing devices, then persist them. Factored out of
/// the interactive prompt so the verify+persist path is testable with a known
/// `(url, token)` pair without driving the prompt. On a verification failure the
/// credentials are NOT persisted.
pub(crate) fn verify_and_save(url: &str, token: &str) -> Result<()> {
    let client = Client::new(url, token).context("building API client")?;
    client
        .devices()
        .context("could not verify credentials: listing devices failed")?;
    config::save(&Config {
        url: Some(url.to_string()),
        token: Some(token.to_string()),
    })
    .context("saving credentials")?;
    Ok(())
}
