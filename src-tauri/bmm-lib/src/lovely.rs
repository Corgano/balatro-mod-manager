//! Lovely (mod loader/injector) installer and version management.
//!
//! Lovely is the mod injection framework that enables Balatro mods to work.
//! This module handles:
//! - Downloading and installing the Lovely injector
//! - Version checking and updates
//! - Platform-specific installation (Windows DLL, macOS dylib, Linux .so)
//!
//! # Platform-Specific Details
//!
//! - **Windows/Linux (Proton)**: Uses `version.dll` for DLL injection
//! - **macOS**: Uses `liblovely.dylib` with `DYLD_INSERT_LIBRARIES`
//! - **Linux (native LOVE)**: Uses `liblovely.so` with `LD_PRELOAD`

use crate::errors::AppError;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::{self};
#[cfg(target_os = "macos")]
use std::fs::{self, File};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Cached Lovely version with timestamp
struct CachedVersion {
    version: String,
    fetched_at: Instant,
}

/// Disk-persisted Lovely version cache
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedVersionCache {
    version: String,
    /// Unix timestamp in seconds when fetched
    fetched_at_unix: u64,
}

/// Global cache for Lovely version (24 hour TTL)
static VERSION_CACHE: OnceLock<Mutex<Option<CachedVersion>>> = OnceLock::new();
const VERSION_CACHE_TTL_SECS: u64 = 60 * 60 * 24; // 24 hours

/// Get the path to the Lovely version cache file
fn get_version_cache_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Balatro").join("lovely_version_cache.json"))
}

/// Load Lovely version from disk cache if still fresh
fn load_persisted_version_cache() -> Option<String> {
    let path = get_version_cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let cached: PersistedVersionCache = serde_json::from_str(&content).ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    if now.saturating_sub(cached.fetched_at_unix) < VERSION_CACHE_TTL_SECS {
        log::debug!("Loaded Lovely version from disk cache: {}", cached.version);
        Some(cached.version)
    } else {
        log::debug!("Lovely version disk cache expired");
        None
    }
}

/// Save Lovely version to disk cache
fn save_persisted_version_cache(version: &str) {
    if let Some(path) = get_version_cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cached = PersistedVersionCache {
            version: version.to_string(),
            fetched_at_unix: now,
        };
        if let Ok(json) = serde_json::to_string(&cached) {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to save Lovely version cache: {}", e);
            } else {
                log::debug!("Saved Lovely version to disk cache: {}", version);
            }
        }
    }
}

/// Ensures the Lovely version.dll exists in the game directory (Windows/Linux).
///
/// Downloads the DLL from GitHub releases if not present.
/// On Linux, caches the DLL in ~/.config/Balatro/bins/ and copies to game directory
/// each launch, since Steam may verify/restore game files.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub async fn ensure_version_dll_exists(game_path: &Path) -> Result<PathBuf, AppError> {
    let dll_path = game_path.join("version.dll");

    #[cfg(target_os = "linux")]
    {
        // On Linux, cache the DLL and copy on each launch to survive Steam file verification
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
            path: bins_dir.clone(),
            source: e.to_string(),
        })?;

        let cached_dll = bins_dir.join("version.dll");
        if !cached_dll.exists() {
            download_version_dll(&cached_dll).await?;
        }

        // Always copy to game directory (may have been removed by Steam)
        if let Err(e) = fs::copy(&cached_dll, &dll_path) {
            return Err(AppError::FileCopy {
                source: cached_dll.display().to_string(),
                dest: dll_path.display().to_string(),
                source_error: e.to_string(),
            });
        }
        log::debug!(
            "Copied version.dll from cache {} to {}",
            cached_dll.display(),
            dll_path.display()
        );

        Ok(dll_path)
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, download directly to game directory
        if !dll_path.exists() {
            download_version_dll(&dll_path).await?;
        }
        Ok(dll_path)
    }
}

