//! Linux display backend management with automatic crash detection and recovery.
//!
//! This module enables native Wayland and GPU acceleration by default, with automatic
//! fallback if a previous session crashed. Users can override behavior with environment variables:
//!
//! Wayland:
//! - `BMM_FORCE_X11=1`: Always use XWayland
//! - `BMM_ALLOW_WAYLAND=1`: Always use native Wayland (even after crash)
//!
//! GPU:
//! - `BMM_DISABLE_GPU=1`: Always disable GPU acceleration
//! - `BMM_ALLOW_GPU=1`: Always enable full GPU acceleration (even after crash)
//!
//! GPU fallback chain: DMABUF → Vulkan → NGL (OpenGL) → GL (Legacy OpenGL) → Software

use std::path::PathBuf;

/// GPU acceleration state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuState {
    /// Full GPU acceleration with DMABUF
    Dmabuf,
    /// Vulkan renderer
    Vulkan,
    /// New OpenGL renderer (GTK4)
    Ngl,
    /// Legacy OpenGL renderer
    Gl,
    /// Software rendering (all GPU paths failed)
    Disabled,
}

impl GpuState {
    /// Returns the lock file string for this state.
    fn as_str(self) -> &'static str {
        match self {
            GpuState::Dmabuf => "dmabuf",
            GpuState::Vulkan => "vulkan",
            GpuState::Ngl => "ngl",
            GpuState::Gl => "gl",
            GpuState::Disabled => "disabled",
        }
    }

    /// Returns the stable (ok) lock file string for this state.
    fn as_ok_str(self) -> &'static str {
        match self {
            GpuState::Dmabuf => "dmabuf-ok",
            GpuState::Vulkan => "vulkan-ok",
            GpuState::Ngl => "ngl-ok",
            GpuState::Gl => "gl-ok",
            GpuState::Disabled => "disabled-ok",
        }
    }

    /// Parses a lock file string into a state.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "dmabuf" | "dmabuf-ok" => Some(GpuState::Dmabuf),
            "vulkan" | "vulkan-ok" => Some(GpuState::Vulkan),
            "ngl" | "ngl-ok" => Some(GpuState::Ngl),
            "gl" | "gl-ok" => Some(GpuState::Gl),
            "disabled" | "disabled-ok" => Some(GpuState::Disabled),
            _ => None,
        }
    }

    /// Returns the next fallback state after a crash.
    fn next_fallback(self) -> Self {
        match self {
            GpuState::Dmabuf => GpuState::Vulkan,
            GpuState::Vulkan => GpuState::Ngl,
            GpuState::Ngl => GpuState::Gl,
            GpuState::Gl => GpuState::Disabled,
            GpuState::Disabled => GpuState::Disabled,
        }
    }

    /// Returns a human-readable description for logging.
    fn description(self) -> &'static str {
        match self {
            GpuState::Dmabuf => "DMABUF",
            GpuState::Vulkan => "Vulkan renderer",
            GpuState::Ngl => "OpenGL renderer (NGL)",
            GpuState::Gl => "Legacy OpenGL renderer",
            GpuState::Disabled => "software rendering",
        }
    }

    /// Applies the environment variables for this GPU state.
    fn apply_env(self) {
        use std::env;
        match self {
            GpuState::Dmabuf => {
                // Default, no env changes needed
            }
            GpuState::Vulkan => {
                unsafe {
                    env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                    env::set_var("GTK_RENDERER", "vulkan");
                };
            }
            GpuState::Ngl => {
                unsafe {
                    env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                    env::set_var("GTK_RENDERER", "ngl");
                };
            }
            GpuState::Gl => {
                unsafe {
                    env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                    env::set_var("GTK_RENDERER", "gl");
                };
            }
            GpuState::Disabled => {
                unsafe {
                    env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                };
            }
        }
    }
}

/// Returns the base directory for lock files.
fn lock_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|p| p.join("balatro-mod-manager"))
}

/// Returns the path to the Wayland session lock file.
fn wayland_lock_path() -> Option<PathBuf> {
    lock_dir().map(|p| p.join("wayland_session.lock"))
}

/// Returns the path to the GPU session lock file.
fn gpu_lock_path() -> Option<PathBuf> {
    lock_dir().map(|p| p.join("gpu_session.lock"))
}

