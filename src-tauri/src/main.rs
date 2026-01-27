// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    let gpu_note = balatro_mod_manager_lib::wayland_session::configure_gpu();
    #[cfg(target_os = "linux")]
    let backend_note = balatro_mod_manager_lib::wayland_session::configure_display_backend();
    let _ = fix_path_env::fix();
    if let Err(e) = bmm_lib::logging::init_logger() {
        eprintln!("Failed to initialize logging: {e}");
    }
    #[cfg(target_os = "linux")]
    if let Some(note) = gpu_note {
        log::info!("{note}");
    }
    #[cfg(target_os = "linux")]
    if let Some(note) = backend_note {
        log::info!("{note}");
    }
    balatro_mod_manager_lib::run();
    log::logger().flush();
}
