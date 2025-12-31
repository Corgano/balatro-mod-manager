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
#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Resolve the mods directory on the host (config dir).
pub fn mods_dir() -> PathBuf {
    crate::local_mod_detection::resolve_mods_dir_path().unwrap_or_else(|_| {
        // Fallback to the platform config dir if detection failed
        dirs::config_dir()
            .unwrap_or_else(|| Path::new(".").to_path_buf())
            .join("Balatro")
            .join("Mods")
    })
}
