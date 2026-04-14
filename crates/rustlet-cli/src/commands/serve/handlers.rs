//! HTTP handlers for the dev server.
//!
//! `/` serves the dev page. `/preview.webp` rerenders the applet and returns
//! the WebP bytes. `/events` is an SSE stream fed by the file watcher.
//!
//! Rendering is isolated from the async runtime via `spawn_blocking` with a
//! 30-second timeout — starlark's `Evaluator` is not `Send`, and a runaway
//! applet must not wedge the tokio worker pool.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
};
use futures_util::stream::Stream;
use rustlet_encode::OutputFormat;
use rustlet_runtime::Applet;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use super::state::SharedState;
use super::templates::INDEX_HTML;
use crate::util::load_applet;

const RENDER_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn root(State(state): State<SharedState>) -> Html<String> {
    let body = INDEX_HTML.replace("{path}", &state.applet_path.display().to_string());
    Html(body)
}

pub async fn preview(State(state): State<SharedState>) -> Response {
    let path = state.applet_path.clone();
    let width = state.width;
    let height = state.height;

    let join = tokio::task::spawn_blocking(move || render_once(&path, width, height));

    match tokio::time::timeout(RENDER_TIMEOUT, join).await {
        Ok(Ok(Ok(bytes))) => Response::builder()
            .header(header::CONTENT_TYPE, "image/webp")
            .header(header::CACHE_CONTROL, "no-store")
            .body(bytes.into())
            .unwrap(),
        Ok(Ok(Err(e))) => render_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
        Ok(Err(join_err)) => render_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &anyhow!("render task panicked: {join_err}"),
        ),
        Err(_elapsed) => render_error(
            StatusCode::GATEWAY_TIMEOUT,
            &anyhow!("render exceeded {}s", RENDER_TIMEOUT.as_secs()),
        ),
    }
}

pub async fn events(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.reload_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(()) => Some(Ok(Event::default().data("reload"))),
        // A Lagged means some reload events were dropped because the client
        // was slow. The next event will catch up, so swallow it quietly.
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

fn render_error(status: StatusCode, err: &anyhow::Error) -> Response {
    let body = format!("{err:#}");
    (status, [(header::CONTENT_TYPE, "text/plain")], body).into_response()
}

/// Fully synchronous render path. Mirrors the `Commands::Render` arm in
/// main.rs but always emits WebP so animated applets work in the browser.
fn render_once(path: &Path, width: u32, height: u32) -> Result<Vec<u8>> {
    let loaded = load_applet(path)?;
    // Auto-2x from manifest, matching `render`.
    let is_2x = loaded
        .manifest
        .as_ref()
        .map(|m| m.supports2x)
        .unwrap_or(false);
    let (width, height) = if is_2x { (128, 64) } else { (width, height) };

    let applet = Applet::new();
    let config = HashMap::new();
    let base_dir: Option<PathBuf> = loaded.base_dir.clone();
    let roots = applet
        .run_with_options(
            &loaded.id,
            &loaded.source,
            &config,
            width,
            height,
            is_2x,
            base_dir.as_deref(),
        )
        .context("evaluating applet")?;

    if roots.is_empty() {
        return Err(anyhow!("main() returned no roots"));
    }

    let root = roots.into_iter().next().unwrap();
    let frames = root.paint_frames(width, height);
    let delay_ms = root.delay as u16;
    let data = rustlet_encode::encode(&frames, delay_ms, OutputFormat::WebP)
        .context("encoding webp")?;
    Ok(data)
}
