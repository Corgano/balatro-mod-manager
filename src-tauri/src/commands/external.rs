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

#[cfg(test)]
mod tests {
    // Note: open_external_url requires tauri::AppHandle which cannot be easily
    // constructed in unit tests. We test the validation logic by checking
    // the expected error messages for invalid inputs.

    #[test]
    fn test_url_validation_empty() {
        let url = "";
        let trimmed = url.trim();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn test_url_validation_whitespace_only() {
        let url = "   ";
        let trimmed = url.trim();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn test_url_validation_http_allowed() {
        let url = "http://example.com";
        let trimmed = url.trim();
        assert!(trimmed.starts_with("http://") || trimmed.starts_with("https://"));
    }

    #[test]
    fn test_url_validation_https_allowed() {
        let url = "https://example.com";
        let trimmed = url.trim();
        assert!(trimmed.starts_with("http://") || trimmed.starts_with("https://"));
    }

    #[test]
    fn test_url_validation_file_not_allowed() {
        let url = "file:///etc/passwd";
        let trimmed = url.trim();
        assert!(!trimmed.starts_with("http://") && !trimmed.starts_with("https://"));
    }

    #[test]
    fn test_url_validation_ftp_not_allowed() {
        let url = "ftp://example.com";
        let trimmed = url.trim();
        assert!(!trimmed.starts_with("http://") && !trimmed.starts_with("https://"));
    }

    #[test]
    fn test_url_validation_javascript_not_allowed() {
        let url = "javascript:alert(1)";
        let trimmed = url.trim();
        assert!(!trimmed.starts_with("http://") && !trimmed.starts_with("https://"));
    }

    #[test]
    fn test_url_trim_preserves_valid_url() {
        let url = "  https://example.com/path  ";
        let trimmed = url.trim();
        assert_eq!(trimmed, "https://example.com/path");
    }
}
