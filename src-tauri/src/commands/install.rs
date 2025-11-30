#[cfg(target_os = "linux")]
use log::{info, warn};
#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::process::Command;

use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::errors::AppError;
#[cfg(target_os = "macos")]
use bmm_lib::lovely;
#[cfg(target_os = "linux")]
use bmm_lib::lovely::{ensure_love_appimage, ensure_lovely_so_exists};
use bmm_lib::smods_installer::{ModInstaller, ModType};
use bmm_lib::{cache, database::InstalledMod};

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
fn ensure_native_mod_dir_link() -> Result<(), String> {
    let Some(host_config) = dirs::config_dir() else {
        return Ok(());
    };
    let host_mods = host_config.join("Balatro").join("Mods");
    let love_mods = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("love/Mods");

    if let Err(e) = fs::create_dir_all(&host_mods) {
        warn!(
            "Failed to create host mods dir {}: {}",
            host_mods.display(),
            e
        );
    }

    if love_mods.exists() {
        if love_mods.is_symlink() {
            return Ok(());
        }
        warn!(
            "LOVE mods path already exists and is not a symlink: {}",
            love_mods.display()
        );
        return Ok(());
    }

    if let Some(parent) = love_mods.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!(
                "Failed to create LOVE mods parent {}: {}",
                parent.display(),
                e
            ));
        }
    }

    symlink(&host_mods, &love_mods).map_err(|e| {
        format!(
            "Failed to link LOVE mods dir {} -> {}: {}",
            love_mods.display(),
            host_mods.display(),
            e
        )
    })?;
    info!(
        "Linked LOVE mods dir to host: {} -> {}",
        love_mods.display(),
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

    // Validate Balatro path
    let _balatro = bmm_lib::balamod::Balatro::from_custom_path(path.clone())
        .ok_or_else(|| "Stored Balatro path is no longer valid".to_string())?;

    // Ensure host mods map to LOVE's native mods dir
    ensure_native_mod_dir_link()?;

    // Ensure Lovely's liblovely.so is present for native LOVE injection
    let lovely_so = ensure_lovely_so_exists(&path)
        .await
        .map_err(|e| format!("Failed to ensure liblovely.so: {e}"))?;

    // Native LOVE launch (no Steam/Proton)
    let love_bin_env = env::var("BMM_LOVE_BIN").ok();
    let mut love_bin_path =
        PathBuf::from(love_bin_env.clone().unwrap_or_else(|| "love".to_string()));
    let mut love_available = Command::new(&love_bin_path)
        .arg("--version")
        .output()
        .is_ok();

    if !love_available {
        // Auto-download the LOVE AppImage if not present on the system.
        match ensure_love_appimage().await {
            Ok(appimage) => {
                love_bin_path = appimage;
                love_available = true;
            }
            Err(e) => {
                return Err(format!(
                    "LOVE is not installed and auto-download failed: {e}. Install love (e.g. sudo apt install love) or set BMM_LOVE_BIN."
                ));
            }
        }
    }

    if !love_available {
        return Err("LOVE is not installed or could not be downloaded automatically.".to_string());
    }
    let using_appimage = love_bin_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("AppImage"))
        .unwrap_or(false);

    let mut love_cmd = Command::new(&love_bin_path);
    love_cmd
        .current_dir(&path)
        .arg("Balatro.exe")
        .env("LD_PRELOAD", &lovely_so);
    if using_appimage {
        // Allow running without FUSE support (common on SteamOS/Deck).
        love_cmd.env("APPIMAGE_EXTRACT_AND_RUN", "1");
    }
    // Avoid inheriting wrapped library paths that can crash the AppImage.
    love_cmd.env_remove("LD_LIBRARY_PATH");
    love_cmd.env_remove("LD_PRELOAD");
    if !lovely_console_enabled {
        love_cmd.env("LOVELY_DISABLE_CONSOLE", "1");
        love_cmd.env("LOVELY_NO_CONSOLE", "1");
        love_cmd.env("LOVELY_CONSOLE", "0");
    }
    strip_python_env(&mut love_cmd);
    strip_wrapper_env(&mut love_cmd);
    love_cmd
        .spawn()
        .map_err(|e| format!("Failed to launch Balatro via native LOVE: {e}"))?;

    Ok(())
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
