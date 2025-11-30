#[cfg(target_os = "linux")]
use log::{info, warn};
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::process::Command;

use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::errors::AppError;
#[cfg(target_os = "macos")]
use bmm_lib::lovely;
#[cfg(target_os = "linux")]
use bmm_lib::lovely::ensure_version_dll_exists;
use bmm_lib::smods_installer::{ModInstaller, ModType};
use bmm_lib::{cache, database::InstalledMod};

#[cfg(target_os = "linux")]
const STEAM_APP_ID: &str = "2379780";

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn get_installation_and_console(
    state: &tauri::State<'_, AppState>,
) -> Result<(String, bool), String> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::LockPoisoned("Database lock poisoned".to_string()).to_string())?;
    let install_path = db
        .get_installation_path()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            AppError::InvalidState("No installation path set".to_string()).to_string()
        })?;
    let lovely_console_enabled = db.is_lovely_console_enabled().map_err(|e| e.to_string())?;
    Ok((install_path, lovely_console_enabled))
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn launch_balatro(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let (path_str, lovely_console_enabled) = get_installation_and_console(&state)?;
    let path = PathBuf::from(path_str);
    let _balatro = bmm_lib::balamod::Balatro::from_custom_path(path.clone())
        .ok_or_else(|| "Stored Balatro path is no longer valid".to_string())?;

    let lovely_path = map_error(lovely::ensure_lovely_exists().await)?;
    let app_bundle = balatro
        .get_app_bundle_path()
        .ok_or_else(|| "Unable to locate Balatro app bundle".to_string())?;
    let balatro_executable = app_bundle.join("Contents/MacOS/love");
    let launch_root = balatro.path.clone();

    if lovely_console_enabled {
        let disable_arg = if !lovely_console_enabled {
            " --disable-console"
        } else {
            ""
        };
        let command_line = format!(
            "cd '{}' && DYLD_INSERT_LIBRARIES='{}' '{}'{}",
            launch_root.display(),
            lovely_path.display(),
            balatro_executable.display(),
            disable_arg
        );

        let applescript = format!("tell application \"Terminal\" to do script \"{command_line}\"");

        Command::new("osascript")
            .arg("-e")
            .arg(applescript)
            .status()
            .map_err(|e| e.to_string())?;
    } else {
        let cmd = format!(
            "DYLD_INSERT_LIBRARIES='{}' '{}'",
            lovely_path.display(),
            balatro_executable.display()
        );
        // Spawn the process without waiting so the UI doesn't block
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn strip_python_env(cmd: &mut Command) {
    // AppImage/runtime wrappers can leak Python env vars that break Proton's python runner.
    cmd.env_remove("PYTHONHOME");
    cmd.env_remove("PYTHONPATH");
    cmd.env_remove("PYTHONNOUSERSITE");
    cmd.env_remove("PYTHONUSERBASE");
}

#[cfg(target_os = "linux")]
fn strip_wrapper_env(cmd: &mut Command) {
    // Drop common AppImage/Snap wrappers that can poison Steam/Proton env.
    cmd.env_remove("APPIMAGE");
    cmd.env_remove("APPDIR");
    cmd.env_remove("LD_LIBRARY_PATH");
    cmd.env_remove("LD_PRELOAD");
    cmd.env_remove("SNAP");
    cmd.env_remove("SNAP_NAME");
    cmd.env_remove("SNAP_REVISION");
    cmd.env_remove("SNAP_INSTANCE_NAME");
    cmd.env_remove("SNAP_INSTANCE_KEY");
    cmd.env_remove("SNAP_ARCH");
    cmd.env_remove("SNAP_LIBRARY_PATH");
}

#[cfg(target_os = "linux")]
fn compat_data_dir_from_game(game_dir: &Path) -> Option<PathBuf> {
    let steamapps_dir = game_dir.parent()?.parent()?;
    let compat = steamapps_dir.join("compatdata/2379780");
    if compat.exists() {
        Some(compat)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn ensure_mod_dir_link(prefix: &Path) -> Result<(), String> {
    let Some(host_config) = dirs::config_dir() else {
        return Ok(());
    };
    let host_mods = host_config.join("Balatro").join("Mods");
    let prefix_mods = prefix.join("drive_c/users/steamuser/AppData/Roaming/Balatro/Mods");

    // Ensure host mods dir exists
    if let Err(e) = fs::create_dir_all(&host_mods) {
        warn!(
            "Failed to create host mods dir {}: {}",
            host_mods.display(),
            e
        );
    }

    if prefix_mods.exists() {
        if prefix_mods.is_symlink() {
            return Ok(());
        }
        warn!(
            "Proton mods path already exists and is not a symlink: {}",
            prefix_mods.display()
        );
        return Ok(());
    }

    if let Some(parent) = prefix_mods.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!(
                "Failed to create Proton mods parent {}: {}",
                parent.display(),
                e
            ));
        }
    }

    symlink(&host_mods, &prefix_mods).map_err(|e| {
        format!(
            "Failed to link Proton mods dir {} -> {}: {}",
            prefix_mods.display(),
            host_mods.display(),
            e
        )
    })?;
    info!(
        "Linked Proton mods dir to host: {} -> {}",
        prefix_mods.display(),
        host_mods.display()
    );
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn launch_balatro(state: tauri::State<'_, AppState>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let (path_str, lovely_console_enabled) = get_installation_and_console(&state)?;
    let path = PathBuf::from(path_str);

    let mut cmd = Command::new(path.join("Balatro.exe"));

    // Respect the "Enable Lovely Console" setting on Windows by hiding the console
    // when disabled. If enabled, let the process manage its own console normally.
    if !lovely_console_enabled {
        // Ask Lovely to suppress its console and also prevent a console window
        // from being created for the process.
        cmd.arg("--disable-console");
        cmd.env("LOVELY_DISABLE_CONSOLE", "1");
        cmd.env("LOVELY_NO_CONSOLE", "1");
        cmd.env("LOVELY_CONSOLE", "0");
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
#[tauri::command]
pub async fn launch_balatro(_state: tauri::State<'_, AppState>) -> Result<(), String> {
    Err("Launching Balatro is not supported on this operating system".to_string())
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub async fn launch_balatro(state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Prefer stored install path; fall back to discovered path if missing
    let install_path = {
        let db = state.db.lock().map_err(|_| {
            AppError::LockPoisoned("Database lock poisoned".to_string()).to_string()
        })?;
        match db.get_installation_path() {
            Ok(Some(p)) => Some(PathBuf::from(p)),
            _ => None,
        }
    };

    let path = install_path
        .or_else(|| bmm_lib::finder::get_balatro_paths().into_iter().next())
        .ok_or_else(|| "No Balatro installation path is configured or detected".to_string())?;

    let lovely_console_enabled = {
        let db = state.db.lock().map_err(|_| {
            AppError::LockPoisoned("Database lock poisoned".to_string()).to_string()
        })?;
        db.is_lovely_console_enabled().map_err(|e| e.to_string())?
    };

    let _balatro = bmm_lib::balamod::Balatro::from_custom_path(path.clone())
        .ok_or_else(|| "Stored Balatro path is no longer valid".to_string())?;

    // Ensure Lovely's version.dll is present before launching
    ensure_version_dll_exists(&path)
        .await
        .map_err(|e| format!("Failed to ensure version.dll: {e}"))?;

    // Keep mods in the Proton prefix pointed at the host-managed mod directory
    let compat_data_dir = compat_data_dir_from_game(&path);
    let proton_prefix = compat_data_dir.as_ref().map(|d| d.join("pfx"));
    if let Some(prefix) = proton_prefix.as_ref() {
        ensure_mod_dir_link(prefix)?;
    }

    // Try to derive Proton/Wine prefix alongside the Steam library to keep environment aligned.
    // Typical path: ~/.local/share/Steam/steamapps/common/Balatro
    let steamapps_dir = path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "Could not determine Steam library root from Balatro path".to_string())?;

    // Compat data root (no trailing pfx); Proton expects STEAM_COMPAT_DATA_PATH here.
    let compat_data_dir = compat_data_dir.or_else(|| {
        let candidate = steamapps_dir.join(format!("compatdata/{STEAM_APP_ID}"));
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    });
    // 1) Prefer the registered Steam URL handler; this should always hit the native client.
    let mut xdg = Command::new("xdg-open");
    xdg.arg(format!("steam://rungameid/{STEAM_APP_ID}"));
    strip_python_env(&mut xdg);
    strip_wrapper_env(&mut xdg);
    if xdg.spawn().is_ok() {
        return Ok(());
    }

    // 2) Fallback: direct steam binary with Lovely/Proton env.
    let mut steam_cmd = Command::new("steam");
    steam_cmd
        .args(["-applaunch", STEAM_APP_ID])
        .env("WINEDLLOVERRIDES", "version=n,b")
        .env("STEAM_COMPAT_APP_ID", STEAM_APP_ID)
        .env("SteamAppId", STEAM_APP_ID)
        .env("SteamGameId", STEAM_APP_ID)
        .env("SteamOverlayGameId", STEAM_APP_ID)
        .env("PROTON_LOG", "0");
    if !lovely_console_enabled {
        steam_cmd.env("LOVELY_DISABLE_CONSOLE", "1");
        steam_cmd.env("LOVELY_NO_CONSOLE", "1");
        steam_cmd.env("LOVELY_CONSOLE", "0");
    }
    if let Some(compat) = compat_data_dir.as_ref() {
        steam_cmd.env("STEAM_COMPAT_DATA_PATH", compat);
    }
    if let Some(steam_root) = steamapps_dir.parent() {
        steam_cmd.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root);
    }
    strip_python_env(&mut steam_cmd);
    strip_wrapper_env(&mut steam_cmd);
    if steam_cmd.spawn().is_ok() {
        return Ok(());
    }

    // 3) Fallback: Flatpak Steam with Lovely/Proton env.
    let mut flatpak_cmd = Command::new("flatpak");
    flatpak_cmd
        .args([
            "run",
            "com.valvesoftware.Steam",
            "steam://rungameid/2379780",
        ])
        .env("WINEDLLOVERRIDES", "version=n,b")
        .env("STEAM_COMPAT_APP_ID", STEAM_APP_ID)
        .env("SteamAppId", STEAM_APP_ID)
        .env("SteamGameId", STEAM_APP_ID)
        .env("SteamOverlayGameId", STEAM_APP_ID)
        .env("PROTON_LOG", "0");
    if !lovely_console_enabled {
        flatpak_cmd.env("LOVELY_DISABLE_CONSOLE", "1");
        flatpak_cmd.env("LOVELY_NO_CONSOLE", "1");
        flatpak_cmd.env("LOVELY_CONSOLE", "0");
    }
    if let Some(compat) = compat_data_dir.as_ref() {
        flatpak_cmd.env("STEAM_COMPAT_DATA_PATH", compat);
    }
    if let Some(steam_root) = steamapps_dir.parent() {
        flatpak_cmd.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root);
    }
    strip_python_env(&mut flatpak_cmd);
    strip_wrapper_env(&mut flatpak_cmd);
    if flatpak_cmd.spawn().is_ok() {
        return Ok(());
    }

    Err("Failed to launch Balatro: Steam must be installed (native or Flatpak) and available on PATH.".to_string())
}

#[tauri::command]
pub async fn get_steamodded_versions() -> Result<Vec<String>, String> {
    let installer = ModInstaller::new(ModType::Steamodded);
    installer
        .get_available_versions()
        .await
        .map(|versions| versions.into_iter().map(|v| v.to_string()).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_steamodded_version(version: String) -> Result<String, String> {
    let installer = ModInstaller::new(ModType::Steamodded);
    installer
        .install_version(&version)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_talisman_versions() -> Result<Vec<String>, String> {
    let installer = ModInstaller::new(ModType::Talisman);
    installer
        .get_available_versions()
        .await
        .map(|versions| versions.into_iter().map(|v| v.to_string()).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_latest_steamodded_release() -> Result<String, String> {
    if let Ok(Some(versions)) = cache::load_versions_cache("steamodded") {
        if !versions.is_empty() {
            let version = &versions[0];
            return Ok(format!(
                "https://github.com/Steamodded/smods/archive/refs/tags/{version}.zip"
            ));
        }
    }

    let installer = ModInstaller::new(ModType::Steamodded);
    installer
        .get_latest_release()
        .await
        .map(|version| match installer.mod_type {
            ModType::Steamodded => {
                format!("https://github.com/Steamodded/smods/archive/refs/tags/{version}.zip")
            }
            _ => format!("https://github.com/Steamodded/smods/archive/refs/tags/{version}.zip"),
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_talisman_version(version: String) -> Result<String, String> {
    let installer = ModInstaller::new(ModType::Talisman);
    installer
        .install_version(&version)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_dependents(mod_name: String) -> Result<Vec<String>, String> {
    let db = bmm_lib::database::Database::new().map_err(|e| e.to_string())?;
    let all_dependents = db.get_dependents(&mod_name).map_err(|e| e.to_string())?;
    let filtered: Vec<String> = all_dependents
        .into_iter()
        .filter(|d| d != &mod_name)
        .collect();
    Ok(filtered)
}

#[tauri::command]
pub async fn cascade_uninstall(
    state: tauri::State<'_, AppState>,
    root_mod: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut to_uninstall = vec![root_mod.clone()];
    let mut processed = std::collections::HashSet::new();

    while let Some(current) = to_uninstall.pop() {
        if processed.contains(&current) {
            continue;
        }
        processed.insert(current.clone());

        let mod_details = map_error(db.get_mod_details(&current))?;
        let dependents = map_error(db.get_dependents(&current))?;
        to_uninstall.extend(dependents);

        map_error(bmm_lib::installer::uninstall_mod(PathBuf::from(
            mod_details.path,
        )))?;
        map_error(db.remove_installed_mod(&current))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn force_remove_mod(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
) -> Result<(), String> {
    map_error(bmm_lib::installer::uninstall_mod(PathBuf::from(path)))?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    map_error(db.remove_installed_mod(&name))
}

#[tauri::command]
pub async fn remove_installed_mod(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    let is_framework = name.to_lowercase() == "steamodded" || name.to_lowercase() == "talisman";
    if is_framework {
        let all_dependents = map_error(db.get_dependents(&name))?;
        let real_deps: Vec<String> = all_dependents.into_iter().filter(|d| d != &name).collect();
        if !real_deps.is_empty() {
            return Err(format!(
                "Use cascade_uninstall to remove {} with {} dependents",
                name,
                real_deps.len()
            ));
        }
    }

    map_error(bmm_lib::installer::uninstall_mod(PathBuf::from(path)))?;
    map_error(db.remove_installed_mod(&name))
}

#[tauri::command]
pub async fn install_mod(url: String, folder_name: String) -> Result<PathBuf, String> {
    let folder_name = if folder_name.is_empty() {
        None
    } else {
        Some(folder_name)
    };
    map_error(bmm_lib::installer::install_mod(url, folder_name).await)
}

#[tauri::command]
pub async fn get_installed_mods_from_db(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstalledMod>, String> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::LockPoisoned("Database lock poisoned".to_string()))?;
    map_error(db.get_installed_mods())
}

#[tauri::command]
pub async fn add_installed_mod(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
    dependencies: Vec<String>,
    current_version: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let current_version = if current_version.is_empty() {
        None
    } else {
        Some(current_version)
    };
    map_error(db.add_installed_mod(&name, &path, &dependencies, current_version))
}
