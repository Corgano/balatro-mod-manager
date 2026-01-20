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

/// Ensures the Lovely version.dll exists in the game directory (Windows/Linux).
///
/// Downloads the DLL from GitHub releases if not present.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub async fn ensure_version_dll_exists(game_path: &Path) -> Result<PathBuf, AppError> {
    let dll_path = game_path.join("version.dll");

    // If the DLL doesn't exist, download it
    if !dll_path.exists() {
        download_version_dll(&dll_path).await?;
    }

    Ok(dll_path)
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
        let balatro_paths = crate::finder::get_balatro_paths();
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
        let balatro_paths = crate::finder::get_balatro_paths();
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
pub async fn get_latest_lovely_version() -> Result<String, AppError> {
    // We intentionally avoid downloading the artifact; just resolve the tag.
    log::debug!("Querying GitHub for latest Lovely release version");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
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

    log::debug!("Latest Lovely version: {}", version);
    Ok(version)
}

/// Remove currently installed Lovely artifacts so a clean reinstall can occur.
pub fn remove_installed_lovely() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let lovely_path = bins_dir.join("liblovely.dylib");
        if lovely_path.exists() {
            std::fs::remove_file(&lovely_path).map_err(|e| AppError::FileWrite {
                path: lovely_path.clone(),
                source: e.to_string(),
            })?;
        }
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let balatro_paths = crate::finder::get_balatro_paths();
        if balatro_paths.is_empty() {
            return Ok(()); // Nothing to remove if we can't detect it
        }
        let game_path = &balatro_paths[0];
        let dll_path = game_path.join("version.dll");
        if dll_path.exists() {
            std::fs::remove_file(&dll_path).map_err(|e| AppError::FileWrite {
                path: dll_path.clone(),
                source: e.to_string(),
            })?;
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
        let balatro_paths = crate::finder::get_balatro_paths();
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
        let balatro_paths = crate::finder::get_balatro_paths();
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
    log::info!("Downloading lovely injector for Linux from {}", url);

    let response = reqwest::get(url).await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Linux: {}", e);
        AppError::Network(format!("Failed to download lovely injector: {e}"))
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

    log::info!("Downloading lovely injector for Windows from {}", url);

    // Download the ZIP file
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Windows: {}", e);
        AppError::Network(format!("Failed to download lovely injector: {e}"))
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
