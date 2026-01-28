use bmm_lib::finder::{is_balatro_running, is_steam_running};

#[tauri::command]
pub async fn check_steam_running() -> bool {
    is_steam_running()
}

#[tauri::command]
pub async fn check_balatro_running() -> bool {
    is_balatro_running()
}

#[tauri::command]
pub async fn get_app_version() -> String {
    // Compile-time crate version from Cargo.toml
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cargo_pkg_version_is_valid_semver() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(!version.is_empty());
        // Should have at least major.minor format
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() >= 2, "Version should have at least major.minor");
        // All parts should be numeric (or contain valid prerelease suffix)
        for (i, part) in parts.iter().enumerate() {
            if i < 2 {
                // Major and minor must be numeric
                assert!(
                    part.chars().all(|c| c.is_ascii_digit()),
                    "Major/minor version parts must be numeric"
                );
            }
        }
    }

    #[test]
    fn test_cargo_pkg_version_not_placeholder() {
        let version = env!("CARGO_PKG_VERSION");
        assert_ne!(version, "0.0.0", "Version should not be placeholder");
        assert!(
            !version.contains("SNAPSHOT"),
            "Version should not contain SNAPSHOT"
        );
    }
}
