//! Automatic Balatro installation path detection.
//!
//! This module provides functions to locate Balatro game installations across
//! different platforms and store types:
//! - Steam installations (via library folders)
//! - Registry entries (Windows)
//! - Common installation directories
//! - Flatpak and Proton prefixes (Linux)
//!
//! # Detection Strategy
//!
//! 1. Check user-configured path in database
//! 2. Query Steam library folders via `libraryfolders.vdf`
//! 3. Check common installation directories
//! 4. (Windows) Query Windows Registry for Steam path

use crate::database::Database;
use log::{debug, error};
use once_cell::sync::Lazy;
#[cfg(target_os = "linux")]
use regex::Regex;
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::fs::File;
#[cfg(target_os = "windows")]
use std::io::{BufReader, Read};
#[cfg(target_os = "windows")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use sysinfo::System;
#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::*;

/// TTL for the Balatro paths cache (5 minutes).
const BALATRO_PATHS_CACHE_TTL: Duration = Duration::from_secs(300);

/// Cached Balatro installation paths with timestamp.
struct CachedPaths {
    paths: Vec<PathBuf>,
    cached_at: Instant,
}

/// Global cache for Balatro paths to avoid repeated filesystem scans.
static BALATRO_PATHS_CACHE: Lazy<RwLock<Option<CachedPaths>>> = Lazy::new(|| RwLock::new(None));

/// Returns cached Balatro paths if available and not expired, otherwise performs a fresh scan.
///
/// This function wraps `get_balatro_paths()` with a time-based cache to avoid
/// repeated filesystem scanning on every call. The cache is valid for 60 seconds.
pub fn get_balatro_paths_cached() -> Vec<PathBuf> {
    // Try to read from cache first
    if let Ok(guard) = BALATRO_PATHS_CACHE.read()
        && let Some(cached) = guard.as_ref()
        && cached.cached_at.elapsed() < BALATRO_PATHS_CACHE_TTL
    {
        debug!("Returning {} cached Balatro paths", cached.paths.len());
        return cached.paths.clone();
    }

    // Cache miss or expired, acquire write lock and refresh
    if let Ok(mut guard) = BALATRO_PATHS_CACHE.write() {
        // Double-check after acquiring write lock
        if let Some(cached) = guard.as_ref()
            && cached.cached_at.elapsed() < BALATRO_PATHS_CACHE_TTL
        {
            return cached.paths.clone();
        }

        // Perform fresh scan
        debug!("Refreshing Balatro paths cache");
        let paths = get_balatro_paths();
        *guard = Some(CachedPaths {
            paths: paths.clone(),
            cached_at: Instant::now(),
        });
        return paths;
    }

    // Fallback if we can't get the lock
    get_balatro_paths()
}

/// Invalidates the Balatro paths cache, forcing a fresh scan on the next call.
///
/// Call this when the user changes their installation path or when you need
/// to ensure fresh results.
pub fn invalidate_balatro_paths_cache() {
    if let Ok(mut guard) = BALATRO_PATHS_CACHE.write() {
        *guard = None;
        debug!("Balatro paths cache invalidated");
    }
}

#[cfg(target_os = "windows")]
fn read_path_from_registry() -> Result<String, std::io::Error> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let steam_path = hklm.open_subkey("SOFTWARE\\WOW6432Node\\Valve\\Steam")?;

    Ok(steam_path.get_value("InstallPath")?)
}

fn remove_unexisting_paths(paths: &mut Vec<PathBuf>) {
    let mut i = 0;
    while i < paths.len() {
        if !paths[i].exists() {
            paths.remove(i);
        } else {
            i += 1;
        }
    }
}

