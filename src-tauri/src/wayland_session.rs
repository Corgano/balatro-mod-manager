//! Wayland session management with automatic crash detection and recovery.
//!
//! This module enables native Wayland by default, with automatic fallback to XWayland
//! if a previous session crashed. Users can override behavior with environment variables:
//! - `BMM_FORCE_X11=1`: Always use XWayland
//! - `BMM_ALLOW_WAYLAND=1`: Always use native Wayland (even after crash)

use std::path::PathBuf;

/// Returns the path to the Wayland session lock file.
fn lock_path() -> Option<PathBuf> {
    dirs::data_dir().map(|p| p.join("balatro-mod-manager").join("wayland_session.lock"))
}

/// Checks if the previous Wayland session crashed (lock file contains "starting").
pub fn previous_session_crashed() -> bool {
    lock_path()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .map(|content| content.trim() == "starting")
        .unwrap_or(false)
}

/// Marks the current Wayland session as starting (called before window creation).
pub fn mark_starting() {
    if let Some(path) = lock_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, "starting");
    }
}

/// Marks the current Wayland session as clean (called on shutdown).
pub fn mark_clean() {
    if let Some(path) = lock_path() {
        let _ = std::fs::remove_file(&path);
    }
}

/// Checks if an environment variable is set to a truthy value.
fn env_is_truthy(var: &str) -> bool {
    std::env::var(var)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Configures the display backend for Linux, with automatic Wayland crash recovery.
///
/// Returns a message describing the backend choice for logging.
pub fn configure_display_backend() -> Option<String> {
    use std::env;

    let set_env_if_absent = |key: &str, value: &str| {
        if env::var_os(key).is_none() {
            // Safety: called during startup before any threads are spawned, so mutating the
            // process environment is safe.
            unsafe { env::set_var(key, value) };
        }
    };

    let force_x11 = |set_env: &dyn Fn(&str, &str)| {
        set_env("WINIT_UNIX_BACKEND", "x11");
        set_env("GDK_BACKEND", "x11");
        set_env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    };

    // Prevent blank window issues on some Linux setups.
    set_env_if_absent("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    let on_wayland = env::var_os("WAYLAND_DISPLAY").is_some()
        || matches!(
            env::var("XDG_SESSION_TYPE"),
            Ok(v) if v.eq_ignore_ascii_case("wayland")
        );
    if !on_wayland {
        return None;
    }

    // Manual override: force X11
    if env_is_truthy("BMM_FORCE_X11") {
        if env::var_os("DISPLAY").is_some() {
            force_x11(&set_env_if_absent);
        }
        return Some("BMM_FORCE_X11 set; using XWayland.".into());
    }

    // Manual override: allow Wayland (even after crash)
    if env_is_truthy("BMM_ALLOW_WAYLAND") {
        mark_starting();
        return Some("BMM_ALLOW_WAYLAND set; using native Wayland.".into());
    }

    // Check for previous crash - fall back to XWayland if detected
    if previous_session_crashed() {
        if env::var_os("DISPLAY").is_some() {
            force_x11(&set_env_if_absent);
            return Some(
                "Previous Wayland session crashed; falling back to XWayland. \
                 Set BMM_ALLOW_WAYLAND=1 to force native Wayland."
                    .into(),
            );
        }
        // No X11 available, have to try Wayland anyway
        mark_starting();
        return Some(
            "Previous Wayland session crashed but X11 unavailable; retrying native Wayland.".into(),
        );
    }

    // Default: try native Wayland
    mark_starting();
    Some("Wayland session detected; using native Wayland.".into())
}
