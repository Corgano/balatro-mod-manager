#[cfg(target_os = "linux")]
use log::{info, warn};
#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::fs::remove_file;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;

use crate::bmi::BmiClient;
use crate::compat_helper;
use crate::state::AppState;
use crate::util::map_error;
#[cfg(target_os = "macos")]
use bmm_lib::lovely;
#[cfg(target_os = "linux")]
use bmm_lib::lovely::ensure_version_dll_exists;
#[cfg(target_os = "linux")]
use bmm_lib::lovely::{ensure_love_binary, ensure_lovely_so_exists, get_latest_lovely_version};
use bmm_lib::smods_installer::{ModInstaller, ModType};
use bmm_lib::{cache, database::InstalledMod};
use bmm_lib::{errors::AppError, local_mod_detection};
#[cfg(target_os = "linux")]
use shell_words::split as split_shell_words;
use tauri::Emitter;

fn sync_compat_helper_after_mod_change(state: &tauri::State<'_, AppState>) {
    let enabled = match state.db.lock() {
        Ok(db) => db.is_compat_helper_enabled().unwrap_or(false),
        Err(_) => false,
    };
    if let Err(err) = compat_helper::sync_compat_helper(enabled) {
        log::warn!("Failed to sync compatibility helper after mod change: {err}");
    }
}

fn emit_installed_mods_changed(app_handle: &tauri::AppHandle) {
    let _ = app_handle.emit("installed-mods-changed", ());
}

fn refresh_mod_detection_cache() {
    local_mod_detection::clear_detection_cache();
}

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
    let (path_str, lovely_console_enabled, launch_mode) = {
        let db = state.db.lock().map_err(|_| {
            AppError::LockPoisoned("Database lock poisoned".to_string()).to_string()
        })?;
        let install_path = db
            .get_installation_path()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                AppError::InvalidState("No installation path set".to_string()).to_string()
            })?;
        let lovely_console = db.is_lovely_console_enabled().map_err(|e| e.to_string())?;
        let mode = db
            .get_launch_mode()
            .unwrap_or_else(|_| "modded".to_string());
        (install_path, lovely_console, mode)
    };

    let is_vanilla = launch_mode == "vanilla";
    let path = PathBuf::from(path_str);
    let balatro = bmm_lib::balamod::Balatro::from_custom_path(path.clone())
        .ok_or_else(|| "Stored Balatro path is no longer valid".to_string())?;

    let app_bundle = balatro
        .get_app_bundle_path()
        .ok_or_else(|| "Unable to locate Balatro app bundle".to_string())?;
    let balatro_executable = app_bundle.join("Contents/MacOS/love");
    let launch_root = balatro.path.clone();

    // Only get lovely path if launching in modded mode
    let lovely_path: Option<PathBuf> = if !is_vanilla {
        Some(map_error(lovely::ensure_lovely_exists().await)?)
    } else {
        None
    };

    if lovely_console_enabled {
        let disable_arg = if !lovely_console_enabled {
            " --disable-console"
        } else {
            ""
        };

        let command_line = if let Some(ref lovely) = lovely_path {
            format!(
                "cd '{}' && DYLD_INSERT_LIBRARIES='{}' '{}'{}",
                launch_root.display(),
                lovely.display(),
                balatro_executable.display(),
                disable_arg
            )
        } else {
            format!(
                "cd '{}' && '{}'{}",
                launch_root.display(),
                balatro_executable.display(),
                disable_arg
            )
        };

        let applescript = format!("tell application \"Terminal\" to do script \"{command_line}\"");

        Command::new("osascript")
            .arg("-e")
            .arg(applescript)
            .status()
            .map_err(|e| e.to_string())?;
    } else {
        let cmd = if let Some(ref lovely) = lovely_path {
            format!(
                "DYLD_INSERT_LIBRARIES='{}' '{}'",
                lovely.display(),
                balatro_executable.display()
            )
        } else {
            format!("'{}'", balatro_executable.display())
        };
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
    cmd.env_remove("SNAP");
    cmd.env_remove("SNAP_NAME");
    cmd.env_remove("SNAP_REVISION");
    cmd.env_remove("SNAP_INSTANCE_NAME");
    cmd.env_remove("SNAP_INSTANCE_KEY");
    cmd.env_remove("SNAP_ARCH");
    cmd.env_remove("SNAP_LIBRARY_PATH");
}

