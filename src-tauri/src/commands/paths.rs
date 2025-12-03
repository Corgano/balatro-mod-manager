use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::balamod::find_balatros;
use bmm_lib::errors::AppError;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn open_directory(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!(
            "Path does not exist: {}",
            path_buf.to_string_lossy()
        ));
    }
    if !path_buf.is_dir() {
        return Err(format!(
            "Path is not a directory: {}",
            path_buf.to_string_lossy()
        ));
    }

    let path_str = path_buf.to_string_lossy().into_owned();
    let mut errors: Vec<String> = Vec::new();

    // Try the Tauri opener plugin first (works better in packaged builds).
    match app.opener().reveal_item_in_dir(path_str.clone()) {
        Ok(_) => return Ok(()),
        Err(e) => errors.push(format!("opener reveal: {e}")),
    }
    match app.opener().open_path(path_str.clone(), None::<String>) {
        Ok(_) => return Ok(()),
        Err(e) => errors.push(format!("opener open_path: {e}")),
    }

    // Fallback to system opener via `open` crate.
    if open::that(&path_buf).is_err() {
        errors.push("open crate failed".into());
    } else {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // Detect WSL so we can route to Windows shell reliably.
        let is_wsl = std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::fs::read_to_string("/proc/version")
                .map(|v| v.to_lowercase().contains("microsoft"))
                .unwrap_or(false);

        // Prefer wslview when available (Windows file explorer from WSL)
        if is_wsl {
            let status = Command::new("wslview").arg(&path).status();
            if status.map(|s| s.success()).unwrap_or(false) {
                return Ok(());
            }

            // Fallback to PowerShell start if wslview is missing or failed
            if let Ok(output) = Command::new("wslpath")
                .args(["-w", path_buf.to_string_lossy().as_ref()])
                .output()
                && output.status.success() {
                    let win_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let status = Command::new("powershell.exe")
                        .args(["-NoProfile", "-Command", "Start-Process", &win_path])
                        .status();
                    if status.map(|s| s.success()).unwrap_or(false) {
                        return Ok(());
                    }
                }
        }

        // Desktop Linux fallback: try xdg-open directly for clearer error
        match Command::new("xdg-open").arg(&path).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("xdg-open exit {}", status)),
            Err(e) => errors.push(format!("xdg-open error {e}")),
        }
    }

    Err(format!(
        "Failed to open directory with system handler: {}; attempts: {}",
        path_buf.to_string_lossy(),
        errors.join("; ")
    ))
}

#[tauri::command]
pub async fn get_mods_folder() -> Result<String, String> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
    let mods_dir = config_dir.join("Balatro").join("Mods");
    std::fs::create_dir_all(&mods_dir).map_err(|e| {
        AppError::DirCreate {
            path: mods_dir.clone(),
            source: e.to_string(),
        }
        .to_string()
    })?;
    Ok(mods_dir.to_string_lossy().into_owned())
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
                "On Linux, Balatro must be launched via Steam because it requires Proton/Wine integration. Custom paths are not supported."
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
    if candidate.is_file()
        && let Some(parent) = candidate.parent() {
            candidate = parent.to_path_buf();
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