#[cfg(target_os = "macos")]
fn detect_architecture() -> Result<&'static str, AppError> {
    use libc::{c_void, size_t, sysctl};

    let mut size: size_t = 0;
    let mut mib = [libc::CTL_HW, libc::HW_MACHINE];

    // First call to get buffer size
    unsafe {
        if sysctl(
            mib.as_mut_ptr(),
            2,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(AppError::SystemDetection(
                "Failed to get architecture buffer size".into(),
            ));
        }
    }

    let mut buf = vec![0u8; size];

    // Second call to get actual value
    unsafe {
        if sysctl(
            mib.as_mut_ptr(),
            2,
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(AppError::SystemDetection(
                "Failed to retrieve architecture".into(),
            ));
        }
    }

    // Convert buffer to string and match architecture
    match String::from_utf8_lossy(&buf).trim_end_matches('\0') {
        "arm64" => Ok("aarch64"),
        "x86_64" => Ok("x86_64"),
        other => Err(AppError::UnsupportedArchitecture(other.into())),
    }
}

pub async fn ensure_lovely_exists() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;

        let bins_dir = config_dir.join("Balatro/bins");
        fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
            path: bins_dir.clone(),
            source: e.to_string(),
        })?;

        let lovely_path = bins_dir.join("liblovely.dylib");

        if !lovely_path.exists() {
            download_and_install_lovely(&lovely_path).await?;
        }

        Ok(lovely_path)
    }

    #[cfg(target_os = "windows")]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
        }

        // Ensure version.dll exists in the game directory
        let game_path = &balatro_paths[0];
        ensure_version_dll_exists(game_path).await?;

        Ok(game_path.join("Balatro.exe"))
    }

    #[cfg(target_os = "linux")]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
        }

        // Ensure version.dll exists in the game directory (Proton/Wine)
        let game_path = &balatro_paths[0];
        ensure_version_dll_exists(game_path).await?;

        Ok(game_path.join("Balatro.exe"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
pub async fn ensure_love_binary() -> Result<(PathBuf, Option<PathBuf>), AppError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let bins_dir = config_dir.join("Balatro/bins");
    fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
        path: bins_dir.clone(),
        source: e.to_string(),
    })?;
    let target_dir = bins_dir.join("love");
    let target_bin = target_dir.join("love");
    if target_bin.exists() {
        // Refresh permissions in case they were lost.
        let perms = std::fs::Permissions::from_mode(0o755);
        let _ = std::fs::set_permissions(&target_bin, perms);
        let lib_dir = target_bin
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("lib"))
            .filter(|p| p.is_dir());
        return Ok((target_bin, lib_dir));
    }

    download_love_appimage_and_extract(&target_dir).await?;
    // Prefer extracted AppImage binary: love/bin/love
    let bin = target_dir.join("bin/love");
    if !bin.exists() {
        return Err(AppError::InvalidState(
            "LOVE AppImage extraction did not produce bin/love".to_string(),
        ));
    }

    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&bin, perms).map_err(|e| AppError::FileWrite {
        path: bin.clone(),
        source: e.to_string(),
    })?;
    let lib_dir = target_dir.join("lib");
    Ok((
        bin,
        if lib_dir.is_dir() {
            Some(lib_dir)
        } else {
            None
        },
    ))
}

#[cfg(target_os = "linux")]
pub async fn ensure_lovely_so_exists(game_path: &Path) -> Result<PathBuf, AppError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let bins_dir = config_dir.join("Balatro/bins");
    fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
        path: bins_dir.clone(),
        source: e.to_string(),
    })?;

    let cached_so = bins_dir.join("liblovely.so");
    if !cached_so.exists() {
        download_lovely_linux(&cached_so).await?;
    }

    let target_so = game_path.join("liblovely.so");
    if let Err(e) = fs::copy(&cached_so, &target_so) {
        return Err(AppError::FileCopy {
            source: cached_so.display().to_string(),
            dest: target_so.display().to_string(),
            source_error: e.to_string(),
        });
    }

    // Ensure it is executable for preload
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&target_so, perms).map_err(|e| AppError::FileWrite {
        path: target_so.clone(),
        source: e.to_string(),
    })?;

    Ok(target_so)
}

