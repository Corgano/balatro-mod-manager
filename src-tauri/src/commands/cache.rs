use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bmm_lib::cache;
use bmm_lib::cache::Mod;

use crate::state::AppState;
use crate::util::map_error;
use once_cell::sync::Lazy;

// Cache the deserialized mods cache to avoid re-reading on every IPC call.
const MOD_CACHE_TTL: Duration = Duration::from_secs(30);
static MOD_CACHE: Lazy<Mutex<Option<CachedMods>>> = Lazy::new(|| Mutex::new(None));

struct CachedMods {
    mods: Arc<Vec<Mod>>,
    loaded_at: Instant,
}

fn load_mods_cache_shared() -> Result<Option<Arc<Vec<Mod>>>, String> {
    if let Ok(mut guard) = MOD_CACHE.lock() {
        if let Some(cached) = guard.as_ref()
            && cached.loaded_at.elapsed() < MOD_CACHE_TTL
        {
            return Ok(Some(cached.mods.clone()));
        }
        let fresh = cache::load_cache()
            .map_err(|e| e.to_string())?
            .map(|(mods, _)| Arc::new(mods));
        if let Some(ref mods) = fresh {
            *guard = Some(CachedMods {
                mods: mods.clone(),
                loaded_at: Instant::now(),
            });
        } else {
            *guard = None;
        }
        Ok(fresh)
    } else {
        // Lock poisoned; fall back to direct load
        cache::load_cache()
            .map_err(|e| e.to_string())
            .map(|opt| opt.map(|(mods, _)| Arc::new(mods)))
    }
}

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

    let cached_mods = match load_mods_cache_shared()? {
        Some(mods) => mods,
        None => return Ok(false),
    };

    for cached_mod in cached_mods.iter() {
        if cached_mod.title == mod_name || (cached_mod.folderName.as_ref() == Some(&mod_name)) {
            if let Some(remote_version) = &cached_mod.version {
                return Ok(remote_version != &last_installed_version);
            }
            break;
        }
    }

    Ok(false)
}

/// Return a map of installed mod names to "update available" flags in a single pass.
#[tauri::command]
pub async fn mods_updates_map(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, bool>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let installed = db.get_installed_mods().map_err(|e| e.to_string())?;

    let cached_mods = match load_mods_cache_shared()? {
        Some(mods) => mods,
        None => return Ok(std::collections::HashMap::new()),
    };

    // Build lookup maps for remote versions by both title and folderName.
    let mut by_title = std::collections::HashMap::with_capacity(cached_mods.len());
    let mut by_folder = std::collections::HashMap::with_capacity(cached_mods.len());
    for m in cached_mods.iter() {
        if let Some(v) = m.version.as_ref() {
            by_title.insert(m.title.to_lowercase(), v.clone());
            if let Some(folder) = m.folderName.as_ref() {
                by_folder.insert(folder.to_lowercase(), v.clone());
            }
        }
    }

    let mut out = std::collections::HashMap::new();
    for m in installed {
        let key = m.name.to_lowercase();
        let installed_version = m.current_version.unwrap_or_default();
        if installed_version.is_empty() {
            out.insert(m.name, false);
            continue;
        }
        let remote = by_title.get(&key).or_else(|| by_folder.get(&key));
        if let Some(remote_version) = remote {
            out.insert(m.name, remote_version != &installed_version);
        } else {
            out.insert(m.name, false);
        }
    }

    Ok(out)
}
