//! File watcher task. Uses notify-debouncer-full on a dedicated OS thread so
//! the sync notify backend doesn't fight tokio. Events are filtered to
//! `.star`, `.yaml`, `.yml` and fanned out via the broadcast channel.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};

use super::state::SharedState;

const DEBOUNCE: Duration = Duration::from_millis(150);

pub fn spawn(state: SharedState) -> Result<()> {
    let watch_root = resolve_watch_root(&state.applet_path);
    let recursive = state.applet_path.is_dir();
    let tx = state.reload_tx.clone();

    std::thread::Builder::new()
        .name("rustlet-watcher".into())
        .spawn(move || {
            if let Err(e) = run(&watch_root, recursive, tx) {
                eprintln!("watcher stopped: {e:#}");
            }
        })
        .context("spawning watcher thread")?;
    Ok(())
}

fn run(
    watch_root: &Path,
    recursive: bool,
    reload_tx: tokio::sync::broadcast::Sender<()>,
) -> Result<()> {
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(DEBOUNCE, None, sync_tx)
        .context("constructing file watcher")?;

    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    debouncer
        .watch(watch_root, mode)
        .with_context(|| format!("watching {}", watch_root.display()))?;

    while let Ok(res) = sync_rx.recv() {
        match res {
            Ok(events) => {
                if events.iter().any(|e| interesting(&e.paths)) {
                    // It's fine if no subscribers are listening yet.
                    let _ = reload_tx.send(());
                }
            }
            Err(errs) => {
                for err in errs {
                    eprintln!("watcher error: {err}");
                }
            }
        }
    }
    Ok(())
}

fn resolve_watch_root(applet_path: &Path) -> PathBuf {
    if applet_path.is_dir() {
        return applet_path.to_path_buf();
    }
    applet_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn interesting(paths: &[PathBuf]) -> bool {
    paths.iter().any(|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("star") | Some("yaml") | Some("yml")
        )
    })
}
