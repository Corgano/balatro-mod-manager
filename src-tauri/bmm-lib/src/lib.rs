pub mod balamod;
pub mod cache;
pub mod database;
pub mod discord_rpc;
pub mod errors;
pub mod finder;
pub mod installer;
pub mod local_mod_detection;
pub mod logging;
pub mod lovely;
pub mod mod_collections;
pub mod smods_installer;

use std::path::{Path, PathBuf};

/// Resolve the mods directory on the host (config dir).
pub fn mods_dir() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("Balatro").join("Mods");
    }
    // Fallback to current dir if config_dir is unavailable
    Path::new(".").join("Mods")
}