#[cfg(target_os = "linux")]
async fn download_love_appimage_and_extract(target_dir: &Path) -> Result<(), AppError> {
    // Pin to LOVE 11.5 linux x86_64 tarball (latest as of Balatro/Lovely expectations).
    let appimage_url =
        "https://github.com/love2d/love/releases/download/11.5/love-11.5-x86_64.AppImage";

    log::info!("Downloading LOVE AppImage from: {}", appimage_url);

    let client = reqwest::Client::builder()
        .user_agent("balatro-mod-manager")
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;

    let response = client.get(appimage_url).send().await.map_err(|e| {
        log::error!("Failed to connect to LOVE AppImage download: {}", e);
        AppError::Network(format!("Failed to download LOVE AppImage: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!(
            "LOVE AppImage download failed with HTTP {}",
            response.status()
        );
        return Err(AppError::Network(format!(
            "LOVE AppImage download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read LOVE AppImage response body: {}", e);
        AppError::Network(format!("Failed to read LOVE AppImage bytes: {e}"))
    })?;

    log::debug!("Downloaded LOVE AppImage: {} bytes", bytes.len());

    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;
    let temp_appimage = temp_dir.path().join("love.AppImage");
    fs::write(&temp_appimage, &bytes).map_err(|e| AppError::FileWrite {
        path: temp_appimage.clone(),
        source: e.to_string(),
    })?;
    // Ensure the AppImage is executable before extraction.
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&temp_appimage, perms).map_err(|e| AppError::FileWrite {
        path: temp_appimage.clone(),
        source: e.to_string(),
    })?;

    if target_dir.exists() {
        fs::remove_dir_all(target_dir).map_err(|e| AppError::DirCreate {
            path: target_dir.to_path_buf(),
            source: e.to_string(),
        })?;
    }
    fs::create_dir_all(target_dir).map_err(|e| AppError::DirCreate {
        path: target_dir.to_path_buf(),
        source: e.to_string(),
    })?;

    // Extract the AppImage contents into target_dir/squashfs-root then move its contents up.
    let mut extract = Command::new(&temp_appimage);
    extract
        .arg("--appimage-extract")
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .current_dir(target_dir);
    let status = extract
        .status()
        .map_err(|e| AppError::InvalidState(format!("Failed to extract LOVE AppImage: {e}")))?;
    if !status.success() {
        return Err(AppError::InvalidState(format!(
            "LOVE AppImage extraction failed with status {status}"
        )));
    }

    let squash_root = target_dir.join("squashfs-root");
    if !squash_root.exists() {
        return Err(AppError::InvalidState(
            "LOVE AppImage extraction did not produce squashfs-root".to_string(),
        ));
    }

    for ent in (fs::read_dir(&squash_root).map_err(|e| AppError::DirCreate {
        path: squash_root.clone(),
        source: e.to_string(),
    })?)
    .flatten()
    {
        let src = ent.path();
        let dst = target_dir.join(ent.file_name());
        let _ = fs::rename(&src, &dst);
    }

    let _ = fs::remove_dir_all(&squash_root);

    Ok(())
}

/// Query GitHub for the latest Lovely release tag (e.g., "0.8.0").
/// Results are cached for 24 hours to avoid blocking game launches.
/// Cache persists to disk to survive app restarts.
pub async fn get_latest_lovely_version() -> Result<String, AppError> {
    // Check in-memory cache first
    let cache = VERSION_CACHE.get_or_init(|| Mutex::new(None));
    {
        let guard = cache.lock().await;
        if let Some(cached) = guard.as_ref()
            && cached.fetched_at.elapsed() < Duration::from_secs(VERSION_CACHE_TTL_SECS)
        {
            log::debug!("Using cached Lovely version: {}", cached.version);
            return Ok(cached.version.clone());
        }
    }

    // Check disk cache (survives app restarts)
    if let Some(version) = load_persisted_version_cache() {
        // Populate in-memory cache from disk
        let mut guard = cache.lock().await;
        *guard = Some(CachedVersion {
            version: version.clone(),
            fetched_at: Instant::now(),
        });
        return Ok(version);
    }

    // Cache miss or expired - fetch from GitHub
    log::debug!("Querying GitHub for latest Lovely release version");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;

    let resp = client
        .get("https://github.com/ethangreen-dev/lovely-injector/releases/latest")
        .send()
        .await
        .map_err(|e| {
            log::error!("Failed to query Lovely releases: {}", e);
            AppError::Network(e.to_string())
        })?;

    // GitHub returns a 3xx with a Location header to /tag/vX.Y.Z
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    // Fallback: if redirect policy changed or header empty, try final URL string
    let url_str = if location.is_empty() {
        resp.url().as_str().to_string()
    } else {
        location
    };

    // Extract the tag part after "/tag/" and strip leading 'v'
    let version = url_str
        .split('/')
        .next_back()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();

    if version.is_empty() {
        log::error!("Failed to parse Lovely version from URL: {}", url_str);
        return Err(AppError::InvalidState(
            "Failed to resolve latest Lovely version tag".to_string(),
        ));
    }

    // Update in-memory cache
    {
        let mut guard = cache.lock().await;
        *guard = Some(CachedVersion {
            version: version.clone(),
            fetched_at: Instant::now(),
        });
    }

    // Persist to disk for next app launch
    save_persisted_version_cache(&version);

    log::debug!("Latest Lovely version: {}", version);
    Ok(version)
}

/// Remove currently installed Lovely artifacts so a clean reinstall can occur.
pub async fn remove_installed_lovely() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let lovely_path = bins_dir.join("liblovely.dylib");
        if lovely_path.exists() {
            tokio::fs::remove_file(&lovely_path)
                .await
                .map_err(|e| AppError::FileWrite {
                    path: lovely_path.clone(),
                    source: e.to_string(),
                })?;
        }
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // Remove from game directory
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if !balatro_paths.is_empty() {
            let game_path = &balatro_paths[0];
            let dll_path = game_path.join("version.dll");
            if dll_path.exists() {
                tokio::fs::remove_file(&dll_path)
                    .await
                    .map_err(|e| AppError::FileWrite {
                        path: dll_path.clone(),
                        source: e.to_string(),
                    })?;
            }
            // Also remove liblovely.so on Linux
            #[cfg(target_os = "linux")]
            {
                let so_path = game_path.join("liblovely.so");
                if so_path.exists() {
                    let _ = tokio::fs::remove_file(&so_path).await;
                }
            }
        }

        // Also remove cached copies on Linux
        #[cfg(target_os = "linux")]
        {
            if let Some(config_dir) = dirs::config_dir() {
                let bins_dir = config_dir.join("Balatro/bins");
                let cached_dll = bins_dir.join("version.dll");
                let cached_so = bins_dir.join("liblovely.so");
                if cached_dll.exists() {
                    let _ = tokio::fs::remove_file(&cached_dll).await;
                    log::info!("Removed cached version.dll");
                }
                if cached_so.exists() {
                    let _ = tokio::fs::remove_file(&cached_so).await;
                    log::info!("Removed cached liblovely.so");
                }
            }
        }

        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Set whether the Lovely injector is enabled or disabled by renaming files.
/// When disabled, the injector file is renamed to .disabled so it won't load.
/// Returns Ok(()) if successful, or an error if the operation fails.
pub fn set_injector_enabled(enabled: bool) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let active_path = bins_dir.join("liblovely.dylib");
        let disabled_path = bins_dir.join("liblovely.dylib.disabled");

        toggle_injector_file(&active_path, &disabled_path, enabled)
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // For Windows and Linux (Proton), we need to toggle version.dll in the game directory
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            // No game path found - if we're enabling, that's an error; if disabling, nothing to do
            if enabled {
                return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
            }
            return Ok(());
        }
        let game_path = &balatro_paths[0];
        let active_path = game_path.join("version.dll");
        let disabled_path = game_path.join("version.dll.disabled");

        toggle_injector_file(&active_path, &disabled_path, enabled)?;

        // On Linux, also handle liblovely.so for native LOVE builds
        #[cfg(target_os = "linux")]
        {
            if let Some(config_dir) = dirs::config_dir() {
                let bins_dir = config_dir.join("Balatro/bins");
                let so_active = bins_dir.join("liblovely.so");
                let so_disabled = bins_dir.join("liblovely.so.disabled");
                // Also check in game directory
                let game_so_active = game_path.join("liblovely.so");
                let game_so_disabled = game_path.join("liblovely.so.disabled");

                // Toggle both locations (ignore errors if files don't exist)
                let _ = toggle_injector_file(&so_active, &so_disabled, enabled);
                let _ = toggle_injector_file(&game_so_active, &game_so_disabled, enabled);
            }
        }

        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Check if the injector is currently enabled (active file exists).
pub fn is_injector_enabled() -> Result<bool, AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let active_path = bins_dir.join("liblovely.dylib");
        Ok(active_path.exists())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            // No game path - consider it "enabled" (no injector to disable)
            return Ok(true);
        }
        let game_path = &balatro_paths[0];
        let active_path = game_path.join("version.dll");
        Ok(active_path.exists())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Helper function to toggle an injector file between active and disabled states.
