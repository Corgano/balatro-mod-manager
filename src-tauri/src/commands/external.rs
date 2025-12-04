use log::{info, warn};
use std::process::Command;
use tauri_plugin_opener::OpenerExt;

/// Open an external URL using the system handler.
#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("URL is empty".to_string());
    }

    // Basic scheme guard: only allow http/https to avoid unexpected handlers.
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http/https URLs are allowed".to_string());
    }

    info!("Opening external URL: {trimmed}");

    // Flatpak: call host xdg-open via flatpak-spawn for the most reliable path.
    if std::env::var("FLATPAK_ID").is_ok() {
        match Command::new("flatpak-spawn")
            .arg("--host")
            .arg("xdg-open")
            .arg(trimmed)
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => warn!("flatpak-spawn --host xdg-open failed for {trimmed}: {e}"),
        }
    }

    // Try opener plugin.
    if let Err(e) = app.opener().open_url(trimmed.to_string(), None::<String>) {
        warn!("opener plugin failed to open url {trimmed}: {e}; falling back to system open");
        open::that_detached(trimmed)
            .map_err(|err| format!("Failed to open URL {trimmed}: {err}"))?;
    } else {
        return Ok(());
    }

    Ok(())
}
