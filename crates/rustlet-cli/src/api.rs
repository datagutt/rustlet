//! Synchronous HTTP client for the Tronbyt/Tidbyt `/v0/devices` API. Both
//! backends share the same wire format, so a single client handles either
//! simply by swapping the base URL and bearer token.
//!
//! Field names are pinned with explicit `#[serde(rename = ...)]` attributes
//! because pixlet's Go JSON tags mix conventions (`deviceID`, `installationID`,
//! `appID`, `displayName`). A single `rename_all = "camelCase"` would produce
//! `deviceId` which the API rejects.

use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

const API_PREFIX: &str = "v0";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Device {
    pub id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DevicesEnvelope {
    #[serde(default)]
    devices: Vec<Device>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Installation {
    pub id: String,
    #[serde(rename = "appID", default)]
    pub app_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallationsEnvelope {
    #[serde(default)]
    installations: Vec<Installation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushPayload {
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub image: String,
    #[serde(rename = "installationID")]
    pub installation_id: String,
    pub background: bool,
}

pub struct Client {
    agent: ureq::Agent,
    base_url: String,
    token: String,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        if base_url.is_empty() {
            bail!("base url is empty");
        }
        if token.is_empty() {
            bail!("api token is empty");
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .user_agent(format!("rustlet/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}/{}",
            self.base_url,
            API_PREFIX,
            path.trim_start_matches('/')
        )
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn push(
        &self,
        device_id: &str,
        image: &[u8],
        installation_id: Option<&str>,
        background: bool,
    ) -> Result<()> {
        if background && installation_id.map(str::is_empty).unwrap_or(true) {
            bail!("--background requires --installation-id");
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(image);
        let payload = PushPayload {
            device_id: device_id.to_string(),
            image: encoded,
            installation_id: installation_id.unwrap_or("").to_string(),
            background,
        };
        let body = serde_json::to_vec(&payload)?;

        let url = self.url(&format!("devices/{device_id}/push"));
        let response = self
            .agent
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .send(&body[..])
            .map_err(|e| anyhow!("push request failed: {e}"))?;

        expect_2xx(response, "push")
    }

    pub fn delete(&self, device_id: &str, installation_id: &str) -> Result<()> {
        let url = self.url(&format!(
            "devices/{device_id}/installations/{installation_id}"
        ));
        let response = self
            .agent
            .delete(&url)
            .header("Authorization", self.auth_header())
            .call()
            .map_err(|e| anyhow!("delete request failed: {e}"))?;

        expect_2xx(response, "delete")
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let url = self.url("devices");
        let response = self
            .agent
            .get(&url)
            .header("Authorization", self.auth_header())
            .call()
            .map_err(|e| anyhow!("devices request failed: {e}"))?;

        let body = read_success_body(response, "devices")?;
        let envelope: DevicesEnvelope =
            serde_json::from_str(&body).context("parsing devices response")?;
        Ok(envelope.devices)
    }

    pub fn installations(&self, device_id: &str) -> Result<Vec<Installation>> {
        let url = self.url(&format!("devices/{device_id}/installations"));
        let response = self
            .agent
            .get(&url)
            .header("Authorization", self.auth_header())
            .call()
            .map_err(|e| anyhow!("installations request failed: {e}"))?;

        let body = read_success_body(response, "installations")?;
        let envelope: InstallationsEnvelope =
            serde_json::from_str(&body).context("parsing installations response")?;
        Ok(envelope.installations)
    }
}

type UreqResponse = ureq::http::Response<ureq::Body>;

fn expect_2xx(response: UreqResponse, op: &str) -> Result<()> {
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }
    let body = read_body_text(response).unwrap_or_default();
    bail!("{op} failed: HTTP {status}: {body}")
}

fn read_success_body(response: UreqResponse, op: &str) -> Result<String> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = read_body_text(response).unwrap_or_default();
        bail!("{op} failed: HTTP {status}: {body}");
    }
    read_body_text(response)
}

fn read_body_text(response: UreqResponse) -> Result<String> {
    let mut body = response.into_body();
    let mut text = String::new();
    body.as_reader()
        .read_to_string(&mut text)
        .context("reading response body")?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_payload_wire_format_is_tidbyt_compatible() {
        let payload = PushPayload {
            device_id: "dev1".into(),
            image: "abc".into(),
            installation_id: "rustlet-demo".into(),
            background: true,
        };
        let json = serde_json::to_string(&payload).unwrap();
        // Mixed casing: the Go ref uses deviceID / installationID (double-cap).
        // Guard against a rename_all regression by asserting exact keys.
        assert!(json.contains("\"deviceID\":\"dev1\""), "json: {json}");
        assert!(
            json.contains("\"installationID\":\"rustlet-demo\""),
            "json: {json}"
        );
        assert!(json.contains("\"image\":\"abc\""), "json: {json}");
        assert!(json.contains("\"background\":true"), "json: {json}");
    }

    #[test]
    fn device_parses_display_name_camel_case() {
        let src = r#"{"id":"dev1","displayName":"Kitchen"}"#;
        let d: Device = serde_json::from_str(src).unwrap();
        assert_eq!(d.id, "dev1");
        assert_eq!(d.display_name, "Kitchen");
    }

    #[test]
    fn installation_parses_app_id_double_cap() {
        let src = r#"{"id":"myapp","appID":"some-app"}"#;
        let i: Installation = serde_json::from_str(src).unwrap();
        assert_eq!(i.id, "myapp");
        assert_eq!(i.app_id, "some-app");
    }

    #[test]
    fn background_without_installation_id_errors() {
        let client = Client::new("http://localhost", "token").unwrap();
        let err = client.push("dev1", b"x", None, true).unwrap_err();
        assert!(err.to_string().contains("--installation-id"));
    }

    #[test]
    fn base_url_trailing_slash_stripped() {
        let client = Client::new("http://tronbyt.local/", "token").unwrap();
        assert_eq!(client.url("devices"), "http://tronbyt.local/v0/devices");
    }
}
