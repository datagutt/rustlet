use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

/// Shared state for the serve HTTP handlers. `reload_tx` fans file-change
/// events out to every connected SSE client.
pub struct AppState {
    pub applet_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub reload_tx: broadcast::Sender<()>,
}

pub type SharedState = Arc<AppState>;
