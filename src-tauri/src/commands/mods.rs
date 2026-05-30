use std::path::{Path, PathBuf};

use crate::state::AppState;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;

/// Maximum directory depth scanned when toggling / checking `.lovelyignore`.
/// Mods rarely nest beyond 2-3 levels; this bound keeps us safe from
/// pathological symlink loops and accidental drag-and-drop of huge trees.
const SUBTREE_MAX_DEPTH: usize = 6;

/// Files/dirs that should never be searched for `.lovelyignore`.
fn is_skipped_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.')
        || lower == "node_modules"
        || lower == "__macosx"
        || lower == ".git"
        || lower == "lovely"
}

/// Recursively collect every directory inside `root` (inclusive) up to
/// `SUBTREE_MAX_DEPTH`. Symlinks are not followed.
fn collect_subdirs(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth >= SUBTREE_MAX_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_dir() || meta.file_type().is_symlink() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if is_skipped_name(name) {
                continue;
            }
            out.push(path.clone());
            stack.push((path, depth + 1));
        }
    }
    out
}

/// Returns `true` if any directory in the subtree (including `mod_dir`)
/// contains a `.lovelyignore` marker. Steamodded's in-game disable can
/// write the marker into a nested mod folder, so a shallow check misses it.
fn is_enabled_recursive(mod_dir: &Path) -> bool {
    !collect_subdirs(mod_dir)
        .into_iter()
        .any(|dir| dir.join(".lovelyignore").exists())
}

fn set_mod_enabled_at_path(mod_dir: &Path, enabled: bool) -> Result<(), String> {
    if !mod_dir.exists() {
        return Err(format!("Mod path does not exist: {}", mod_dir.display()));
    }

    let dirs = collect_subdirs(mod_dir);

    if enabled {
        dirs.par_iter().try_for_each(|dir| {
            let ignore = dir.join(".lovelyignore");
            if ignore.exists() {
                fs::remove_file(&ignore).map_err(|e| {
                    format!("Failed to remove .lovelyignore in {}: {e}", dir.display())
                })
            } else {
                Ok(())
            }
        })?;
    } else {
        dirs.par_iter().try_for_each(|dir| {
            fs::write(dir.join(".lovelyignore"), "")
                .map_err(|e| format!("Failed to create .lovelyignore in {}: {e}", dir.display()))
        })?;
    }

    Ok(())
}

/// Resolve an installed mod's path by name, holding the global DB lock only
/// for the lookup itself.
///
/// The `.lovelyignore` toggling below is plain filesystem work that does not
/// touch the database, yet the database lives behind a single process-wide
/// `Mutex`. Holding that lock across the (potentially slow, e.g. Windows
/// Defender scanning freshly written files) I/O would block *every* other
/// command that needs the DB, freezing the whole UI. So we copy out the path
/// and drop the guard before doing any filesystem work.
fn installed_mod_path(state: &AppState, mod_name: &str) -> Result<PathBuf, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let installed_mods = db.get_installed_mods()?;
    installed_mods
        .iter()
        .find(|m| m.name == mod_name)
        .map(|m| PathBuf::from(&m.path))
        .ok_or_else(|| format!("Mod not found: {mod_name}"))
}

#[tauri::command]
pub async fn is_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
) -> Result<bool, String> {
    let mod_dir = installed_mod_path(&state, &mod_name)?;

    if !mod_dir.exists() {
        return Err(format!("Mod directory not found: {mod_name}"));
    }

    Ok(is_enabled_recursive(&mod_dir))
}

#[tauri::command]
pub async fn toggle_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
    enabled: bool,
) -> Result<(), String> {
    let mod_dir = installed_mod_path(&state, &mod_name)?;
    // Run the (blocking, parallel) filesystem work off the async runtime's
    // worker threads so a slow toggle never starves other commands.
    tauri::async_runtime::spawn_blocking(move || set_mod_enabled_at_path(&mod_dir, enabled))
        .await
        .map_err(|e| format!("Toggle task failed: {e}"))?
}

#[tauri::command]
pub async fn is_mod_enabled_by_path(mod_path: String) -> Result<bool, String> {
    let path = PathBuf::from(&mod_path);
    if !path.exists() {
        return Err(format!("Mod path does not exist: {mod_path}"));
    }
    Ok(is_enabled_recursive(&path))
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
    // Resolve DB-tracked paths under the lock, then release it before doing
    // any filesystem work (see `installed_mod_path`).
    let mut path_map: HashMap<String, PathBuf> = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installed_mods()?
            .into_iter()
            .map(|m| (m.name, PathBuf::from(m.path)))
            .collect()
    };

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

    // Off-load the blocking parallel filesystem work from the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        tasks
            .par_iter()
            .try_for_each(|(path, enabled)| set_mod_enabled_at_path(path, *enabled))
    })
    .await
    .map_err(|e| format!("Batch toggle task failed: {e}"))??;

    Ok(())
}

/// Return an enabled/disabled map for all installed mods in the DB plus provided local paths.
#[tauri::command]
pub async fn enabled_state_map(
    state: tauri::State<'_, AppState>,
    local_paths: Option<Vec<String>>,
) -> Result<HashMap<String, bool>, String> {
    let mut out: HashMap<String, bool> = HashMap::new();

    // Resolve DB-tracked mods under the lock, then release it before the
    // (potentially slow) recursive filesystem scans below.
    let installed_mods: Vec<(String, PathBuf)> = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installed_mods()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| (m.name, PathBuf::from(m.path)))
            .collect()
    };
    for (name, p) in installed_mods {
        out.insert(name, is_enabled_recursive(&p));
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
            out.insert(name, is_enabled_recursive(&path));
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

    #[test]
    fn test_set_mod_enabled_walks_deep_nesting() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        let deep = mod_dir.join("sub1").join("sub2").join("sub3");
        fs::create_dir_all(&deep).unwrap();

        set_mod_enabled_at_path(&mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());
        assert!(mod_dir.join("sub1").join(".lovelyignore").exists());
        assert!(mod_dir.join("sub1/sub2").join(".lovelyignore").exists());
        assert!(deep.join(".lovelyignore").exists());

        set_mod_enabled_at_path(&mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
        assert!(!deep.join(".lovelyignore").exists());
    }

    #[test]
    fn test_is_enabled_recursive_detects_nested_marker() {
        // Mimics Steamodded disabling a nested mod folder (e.g. Cryptid).
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("Cryptid");
        let inner = mod_dir.join("Cryptid-main");
        fs::create_dir_all(&inner).unwrap();

        assert!(is_enabled_recursive(&mod_dir));

        // Only the inner folder has the marker; top-level is clean.
        fs::write(inner.join(".lovelyignore"), "").unwrap();
        assert!(!is_enabled_recursive(&mod_dir));
    }

    #[test]
    fn test_is_enabled_recursive_ignores_dot_and_system_dirs() {
        // .git / node_modules shouldn't be scanned for markers; otherwise a
        // hidden-folder commit could confuse the enabled-state detector.
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("Mod");
        let dotdir = mod_dir.join(".git");
        fs::create_dir_all(&dotdir).unwrap();
        fs::write(dotdir.join(".lovelyignore"), "").unwrap();
        assert!(is_enabled_recursive(&mod_dir));
    }
}