#[cfg(target_os = "windows")]
pub fn get_balatro_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![];

    // 1) Respect custom Balatro path from our database first
    if let Ok(db) = Database::new()
        && let Ok(Some(custom_path)) = db.get_installation_path()
    {
        let p = PathBuf::from(&custom_path);
        if p.exists() {
            paths.push(p);
        }
    }

    // 2) Discover Steam libraries (registry + libraryfolders.vdf)
    let steam_path = read_path_from_registry();
    let mut steam_path = steam_path.unwrap_or_else(|_| {
        error!(
            "Could not read steam install path from Registry! Trying standard installation path in C:\\"
        );
        String::from("C:\\Program Files (x86)\\Steam")
    });

    steam_path.push_str("\\steamapps\\libraryfolders.vdf");
    let libraryfolders_path = Path::new(&steam_path);
    if !libraryfolders_path.exists() {
        error!(
            "'{}' not found.",
            libraryfolders_path.to_str().unwrap_or("<invalid path>")
        );
        // Return whatever we have (e.g., custom path), after cleaning
        remove_unexisting_paths(&mut paths);
        dedup_paths_case_insensitive(&mut paths);
        return paths;
    }

    let libraryfolders_file = match File::open(libraryfolders_path) {
        Ok(f) => f,
        Err(e) => {
            error!(
                "Failed to open libraryfolders.vdf at {}: {}",
                libraryfolders_path.to_string_lossy(),
                e
            );
            remove_unexisting_paths(&mut paths);
            dedup_paths_case_insensitive(&mut paths);
            return paths;
        }
    };

    let mut libraryfolders_contents = String::new();
    let mut libraryfolders_reader = BufReader::new(libraryfolders_file);
    if let Err(e) = libraryfolders_reader.read_to_string(&mut libraryfolders_contents) {
        error!("Failed to read libraryfolders.vdf: {}", e);
        remove_unexisting_paths(&mut paths);
        dedup_paths_case_insensitive(&mut paths);
        return paths;
    }

    let lines = libraryfolders_contents.split('\n').collect::<Vec<&str>>();
    for line in lines {
        if line.contains("\t\t\"path\"\t\t") {
            let parts = line.split('\"').collect::<Vec<&str>>();
            if parts.len() > 3 {
                let path = parts[3];
                paths.push(PathBuf::from(path).join("steamapps\\common\\Balatro"));
            }
        }
    }

    remove_unexisting_paths(&mut paths);
    dedup_paths_case_insensitive(&mut paths);
    debug!("Found {} Balatro installations: {:?}", paths.len(), paths);
    paths
}

fn dedup_paths_case_insensitive(paths: &mut Vec<PathBuf>) {
    let mut seen: HashSet<String> = HashSet::new();
    paths.retain(|p| {
        let canon = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let key = canon.to_string_lossy().to_string().to_lowercase();
        seen.insert(key)
    });
}

#[cfg(target_os = "macos")]
pub fn get_balatro_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![];

    // Prefer custom DB path first
    if let Ok(db) = Database::new()
        && let Ok(Some(custom_path)) = db.get_installation_path()
    {
        let p = PathBuf::from(&custom_path);
        if p.exists() {
            paths.push(p);
        }
    }
    match home::home_dir() {
        Some(path) => {
            let mut path = path;
            path.push("Library/Application Support/Steam/steamapps/common/Balatro");
            paths.push(path);
        }
        None => error!("Impossible to get your home dir!"),
    }
    remove_unexisting_paths(&mut paths);
    dedup_paths_case_insensitive(&mut paths);
    debug!("Found {} Balatro installations: {:?}", paths.len(), paths);
    paths
}

#[cfg(target_os = "linux")]
fn parse_libraryfolders(library_path: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let contents = match std::fs::read_to_string(library_path) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Failed to read libraryfolders.vdf at {}: {}",
                library_path.to_string_lossy(),
                e
            );
            return results;
        }
    };

    // Matches lines like: "path"    "/mnt/games/SteamLibrary"
    let re = Regex::new(r#""path"\s*"([^"]+)""#).unwrap();
    for caps in re.captures_iter(&contents) {
        if let Some(path_match) = caps.get(1) {
            let path_str = path_match.as_str();
            // Normalize escaped backslashes in case they appear
            let cleaned = path_str.replace("\\\\", "\\");
            results.push(PathBuf::from(cleaned).join("steamapps/common/Balatro"));
        }
    }

    results
}

