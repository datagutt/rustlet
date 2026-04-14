use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

/// Shared state for the serve HTTP handlers. `reload_tx` fans file-change
/// events out to every connected SSE client.
pub struct AppState {
    pub applet_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub is_2x: bool,
    pub max_duration: Duration,
    pub timeout: Duration,
    pub save_config: Option<PathBuf>,
    pub reload_tx: broadcast::Sender<()>,
}

pub type SharedState = Arc<AppState>;
