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
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
};
use futures_util::stream::Stream;
use rustlet_encode::OutputFormat;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use super::state::SharedState;
use super::templates::INDEX_HTML;
use crate::util::{render_bytes, RenderBytesOptions};

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

/// Thin adapter over the shared [`render_bytes`] helper. Serve always emits
/// WebP so animated applets work in the browser, and uses an empty config
/// until phase 8 wires up the schema form.
fn render_once(path: &Path, width: u32, height: u32) -> Result<Vec<u8>> {
    let opts = RenderBytesOptions {
        width,
        height,
        format: OutputFormat::WebP,
        silent: false,
        max_duration: None,
        ..Default::default()
    };
    render_bytes(path, &HashMap::new(), &opts)
}
