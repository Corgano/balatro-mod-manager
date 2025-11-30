use crate::errors::AppError;
#[cfg(target_os = "linux")]
use serde_json::Value;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::{self};
#[cfg(target_os = "macos")]
use std::fs::{self, File};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

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
pub async fn ensure_love_appimage() -> Result<PathBuf, AppError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let bins_dir = config_dir.join("Balatro/bins");
    fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
        path: bins_dir.clone(),
        source: e.to_string(),
    })?;
    let target_path = bins_dir.join("love.AppImage");
    if target_path.exists() {
        // Refresh permissions in case they were lost.
        let perms = std::fs::Permissions::from_mode(0o755);
        let _ = std::fs::set_permissions(&target_path, perms);
        return Ok(target_path);
    }

    download_love_appimage(&target_path).await?;
    Ok(target_path)
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

/// Query GitHub for the latest Lovely release tag (e.g., "0.8.0").
pub async fn get_latest_lovely_version() -> Result<String, AppError> {
    // We intentionally avoid downloading the artifact; just resolve the tag.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;

    let resp = client
        .get("https://github.com/ethangreen-dev/lovely-injector/releases/latest")
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

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
        return Err(AppError::InvalidState(
            "Failed to resolve latest Lovely version tag".to_string(),
        ));
    }

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

    // Download latest release
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;

    // Save to temp file
    let temp_tar_gz = temp_dir.path().join("lovely.tar.gz");
    let mut file = File::create(&temp_tar_gz).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
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

    let url =
        "https://github.com/ethangreen-dev/lovely-injector/releases/latest/download/lovely-x86_64-unknown-linux-gnu.tar.gz";
    log::info!("Downloading lovely injector for Linux from {}", url);

    let resp = reqwest::get(url)
        .await
        .map_err(|e| AppError::Network(format!("Failed to download lovely injector: {e}")))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read download response: {e}")))?;

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
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to download lovely injector: {e}")))?;

    // Save to temp zip file
    let temp_zip = temp_dir.path().join("lovely.zip");
    let mut file = File::create(&temp_zip).map_err(|e| AppError::FileWrite {
        path: temp_zip.clone(),
        source: e.to_string(),
    })?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read download response: {e}")))?;

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

#[cfg(target_os = "linux")]
async fn download_love_appimage(target_path: &Path) -> Result<(), AppError> {
    let client = reqwest::Client::builder()
        .user_agent("balatro-mod-manager")
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;

    let resp = client
        .get("https://api.github.com/repos/love2d/love/releases/latest")
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to query LOVE releases: {e}")))?;
    let release: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Network(format!("Failed to parse LOVE release data: {e}")))?;

    let assets = release
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::InvalidState("LOVE release data missing assets array".to_string())
        })?;

    let appimage_url = assets
        .iter()
        .filter_map(|asset| {
            let name = asset.get("name")?.as_str()?;
            let url = asset.get("browser_download_url")?.as_str()?;
            if name.contains("AppImage") && name.contains("x86_64") {
                Some(url.to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            AppError::InvalidState("Could not find LOVE AppImage in latest release".to_string())
        })?;

    let bytes = client
        .get(appimage_url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to download LOVE AppImage: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("Failed to read LOVE AppImage bytes: {e}")))?;

    fs::write(target_path, &bytes).map_err(|e| AppError::FileWrite {
        path: target_path.to_path_buf(),
        source: e.to_string(),
    })?;

    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(target_path, perms).map_err(|e| AppError::FileWrite {
        path: target_path.to_path_buf(),
        source: e.to_string(),
    })?;

    Ok(())
}
