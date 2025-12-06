use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bmm_lib::cache;
use bmm_lib::cache::Mod;
use serde::Serialize;

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

pub(crate) fn load_mods_cache_shared() -> Result<Option<Arc<Vec<Mod>>>, String> {
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

#[derive(Serialize)]
pub struct InstalledSummary {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct ModsStateSummary {
    pub installed: Vec<InstalledSummary>,
    pub enabled: std::collections::HashMap<String, bool>,
    pub updates: std::collections::HashMap<String, bool>,
    pub thumbnails: std::collections::HashMap<String, String>,
    pub descriptions: std::collections::HashMap<String, String>,
}

/// Return installed list, enabled map, and updates map in a single IPC.
#[tauri::command]
pub async fn mods_state_summary(
    state: tauri::State<'_, AppState>,
    local_paths: Option<Vec<String>>,
    catalog_titles: Option<Vec<String>>,
) -> Result<ModsStateSummary, String> {
    use std::collections::HashMap;
    use std::path::PathBuf;

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let installed_mods = db.get_installed_mods().map_err(|e| e.to_string())?;

    // Installed list and enabled map (DB mods)
    let mut installed_list: Vec<InstalledSummary> = Vec::with_capacity(installed_mods.len());
    let mut enabled_map: HashMap<String, bool> = HashMap::new();
    for m in installed_mods {
        let p = PathBuf::from(&m.path);
        let enabled = !p.join(".lovelyignore").exists();
        enabled_map.insert(m.name.clone(), enabled);
        installed_list.push(InstalledSummary {
            name: m.name,
            path: m.path,
        });
    }

    // Local mods passed from UI
    if let Some(paths) = local_paths {
        for p in paths {
            let path = PathBuf::from(&p);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let enabled = !path.join(".lovelyignore").exists();
            enabled_map.insert(name, enabled);
        }
    }

    // Updates map (reuse cached remote catalog)
    let cached_mods = load_mods_cache_shared()?.unwrap_or_else(|| Arc::new(Vec::new()));
    let mut by_title = HashMap::with_capacity(cached_mods.len());
    let mut by_folder = HashMap::with_capacity(cached_mods.len());
    for m in cached_mods.iter() {
        if let Some(v) = m.version.as_ref() {
            by_title.insert(m.title.to_lowercase(), v.clone());
            if let Some(folder) = m.folderName.as_ref() {
                by_folder.insert(folder.to_lowercase(), v.clone());
            }
        }
    }

    let mut updates: HashMap<String, bool> = HashMap::new();
    for m in &installed_list {
        let key = m.name.to_lowercase();
        let installed_version = db
            .get_last_installed_version(&m.name)
            .map_err(|e| e.to_string())?;
        if installed_version.is_empty() {
            updates.insert(m.name.clone(), false);
            continue;
        }
        let remote = by_title.get(&key).or_else(|| by_folder.get(&key));
        if let Some(remote_version) = remote {
            updates.insert(m.name.clone(), remote_version != &installed_version);
        } else {
            updates.insert(m.name.clone(), false);
        }
    }

    // Cached thumbnails and descriptions for installed mods and visible catalog mods
    let mut thumbnails: HashMap<String, String> = HashMap::new();
    let mut descriptions: HashMap<String, String> = HashMap::new();
    if let Ok((thumbs_dir, desc_dir)) = ensure_assets_dirs() {
        for m in &installed_list {
            let slug = safe_slug(&m.name);
            let path = thumbs_dir.join(format!("{slug}.jpg"));
            if path.exists()
                && let Some(s) = path.to_str()
            {
                thumbnails.insert(m.name.clone(), s.to_string());
            }

            let desc_path = desc_dir.join(format!("{slug}.md"));
            if desc_path.exists()
                && let Ok(text) = std::fs::read_to_string(&desc_path)
            {
                descriptions.insert(m.name.clone(), text);
            }
        }

        if let Some(titles) = catalog_titles {
            for title in titles {
                let slug = safe_slug(&title);
                let thumb_path = thumbs_dir.join(format!("{slug}.jpg"));
                if !thumbnails.contains_key(&title)
                    && thumb_path.exists()
                    && let Some(s) = thumb_path.to_str()
                {
                    thumbnails.insert(title.clone(), s.to_string());
                }

                if descriptions.contains_key(&title) {
                    continue;
                }
                let desc_path = desc_dir.join(format!("{slug}.md"));
                if desc_path.exists()
                    && let Ok(text) = std::fs::read_to_string(&desc_path)
                {
                    descriptions.insert(title, text);
                }
            }
        }
    }

    Ok(ModsStateSummary {
        installed: installed_list,
        enabled: enabled_map,
        updates,
        thumbnails,
        descriptions,
    })
}

// Minimal helpers duplicated from repo.rs; keep in sync if changed there.
fn safe_slug(input: &str) -> String {
    let mut s = input.trim().to_lowercase();
    s = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
}

fn ensure_assets_dirs() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let config_dir = dirs::config_dir().ok_or_else(|| "config dir not found".to_string())?;
    let base = config_dir.join("Balatro").join("mod_assets");
    let thumbs = base.join("thumbnails");
    let descs = base.join("descriptions");
    std::fs::create_dir_all(&thumbs).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&descs).map_err(|e| e.to_string())?;
    Ok((thumbs, descs))
}