#[cfg(target_os = "linux")]
fn log_launch_env(label: &str) {
    let keys = [
        "FLATPAK_ID",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SDL_VIDEODRIVER",
        "GDK_BACKEND",
        "MESA_GL_VERSION_OVERRIDE",
        "MESA_GLSL_VERSION_OVERRIDE",
        "VK_ICD_FILENAMES",
        "VK_DRIVER_FILES",
        "DRI_PRIME",
        "LIBGL_ALWAYS_SOFTWARE",
        "LIBGL_ALWAYS_INDIRECT",
    ];

    let mut dump = String::new();
    for key in keys {
        let val = std::env::var(key).unwrap_or_else(|_| "<unset>".to_string());
        dump.push_str(key);
        dump.push('=');
        dump.push_str(&val);
        dump.push('\n');
    }
    info!("Linux launch env ({label}):\n{dump}");
}

#[cfg(target_os = "linux")]
fn split_prefix_env(parts: &[String]) -> (Vec<(String, String)>, Vec<String>) {
    let mut envs = Vec::new();
    let mut rest = Vec::new();
    let mut in_env = true;

    for part in parts {
        if in_env
            && is_env_assignment(part)
            && let Some((key, value)) = part.split_once('=')
        {
            envs.push((key.to_string(), value.to_string()));
            continue;
        }
        in_env = false;
        rest.push(part.clone());
    }

    (envs, rest)
}

#[cfg(target_os = "linux")]
fn is_env_assignment(value: &str) -> bool {
    let Some((key, rhs)) = value.split_once('=') else {
        return false;
    };
    if key.is_empty() || rhs.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(target_os = "linux")]
