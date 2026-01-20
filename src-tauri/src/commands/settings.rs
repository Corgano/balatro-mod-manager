use crate::compat_helper;
use crate::state::AppState;
use crate::util::map_error;
use bmm_lib::lovely;

#[tauri::command]
pub async fn get_lovely_console_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().await;
    map_error(db.is_lovely_console_enabled())
}

#[tauri::command]
pub async fn set_lovely_console_status(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    map_error(db.set_lovely_console_status(enabled))
}

#[tauri::command]
pub async fn get_discord_rpc_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().await;
    db.is_discord_rpc_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_discord_rpc_status(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;

    db.set_discord_rpc_enabled(enabled)
        .map_err(|e| e.to_string())?;

    // update the runtime status so changes take effect immediately
    let discord_rpc = state.discord_rpc.lock().await;
    discord_rpc.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn get_background_state(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().await;
    map_error(db.get_background_enabled())
}

#[tauri::command]
pub async fn set_background_state(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    map_error(db.set_background_enabled(enabled))
}

#[tauri::command]
pub async fn get_compat_helper_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().await;
    map_error(db.is_compat_helper_enabled())
}

#[tauri::command]
pub async fn set_compat_helper_status(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    map_error(db.set_compat_helper_enabled(enabled))?;
    compat_helper::sync_compat_helper(enabled)?;
    Ok(())
}

#[tauri::command]
pub async fn set_linux_prefix(
    state: tauri::State<'_, AppState>,
    value: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    map_error(db.set_linux_prefix(&value))
}

#[tauri::command]
pub async fn get_linux_prefix(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let db = state.db.lock().await;
    Ok(db
        .get_linux_prefix()
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

#[tauri::command]
pub async fn is_security_warning_acknowledged(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().await;
    map_error(db.is_security_warning_acknowledged())
}

#[tauri::command]
pub async fn set_security_warning_acknowledged(
    state: tauri::State<'_, AppState>,
    acknowledged: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    map_error(db.set_security_warning_acknowledged(acknowledged))
}

#[tauri::command]
pub async fn get_launch_mode(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let db = state.db.lock().await;
    map_error(db.get_launch_mode())
}

#[tauri::command]
pub async fn set_launch_mode(
    state: tauri::State<'_, AppState>,
    mode: String,
) -> Result<(), String> {
    // Validate mode
    if mode != "modded" && mode != "vanilla" {
        return Err(format!(
            "Invalid launch mode: {}. Must be 'modded' or 'vanilla'",
            mode
        ));
    }

    // Toggle the injector files based on mode
    let enable_injector = mode == "modded";
    map_error(lovely::set_injector_enabled(enable_injector))?;

    // Save the preference to database
    let db = state.db.lock().await;
    map_error(db.set_launch_mode(&mode))
}
