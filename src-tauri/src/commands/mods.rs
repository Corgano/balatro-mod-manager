use std::path::{Path, PathBuf};

use crate::state::AppState;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;

fn set_mod_enabled_at_path(mod_dir: &Path, enabled: bool) -> Result<(), String> {
    if !mod_dir.exists() {
        return Err(format!("Mod path does not exist: {}", mod_dir.display()));
    }

    let entries: Vec<_> = fs::read_dir(mod_dir)
        .map_err(|e| format!("Failed to read mod directory: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Failed to read entry: {e}"))?;

    let ignore_file_path = mod_dir.join(".lovelyignore");

    if enabled {
        entries
            .par_iter()
            .filter(|entry| entry.path().is_dir())
            .try_for_each(|entry| {
                let ignore_path = entry.path().join(".lovelyignore");
                if ignore_path.exists() {
                    fs::remove_file(&ignore_path).map_err(|e| {
                        format!(
                            "Failed to remove .lovelyignore in {}: {}",
                            entry.path().display(),
                            e
                        )
                    })
                } else {
                    Ok(())
                }
            })?;

        if ignore_file_path.exists() {
            fs::remove_file(&ignore_file_path)
                .map_err(|e| format!("Failed to remove top-level .lovelyignore: {e}"))?;
        }
    } else {
        entries
            .par_iter()
            .filter(|entry| entry.path().is_dir())
            .try_for_each(|entry| {
                fs::write(entry.path().join(".lovelyignore"), "").map_err(|e| {
                    format!(
                        "Failed to create .lovelyignore in {}: {}",
                        entry.path().display(),
                        e
                    )
                })
            })?;

        fs::write(&ignore_file_path, "")
            .map_err(|e| format!("Failed to create top-level .lovelyignore: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn is_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
) -> Result<bool, String> {
    let db = state.db.lock().await;
    let installed_mods = db.get_installed_mods()?;
    let mod_dir = &installed_mods
        .iter()
        .find(|m| m.name == mod_name)
        .ok_or_else(|| format!("Mod not found: {mod_name}"))?
        .path
        .clone();
    let mod_dir: &Path = Path::new(mod_dir);

    if !mod_dir.exists() {
        return Err(format!("Mod directory not found: {mod_name}"));
    }

    let ignore_file_path = mod_dir.join(".lovelyignore");
    Ok(!ignore_file_path.exists())
}

#[tauri::command]
pub async fn toggle_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
    enabled: bool,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let installed_mods = db.get_installed_mods()?;
    let mod_dir = &installed_mods
        .iter()
        .find(|m| m.name == mod_name)
        .ok_or_else(|| format!("Mod not found: {mod_name}"))?
        .path
        .clone();
    let mod_dir: &Path = Path::new(mod_dir);
    set_mod_enabled_at_path(mod_dir, enabled)
}

#[tauri::command]
pub async fn is_mod_enabled_by_path(mod_path: String) -> Result<bool, String> {
    let path = PathBuf::from(&mod_path);
    if !path.exists() {
        return Err(format!("Mod path does not exist: {mod_path}"));
    }
    let ignore_file_path = path.join(".lovelyignore");
    Ok(!ignore_file_path.exists())
}

#[tauri::command]
pub async fn toggle_mod_enabled_by_path(mod_path: String, enabled: bool) -> Result<(), String> {
    let path = PathBuf::from(&mod_path);
    set_mod_enabled_at_path(&path, enabled)
}

#[tauri::command]
pub async fn toggle_mods_enabled_batch(
    state: tauri::State<'_, AppState>,
    enabled: Vec<String>,
    disabled: Vec<String>,
    local_paths: Option<Vec<String>>,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let installed_mods = db.get_installed_mods()?;
    let path_map: HashMap<String, PathBuf> = installed_mods
        .into_iter()
        .map(|m| (m.name, PathBuf::from(m.path)))
        .collect();
    let mut path_map = path_map;

    if let Some(paths) = local_paths {
        for p in paths {
            let path = PathBuf::from(&p);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            path_map.entry(name).or_insert(path);
        }
    }

    let mut tasks: Vec<(PathBuf, bool)> = Vec::new();
    for name in enabled {
        if let Some(path) = path_map.get(&name) {
            tasks.push((path.clone(), true));
        }
    }
    for name in disabled {
        if let Some(path) = path_map.get(&name) {
            tasks.push((path.clone(), false));
        }
    }

    tasks
        .par_iter()
        .try_for_each(|(path, enabled)| set_mod_enabled_at_path(path, *enabled))?;

    Ok(())
}

/// Return an enabled/disabled map for all installed mods in the DB plus provided local paths.
#[tauri::command]
pub async fn enabled_state_map(
    state: tauri::State<'_, AppState>,
    local_paths: Option<Vec<String>>,
) -> Result<HashMap<String, bool>, String> {
    let mut out: HashMap<String, bool> = HashMap::new();

    // DB-installed mods
    let db = state.db.lock().await;
    let installed_mods = db.get_installed_mods().map_err(|e| e.to_string())?;
    for m in installed_mods {
        let p = PathBuf::from(&m.path);
        let ignore = p.join(".lovelyignore");
        let enabled = !ignore.exists();
        out.insert(m.name, enabled);
    }

    // Local mods passed from frontend
    if let Some(paths) = local_paths {
        for p in paths {
            let path = PathBuf::from(&p);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let ignore = path.join(".lovelyignore");
            let enabled = !ignore.exists();
            out.insert(name, enabled);
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_set_mod_enabled_creates_lovelyignore() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        fs::create_dir(&mod_dir).unwrap();

        // Initially no .lovelyignore
        assert!(!mod_dir.join(".lovelyignore").exists());

        // Disable the mod
        set_mod_enabled_at_path(&mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());

        // Enable the mod
        set_mod_enabled_at_path(&mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
    }

    #[test]
    fn test_set_mod_enabled_handles_subdirectories() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        let sub_dir = mod_dir.join("submod");
        fs::create_dir_all(&sub_dir).unwrap();

        // Disable - should create .lovelyignore in both places
        set_mod_enabled_at_path(&mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());
        assert!(sub_dir.join(".lovelyignore").exists());

        // Enable - should remove both
        set_mod_enabled_at_path(&mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
        assert!(!sub_dir.join(".lovelyignore").exists());
    }

    #[test]
    fn test_set_mod_enabled_nonexistent_path() {
        let result = set_mod_enabled_at_path(Path::new("/nonexistent/path"), true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_set_mod_enabled_idempotent() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        fs::create_dir(&mod_dir).unwrap();

        // Disable twice should work
        set_mod_enabled_at_path(&mod_dir, false).unwrap();
        set_mod_enabled_at_path(&mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());

        // Enable twice should work
        set_mod_enabled_at_path(&mod_dir, true).unwrap();
        set_mod_enabled_at_path(&mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
    }
}
