//! HTTP handlers for the dev server.
//!
//! Endpoints:
//!
//!   - `GET /`                       — the dev shell HTML page
//!   - `GET /events`                 — SSE stream driven by the file watcher
//!   - `GET /preview.webp`           — legacy shortcut; renders with empty config
//!   - `POST /api/v1/preview`        — multipart form, returns JSON preview
//!   - `POST /api/v1/preview.webp`   — multipart form, returns raw WebP
//!   - `POST /api/v1/preview.gif`    — multipart form, returns raw GIF
//!   - `GET  /api/v1/schema`         — schema JSON for the current applet
//!   - `POST /api/v1/handlers/{h}`   — invokes a named starlark handler
//!
//! Rendering is isolated from the async runtime via `spawn_blocking` —
//! starlark's `Evaluator` is not `Send`, and a runaway applet must not wedge
//! tokio worker threads. Each response is wall-clock timeout-wrapped with
//! `state.timeout`.
//!
//! Reserved form keys `_renderScale`, `_metaLocale`, `_metaTimezone` are
//! stripped from the config map before the applet sees it, matching pixlet's
//! browser.go behavior.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;

use anyhow::{anyhow, Result};
use axum::{
    body::Bytes,
    extract::{Multipart, Path as AxumPath, State},
    http::{header, StatusCode},
    response::{sse::Event, Html, IntoResponse, Json, Response, Sse},
};
use base64::Engine;
use futures_util::stream::Stream;
use rustlet_encode::OutputFormat;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use super::state::SharedState;
use super::templates::INDEX_HTML;
use crate::util::{load_applet, render_bytes, RenderBytesOptions};

const META_RENDER_SCALE: &str = "_renderScale";
const META_LOCALE: &str = "_metaLocale";
const META_TIMEZONE: &str = "_metaTimezone";

// ----- Root page and SSE -----

pub async fn root(State(state): State<SharedState>) -> Html<String> {
    let body = INDEX_HTML.replace("{path}", &state.applet_path.display().to_string());
    Html(body)
}