/// Reads the content of a lock file.
fn read_lock_file(path: &PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Writes content to a lock file.
fn write_lock_file(path: &PathBuf, content: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, content);
}

/// Removes a lock file.
fn remove_lock_file(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

/// Gets the current GPU state from the lock file.
fn get_gpu_state() -> Option<GpuState> {
    let path = gpu_lock_path()?;
    let content = read_lock_file(&path)?;
    GpuState::from_str(&content)
}

/// Checks if the current GPU state is stable (ended with clean shutdown).
fn is_gpu_state_stable() -> bool {
    gpu_lock_path()
        .and_then(|p| read_lock_file(&p))
        .map(|s| s.ends_with("-ok"))
        .unwrap_or(false)
}

/// Sets the GPU state in the lock file.
fn set_gpu_state(state: GpuState) {
    if let Some(path) = gpu_lock_path() {
        write_lock_file(&path, state.as_str());
    }
}

/// Marks the GPU state as stable (working).
fn mark_gpu_state_stable(state: GpuState) {
    if let Some(path) = gpu_lock_path() {
        write_lock_file(&path, state.as_ok_str());
    }
}

/// Clears the GPU lock file (successful session with DMABUF).
fn clear_gpu_state() {
    if let Some(path) = gpu_lock_path() {
        remove_lock_file(&path);
    }
}

/// Checks if the previous Wayland session crashed.
pub fn previous_wayland_session_crashed() -> bool {
    wayland_lock_path()
        .and_then(|p| read_lock_file(&p))
        .map(|s| s == "starting")
        .unwrap_or(false)
}

/// Checks if the previous GPU session crashed (was trying something that didn't complete cleanly).
pub fn previous_gpu_session_crashed() -> bool {
    // Any state other than None means we were trying something
    get_gpu_state().is_some()
}

/// Marks the current Wayland session as starting (called before window creation).
pub fn mark_wayland_starting() {
    if let Some(path) = wayland_lock_path() {
        write_lock_file(&path, "starting");
    }
}

/// Marks all sessions as clean (called on shutdown).
pub fn mark_clean() {
    if let Some(path) = wayland_lock_path() {
        remove_lock_file(&path);
    }
    // On clean shutdown, mark the current GPU mode as stable.
    // For DMABUF, clear the lock entirely (it's the default).
    // For other modes, write the "-ok" suffix to remember it works.
    if let Some(state) = get_gpu_state() {
        match state {
            GpuState::Dmabuf => {
                // DMABUF worked! Clear the lock so next launch uses DMABUF again.
                clear_gpu_state();
            }
            _ => {
                // Other mode worked. Mark it as stable.
                mark_gpu_state_stable(state);
            }
        }
    }
}

/// Checks if an environment variable is set to a truthy value.
fn env_is_truthy(var: &str) -> bool {
    std::env::var(var)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Configures GPU acceleration for Linux, with automatic crash recovery.
///
/// Fallback chain: DMABUF → Vulkan → NGL → GL → Software
///
/// Returns a message describing the GPU choice for logging.
pub fn configure_gpu() -> Option<String> {
    use std::env;

    // Manual override: disable GPU entirely
    if env_is_truthy("BMM_DISABLE_GPU") {
        unsafe { env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
        return Some("BMM_DISABLE_GPU set; GPU acceleration disabled.".into());
    }

    // Manual override: force full GPU (DMABUF)
    if env_is_truthy("BMM_ALLOW_GPU") {
        set_gpu_state(GpuState::Dmabuf);
        return Some("BMM_ALLOW_GPU set; full GPU acceleration enabled.".into());
    }

    // Check previous state and determine next action
    let state = get_gpu_state();
    let stable = is_gpu_state_stable();

    match (state, stable) {
        // No state = first launch, try DMABUF
        (None, _) => {
            set_gpu_state(GpuState::Dmabuf);
            Some("GPU acceleration enabled (DMABUF).".into())
        }
        // Stable state - keep using it
        (Some(current), true) => {
            set_gpu_state(current);
            current.apply_env();
            Some(format!(
                "Using {} (previously verified working).",
                current.description()
            ))
        }
        // Unstable state - crashed, try next fallback
        (Some(current), false) => {
            let next = current.next_fallback();
            set_gpu_state(next);
            next.apply_env();

            if next == GpuState::Disabled {
                Some(format!(
                    "{} crashed; disabling GPU acceleration. Set BMM_ALLOW_GPU=1 to retry.",
                    current.description()
                ))
            } else {
                Some(format!(
                    "{} crashed; trying {}. Set BMM_ALLOW_GPU=1 to force DMABUF.",
                    current.description(),
                    next.description()
                ))
            }
        }
    }
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
    };

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
        mark_wayland_starting();
        return Some("BMM_ALLOW_WAYLAND set; using native Wayland.".into());
    }

    // Check for previous crash - fall back to XWayland if detected
    // But only if GPU didn't also crash (GPU crash is likely the real culprit)
    if previous_wayland_session_crashed() && !previous_gpu_session_crashed() {
        if env::var_os("DISPLAY").is_some() {
            force_x11(&set_env_if_absent);
            return Some(
                "Previous Wayland session crashed; falling back to XWayland. \
                 Set BMM_ALLOW_WAYLAND=1 to force native Wayland."
                    .into(),
            );
        }
        // No X11 available, have to try Wayland anyway
        mark_wayland_starting();
        return Some(
            "Previous Wayland session crashed but X11 unavailable; retrying native Wayland.".into(),
        );
    }

    // Default: try native Wayland
    mark_wayland_starting();
    Some("Wayland session detected; using native Wayland.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== GpuState enum tests ====================

    #[test]
    fn test_gpu_state_as_str() {
        assert_eq!(GpuState::Dmabuf.as_str(), "dmabuf");
        assert_eq!(GpuState::Vulkan.as_str(), "vulkan");
        assert_eq!(GpuState::Ngl.as_str(), "ngl");
        assert_eq!(GpuState::Gl.as_str(), "gl");
        assert_eq!(GpuState::Disabled.as_str(), "disabled");
    }

    #[test]
    fn test_gpu_state_as_ok_str() {
        assert_eq!(GpuState::Dmabuf.as_ok_str(), "dmabuf-ok");
        assert_eq!(GpuState::Vulkan.as_ok_str(), "vulkan-ok");
        assert_eq!(GpuState::Ngl.as_ok_str(), "ngl-ok");
        assert_eq!(GpuState::Gl.as_ok_str(), "gl-ok");
        assert_eq!(GpuState::Disabled.as_ok_str(), "disabled-ok");
    }

    #[test]
    fn test_gpu_state_from_str_basic() {
        assert_eq!(GpuState::from_str("dmabuf"), Some(GpuState::Dmabuf));
        assert_eq!(GpuState::from_str("vulkan"), Some(GpuState::Vulkan));
        assert_eq!(GpuState::from_str("ngl"), Some(GpuState::Ngl));
        assert_eq!(GpuState::from_str("gl"), Some(GpuState::Gl));
        assert_eq!(GpuState::from_str("disabled"), Some(GpuState::Disabled));
    }

    #[test]
    fn test_gpu_state_from_str_ok_variants() {
        assert_eq!(GpuState::from_str("dmabuf-ok"), Some(GpuState::Dmabuf));
        assert_eq!(GpuState::from_str("vulkan-ok"), Some(GpuState::Vulkan));
        assert_eq!(GpuState::from_str("ngl-ok"), Some(GpuState::Ngl));
        assert_eq!(GpuState::from_str("gl-ok"), Some(GpuState::Gl));
        assert_eq!(GpuState::from_str("disabled-ok"), Some(GpuState::Disabled));
    }

    #[test]
    fn test_gpu_state_from_str_invalid() {
        assert_eq!(GpuState::from_str(""), None);
        assert_eq!(GpuState::from_str("invalid"), None);
        assert_eq!(GpuState::from_str("DMABUF"), None); // case-sensitive
        assert_eq!(GpuState::from_str("dmabuf-"), None);
        assert_eq!(GpuState::from_str("ok"), None);
    }

    #[test]
    fn test_gpu_state_next_fallback_chain() {
        // Test the complete fallback chain
        let mut state = GpuState::Dmabuf;
        assert_eq!(state.next_fallback(), GpuState::Vulkan);

        state = GpuState::Vulkan;
        assert_eq!(state.next_fallback(), GpuState::Ngl);

        state = GpuState::Ngl;
        assert_eq!(state.next_fallback(), GpuState::Gl);

        state = GpuState::Gl;
        assert_eq!(state.next_fallback(), GpuState::Disabled);

        // Disabled stays at Disabled (no further fallback)
        state = GpuState::Disabled;
        assert_eq!(state.next_fallback(), GpuState::Disabled);
    }

    #[test]
    fn test_gpu_state_description() {
        assert_eq!(GpuState::Dmabuf.description(), "DMABUF");
        assert_eq!(GpuState::Vulkan.description(), "Vulkan renderer");
        assert_eq!(GpuState::Ngl.description(), "OpenGL renderer (NGL)");
        assert_eq!(GpuState::Gl.description(), "Legacy OpenGL renderer");
        assert_eq!(GpuState::Disabled.description(), "software rendering");
    }

    #[test]
    fn test_gpu_state_roundtrip() {
        // Ensure as_str/from_str roundtrip works for all states
        for state in [
            GpuState::Dmabuf,
            GpuState::Vulkan,
            GpuState::Ngl,
            GpuState::Gl,
            GpuState::Disabled,
        ] {
            let str_repr = state.as_str();
            let parsed = GpuState::from_str(str_repr);
            assert_eq!(parsed, Some(state), "roundtrip failed for {:?}", state);
        }
    }

    #[test]
    fn test_gpu_state_ok_roundtrip() {
        // Ensure as_ok_str/from_str roundtrip works for all states
        for state in [
            GpuState::Dmabuf,
            GpuState::Vulkan,
            GpuState::Ngl,
            GpuState::Gl,
            GpuState::Disabled,
        ] {
            let ok_str_repr = state.as_ok_str();
            let parsed = GpuState::from_str(ok_str_repr);
            assert_eq!(parsed, Some(state), "ok roundtrip failed for {:?}", state);
        }
    }

    // ==================== Lock file utility tests ====================

    #[test]
    fn test_read_lock_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("test.lock");
        fs::write(&lock_path, "test-content\n").unwrap();

        let result = read_lock_file(&lock_path);
        assert_eq!(result, Some("test-content".to_string()));
    }

    #[test]
    fn test_read_lock_file_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("nonexistent.lock");

        let result = read_lock_file(&lock_path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_lock_file_trims_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("test.lock");
        fs::write(&lock_path, "  content-with-spaces  \n\n").unwrap();

        let result = read_lock_file(&lock_path);
        assert_eq!(result, Some("content-with-spaces".to_string()));
    }

    #[test]
    fn test_write_lock_file_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a/b/c/test.lock");

        write_lock_file(&nested_path, "nested-content");

        assert!(nested_path.exists());
        let content = fs::read_to_string(&nested_path).unwrap();
        assert_eq!(content, "nested-content");
    }

    #[test]
    fn test_write_lock_file_overwrites() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("test.lock");

        write_lock_file(&lock_path, "first");
        write_lock_file(&lock_path, "second");

        let content = fs::read_to_string(&lock_path).unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn test_remove_lock_file() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("test.lock");
        fs::write(&lock_path, "content").unwrap();
        assert!(lock_path.exists());

        remove_lock_file(&lock_path);

        assert!(!lock_path.exists());
    }

    #[test]
    fn test_remove_lock_file_nonexistent_is_ok() {
        let temp_dir = TempDir::new().unwrap();
        let lock_path = temp_dir.path().join("nonexistent.lock");

        // Should not panic
        remove_lock_file(&lock_path);
    }

    // ==================== env_is_truthy tests ====================

    #[test]
    fn test_env_is_truthy_one() {
        unsafe { std::env::set_var("BMM_TEST_VAR_1", "1") };
        assert!(env_is_truthy("BMM_TEST_VAR_1"));
        unsafe { std::env::remove_var("BMM_TEST_VAR_1") };
    }

    #[test]
    fn test_env_is_truthy_true() {
        unsafe { std::env::set_var("BMM_TEST_VAR_TRUE", "true") };
        assert!(env_is_truthy("BMM_TEST_VAR_TRUE"));
        unsafe { std::env::remove_var("BMM_TEST_VAR_TRUE") };
    }

    #[test]
    fn test_env_is_truthy_yes() {
        unsafe { std::env::set_var("BMM_TEST_VAR_YES", "yes") };
        assert!(env_is_truthy("BMM_TEST_VAR_YES"));
        unsafe { std::env::remove_var("BMM_TEST_VAR_YES") };
    }

    #[test]
    fn test_env_is_truthy_case_insensitive() {
        unsafe { std::env::set_var("BMM_TEST_VAR_CASE", "TRUE") };
        assert!(env_is_truthy("BMM_TEST_VAR_CASE"));
        unsafe { std::env::set_var("BMM_TEST_VAR_CASE", "True") };
        assert!(env_is_truthy("BMM_TEST_VAR_CASE"));
        unsafe { std::env::set_var("BMM_TEST_VAR_CASE", "YES") };
        assert!(env_is_truthy("BMM_TEST_VAR_CASE"));
        unsafe { std::env::remove_var("BMM_TEST_VAR_CASE") };
    }

    #[test]
    fn test_env_is_truthy_falsy_values() {
        unsafe { std::env::set_var("BMM_TEST_VAR_FALSY", "0") };
        assert!(!env_is_truthy("BMM_TEST_VAR_FALSY"));
        unsafe { std::env::set_var("BMM_TEST_VAR_FALSY", "false") };
        assert!(!env_is_truthy("BMM_TEST_VAR_FALSY"));
        unsafe { std::env::set_var("BMM_TEST_VAR_FALSY", "no") };
        assert!(!env_is_truthy("BMM_TEST_VAR_FALSY"));
        unsafe { std::env::set_var("BMM_TEST_VAR_FALSY", "") };
        assert!(!env_is_truthy("BMM_TEST_VAR_FALSY"));
        unsafe { std::env::remove_var("BMM_TEST_VAR_FALSY") };
    }

    #[test]
    fn test_env_is_truthy_unset() {
        unsafe { std::env::remove_var("BMM_TEST_VAR_UNSET") };
        assert!(!env_is_truthy("BMM_TEST_VAR_UNSET"));
    }

    // ==================== Fallback chain simulation tests ====================

    #[test]
    fn test_full_fallback_chain_simulation() {
        // Simulate multiple crashes walking through the fallback chain
        let mut state = GpuState::Dmabuf;

        // Crash 1: DMABUF -> Vulkan
        state = state.next_fallback();
        assert_eq!(state, GpuState::Vulkan);

        // Crash 2: Vulkan -> NGL
        state = state.next_fallback();
        assert_eq!(state, GpuState::Ngl);

        // Crash 3: NGL -> GL
        state = state.next_fallback();
        assert_eq!(state, GpuState::Gl);

        // Crash 4: GL -> Disabled
        state = state.next_fallback();
        assert_eq!(state, GpuState::Disabled);

        // Crash 5: Disabled stays Disabled (terminal state)
        state = state.next_fallback();
        assert_eq!(state, GpuState::Disabled);
    }

    #[test]
    fn test_lock_dir_returns_some() {
        // This test verifies lock_dir returns Some on systems with a data directory
        let result = lock_dir();
        // On most systems this should be Some, but we don't fail if it's None
        // (e.g., in unusual test environments)
        if let Some(path) = result {
            assert!(path.to_string_lossy().contains("balatro-mod-manager"));
        }
    }

    #[test]
    fn test_wayland_lock_path_contains_expected_name() {
        if let Some(path) = wayland_lock_path() {
            assert!(path.to_string_lossy().contains("wayland_session.lock"));
        }
    }

    #[test]
    fn test_gpu_lock_path_contains_expected_name() {
        if let Some(path) = gpu_lock_path() {
            assert!(path.to_string_lossy().contains("gpu_session.lock"));
        }
    }

    // ==================== GpuState equality and copy tests ====================

    #[test]
    fn test_gpu_state_equality() {
        assert_eq!(GpuState::Dmabuf, GpuState::Dmabuf);
        assert_ne!(GpuState::Dmabuf, GpuState::Vulkan);
        assert_ne!(GpuState::Vulkan, GpuState::Ngl);
    }

    #[test]
    fn test_gpu_state_is_copy() {
        let state = GpuState::Dmabuf;
        let copied = state;
        // Both should still be valid (Copy trait)
        assert_eq!(state, copied);
    }

    #[test]
    fn test_gpu_state_debug() {
        // Verify Debug is implemented
        let debug_str = format!("{:?}", GpuState::Dmabuf);
        assert!(debug_str.contains("Dmabuf"));
    }
}