#[cfg(target_os = "linux")]
fn scan_for_steamapps_dirs(base: &Path, max_depth: usize, max_dirs: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    let mut visited: usize = 0;

    if !base.exists() {
        return found;
    }

    queue.push_back((base.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        if visited >= max_dirs {
            break;
        }
        visited += 1;

        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if dir_name == "steamapps" {
            found.push(dir);
            continue;
        }

        if depth >= max_depth {
            continue;
        }

        if dir_name == "node_modules"
            || dir_name == ".git"
            || dir_name == "cache"
            || dir_name == ".cache"
            || dir_name == "trash"
            || dir_name == ".trash"
        {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    queue.push_back((path, depth + 1));
                }
            }
        }
    }

    found
}

#[cfg(target_os = "linux")]
pub fn get_balatro_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![];

    // Prefer custom DB path first
    if let Ok(db) = Database::new()
        && let Ok(Some(custom_path)) = db.get_installation_path()
    {
        let p = PathBuf::from(&custom_path);
        if p.exists() {
            paths.push(p);
        }
    }

    let mut steam_roots: Vec<PathBuf> = Vec::new();
    let mut seen_roots: HashSet<String> = HashSet::new();
    let mut add_root = |root: PathBuf| {
        let key = root.to_string_lossy().to_string().to_lowercase();
        if seen_roots.insert(key) {
            steam_roots.push(root);
        }
    };

    match home::home_dir() {
        Some(home) => {
            // Native Steam installations
            add_root(home.join(".local/share/Steam"));
            add_root(home.join(".steam/steam"));
            add_root(home.join(".steam/root"));
            add_root(home.join(".steam/debian-installation"));

            // Flatpak Steam installations - check all known variants
            add_root(home.join(".var/app/com.valvesoftware.Steam/data/Steam"));
            add_root(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
            add_root(home.join(".var/app/com.valvesoftware.Steam/.steam/steam"));
            add_root(home.join(".var/app/com.valvesoftware.Steam/.steam/root"));
            add_root(home.join(".var/app/com.valvesoftware.Steam/.steam/debian-installation"));

            // Snap installs keep the real Steam data under this prefix.
            add_root(home.join("snap/steam/common/.local/share/Steam"));

            debug!("Home directory: {:?}", home);
        }
        None => error!("Impossible to get your home dir!"),
    }

    // When running inside a Flatpak, also try the host home via /var/home or direct path
    // in case the sandbox path resolution differs
    if std::env::var("FLATPAK_ID").is_ok() {
        debug!("Running inside Flatpak sandbox, checking additional host paths");
        if let Ok(user) = std::env::var("USER") {
            let host_home = PathBuf::from("/var/home").join(&user);
            if host_home.exists() {
                add_root(host_home.join(".local/share/Steam"));
                add_root(host_home.join(".steam/steam"));
                add_root(host_home.join(".var/app/com.valvesoftware.Steam/data/Steam"));
            }
            // Also try /home directly (common Flatpak host mount)
            let direct_home = PathBuf::from("/home").join(&user);
            add_root(direct_home.join(".local/share/Steam"));
            add_root(direct_home.join(".steam/steam"));
            add_root(direct_home.join(".var/app/com.valvesoftware.Steam/data/Steam"));
        }
    }

    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        let xdg = PathBuf::from(xdg_data_home);
        add_root(xdg.join("Steam"));
        add_root(xdg.join("steam"));
    }

    add_root(PathBuf::from("/opt/steam"));
    add_root(PathBuf::from("/opt/Steam"));
    add_root(PathBuf::from("/usr/lib/steam"));
    add_root(PathBuf::from("/usr/local/steam"));
    add_root(PathBuf::from("/var/lib/steam"));

    let mut steamapps_dirs: Vec<PathBuf> = Vec::new();
    let mut seen_steamapps: HashSet<String> = HashSet::new();
    let mut add_steamapps = |path: PathBuf| {
        let key = path.to_string_lossy().to_string().to_lowercase();
        if seen_steamapps.insert(key) {
            steamapps_dirs.push(path);
        }
    };

    for root in steam_roots {
        add_steamapps(root.join("steamapps"));
    }

    if let Some(home) = home::home_dir() {
        add_steamapps(home.join(".steam/steamapps"));
        let bases = vec![
            home.join(".local/share"),
            home.join(".steam"),
            home.join(".var/app"),
            home.join("snap"),
            home.clone(),
        ];
        for base in bases {
            for steamapps in scan_for_steamapps_dirs(&base, 5, 2000) {
                add_steamapps(steamapps);
            }
        }
    }

    if let Ok(user) = std::env::var("USER") {
        let media_base = PathBuf::from("/run/media").join(user);
        for steamapps in scan_for_steamapps_dirs(&media_base, 4, 1500) {
            add_steamapps(steamapps);
        }
    }

    for steamapps in steamapps_dirs {
        paths.push(steamapps.join("common/Balatro"));

        let libraryfolders = steamapps.join("libraryfolders.vdf");
        if libraryfolders.exists() {
            paths.extend(parse_libraryfolders(&libraryfolders));
        }
    }

    remove_unexisting_paths(&mut paths);
    dedup_paths_case_insensitive(&mut paths);
    debug!("Found {} Balatro installations: {:?}", paths.len(), paths);
    paths
}

