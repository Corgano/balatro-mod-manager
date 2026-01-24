use crate::thumb_queue::ThumbnailManager;
use bmm_lib::{database::Database, discord_rpc::DiscordRpcManager};

/// Global application state shared with Tauri commands.
pub struct AppState {
    pub db: std::sync::Mutex<Database>,
    pub discord_rpc: tokio::sync::Mutex<DiscordRpcManager>,
    pub thumbs: ThumbnailManager,
}
