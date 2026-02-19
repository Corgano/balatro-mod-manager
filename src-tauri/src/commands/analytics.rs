use crate::state::AppState;
use serde_json::Value;

/// Toggle analytics opt-out from the frontend settings page.
#[tauri::command]
pub async fn get_analytics_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.is_analytics_enabled().map_err(|e| e.to_string())
}

/// Toggle analytics opt-out from the frontend settings page.
#[tauri::command]
pub async fn set_analytics_status(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.set_analytics_enabled(enabled).map_err(|e| e.to_string())
}

/// Fire-and-forget event tracking command. Called from the frontend.
/// Silently does nothing if analytics is disabled.
#[tauri::command]
pub async fn track_event(
    state: tauri::State<'_, AppState>,
    name: String,
    props: Option<Value>,
) -> Result<(), String> {
    let enabled = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.is_analytics_enabled().unwrap_or(true)
    };
    if !enabled {
        return Ok(());
    }

    let version = env!("CARGO_PKG_VERSION").to_string();
    let props = props.unwrap_or(serde_json::json!({}));

    tauri::async_runtime::spawn(async move {
        crate::analytics::send_event(&name, props, &version).await;
    });

    Ok(())
}