pub fn is_steam_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let system = System::new_all();
        let x = system
            .processes_by_exact_name(std::ffi::OsStr::new("steam.exe"))
            .next()
            .is_some();
        x
    }

    #[cfg(target_family = "unix")]
    {
        use libproc::proc_pid::name;
        use libproc::processes;

        if let Ok(pids) = processes::pids_by_type(processes::ProcFilter::All) {
            for pid in pids {
                if let Ok(name) = name(pid as i32)
                    && name.to_lowercase().contains("steam")
                {
                    return true;
                }
            }
        }
        false
    }
}

pub fn get_installed_mods() -> Vec<String> {
    let mut installed_mods_paths: Vec<PathBuf> = vec![];

    let mod_dir = crate::mods_dir();

    if !mod_dir.exists() {
        return vec![];
    }

    match mod_dir.read_dir() {
        Ok(read_dir) => {
            for entry in read_dir.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    installed_mods_paths.push(entry.path());
                }
            }
        }
        Err(e) => {
            error!("Failed to read mods directory {}: {}", mod_dir.display(), e);
            return vec![];
        }
    }

    installed_mods_paths
        .iter()
        .filter_map(|p| {
            // Filter out internal directories by exact folder name match (not substring)
            let folder_name = p.file_name()?.to_str()?.to_lowercase();
            if folder_name == ".lovely" || folder_name == "lovely" || folder_name == "bmm-compat" {
                return None;
            }
            p.to_str().map(|s| s.to_string())
        })
        .collect()
}

