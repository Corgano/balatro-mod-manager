use std::path::{Path, PathBuf};

use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::balamod::find_balatros;
use bmm_lib::errors::AppError;

#[tauri::command]
pub async fn open_directory(path: String) -> Result<(), String> {
    match open::that(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to open directory: {e}")),
    }
}

#[tauri::command]
pub async fn get_mods_folder() -> Result<String, String> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
    Ok(config_dir
        .join("Balatro")
        .join("Mods")
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub async fn get_balatro_path(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    map_error(db.get_installation_path())
}

#[tauri::command]
pub async fn set_balatro_path(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let path_buf = PathBuf::from(&path);
        if !looks_like_steam_install(&path_buf) {
            return Err(
                "On Linux, Balatro must be launched via Steam; custom paths are not supported."
                    .into(),
            );
        }
    }
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(e) => return Err(e.to_string()),
    };
    map_error(db.set_installation_path(&path))
}

#[tauri::command]
pub async fn find_steam_balatro(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut balatros = find_balatros();
    if balatros.is_empty() {
        return Ok(Vec::new());
    }

    let steam_index = balatros
        .iter()
        .position(|b| looks_like_steam_install(&b.path));

    if let Some(idx) = steam_index {
        let steam_balatro = balatros.remove(idx);
        let steam_path = steam_balatro.path.to_string_lossy().into_owned();
        {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            map_error(db.set_installation_path(&steam_path))?;
        }

        let mut ordered = Vec::with_capacity(balatros.len() + 1);
        ordered.push(steam_path);
        ordered.extend(
            balatros
                .into_iter()
                .map(|b| b.path.to_string_lossy().into_owned()),
        );
        Ok(ordered)
    } else {
        let first_path = balatros
            .first()
            .map(|b| b.path.to_string_lossy().into_owned());

        if let Some(ref path_str) = first_path {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            map_error(db.set_installation_path(path_str))?;
        }

        Ok(balatros
            .into_iter()
            .map(|b| b.path.to_string_lossy().into_owned())
            .collect())
    }
}

#[tauri::command]
pub async fn verify_path_exists(path: String) -> bool {
    match std::fs::exists(PathBuf::from(path)) {
        Ok(exists) => exists,
        Err(e) => {
            log::error!("Failed to check path existence: {e}");
            false
        }
    }
}

#[tauri::command]
pub async fn path_exists(path: String) -> Result<bool, String> {
    let path = PathBuf::from(path);
    Ok(path.exists())
}

fn looks_like_steam_install(path: &Path) -> bool {
    let segments: Vec<String> = path
        .components()
        .filter_map(|component| {
            if let std::path::Component::Normal(segment) = component {
                Some(segment.to_string_lossy().to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect();

    segments
        .windows(3)
        .any(|window| window[0] == "steamapps" && window[1] == "common" && window[2] == "balatro")
}

#[tauri::command]
pub async fn check_custom_balatro(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        // Linux builds require Steam-managed installs; custom paths are not supported.
        let path_buf = PathBuf::from(&path);
        if !looks_like_steam_install(&path_buf) {
            return Ok(false);
        }
    }

    let path_buf = PathBuf::from(&path);

    let mut candidate = path_buf.clone();
    if candidate.is_file() {
        if let Some(parent) = candidate.parent() {
            candidate = parent.to_path_buf();
        }
    }

    if let Some(balatro) = bmm_lib::balamod::Balatro::from_custom_path(candidate) {
        let canonical = balatro.path.to_string_lossy().into_owned();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        map_error(db.set_installation_path(&canonical))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn check_existing_installation(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::LockPoisoned("Database lock poisoned".to_string()))?;
    if let Some(path) = db.get_installation_path()? {
        let path_buf = PathBuf::from(&path);
        if bmm_lib::balamod::Balatro::from_custom_path(path_buf).is_some() {
            Ok(Some(path))
        } else {
            map_error(db.remove_installation_path())?;
            Ok(None)
        }
    } else {
        Ok(None)
    }
}