fn toggle_injector_file(
    active_path: &Path,
    disabled_path: &Path,
    enable: bool,
) -> Result<(), AppError> {
    if enable {
        // Enable: rename .disabled to active
        if disabled_path.exists() {
            // If both exist (corrupt state), remove the disabled one
            if active_path.exists() {
                std::fs::remove_file(disabled_path).map_err(|e| AppError::FileWrite {
                    path: disabled_path.to_path_buf(),
                    source: e.to_string(),
                })?;
            } else {
                std::fs::rename(disabled_path, active_path).map_err(|e| AppError::FileWrite {
                    path: active_path.to_path_buf(),
                    source: format!("Failed to enable injector: {}", e),
                })?;
            }
        }
        // If neither exists, Lovely isn't installed - that's fine, we just save the preference
        Ok(())
    } else {
        // Disable: rename active to .disabled
        if active_path.exists() {
            // If both exist (corrupt state), remove the active one since we're disabling
            if disabled_path.exists() {
                std::fs::remove_file(active_path).map_err(|e| AppError::FileWrite {
                    path: active_path.to_path_buf(),
                    source: e.to_string(),
                })?;
            } else {
                std::fs::rename(active_path, disabled_path).map_err(|e| AppError::FileWrite {
                    path: disabled_path.to_path_buf(),
                    source: format!("Failed to disable injector: {}", e),
                })?;
            }
        }
        // If active doesn't exist, already disabled - that's fine
        Ok(())
    }
}

