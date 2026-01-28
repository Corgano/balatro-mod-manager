//! Tauri commands for backup and restore functionality.

use bmm_lib::backup::{self, BackupMetadata, BackupTrigger};
use serde::{Deserialize, Serialize};

/// Serializable backup trigger for frontend communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupTriggerInput {
    AutoUpdate,
    AutoUninstall,
    AutoBulk,
    Manual,
}

impl From<BackupTriggerInput> for BackupTrigger {
    fn from(input: BackupTriggerInput) -> Self {
        match input {
            BackupTriggerInput::AutoUpdate => BackupTrigger::AutoUpdate,
            BackupTriggerInput::AutoUninstall => BackupTrigger::AutoUninstall,
            BackupTriggerInput::AutoBulk => BackupTrigger::AutoBulk,
            BackupTriggerInput::Manual => BackupTrigger::Manual,
        }
    }
}

/// Create a new backup.
#[tauri::command]
pub async fn create_backup(
    trigger: BackupTriggerInput,
    name: Option<String>,
    context: Option<String>,
) -> Result<BackupMetadata, String> {
    backup::create_backup(trigger.into(), name, context)
        .await
        .map_err(|e| e.to_string())
}

/// List all available backups.
#[tauri::command]
pub async fn list_backups() -> Result<Vec<BackupMetadata>, String> {
    backup::list_backups().await.map_err(|e| e.to_string())
}

/// Restore a backup by ID.
#[tauri::command]
pub async fn restore_backup(backup_id: String) -> Result<(), String> {
    backup::restore_backup(&backup_id)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a backup by ID.
#[tauri::command]
pub async fn delete_backup(backup_id: String) -> Result<(), String> {
    backup::delete_backup(&backup_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get the total size of all backups in bytes.
#[tauri::command]
pub async fn get_backups_total_size() -> Result<u64, String> {
    backup::get_total_backups_size()
        .await
        .map_err(|e| e.to_string())
}

/// Get the backups directory path (creates it if it doesn't exist).
#[tauri::command]
pub async fn get_backups_directory() -> Result<String, String> {
    let path = backup::get_backups_dir().map_err(|e| e.to_string())?;

    // Create the directory if it doesn't exist
    if !path.exists() {
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| format!("Failed to create backups directory: {}", e))?;
    }

    Ok(path.to_string_lossy().to_string())
}

/// Check if a restore was interrupted.
#[tauri::command]
pub async fn check_interrupted_restore() -> Result<Option<String>, String> {
    Ok(backup::check_interrupted_restore())
}

/// Clear the interrupted restore marker.
#[tauri::command]
pub async fn clear_interrupted_restore() -> Result<(), String> {
    backup::clear_interrupted_restore_marker().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_trigger_from_auto_update() {
        let input = BackupTriggerInput::AutoUpdate;
        let trigger: BackupTrigger = input.into();
        assert!(matches!(trigger, BackupTrigger::AutoUpdate));
    }

    #[test]
    fn test_backup_trigger_from_auto_uninstall() {
        let input = BackupTriggerInput::AutoUninstall;
        let trigger: BackupTrigger = input.into();
        assert!(matches!(trigger, BackupTrigger::AutoUninstall));
    }

    #[test]
    fn test_backup_trigger_from_auto_bulk() {
        let input = BackupTriggerInput::AutoBulk;
        let trigger: BackupTrigger = input.into();
        assert!(matches!(trigger, BackupTrigger::AutoBulk));
    }

    #[test]
    fn test_backup_trigger_from_manual() {
        let input = BackupTriggerInput::Manual;
        let trigger: BackupTrigger = input.into();
        assert!(matches!(trigger, BackupTrigger::Manual));
    }

    #[test]
    fn test_backup_trigger_input_serialize() {
        let input = BackupTriggerInput::Manual;
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, "\"manual\"");
    }

    #[test]
    fn test_backup_trigger_input_deserialize() {
        let json = "\"auto_update\"";
        let input: BackupTriggerInput = serde_json::from_str(json).unwrap();
        assert!(matches!(input, BackupTriggerInput::AutoUpdate));
    }

    #[test]
    fn test_backup_trigger_input_all_variants_serialize() {
        assert_eq!(
            serde_json::to_string(&BackupTriggerInput::AutoUpdate).unwrap(),
            "\"auto_update\""
        );
        assert_eq!(
            serde_json::to_string(&BackupTriggerInput::AutoUninstall).unwrap(),
            "\"auto_uninstall\""
        );
        assert_eq!(
            serde_json::to_string(&BackupTriggerInput::AutoBulk).unwrap(),
            "\"auto_bulk\""
        );
        assert_eq!(
            serde_json::to_string(&BackupTriggerInput::Manual).unwrap(),
            "\"manual\""
        );
    }
}