fn ensure_prefix_log_dir(envs: &[(String, String)]) -> Result<(), String> {
    let log_dir = envs
        .iter()
        .find(|(key, _)| key == "PROTON_LOG_DIR")
        .map(|(_, value)| value);
    let Some(log_dir) = log_dir else {
        return Ok(());
    };
    if log_dir.trim().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(log_dir)
        .map_err(|e| format!("Failed to create PROTON_LOG_DIR {log_dir}: {e}"))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_prefix_log_file() -> Result<std::fs::File, String> {
    let Some(config_dir) = dirs::config_dir() else {
        return Err("Failed to resolve config directory for prefix log".to_string());
    };
    let log_dir = config_dir.join("Balatro/logs");
    fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Failed to create prefix log dir {}: {e}", log_dir.display()))?;
    let log_path = log_dir.join("prefix-launch.log");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open prefix log file {}: {e}", log_path.display()))
}

#[cfg(target_os = "linux")]
fn validate_prefix_executable(exe: &str) -> Result<(), String> {
    if !exe.contains('/') {
        return Ok(());
    }
    let path = std::path::Path::new(exe);
    if !path.exists() {
        return Err(format!("Prefix executable not found: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!(
            "Prefix executable is not a file: {}",
            path.display()
        ));
    }
    let mode = path
        .metadata()
        .map_err(|e| format!("Failed to stat prefix executable: {e}"))?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        return Err(format!(
            "Prefix executable is not executable: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn ensure_native_mod_dir_link() -> Result<(), String> {
    // Prefer host config/data paths even inside Flatpak, so mods/logs live on the host.
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(());
    };
    let host_config = home_dir.join(".config").join("Balatro");
    let host_mods = host_config.join("Mods");

    let link_mods = |love_mods: PathBuf| -> Result<(), String> {
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

        if let Some(parent) = love_mods.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Err(format!(
                "Failed to create LOVE mods parent {}: {}",
                parent.display(),
                e
            ));
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
    };

    // Ensure host mods dir exists
    if let Err(e) = fs::create_dir_all(&host_mods) {
        warn!(
            "Failed to create host mods dir {}: {}",
            host_mods.display(),
            e
        );
    }

    // Link both data and config locations that LOVE may use
    // LOVE uses game-specific subdirectories based on the identity in conf.lua
    // For Balatro, this is "Balatro", so mods should be in ~/.local/share/love/Balatro/Mods
    let love_mods_data = home_dir.join(".local/share/love/Balatro/Mods");
    let _ = link_mods(love_mods_data);
    let love_mods_config = host_config.join("love/Balatro/Mods");
    let _ = link_mods(love_mods_config);

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

    let (lovely_console_enabled, linux_prefix, launch_mode) = {
        let db = state.db.lock().map_err(|_| {
            AppError::LockPoisoned("Database lock poisoned".to_string()).to_string()
        })?;
        let lovely = db.is_lovely_console_enabled().map_err(|e| e.to_string())?;
        let prefix = db
            .get_linux_prefix()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let mode = db
            .get_launch_mode()
            .unwrap_or_else(|_| "modded".to_string());
        (lovely, prefix, mode)
    };

    let is_vanilla = launch_mode == "vanilla";

    // Validate Balatro path
    let _balatro = bmm_lib::balamod::Balatro::from_custom_path(path.clone())
        .ok_or_else(|| "Stored Balatro path is no longer valid".to_string())?;

    let linux_prefix = linux_prefix.trim().to_string();
    if !linux_prefix.is_empty() {
        let parts = split_shell_words(&linux_prefix)
            .map_err(|e| format!("Invalid Linux prefix command: {e}"))?;
        if parts.is_empty() {
            return Err("Linux prefix command is empty".to_string());
        }
        let (envs, cmd_parts) = split_prefix_env(&parts);
        if cmd_parts.is_empty() {
            return Err("Linux prefix command missing executable".to_string());
        }
        validate_prefix_executable(&cmd_parts[0])?;
        ensure_prefix_log_dir(&envs)?;

        // Only ensure version.dll exists if launching in modded mode
        if !is_vanilla {
            ensure_version_dll_exists(&path)
                .await
                .map_err(|e| format!("Failed to ensure version.dll: {e}"))?;
        }

        bmm_lib::local_mod_detection::ensure_proton_mod_dir_link(Some(&path))?;

        let exe_path = find_executable_in_directory(&path)
            .ok_or_else(|| format!("No executable found in {}", path.display()))?;
        log_launch_env("pre-spawn (prefix)");
        info!(
            "Launching Balatro via Linux prefix (mode: {})\n  prefix_cmd={}\n  exe={}\n  cwd={}",
            launch_mode,
            linux_prefix,
            exe_path.display(),
            path.display()
        );

        let exe_arg = exe_path.to_string_lossy().to_string();
        let uses_placeholder = cmd_parts.iter().any(|part| part == "{exe}");
        let resolved_parts: Vec<String> = cmd_parts
            .iter()
            .map(|part| {
                if part == "{exe}" {
                    exe_arg.clone()
                } else {
                    part.clone()
                }
            })
            .collect();
        let is_steam_applaunch = resolved_parts.first().is_some_and(|first| {
            let last = first.rsplit('/').next().unwrap_or(first.as_str());
            (last == "steam" || last == "steam.sh")
                && resolved_parts.iter().any(|p| p == "-applaunch")
        });
        let mut launch_parts = resolved_parts.clone();
        if is_steam_applaunch && std::env::var_os("FLATPAK_ID").is_some() {
            // Flatpak sandbox can't see host steam directly; route via flatpak-spawn.
            if let Some(first) = launch_parts.first().cloned() {
                launch_parts = vec!["flatpak-spawn".to_string(), "--host".to_string(), first];
                launch_parts.extend(resolved_parts.iter().skip(1).cloned());
                info!("Flatpak detected; using flatpak-spawn --host for Steam launch");
            }
        }
        let append_exe = !uses_placeholder && !is_steam_applaunch;

        info!(
            "Prefix exec: {} args: {:?} append_exe={}",
            launch_parts[0],
            &launch_parts[1..],
            append_exe
        );
        let mut cmd = Command::new(&launch_parts[0]);
        if launch_parts.len() > 1 {
            cmd.args(&launch_parts[1..]);
        }

        // Apply user-provided environment variables
        for (key, value) in &envs {
            cmd.env(key, value);
        }

        // Only set WINEDLLOVERRIDES for Lovely injection if in modded mode
        if !is_vanilla {
            let winedll_idx = envs
                .iter()
                .position(|(key, _)| key.eq_ignore_ascii_case("WINEDLLOVERRIDES"));
            match winedll_idx {
                None => {
                    cmd.env("WINEDLLOVERRIDES", "version=n,b");
                    info!(
                        "WINEDLLOVERRIDES not set; defaulting to version=n,b for Lovely injection"
                    );
                }
                Some(idx) => {
                    let value = envs.get(idx).map(|(_, v)| v.as_str()).unwrap_or("");
                    if !value.contains("version") {
                        let updated = if value.is_empty() {
                            "version=n,b".to_string()
                        } else {
                            format!("{value};version=n,b")
                        };
                        cmd.env("WINEDLLOVERRIDES", updated);
                        info!(
                            "WINEDLLOVERRIDES missing version; appended version=n,b for Lovely injection"
                        );
                    }
                }
            }
        } else {
            info!("Vanilla mode: skipping WINEDLLOVERRIDES injection");
        }
        cmd.current_dir(&path);
        if append_exe {
            cmd.arg(exe_arg);
        }
        if !lovely_console_enabled {
            cmd.arg("--disable-console");
        }
        if let Ok(log_file) = open_prefix_log_file() {
            let stderr_file = match log_file.try_clone() {
                Ok(file) => file,
                Err(_) => {
                    // If clone fails, avoid capturing stderr rather than moving stdout handle.
                    cmd.stdout(Stdio::from(log_file));
                    return cmd
                        .spawn()
                        .map_err(|e| format!("Failed to launch via prefix: {e}"))
                        .map(|_| ());
                }
            };
            cmd.stdout(Stdio::from(log_file));
            cmd.stderr(Stdio::from(stderr_file));
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to launch via prefix: {e}"))?;
        return Ok(());
    }

    // Ensure host mods map to LOVE's native mods dir
    ensure_native_mod_dir_link()?;

    // Only set up Lovely injection if in modded mode
    let lovely_so: Option<PathBuf> = if !is_vanilla {
        // Ensure Lovely's liblovely.so is present for native LOVE injection
        // Refresh Lovely if a newer version is available
        if let Ok(latest) = get_latest_lovely_version().await
            && let Ok(db) = state.db.lock()
            && let Ok(current) = db.get_lovely_version()
            && current.as_deref() != Some(latest.as_str())
        {
            let _ = db.set_lovely_version(&latest);
            if let Some(config_dir) = dirs::config_dir() {
                let _ = remove_file(config_dir.join("Balatro/bins/liblovely.so"));
            }
            let _ = remove_file(path.join("liblovely.so"));
        }

        Some(
            ensure_lovely_so_exists(&path)
                .await
                .map_err(|e| format!("Failed to ensure liblovely.so: {e}"))?,
        )
    } else {
        info!("Vanilla mode: skipping Lovely injection setup");
        None
    };

    // Native LOVE launch (no Steam/Proton)
    let love_bin_env = env::var("BMM_LOVE_BIN").ok();
    let mut love_bin_path =
        PathBuf::from(love_bin_env.clone().unwrap_or_else(|| "love".to_string()));
    let mut love_lib_path: Option<PathBuf> = None;
    let mut love_available = Command::new(&love_bin_path)
        .arg("--version")
        .output()
        .is_ok();

    if !love_available {
        // Auto-download the LOVE tarball if not present on the system.
        match ensure_love_binary().await {
            Ok((bin, lib_dir)) => {
                love_bin_path = bin;
                love_lib_path = lib_dir;
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

    // Ensure Balatro.exe is available as a .love zip for LOVE to load cleanly.
    let balatro_love = path.join("Balatro.love");
    let balatro_exe = path.join("Balatro.exe");
    if balatro_exe.exists() && !balatro_love.exists() {
        let _ = fs::copy(&balatro_exe, &balatro_love);
    }

    let mut love_cmd = Command::new(&love_bin_path);
    love_cmd.current_dir(&path).arg("Balatro.love");

    // Only set LD_PRELOAD for Lovely injection if in modded mode
    if let Some(ref lovely_so_path) = lovely_so {
        love_cmd.env("LD_PRELOAD", lovely_so_path);
    }

    // Use host config dir so mods/logs live outside the Flatpak sandbox.
    if let Some(home) = dirs::home_dir() {
        let host_config = home.join(".config").join("Balatro");
        let _ = fs::create_dir_all(&host_config);
        if env::var("FLATPAK_ID").is_ok() || env::var("XDG_CONFIG_HOME").is_err() {
            love_cmd.env("XDG_CONFIG_HOME", &host_config);
        }
    }
    // Nudge SDL to a usable video backend inside Flatpak without hard-forcing X11
    // when it's unavailable. If DISPLAY exists, prefer X11; otherwise fall back to Wayland.
    if env::var("SDL_VIDEODRIVER").is_err() && env::var("FLATPAK_ID").is_ok() {
        if env::var("DISPLAY").is_ok() {
            love_cmd.env("SDL_VIDEODRIVER", "x11");
        } else if env::var("WAYLAND_DISPLAY").is_ok() {
            love_cmd.env("SDL_VIDEODRIVER", "wayland");
        }
    }
    if let Some(ref lib_dir) = love_lib_path {
        love_cmd.env("LD_LIBRARY_PATH", lib_dir);
    }
    if !lovely_console_enabled {
        love_cmd.env("LOVELY_DISABLE_CONSOLE", "1");
        love_cmd.env("LOVELY_NO_CONSOLE", "1");
        love_cmd.env("LOVELY_CONSOLE", "0");
    }
    unsafe {
        love_cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    strip_python_env(&mut love_cmd);
    strip_wrapper_env(&mut love_cmd);
    log_launch_env("pre-spawn");
    info!(
        "Launching Balatro via LOVE (mode: {})\n  love_bin={}\n  love_lib={}\n  preload={}\n  cwd={}",
        launch_mode,
        love_bin_path.display(),
        love_lib_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        lovely_so
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        path.display()
    );
    let spawn_result = love_cmd.spawn();

    spawn_result.map_err(|e| format!("Failed to launch Balatro via native LOVE: {e}"))?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn find_executable_in_directory(dir: &PathBuf) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut executables: Vec<PathBuf> = Vec::new();

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("exe")) {
                executables.push(path);
            }
        }

        if executables.is_empty() {
            return None;
        }

        for exe in &executables {
            if let Some(file_name) = exe.file_name().and_then(|n| n.to_str())
                && file_name.to_lowercase().contains("balatro")
            {
                return Some(exe.clone());
            }
        }

        return Some(executables[0].clone());
    }

    None
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
pub async fn install_steamodded_version(
    state: tauri::State<'_, AppState>,
    version: String,
) -> Result<String, String> {
    let installer = ModInstaller::new(ModType::Steamodded);
    let path = installer
        .install_version(&version)
        .await
        .map_err(|e| e.to_string())?;
    sync_compat_helper_after_mod_change(&state);
    Ok(path)
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
    if let Ok(Some(versions)) = cache::load_versions_cache("steamodded")
        && !versions.is_empty()
    {
        let version = &versions[0];
        return Ok(format!(
            "https://github.com/Steamodded/smods/archive/refs/tags/{version}.zip"
        ));
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
pub async fn install_talisman_version(
    state: tauri::State<'_, AppState>,
    version: String,
) -> Result<String, String> {
    let installer = ModInstaller::new(ModType::Talisman);
    let path = installer
        .install_version(&version)
        .await
        .map_err(|e| e.to_string())?;
    sync_compat_helper_after_mod_change(&state);
    Ok(path)
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
    app_handle: tauri::AppHandle,
    root_mod: String,
) -> Result<(), String> {
    {
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

            let mod_path_str = mod_details.path.clone();
            let mod_path = PathBuf::from(&mod_path_str);
            if mod_path.exists() {
                map_error(bmm_lib::installer::uninstall_mod(mod_path))?;
            }
            map_error(db.remove_installed_mod_by_name_or_path(&current, &mod_path_str))?;
        }
    }

    sync_compat_helper_after_mod_change(&state);
    refresh_mod_detection_cache();
    emit_installed_mods_changed(&app_handle);
    Ok(())
}

#[tauri::command]
pub async fn force_remove_mod(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    name: String,
    path: String,
) -> Result<(), String> {
    if !path.trim().is_empty() {
        let path_buf = PathBuf::from(&path);
        if path_buf.exists() {
            map_error(bmm_lib::installer::uninstall_mod(path_buf))?;
        }
    }
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        map_error(db.remove_installed_mod_by_name_or_path(&name, &path))?;
    }
    sync_compat_helper_after_mod_change(&state);
    refresh_mod_detection_cache();
    emit_installed_mods_changed(&app_handle);
    Ok(())
}

#[tauri::command]
pub async fn remove_installed_mod(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    name: String,
    path: String,
) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;

        let is_framework = name.to_lowercase() == "steamodded" || name.to_lowercase() == "talisman";
        if is_framework {
            let all_dependents = map_error(db.get_dependents(&name))?;
            let real_deps: Vec<String> =
                all_dependents.into_iter().filter(|d| d != &name).collect();
            if !real_deps.is_empty() {
                return Err(format!(
                    "Use cascade_uninstall to remove {} with {} dependents",
                    name,
                    real_deps.len()
                ));
            }
        }

        if !path.trim().is_empty() {
            let path_buf = PathBuf::from(&path);
            if path_buf.exists() {
                map_error(bmm_lib::installer::uninstall_mod(path_buf))?;
            }
        }
        map_error(db.remove_installed_mod_by_name_or_path(&name, &path))?;
    }
    sync_compat_helper_after_mod_change(&state);
    refresh_mod_detection_cache();
    emit_installed_mods_changed(&app_handle);
    Ok(())
}

#[tauri::command]
pub async fn install_mod(
    state: tauri::State<'_, AppState>,
    url: String,
    folder_name: String,
) -> Result<PathBuf, String> {
    let folder_name = if folder_name.is_empty() {
        None
    } else {
        Some(folder_name)
    };
    let resolved_url = if let Some(id) = url.strip_prefix("bmi://") {
        if id.trim().is_empty() {
            return Err("BMI download missing mod id".to_string());
        }
        let client = BmiClient::new()?;
        client.post_download(id).await?
    } else {
        url
    };
    let path = map_error(bmm_lib::installer::install_mod(resolved_url, folder_name).await)?;
    sync_compat_helper_after_mod_change(&state);
    Ok(path)
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
    app_handle: tauri::AppHandle,
    name: String,
    path: String,
    dependencies: Vec<String>,
    current_version: String,
) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let current_version = if current_version.is_empty() {
            None
        } else {
            Some(current_version)
        };
        map_error(db.add_installed_mod(&name, &path, &dependencies, current_version))?;
    }
    refresh_mod_detection_cache();
    emit_installed_mods_changed(&app_handle);
    Ok(())
}