#[cfg(target_os = "macos")]
async fn download_and_install_lovely(target_path: &Path) -> Result<(), AppError> {
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;

    let arch = detect_architecture()?;
    let url = format!(
        "https://github.com/ethangreen-dev/lovely-injector/releases/latest/download/\
    lovely-{arch}-apple-darwin.tar.gz"
    );

    log::info!("Downloading Lovely injector for macOS from: {}", url);

    // Download latest release
    let client = reqwest::Client::new();
    let response = client.get(&url).send().await.map_err(|e| {
        log::error!("Failed to download Lovely injector: {}", e);
        AppError::Network(e.to_string())
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    // Save to temp file
    let temp_tar_gz = temp_dir.path().join("lovely.tar.gz");
    let mut file = File::create(&temp_tar_gz).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(e.to_string())
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());
    std::io::copy(&mut bytes.as_ref(), &mut file).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    // Extract and install
    let tar_gz = File::open(&temp_tar_gz).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    archive.unpack(&temp_dir).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    // Find the library in extracted files
    let extracted_lib = temp_dir.path().join("liblovely.dylib");
    fs::copy(&extracted_lib, target_path).map_err(|e| AppError::FileCopy {
        source: extracted_lib.display().to_string(),
        dest: target_path.display().to_string(),
        source_error: e.to_string(),
    })?;

    // Set permissions
    std::fs::set_permissions(target_path, std::fs::Permissions::from_mode(0o755))?;

    Ok(())
}

#[cfg(target_os = "linux")]
async fn download_lovely_linux(target_path: &Path) -> Result<(), AppError> {
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;
    let temp_tar_gz = temp_dir.path().join("lovely.tar.gz");

    let url = "https://github.com/ethangreen-dev/lovely-injector/releases/latest/download/lovely-x86_64-unknown-linux-gnu.tar.gz";
    log::info!("Downloading Lovely injector for Linux from: {}", url);

    let response = reqwest::get(url).await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Linux: {}", e);
        AppError::Network(format!("Failed to download Lovely injector: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(format!("Failed to read download response: {e}"))
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());

    fs::write(&temp_tar_gz, &bytes).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let tar_gz = File::open(&temp_tar_gz).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    archive.unpack(&temp_dir).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let extracted_lib = temp_dir.path().join("liblovely.so");
    fs::copy(&extracted_lib, target_path).map_err(|e| AppError::FileCopy {
        source: extracted_lib.display().to_string(),
        dest: target_path.display().to_string(),
        source_error: e.to_string(),
    })?;

    // Ensure it is executable for preload
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(target_path, perms).map_err(|e| AppError::FileWrite {
        path: target_path.to_path_buf(),
        source: e.to_string(),
    })?;

    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
async fn download_version_dll(target_path: &PathBuf) -> Result<(), AppError> {
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;

    // URL to the latest version.dll in the lovely injector repository
    let url = "https://github.com/ethangreen-dev/lovely-injector/releases/latest/download/lovely-x86_64-pc-windows-msvc.zip";

    log::info!("Downloading Lovely injector for Windows from: {}", url);

    // Download the ZIP file
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Windows: {}", e);
        AppError::Network(format!("Failed to download Lovely injector: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    // Save to temp zip file
    let temp_zip = temp_dir.path().join("lovely.zip");
    let mut file = File::create(&temp_zip).map_err(|e| AppError::FileWrite {
        path: temp_zip.clone(),
        source: e.to_string(),
    })?;

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(format!("Failed to read download response: {e}"))
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());

    std::io::copy(&mut bytes.as_ref(), &mut file).map_err(|e| AppError::FileWrite {
        path: temp_zip.clone(),
        source: e.to_string(),
    })?;

    // Extract the ZIP file
    let zip_file = File::open(&temp_zip).map_err(|e| AppError::FileRead {
        path: temp_zip.clone(),
        source: e.to_string(),
    })?;

    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| AppError::FileRead {
        path: temp_zip.clone(),
        source: e.to_string(),
    })?;

    // Find and extract version.dll from the ZIP
    let mut found_dll = false;
    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(file) => file,
            Err(e) => {
                log::warn!("Failed to access zip entry: {}", e);
                continue;
            }
        };

        let entry_name = file.name().to_string();

        if entry_name.ends_with("version.dll") {
            log::info!("Found version.dll in zip archive");
            let mut outfile = File::create(target_path).map_err(|e| AppError::FileWrite {
                path: target_path.to_path_buf(),
                source: e.to_string(),
            })?;

            std::io::copy(&mut file, &mut outfile).map_err(|e| AppError::FileWrite {
                path: target_path.to_path_buf(),
                source: e.to_string(),
            })?;

            found_dll = true;
            break;
        }
    }

    if !found_dll {
        return Err(AppError::InvalidState(
            "version.dll not found in downloaded zip".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_toggle_injector_enable_from_disabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create the disabled file
        std::fs::write(&disabled, b"test").unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());

        // Enable should rename disabled to active
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());
    }

    #[test]
    fn test_toggle_injector_disable_from_enabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create the active file
        std::fs::write(&active, b"test").unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());

        // Disable should rename active to disabled
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());
    }

    #[test]
    fn test_toggle_injector_already_enabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create both files (corrupt state)
        std::fs::write(&active, b"active").unwrap();
        std::fs::write(&disabled, b"disabled").unwrap();

        // Enable should remove the disabled file
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());
        assert_eq!(std::fs::read(&active).unwrap(), b"active");
    }

    #[test]
    fn test_toggle_injector_already_disabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create both files (corrupt state)
        std::fs::write(&active, b"active").unwrap();
        std::fs::write(&disabled, b"disabled").unwrap();

        // Disable should remove the active file
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());
        assert_eq!(std::fs::read(&disabled).unwrap(), b"disabled");
    }

    #[test]
    fn test_toggle_injector_no_files_exist() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Neither file exists
        assert!(!active.exists());
        assert!(!disabled.exists());

        // Enable should succeed without error
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(!active.exists());
        assert!(!disabled.exists());

        // Disable should also succeed without error
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(!disabled.exists());
    }
}
