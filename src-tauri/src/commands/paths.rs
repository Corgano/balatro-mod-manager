use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;

use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::balamod::find_balatros;
use bmm_lib::errors::AppError;
use bmm_lib::finder::invalidate_balatro_paths_cache;
use bmm_lib::local_mod_detection;
use log::error;
#[cfg(target_os = "linux")]
use log::{info, warn};
#[cfg(target_os = "linux")]
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

    let mut errors: Vec<String> = Vec::new();

    #[cfg(not(target_os = "linux"))]
    let _ = app;

    #[cfg(target_os = "linux")]
    {
        let is_flatpak = std::env::var_os("FLATPAK_ID").is_some();
        let (host_target, sandbox_target) = map_flatpak_paths(&path_buf);
        let host_str = host_target.to_string_lossy().into_owned();
        let sandbox_str = sandbox_target.to_string_lossy().into_owned();
        info!(
            "open_directory request: host='{}' sandbox='{}' (flatpak={})",
            host_str, sandbox_str, is_flatpak
        );

        if is_flatpak {
            // Try host opener first using host/portal path.
            match Command::new("flatpak-spawn")
                .args(["--host", "xdg-open", &host_str])
                .status()
            {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => {
                    let msg = format!("flatpak-spawn xdg-open exit {}", status);
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
                Err(e) => {
                    let msg = format!("flatpak-spawn xdg-open error {e}");
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
            }
        }

        if !is_flatpak {
            // Prefer native xdg-open on desktop Linux to avoid portal no-op cases.
            match Command::new("xdg-open").arg(&host_str).status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => {
                    let msg = format!("xdg-open exit {}", status);
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
                Err(e) => {
                    let msg = format!("xdg-open error {e}");
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
            }
        }

        // Try the Tauri opener plugin (portal-aware) using reveal first for dirs.
        match app.opener().reveal_item_in_dir(host_str.clone()) {
            Ok(_) => {
                info!("open_directory: opener reveal succeeded for {}", host_str);
                return Ok(());
            }
            Err(e) => {
                let msg = format!("opener reveal: {e}");
                warn!("open_directory {}", msg);
                errors.push(msg);
            }
        }
        match app.opener().open_path(host_str.clone(), None::<String>) {
            Ok(_) => {
                info!(
                    "open_directory: opener open_path succeeded for {}",
                    host_str
                );
                return Ok(());
            }
            Err(e) => {
                let msg = format!("opener open_path: {e}");
                warn!("open_directory {}", msg);
                errors.push(msg);
            }
        }

        if is_flatpak {
            // Try gio with host/portal path, then sandbox path as a fallback.
            match Command::new("gio").args(["open", &host_str]).status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => {
                    let msg = format!("gio open (host path) exit {}", status);
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
                Err(e) => {
                    let msg = format!("gio open (host path) error {e}");
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
            }
            if sandbox_str != host_str {
                match Command::new("gio").args(["open", &sandbox_str]).status() {
                    Ok(status) if status.success() => return Ok(()),
                    Ok(status) => {
                        let msg = format!("gio open (sandbox path) exit {}", status);
                        warn!("open_directory {}", msg);
                        errors.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("gio open (sandbox path) error {e}");
                        warn!("open_directory {}", msg);
                        errors.push(msg);
                    }
                }
            }
        }

        // Detect WSL so we can route to Windows shell reliably.
        let is_wsl = std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::fs::read_to_string("/proc/version")
                .map(|v| v.to_lowercase().contains("microsoft"))
                .unwrap_or(false);

        // Prefer wslview when available (Windows file explorer from WSL)
        if is_wsl {
            let status = Command::new("wslview").arg(&host_str).status();
            if status.map(|s| s.success()).unwrap_or(false) {
                return Ok(());
            }

            // Fallback to PowerShell start if wslview is missing or failed
            if let Ok(output) = Command::new("wslpath")
                .args(["-w", host_str.as_ref()])
                .output()
                && output.status.success()
            {
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
        if is_flatpak {
            match Command::new("xdg-open").arg(&host_str).status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => {
                    let msg = format!("xdg-open exit {}", status);
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
                Err(e) => {
                    let msg = format!("xdg-open error {e}");
                    warn!("open_directory {}", msg);
                    errors.push(msg);
                }
            }
        }
    }

    // Cross-platform fallback
    if open::that(&path_buf).is_ok() {
        return Ok(());
    }
    errors.push("open crate failed".into());

    let message = format!(
        "Failed to open directory with system handler: {}; attempts: {}",
        path_buf.to_string_lossy(),
        errors.join("; ")
    );
    error!("{message}");
    Err(message)
}

#[tauri::command]
pub async fn get_mods_folder() -> Result<String, String> {
    let mods_dir = resolve_mods_dir_for_open()
        .map_err(|e| AppError::DirNotFound(PathBuf::from(e)).to_string())?;
    Ok(mods_dir.to_string_lossy().into_owned())
}

fn resolve_mods_dir_for_open() -> Result<PathBuf, String> {
    let mods_dir = local_mod_detection::resolve_mods_dir_path()?;
    std::fs::create_dir_all(&mods_dir)
        .map_err(|e| format!("Failed to create mods directory: {}", e))?;

    // On Flatpak, also ensure the host-visible Mods folder exists so openers that escape
    // the sandbox (flatpak-spawn --host) have a valid target path.
    if std::env::var_os("FLATPAK_ID").is_some()
        && let Some(home) = dirs::home_dir()
    {
        let host_mods = home.join(".config/Balatro/Mods");
        if host_mods != mods_dir
            && !host_mods.exists()
            && let Err(e) = std::fs::create_dir_all(&host_mods)
        {
            log::warn!(
                "Failed to create host mods directory at {}: {}",
                host_mods.display(),
                e
            );
        }
        if host_mods.exists() {
            return Ok(host_mods);
        }
    }

    Ok(mods_dir)
}

#[cfg(target_os = "linux")]
fn map_flatpak_paths(path_buf: &Path) -> (PathBuf, PathBuf) {
    let is_flatpak = std::env::var_os("FLATPAK_ID").is_some();
    if !is_flatpak {
        return (path_buf.to_path_buf(), path_buf.to_path_buf());
    }

    let Some(home) = dirs::home_dir() else {
        return (path_buf.to_path_buf(), path_buf.to_path_buf());
    };

    let host_prefix = home.join(".config/Balatro/Mods");
    let sandbox_prefix = home.join(".var/app/io.balatro.ModManager/config/Balatro/Mods");

    // Always drive host path from host prefix; mirror into sandbox for fallbacks.
    let rel = if path_buf.starts_with(&host_prefix) {
        path_buf
            .strip_prefix(&host_prefix)
            .unwrap_or_else(|_| Path::new(""))
    } else if path_buf.starts_with(&sandbox_prefix) {
        path_buf
            .strip_prefix(&sandbox_prefix)
            .unwrap_or_else(|_| Path::new(""))
    } else {
        Path::new("")
    };

    let host_path = host_prefix.join(rel);
    let sandbox_path = sandbox_prefix.join(rel);

    // Make sure both sides exist so openers have a target.
    if let Err(e) = std::fs::create_dir_all(&sandbox_path) {
        warn!(
            "open_directory: failed to ensure sandbox mods dir {}: {}",
            sandbox_path.display(),
            e
        );
    }
    if let Err(e) = std::fs::create_dir_all(&host_path) {
        warn!(
            "open_directory: failed to ensure host mods dir {}: {}",
            host_path.display(),
            e
        );
    }

    info!(
        "open_directory path mapping: input='{}' host_prefix='{}' sandbox_prefix='{}' -> host='{}' sandbox='{}'",
        path_buf.display(),
        host_prefix.display(),
        sandbox_prefix.display(),
        host_path.display(),
        sandbox_path.display()
    );

    (host_path, sandbox_path)
}

#[tauri::command]
pub async fn get_balatro_path(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let db = state.db.lock().await;
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
        // Ensure Proton symlinks are set up when the path is configured
        if let Err(e) = local_mod_detection::ensure_proton_mod_dir_link(Some(&path_buf)) {
            log::warn!("Failed to ensure Proton mod dir link: {}", e);
        }
    }
    let db = state.db.lock().await;
    let result = map_error(db.set_installation_path(&path));
    if result.is_ok() {
        invalidate_balatro_paths_cache();
    }
    result
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
            let db = state.db.lock().await;
            map_error(db.set_installation_path(&steam_path))?;
            invalidate_balatro_paths_cache();
        }

        // Ensure Proton symlinks are set up when Steam path is auto-detected
        #[cfg(target_os = "linux")]
        {
            if let Err(e) =
                local_mod_detection::ensure_proton_mod_dir_link(Some(&steam_balatro.path))
            {
                log::warn!("Failed to ensure Proton mod dir link: {}", e);
            }
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
            let db = state.db.lock().await;
            map_error(db.set_installation_path(path_str))?;
            invalidate_balatro_paths_cache();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_looks_like_steam_install_valid_paths() {
        // Windows-style Steam path
        let win_path = PathBuf::from("C:/Program Files/Steam/steamapps/common/Balatro");
        assert!(looks_like_steam_install(&win_path));

        // Linux-style Steam path
        let linux_path = PathBuf::from("/home/user/.steam/steam/steamapps/common/Balatro");
        assert!(looks_like_steam_install(&linux_path));

        // macOS-style Steam path
        let mac_path =
            PathBuf::from("/Users/user/Library/Application Support/Steam/steamapps/common/Balatro");
        assert!(looks_like_steam_install(&mac_path));

        // Case insensitivity check
        let mixed_case = PathBuf::from("/home/user/SteamApps/Common/BALATRO");
        assert!(looks_like_steam_install(&mixed_case));
    }

    #[test]
    fn test_looks_like_steam_install_invalid_paths() {
        // Custom install path
        let custom_path = PathBuf::from("/home/user/Games/Balatro");
        assert!(!looks_like_steam_install(&custom_path));

        // Missing common directory
        let missing_common = PathBuf::from("/home/user/.steam/steamapps/Balatro");
        assert!(!looks_like_steam_install(&missing_common));

        // Wrong game name
        let wrong_game = PathBuf::from("/home/user/.steam/steamapps/common/OtherGame");
        assert!(!looks_like_steam_install(&wrong_game));

        // Empty path
        let empty = PathBuf::new();
        assert!(!looks_like_steam_install(&empty));

        // Just balatro without steam structure
        let just_balatro = PathBuf::from("/Balatro");
        assert!(!looks_like_steam_install(&just_balatro));
    }

    #[test]
    fn test_looks_like_steam_install_nested_paths() {
        // Subdirectory within Steam install
        let nested =
            PathBuf::from("/home/user/.steam/steam/steamapps/common/Balatro/resources/data");
        assert!(looks_like_steam_install(&nested));

        // Proton prefix path
        let proton = PathBuf::from(
            "/home/user/.steam/steam/steamapps/compatdata/2379780/pfx/drive_c/Balatro",
        );
        assert!(!looks_like_steam_install(&proton));
    }

    #[test]
    fn test_verify_path_exists_format() {
        // Test that path checking works with string conversion
        let path_str = "/some/test/path".to_string();
        let path_buf = PathBuf::from(&path_str);
        assert_eq!(path_buf.to_string_lossy(), path_str);
    }
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
        && let Some(parent) = candidate.parent()
    {
        candidate = parent.to_path_buf();
    }

    if let Some(balatro) = bmm_lib::balamod::Balatro::from_custom_path(candidate) {
        let canonical = balatro.path.to_string_lossy().into_owned();
        let db = state.db.lock().await;
        map_error(db.set_installation_path(&canonical))?;
        invalidate_balatro_paths_cache();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn check_existing_installation(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db = state.db.lock().await;
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
