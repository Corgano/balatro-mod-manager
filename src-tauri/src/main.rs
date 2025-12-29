// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_display_backend() -> Option<String> {
    use std::env;

    let set_env_if_absent = |key: &str, value: &str| {
        if env::var_os(key).is_none() {
            // Safety: called during startup before any threads are spawned, so mutating the
            // process environment is safe.
            unsafe { env::set_var(key, value) };
        }
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

    // Allow users to explicitly keep Wayland if they know their setup is stable.
    let allow_wayland = matches!(
        env::var("BMM_ALLOW_WAYLAND"),
        Ok(v) if matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
    );
    if allow_wayland {
        return Some("Wayland session detected; respecting BMM_ALLOW_WAYLAND=1".into());
    }

    // Prefer XWayland when available to avoid Wayland protocol errors seen during startup.
    if env::var_os("DISPLAY").is_some() {
        set_env_if_absent("WINIT_UNIX_BACKEND", "x11");
        set_env_if_absent("GDK_BACKEND", "x11");
        set_env_if_absent("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        return Some(
            "Wayland session detected; forcing X11 backend to avoid compositor protocol errors. \
             Set BMM_ALLOW_WAYLAND=1 to keep native Wayland."
                .into(),
        );
    }

    Some(
        "Wayland session detected without X11; leaving Wayland enabled (set WINIT_UNIX_BACKEND/GDK_BACKEND manually if needed)."
            .into(),
    )
}

#[cfg(target_os = "macos")]
fn scrub_dyld_injection_env() {
    // When a global DYLD_INSERT_LIBRARIES/related env is set (e.g., from game launchers),
    // the manager process itself can get preloaded with external libs and crash on startup.
    for key in [
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "DYLD_FRAMEWORK_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ] {
        // Safety: called during startup before threads spawn, so mutating process env is safe.
        unsafe {
            std::env::remove_var(key);
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    scrub_dyld_injection_env();
    #[cfg(target_os = "linux")]
    let backend_note = configure_display_backend();
    let _ = fix_path_env::fix();
    if let Err(e) = bmm_lib::logging::init_logger() {
        eprintln!("Failed to initialize logging: {e}");
    }
    #[cfg(target_os = "linux")]
    if let Some(note) = backend_note {
        log::info!("{note}");
    }
    balatro_mod_manager_lib::run();
    log::logger().flush();
}