pub async fn events(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.reload_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(()) => Some(Ok(Event::default().data("reload"))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

// ----- Legacy shortcut that still serves the old URL for the dev page's <img> -----

pub async fn preview_legacy(State(state): State<SharedState>) -> Response {
    preview_image_response(state, HashMap::new(), OutputFormat::WebP, "image/webp").await
}

// ----- /api/v1/preview.webp and .gif -----

pub async fn api_preview_webp(
    State(state): State<SharedState>,
    multipart: Multipart,
) -> Response {
    let config = match parse_multipart_config(multipart).await {
        Ok(c) => c,
        Err(e) => return bad_request(&e),
    };
    preview_image_response(state, config, OutputFormat::WebP, "image/webp").await
}

pub async fn api_preview_gif(
    State(state): State<SharedState>,
    multipart: Multipart,
) -> Response {
    let config = match parse_multipart_config(multipart).await {
        Ok(c) => c,
        Err(e) => return bad_request(&e),
    };
    preview_image_response(state, config, OutputFormat::Gif, "image/gif").await
}

// ----- /api/v1/preview -----

#[derive(Debug, Serialize)]
struct PreviewJson {
    title: String,
    img: String,
    img_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn api_preview(
    State(state): State<SharedState>,
    multipart: Multipart,
) -> Response {
    let config = match parse_multipart_config(multipart).await {
        Ok(c) => c,
        Err(e) => return bad_request(&e),
    };
    maybe_save_config(&state, &config);

    let (scale_is_2x, _) = meta_scale(&config);
    let stripped = strip_meta_keys(&config);
    let is_2x = state.is_2x || scale_is_2x;

    let path = state.applet_path.clone();
    let width = state.width;
    let height = state.height;
    let max_duration = Some(state.max_duration);
    let timeout = state.timeout;

    let join = tokio::task::spawn_blocking(move || {
        render_config(&path, &stripped, width, height, is_2x, OutputFormat::WebP, max_duration)
    });
    let bytes = match tokio::time::timeout(timeout, join).await {
        Ok(Ok(Ok(b))) => b,
        Ok(Ok(Err(e))) => {
            return Json(PreviewJson {
                title: applet_title(&state),
                img: String::new(),
                img_type: "webp".into(),
                error: Some(format!("{e:#}")),
            })
            .into_response();
        }
        Ok(Err(join_err)) => {
            return Json(PreviewJson {
                title: applet_title(&state),
                img: String::new(),
                img_type: "webp".into(),
                error: Some(format!("render task panicked: {join_err}")),
            })
            .into_response();
        }
        Err(_elapsed) => {
            return Json(PreviewJson {
                title: applet_title(&state),
                img: String::new(),
                img_type: "webp".into(),
                error: Some(format!("render exceeded {}s", timeout.as_secs())),
            })
            .into_response();
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Json(PreviewJson {
        title: applet_title(&state),
        img: b64,
        img_type: "webp".into(),
        error: None,
    })
    .into_response()
}

// ----- /api/v1/schema -----

pub async fn api_schema(State(state): State<SharedState>) -> Response {
    let path = state.applet_path.clone();
    let timeout = state.timeout;

    let join = tokio::task::spawn_blocking(move || -> Result<String> {
        let loaded = load_applet(&path)?;
        let applet = rustlet_runtime::Applet::new();
        applet.schema_json(&loaded.id, &loaded.source, loaded.base_dir.as_deref())
    });

    match tokio::time::timeout(timeout, join).await {
        Ok(Ok(Ok(json_text))) => {
            let parsed: Value =
                serde_json::from_str(&json_text).unwrap_or_else(|_| json!({"schema": []}));
            Json(parsed).into_response()
        }
        Ok(Ok(Err(e))) => {
            // No get_schema() or load failure: return an empty schema so the
            // React frontend still renders the preview area.
            if e.to_string().contains("get_schema") {
                return Json(json!({"version": "1", "schema": [], "notifications": []}))
                    .into_response();
            }
            render_error(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
        Ok(Err(join_err)) => render_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &anyhow!("schema task panicked: {join_err}"),
        ),
        Err(_elapsed) => render_error(
            StatusCode::GATEWAY_TIMEOUT,
            &anyhow!("schema exceeded {}s", timeout.as_secs()),
        ),
    }
}

// ----- /api/v1/push -----

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    #[serde(default, rename = "deviceID", alias = "deviceId")]
    pub device_id: String,
    #[serde(default, rename = "apiToken")]
    pub api_token: String,
    #[serde(default, rename = "installationID", alias = "installationId")]
    pub installation_id: String,
    /// Pixlet sends this as a string "true"/"false". Accept either.
    #[serde(default)]
    pub background: StringBool,
    /// Override the default base URL (e.g. for Tronbyt). When empty the
    /// api::Client's `url` argument is treated as required; we surface an
    /// error to the caller.
    #[serde(default)]
    pub url: Option<String>,
    /// Every other field in the body is treated as a config override.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
pub enum StringBool {
    Bool(bool),
    Str(String),
    #[default]
    #[serde(skip)]
    Unset,
}

impl StringBool {
    fn as_bool(&self) -> bool {
        match self {
            StringBool::Bool(b) => *b,
            StringBool::Str(s) => s == "true",
            StringBool::Unset => false,
        }
    }
}

pub async fn api_push(
    State(state): State<SharedState>,
    Json(req): Json<PushRequest>,
) -> Response {
    if req.device_id.is_empty() {
        return bad_request(&anyhow!("deviceID is required"));
    }
    if req.api_token.is_empty() {
        return bad_request(&anyhow!("apiToken is required"));
    }

    // Merge any extra config fields into a string map for the render.
    let mut config: HashMap<String, String> = HashMap::new();
    for (k, v) in req.extra {
        if let Some(s) = v.as_str() {
            config.insert(k, s.to_string());
        } else {
            config.insert(k, v.to_string());
        }
    }
    let stripped = strip_meta_keys(&config);

    // Render the current frame with the supplied config.
    let path = state.applet_path.clone();
    let width = state.width;
    let height = state.height;
    let is_2x = state.is_2x;
    let max_duration = Some(state.max_duration);
    let timeout = state.timeout;

    let render_join = tokio::task::spawn_blocking(move || {
        render_config(&path, &stripped, width, height, is_2x, OutputFormat::WebP, max_duration)
    });
    let bytes = match tokio::time::timeout(timeout, render_join).await {
        Ok(Ok(Ok(b))) => b,
        Ok(Ok(Err(e))) => return render_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
        Ok(Err(j)) => {
            return render_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &anyhow!("render task panicked: {j}"),
            );
        }
        Err(_) => {
            return render_error(
                StatusCode::GATEWAY_TIMEOUT,
                &anyhow!("render exceeded {}s", timeout.as_secs()),
            );
        }
    };

    // Fall through to the api::Client on a blocking thread (ureq is sync).
    let url = match req.url.filter(|s| !s.is_empty()) {
        Some(u) => u,
        None => "https://api.tidbyt.com".to_string(),
    };
    let token = req.api_token;
    let device_id = req.device_id;
    let installation_id = (!req.installation_id.is_empty()).then_some(req.installation_id);
    let background = req.background.as_bool();

    let push_join = tokio::task::spawn_blocking(move || -> Result<()> {
        let client = crate::api::Client::new(&url, &token)?;
        client.push(&device_id, &bytes, installation_id.as_deref(), background)
    });
    match push_join.await {
        Ok(Ok(())) => (StatusCode::OK, "pushed").into_response(),
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, format!("push failed: {e:#}")).into_response(),
        Err(j) => render_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &anyhow!("push task panicked: {j}"),
        ),
    }
}

// ----- /api/v1/handlers/{handler_name} -----

#[derive(Debug, Deserialize)]
pub struct HandlerRequest {
    #[serde(default)]
    pub config: HashMap<String, String>,
    #[serde(default)]
    pub param: String,
    // pixlet also sends an `id` field identifying the schema field; we
    // accept and ignore it via serde's default unknown-field tolerance.
}

pub async fn api_handler(
    State(state): State<SharedState>,
    AxumPath(handler_name): AxumPath<String>,
    Json(req): Json<HandlerRequest>,
) -> Response {
    let path = state.applet_path.clone();
    let timeout = state.timeout;
    let param = req.param;
    let config = strip_meta_keys(&req.config);

    let join = tokio::task::spawn_blocking(move || -> Result<String> {
        let loaded = load_applet(&path)?;
        let applet = rustlet_runtime::Applet::new();
        applet.call_schema_handler(
            &loaded.id,
            &loaded.source,
            loaded.base_dir.as_deref(),
            &handler_name,
            &config,
            &param,
        )
    });

    match tokio::time::timeout(timeout, join).await {
        Ok(Ok(Ok(json_text))) => {
            let parsed: Value =
                serde_json::from_str(&json_text).unwrap_or(Value::Null);
            Json(parsed).into_response()
        }
        Ok(Ok(Err(e))) => render_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
        Ok(Err(join_err)) => render_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &anyhow!("handler task panicked: {join_err}"),
        ),
        Err(_elapsed) => render_error(
            StatusCode::GATEWAY_TIMEOUT,
            &anyhow!("handler exceeded {}s", timeout.as_secs()),
        ),
    }
}

// ----- shared helpers -----

async fn preview_image_response(
    state: SharedState,
    config: HashMap<String, String>,
    format: OutputFormat,
    content_type: &'static str,
) -> Response {
    maybe_save_config(&state, &config);

    let (scale_is_2x, _) = meta_scale(&config);
    let stripped = strip_meta_keys(&config);
    let is_2x = state.is_2x || scale_is_2x;

    let path = state.applet_path.clone();
    let width = state.width;
    let height = state.height;
    let max_duration = Some(state.max_duration);
    let timeout = state.timeout;

    let join = tokio::task::spawn_blocking(move || {
        render_config(&path, &stripped, width, height, is_2x, format, max_duration)
    });

    let result: std::result::Result<Vec<u8>, (StatusCode, anyhow::Error)> =
        match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(bytes))) => Ok(bytes),
            Ok(Ok(Err(e))) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
            Ok(Err(join_err)) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                anyhow!("render task panicked: {join_err}"),
            )),
            Err(_elapsed) => Err((
                StatusCode::GATEWAY_TIMEOUT,
                anyhow!("render exceeded {}s", timeout.as_secs()),
            )),
        };

    match result {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Bytes::from(bytes).into())
            .unwrap(),
        Err((status, err)) => render_error(status, &err),
    }
}