pub fn is_balatro_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let system = System::new_all();
        let exact = system
            .processes_by_exact_name(std::ffi::OsStr::new("Balatro.exe"))
            .next()
            .is_some();
        if exact {
            return true;
        }
        for process in system.processes().values() {
            let name = process.name().to_string_lossy().to_lowercase();
            if name.contains("balatro")
                && !name.contains("balatro-mod")
                && !name.contains("balatro mod")
                && !name.contains("balatro_mod")
            {
                return true;
            }
        }
        false
    }

    #[cfg(target_family = "unix")]
    {
        use libproc::proc_pid::name;
        use libproc::processes;

        let balatro_roots: Vec<_> = crate::finder::get_balatro_paths_cached()
            .into_iter()
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();
        #[cfg(target_os = "macos")]
        let balatro_exec_roots: Vec<_> = balatro_roots
            .iter()
            .map(|root| root.join("Balatro.app").join("Contents").join("MacOS"))
            .collect();

        let current_pid = std::process::id() as i32;
        let self_exe_path = std::env::current_exe().ok();
        let self_exe_name = self_exe_path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));

        if let Ok(pids) = processes::pids_by_type(processes::ProcFilter::All) {
            for pid in pids {
                let pid = pid as i32;
                if pid == current_pid {
                    continue;
                }

                if let Ok(name) = name(pid) {
                    let name_lower = name.to_lowercase();

                    #[cfg(target_os = "macos")]
                    {
                        if let Ok(proc_path) = libproc::proc_pid::pidpath(pid) {
                            let proc_path = PathBuf::from(proc_path);
                            if proc_path
                                .to_string_lossy()
                                .to_lowercase()
                                .contains("balatro.app")
                            {
                                return true;
                            }
                            for root in &balatro_exec_roots {
                                if proc_path.starts_with(root) {
                                    return true;
                                }
                            }
                        }
                    }

                    #[cfg(target_os = "linux")]
                    {
                        // Skip processes whose executable path matches our own (covers AppImage/wrapped launches).
                        if let (Some(self_path), Ok(proc_path)) = (
                            &self_exe_path,
                            std::fs::read_link(format!("/proc/{pid}/exe")),
                        ) && proc_path == *self_path
                        {
                            continue;
                        }
                    }

                    // Linux can truncate binary names; skip anything that matches our own executable name or obvious variants.
                    if let Some(self_name) = self_exe_name.as_deref()
                        && (name_lower == self_name
                            || self_name.starts_with(&name_lower)
                            || name_lower.starts_with(self_name))
                    {
                        continue;
                    }
                    if name_lower.contains("balatro-mod")
                        || name_lower.contains("balatro mod")
                        || name_lower.contains("balatro_mod")
                        || name_lower == "bmm"
                    {
                        continue;
                    }

                    if name_lower.contains("balatro") {
                        return true;
                    }
                    if name_lower == "love"
                        && let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd"))
                    {
                        let cwd = cwd.canonicalize().unwrap_or(cwd);
                        if balatro_roots.contains(&cwd) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use tempfile::tempdir;

    fn set_var(key: &str, val: impl AsRef<OsStr>) {
        unsafe { std::env::set_var(key, val) };
    }

    fn remove_var(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn test_get_installed_mods_filters_lovely_dirs() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        let tmp = tempdir().unwrap();
        // Redirect config dir to temp using XDG_CONFIG_HOME and HOME (for macOS)
        let original_cfg = std::env::var_os("XDG_CONFIG_HOME");
        let original_home = std::env::var_os("HOME");
        set_var("XDG_CONFIG_HOME", tmp.path());
        // Ensure HOME points at temp so path resolution doesn't touch real user dirs.
        set_var("HOME", tmp.path());

        let mods_root = dirs::config_dir().unwrap().join("Balatro").join("Mods");
        std::fs::create_dir_all(&mods_root).unwrap();

        // Create sample mod directories
        let keep_a = mods_root.join("CoolMod");
        let keep_b = mods_root.join("Another");
        let ignore_a = mods_root.join(".lovely");
        let ignore_b = mods_root.join("lovely");

        std::fs::create_dir_all(&keep_a).unwrap();
        std::fs::create_dir_all(&keep_b).unwrap();
        std::fs::create_dir_all(&ignore_a).unwrap();
        std::fs::create_dir_all(&ignore_b).unwrap();

        let mut mods = super::get_installed_mods();
        mods.sort();

        // Should only include the two non-lovely directories
        assert_eq!(mods.len(), 2);
        assert!(mods.iter().any(|p| p.ends_with("CoolMod")));
        assert!(mods.iter().any(|p| p.ends_with("Another")));

        // restore environment
        match original_cfg {
            Some(val) => set_var("XDG_CONFIG_HOME", val),
            None => remove_var("XDG_CONFIG_HOME"),
        }
        match original_home {
            Some(val) => set_var("HOME", val),
            None => remove_var("HOME"),
        }
    }
}
