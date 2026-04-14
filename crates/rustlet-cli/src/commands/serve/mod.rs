//! `rustlet serve` — axum-backed dev server with SSE hot reload.
//!
//! NOTE: This module is the only place in rustlet-cli that uses tokio. All
//! other commands stay fully synchronous. If you ever want to drop the async
//! runtime, this is the one contamination point to audit.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    routing::{get, post},
    Router,
};
use tokio::sync::broadcast;

mod handlers;
mod state;
mod templates;
mod watcher;

use state::{AppState, SharedState};

pub struct Args {
    pub path: PathBuf,
    pub host: String,
    pub port: u16,
    pub width: u32,
    pub height: u32,
    pub no_browser: bool,
    pub watch: bool,
    pub max_duration: Duration,
    pub timeout: Duration,
    pub path_prefix: String,
    pub save_config: Option<PathBuf>,
    pub is_2x: bool,
    pub webp_level: Option<u8>,
}

pub fn run(args: Args) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;
    rt.block_on(run_inner(args))
}

/// Normalize the user-provided path prefix into a `/foo/` shape. Matches
/// pixlet's browser.go:102-107.
fn normalize_prefix(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s.is_empty() || s == "/" {
        return "/".to_string();
    }
    if !s.starts_with('/') {
        s.insert(0, '/');
    }
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

async fn run_inner(args: Args) -> Result<()> {
    if !args.path.exists() {
        anyhow::bail!("path does not exist: {}", args.path.display());
    }

    if let Some(level) = args.webp_level {
        rustlet_encode::set_webp_level(level);
    }

    let prefix = normalize_prefix(&args.path_prefix);

    let (reload_tx, _) = broadcast::channel::<()>(16);
    let state: SharedState = Arc::new(AppState {
        applet_path: args.path.clone(),
        width: args.width,
        height: args.height,
        is_2x: args.is_2x,
        max_duration: args.max_duration,
        timeout: args.timeout,
        save_config: args.save_config,
        reload_tx,
    });

    if args.watch {
        watcher::spawn(state.clone())?;
    }

    // Build routes with the prefix baked into each path. axum 0.8's `nest`
    // does not forward requests to a root `/` route inside the nested
    // router, so we concatenate the prefix with each route's suffix
    // manually. The `&'static str` requirement is satisfied by leaking the
    // short concatenated strings — one allocation per route at startup is
    // fine.
    let prefix_trimmed = prefix.trim_end_matches('/');
    let leak = |suffix: &str| -> &'static str {
        Box::leak(format!("{prefix_trimmed}{suffix}").into_boxed_str())
    };
    let root_path: &'static str = if prefix == "/" { "/" } else { leak("/") };

    let app = Router::new()
        .route(root_path, get(handlers::root))
        .route(leak("/events"), get(handlers::events))
        .route(leak("/preview.webp"), get(handlers::preview_legacy))
        .route(leak("/api/v1/preview"), post(handlers::api_preview))
        .route(leak("/api/v1/preview.webp"), post(handlers::api_preview_webp))
        .route(leak("/api/v1/preview.gif"), post(handlers::api_preview_gif))
        .route(leak("/api/v1/schema"), get(handlers::api_schema))
        .route(leak("/api/v1/push"), post(handlers::api_push))
        .route(
            leak("/api/v1/handlers/{handler_name}"),
            post(handlers::api_handler),
        )
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
    let url = format!("http://{}:{}{}", bound.ip(), bound.port(), prefix);
    eprintln!("serving {} at {url}", args.path.display());

    if !args.no_browser {
        let to_open = url.clone();
        tokio::spawn(async move {
            // Small delay so the listener is definitely accepting before the
            // browser fetches /. 250ms matches pixlet.
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Err(e) = open::that(&to_open) {
                eprintln!("could not open browser: {e}");
            }
        });
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve failed")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_prefix_shapes() {
        assert_eq!(normalize_prefix(""), "/");
        assert_eq!(normalize_prefix("/"), "/");
        assert_eq!(normalize_prefix("foo"), "/foo/");
        assert_eq!(normalize_prefix("/foo"), "/foo/");
        assert_eq!(normalize_prefix("foo/"), "/foo/");
        assert_eq!(normalize_prefix("/foo/"), "/foo/");
    }
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install ctrl-c handler: {e}");
    }
    eprintln!("shutting down");
}
