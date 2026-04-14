//! `rustlet api` — dev HTTP render server. Accepts JSON POSTs with a path
//! and optional config overrides, returns rendered image bytes.
//!
//! Mirrors pixlet's `cmd/api.go`. Path sandboxing is a canonicalize-and-
//! startswith check against the server's working directory; paths that
//! escape return 400.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::util::{render_bytes, RenderBytesOptions};

const RENDER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_DURATION: Duration = Duration::from_secs(15);

pub struct Args {
    pub host: String,
    pub port: u16,
    pub format: Format,
    pub silent: bool,
}

#[derive(Clone, Copy)]
pub enum Format {
    Gif,
    Webp,
}

impl Format {
    fn output(self) -> rustlet_encode::OutputFormat {
        match self {
            Format::Gif => rustlet_encode::OutputFormat::Gif,
            Format::Webp => rustlet_encode::OutputFormat::WebP,
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Format::Gif => "image/gif",
            Format::Webp => "image/webp",
        }
    }
}

struct AppState {
    root: PathBuf,
    format: Format,
    silent: bool,
}

#[derive(Debug, Deserialize)]
struct RenderRequest {
    path: String,
    #[serde(default)]
    config: HashMap<String, String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    magnify: Option<u32>,
    #[serde(default)]
    color_filter: Option<String>,
    #[serde(default, rename = "2x")]
    is_2x: bool,
    #[serde(default)]
    locale: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;
    rt.block_on(run_inner(args))
}

async fn run_inner(args: Args) -> Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let state = Arc::new(AppState {
        root,
        format: args.format,
        silent: args.silent,
    });

    let app = Router::new()
        .route("/api/render", post(handle_render))
        .with_state(state);

    let host: IpAddr = args
        .host
        .parse()
        .with_context(|| format!("parsing host {}", args.host))?;
    let addr = SocketAddr::new(host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    let bound = listener.local_addr()?;
    eprintln!("api serving at http://{}:{}/api/render", bound.ip(), bound.port());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve failed")?;
    Ok(())
}

async fn handle_render(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RenderRequest>,
) -> Response {
    let sandboxed = match sandbox_path(&state.root, &req.path) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("path rejected: {e:#}"),
            )
                .into_response()
        }
    };

    let color_filter = match req.color_filter.as_deref() {
        None | Some("") | Some("none") => rustlet_encode::Filter::None,
        Some(other) => match parse_filter(other) {
            Ok(f) => f,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("{e}")).into_response();
            }
        },
    };

    let opts = RenderBytesOptions {
        width: req.width.unwrap_or(64),
        height: req.height.unwrap_or(32),
        magnify: req.magnify.unwrap_or(1).max(1),
        is_2x: req.is_2x,
        color_filter,
        silent: state.silent,
        locale: req.locale,
        format: state.format.output(),
        max_duration: Some(MAX_DURATION),
    };

    let join = tokio::task::spawn_blocking(move || render_bytes(&sandboxed, &req.config, &opts));
    match tokio::time::timeout(RENDER_TIMEOUT, join).await {
        Ok(Ok(Ok(bytes))) => Response::builder()
            .header(header::CONTENT_TYPE, state.format.content_type())
            .header(header::CACHE_CONTROL, "no-store")
            .body(bytes.into())
            .unwrap(),
        Ok(Ok(Err(e))) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("render failed: {e:#}"),
        )
            .into_response(),
        Ok(Err(join_err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("render task panicked: {join_err}"),
        )
            .into_response(),
        Err(_elapsed) => (
            StatusCode::GATEWAY_TIMEOUT,
            format!("render exceeded {}s", RENDER_TIMEOUT.as_secs()),
        )
            .into_response(),
    }
}

fn sandbox_path(root: &Path, requested: &str) -> Result<PathBuf> {
    if requested.is_empty() {
        bail!("path is empty");
    }
    if requested.contains("..") {
        bail!("path contains `..`");
    }
    let joined = root.join(requested);
    let canonical = std::fs::canonicalize(&joined)
        .with_context(|| format!("resolving {}", joined.display()))?;
    if !canonical.starts_with(root) {
        bail!("path escapes server root");
    }
    Ok(canonical)
}

fn parse_filter(name: &str) -> Result<rustlet_encode::Filter> {
    use rustlet_encode::Filter;
    Ok(match name {
        "none" => Filter::None,
        "dimmed" => Filter::Dimmed,
        "red-shift" | "redshift" => Filter::RedShift,
        "warm" => Filter::Warm,
        "sunset" => Filter::Sunset,
        "sepia" => Filter::Sepia,
        "vintage" => Filter::Vintage,
        "dusk" => Filter::Dusk,
        "cool" => Filter::Cool,
        "bw" | "b-w" => Filter::BW,
        "ice" => Filter::Ice,
        "moonlight" => Filter::Moonlight,
        "neon" => Filter::Neon,
        "pastel" => Filter::Pastel,
        other => bail!("unknown color filter: {other}"),
    })
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install ctrl-c handler: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_rejects_dotdot() {
        let tmp = std::env::temp_dir();
        assert!(sandbox_path(&tmp, "../etc/passwd").is_err());
    }

    #[test]
    fn sandbox_rejects_empty() {
        let tmp = std::env::temp_dir();
        assert!(sandbox_path(&tmp, "").is_err());
    }

    #[test]
    fn parse_filter_known_names() {
        assert!(matches!(
            parse_filter("sepia").unwrap(),
            rustlet_encode::Filter::Sepia
        ));
        assert!(matches!(
            parse_filter("bw").unwrap(),
            rustlet_encode::Filter::BW
        ));
        assert!(parse_filter("bogus").is_err());
    }
}
