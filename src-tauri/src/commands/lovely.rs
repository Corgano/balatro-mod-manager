#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use bmm_lib::errors::AppError;
use bmm_lib::lovely;

use crate::state::AppState;

/// Check whether Lovely is currently installed/present on this system.
/// - macOS: checks for `~/Library/Application Support/Balatro/bins/liblovely.dylib` (via config dir)
/// - Windows/Linux (Proton/Wine): checks that a `version.dll` exists in the Balatro game directory
#[tauri::command]
pub async fn is_lovely_installed(_state: tauri::State<'_, AppState>) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
        let lovely_path = config_dir
            .join("Balatro")
            .join("bins")
            .join("liblovely.dylib");
        Ok(lovely_path.exists())
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer database install path if present
        let db = _state.db.lock().await;
        if let Some(path) = db.get_installation_path().map_err(|e| e.to_string())? {
            let dll = PathBuf::from(path).join("version.dll");
            return Ok(dll.exists());
        }

        // Fallback to first detected Balatro path
        let candidates = bmm_lib::finder::get_balatro_paths();
        if let Some(p) = candidates.first() {
            let dll = p.join("version.dll");
            return Ok(dll.exists());
        }
        return Ok(false);
    }

    #[cfg(target_os = "linux")]
    {
        // Prefer database install path if present
        let db = _state.db.lock().await;
        if let Some(path) = db.get_installation_path().map_err(|e| e.to_string())? {
            let so = PathBuf::from(path).join("liblovely.so");
            return Ok(so.exists());
        }

        // Fallback to first detected Balatro path
        let candidates = bmm_lib::finder::get_balatro_paths();
        if let Some(p) = candidates.first() {
            let so = p.join("liblovely.so");
            return Ok(so.exists());
        }
        Ok(false)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Other targets: Lovely injector not managed; do not warn.
        Ok(true)
    }
}

#[tauri::command]
pub async fn check_lovely_update(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Unsupported target; skip.
        return Ok(None);
    }

    // Load latest from GitHub
    let latest = lovely::get_latest_lovely_version()
        .await
        .map_err(|e| e.to_string())?;

    // Compare to DB-stored version
    let db = state.db.lock().await;
    match db.get_lovely_version() {
        Ok(Some(installed)) => {
            if installed.trim() != latest {
                Ok(Some(latest))
            } else {
                Ok(None)
            }
        }
        Ok(None) => Ok(Some(latest)), // Missing setting implies update/reinstall needed
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn update_lovely_to_latest(state: tauri::State<'_, AppState>) -> Result<String, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        return Err(
            "Lovely injection is only supported on Windows, macOS, and Linux/Proton.".into(),
        );
    }

    let latest = lovely::get_latest_lovely_version()
        .await
        .map_err(|e| e.to_string())?;

    // Remove current install and reinstall
    lovely::remove_installed_lovely().map_err(|e| e.to_string())?;
    lovely::ensure_lovely_exists()
        .await
        .map_err(|e| e.to_string())?;

    // Persist version
    let db = state.db.lock().await;
    db.set_lovely_version(&latest).map_err(|e| e.to_string())?;

    Ok(latest)
}
