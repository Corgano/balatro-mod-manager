use std::path::PathBuf;

use serde::Serialize;

#[cfg(target_os = "macos")]
use bmm_lib::errors::AppError;
use bmm_lib::lovely;

use crate::state::AppState;
use crate::util::map_error;

/// Combined app initialization data returned in a single IPC call.
/// Replaces multiple separate calls: get_app_version, check_existing_installation,
/// is_security_warning_acknowledged, is_lovely_installed, check_lovely_update, get_launch_mode.
#[derive(Serialize)]
pub struct AppInitData {
    pub version: String,
    pub existing_installation: Option<String>,
    pub security_acknowledged: bool,
    pub lovely_installed: bool,
    pub lovely_update_available: Option<String>,
    pub launch_mode: String,
}

/// Single IPC call that returns all data needed to initialize the app.
/// This batches 6 separate calls into 1, reducing startup IPC overhead.
#[tauri::command]
pub async fn get_app_init_data(state: tauri::State<'_, AppState>) -> Result<AppInitData, String> {
    let db = state.db.lock().await;

    // 1. App version (compile-time constant, no DB access needed)
    let version = env!("CARGO_PKG_VERSION").to_string();

    // 2. Check existing installation
    let existing_installation = if let Some(path) = db.get_installation_path()? {
        let path_buf = PathBuf::from(&path);
        if bmm_lib::balamod::Balatro::from_custom_path(path_buf).is_some() {
            Some(path)
        } else {
            map_error(db.remove_installation_path())?;
            None
        }
    } else {
        None
    };

    // 3. Security warning acknowledged
    let security_acknowledged = map_error(db.is_security_warning_acknowledged())?;

    // 4. Is lovely installed
    let lovely_installed = check_lovely_installed_inner(&db, &existing_installation)?;

    // 5. Launch mode
    let launch_mode = map_error(db.get_launch_mode())?;

    // Drop DB lock before network call
    drop(db);

    // 6. Check lovely update (network call)
    let lovely_update_available = check_lovely_update_inner(&state).await?;

    Ok(AppInitData {
        version,
        existing_installation,
        security_acknowledged,
        lovely_installed,
        lovely_update_available,
        launch_mode,
    })
}

/// Combined settings data returned in a single IPC call.
/// Replaces separate calls: get_discord_rpc_status, get_lovely_console_status,
/// get_background_state, get_compat_helper_status, get_linux_prefix, get_launch_mode.
#[derive(Serialize)]
pub struct AllSettings {
    pub discord_rpc: bool,
    pub lovely_console: bool,
    pub background_enabled: bool,
    pub compat_helper: bool,
    pub linux_prefix: String,
    pub launch_mode: String,
}

/// Single IPC call that returns all settings for the Settings page.
/// This batches 6 separate calls into 1.
#[tauri::command]
pub async fn get_all_settings(state: tauri::State<'_, AppState>) -> Result<AllSettings, String> {
    let db = state.db.lock().await;

    let discord_rpc = db.is_discord_rpc_enabled().map_err(|e| e.to_string())?;
    let lovely_console = map_error(db.is_lovely_console_enabled())?;
    let background_enabled = map_error(db.get_background_enabled())?;
    let compat_helper = map_error(db.is_compat_helper_enabled())?;
    let linux_prefix = db
        .get_linux_prefix()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let launch_mode = map_error(db.get_launch_mode())?;

    Ok(AllSettings {
        discord_rpc,
        lovely_console,
        background_enabled,
        compat_helper,
        linux_prefix,
        launch_mode,
    })
}

// Internal helpers to avoid code duplication

fn check_lovely_installed_inner(
    db: &bmm_lib::database::Database,
    existing_installation: &Option<String>,
) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = (db, existing_installation);
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
        // Use existing installation if already validated
        if let Some(path) = existing_installation {
            let dll = PathBuf::from(path).join("version.dll");
            return Ok(dll.exists());
        }

        // Fall back to DB-stored path
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
        Ok(false)
    }

    #[cfg(target_os = "linux")]
    {
        // Use existing installation if already validated
        if let Some(path) = existing_installation {
            let so = PathBuf::from(path).join("liblovely.so");
            return Ok(so.exists());
        }

        // Fall back to DB-stored path
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
        let _ = (db, existing_installation);
        Ok(true)
    }
}

async fn check_lovely_update_inner(
    state: &tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = state;
        return Ok(None);
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
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
}