async fn parse_multipart_config(mut multipart: Multipart) -> Result<HashMap<String, String>> {
    let mut config = HashMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| anyhow!("reading multipart: {e}"))?
    {
        let name = match field.name() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let value = field
            .text()
            .await
            .map_err(|e| anyhow!("reading field `{name}`: {e}"))?;
        config.insert(name, value);
    }
    Ok(config)
}

/// Strip the three reserved meta keys pixlet uses to carry UI state through
/// the preview form without them reaching the applet's config map.
fn strip_meta_keys(config: &HashMap<String, String>) -> HashMap<String, String> {
    config
        .iter()
        .filter(|(k, _)| {
            k.as_str() != META_RENDER_SCALE
                && k.as_str() != META_LOCALE
                && k.as_str() != META_TIMEZONE
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Extract `_renderScale` and return `(is_2x, raw_value)`. Invalid values
/// default to false.
fn meta_scale(config: &HashMap<String, String>) -> (bool, Option<&str>) {
    match config.get(META_RENDER_SCALE).map(String::as_str) {
        Some("2") => (true, Some("2")),
        Some(other) => (false, Some(other)),
        None => (false, None),
    }
}

fn maybe_save_config(state: &SharedState, config: &HashMap<String, String>) {
    let Some(ref path) = state.save_config else {
        return;
    };
    match serde_json::to_vec_pretty(config) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, &bytes) {
                eprintln!("could not write save-config {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("could not serialize save-config: {e}"),
    }
}

fn applet_title(state: &SharedState) -> String {
    state
        .applet_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("rustlet")
        .to_string()
}

fn render_error(status: StatusCode, err: &anyhow::Error) -> Response {
    let body = format!("{err:#}");
    (status, [(header::CONTENT_TYPE, "text/plain")], body).into_response()
}

fn bad_request(err: &anyhow::Error) -> Response {
    render_error(StatusCode::BAD_REQUEST, err)
}

fn render_config(
    path: &Path,
    config: &HashMap<String, String>,
    width: u32,
    height: u32,
    is_2x: bool,
    format: OutputFormat,
    max_duration: Option<std::time::Duration>,
) -> Result<Vec<u8>> {
    let opts = RenderBytesOptions {
        width,
        height,
        is_2x,
        format,
        silent: false,
        max_duration,
        ..Default::default()
    };
    render_bytes(path, config, &opts)
}
