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
use axum::{routing::get, Router};
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
}

pub fn run(args: Args) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;
    rt.block_on(run_inner(args))
}

async fn run_inner(args: Args) -> Result<()> {
    if !args.path.exists() {
        anyhow::bail!("path does not exist: {}", args.path.display());
    }

    let (reload_tx, _) = broadcast::channel::<()>(16);
    let state: SharedState = Arc::new(AppState {
        applet_path: args.path.clone(),
        width: args.width,
        height: args.height,
        reload_tx,
    });

    watcher::spawn(state.clone())?;

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/preview.webp", get(handlers::preview))
        .route("/events", get(handlers::events))
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
    let url = format!("http://{}:{}/", bound.ip(), bound.port());
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

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("failed to install ctrl-c handler: {e}");
    }
    eprintln!("shutting down");
}
