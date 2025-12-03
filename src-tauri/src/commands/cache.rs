use std::time::{SystemTime, UNIX_EPOCH};

use bmm_lib::cache;
use bmm_lib::cache::Mod;

use crate::state::AppState;
use crate::util::map_error;

#[tauri::command]
pub async fn save_versions_cache(mod_type: String, versions: Vec<String>) -> Result<(), String> {
    map_error(cache::save_versions_cache(&mod_type, &versions))
}

#[tauri::command]
pub async fn load_versions_cache(mod_type: String) -> Result<Option<(Vec<String>, u64)>, String> {
    cache::load_versions_cache(&mod_type)
        .map(|res| {
            res.map(|versions| {
                (
                    versions,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                )
            })
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_mods_cache(mods: Vec<Mod>) -> Result<(), String> {
    map_error(cache::save_cache(&mods))
}

#[tauri::command]
pub async fn load_mods_cache() -> Result<Option<(Vec<Mod>, u64)>, String> {
    map_error(cache::load_cache())
}

#[tauri::command]
pub async fn clear_cache() -> Result<(), String> {
    // Clear legacy/app caches stored under the OS cache directory
    let mut errors: Vec<String> = Vec::new();
    if let Err(e) = cache::clear_cache() {
        errors.push(e.to_string());
    }

    // Also clear the GitLab mod index cache we maintain under the config directory
    let config_dir = match dirs::config_dir() {
        Some(p) => p,
        None => {
            // If we can't resolve config dir, return any prior error or success for the primary cache
            return if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            };
        }
    };
    let mod_index_cache_dir = config_dir.join("Balatro").join("mod_index_cache");
    if mod_index_cache_dir.exists()
        && let Err(e) = std::fs::remove_dir_all(&mod_index_cache_dir)
    {
        errors.push(format!(
            "Failed to clear mod index cache at {}: {}",
            mod_index_cache_dir.display(),
            e
        ));
    }

    // Clear UI assets cache (thumbnails/descriptions)
    let mod_assets_dir = config_dir.join("Balatro").join("mod_assets");
    if mod_assets_dir.exists()
        && let Err(e) = std::fs::remove_dir_all(&mod_assets_dir)
    {
        errors.push(format!(
            "Failed to clear mod assets at {}: {}",
            mod_assets_dir.display(),
            e
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[tauri::command]
pub async fn get_last_fetched(state: tauri::State<'_, AppState>) -> Result<u64, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_last_fetched().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_last_fetched(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_last_fetched(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mod_update_available(
    mod_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let last_installed_version = db
        .get_last_installed_version(&mod_name)
        .map_err(|e| e.to_string())?;

    if last_installed_version.is_empty() {
        return Ok(false);
    }

    let cached_mods = match cache::load_cache().map_err(|e| e.to_string())? {
        Some((mods, _)) => mods,
        None => return Ok(false),
    };

    for cached_mod in cached_mods {
        if cached_mod.title == mod_name || (cached_mod.folderName.as_ref() == Some(&mod_name)) {
            if let Some(remote_version) = cached_mod.version {
                return Ok(remote_version != last_installed_version);
            }
            break;
        }
    }

    Ok(false)
}
