// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_display_backend() -> Option<String> {
    use std::env;

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
        if env::var_os("WINIT_UNIX_BACKEND").is_none() {
            env::set_var("WINIT_UNIX_BACKEND", "x11");
        }
        if env::var_os("GDK_BACKEND").is_none() {
            env::set_var("GDK_BACKEND", "x11");
        }
        if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        return Some(
            "Wayland session detected; forcing X11 backend to avoid compositor protocol errors. \
             Set BMM_ALLOW_WAYLAND=1 to keep native Wayland."
                .into(),
        );
    }

    if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    Some(
        "Wayland session detected without X11; leaving Wayland enabled (set WINIT_UNIX_BACKEND/GDK_BACKEND manually if needed)."
            .into(),
    )
}

#[cfg(not(target_os = "linux"))]
fn configure_display_backend() -> Option<String> {
    None
}

fn main() {
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
