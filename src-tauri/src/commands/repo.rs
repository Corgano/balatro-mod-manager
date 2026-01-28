use std::cmp::Ordering;
use std::path::PathBuf;

use crate::assets::{ensure_assets_dirs_async, safe_slug};
use crate::bmi::{self, BmiClient, SortMode, SyncCache};
use crate::lfs::{LfsError, resolve_lfs_pointer_bytes};
use crate::models::{ModDownloads, ModMeta};
use bmm_lib::errors::AppError;
use serde::{Deserialize, Serialize};

const BMI_CACHE_FILE: &str = "bmi_mods_cache";
const THUMB_CACHE_TTL_SECS: u64 = 60 * 60 * 24 * 7;

/// Unified mod details response - fetches all needed data in one API call
#[derive(Debug, Clone, Serialize)]
pub struct ModDetails {
    pub description: String,
    pub requires_steamodded: bool,
    pub requires_talisman: bool,
    pub repo_url: Option<String>,
}

/// Get all mod details in a single API call (description, requirements, repo URL)
/// This replaces 3 separate `fetch_mod` calls with 1.
#[tauri::command]
pub async fn get_mod_details(title: String, dir_name: String) -> Result<ModDetails, String> {
    let (_, descs_dir) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let desc_path = descs_dir.join(format!("{slug}.md"));

    // Check cached description first
    let cached_desc = if tokio::fs::metadata(&desc_path).await.is_ok() {
        let cached = tokio::fs::read_to_string(&desc_path).await.ok();
        cached.filter(|s| !s.trim().is_empty() && is_meaningful_description(s, &title))
    } else {
        None
    };

    // If we have cached description, we still need requirements and repo URL
    // But we can skip the API call if user doesn't need them
    let client = BmiClient::new()?;
    let detail = client.fetch_mod(&dir_name).await?;

    // Extract description
    let description = cached_desc.unwrap_or_else(|| {
        let text = match detail.description_html.as_deref() {
            Some(desc) if !desc.trim().is_empty() => desc.to_string(),
            _ => match detail.description.as_deref() {
                Some(desc) if !desc.trim().is_empty() => desc.to_string(),
                _ => detail.summary.clone().unwrap_or_default(),
            },
        };
        // Cache the description for next time
        let path = desc_path.clone();
        let text_clone = text.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::write(&path, &text_clone).await;
        });
        text
    });

    // Extract requirements
    let (requires_steamodded, requires_talisman) = bmi::derive_requires(&detail);

    // Extract repo URL
    let repo_url = detail
        .repo
        .clone()
        .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
        .or_else(|| {
            detail
                .homepage
                .clone()
                .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
        })
        .or_else(|| {
            detail.meta.and_then(|m| {
                if m.repo.trim().is_empty() {
                    None
                } else {
                    Some(m.repo)
                }
            })
        });

    Ok(ModDetails {
        description,
        requires_steamodded,
        requires_talisman,
        repo_url,
    })
}

#[tauri::command]
pub async fn list_repo_mods() -> Result<Vec<String>, String> {
    let items = fetch_repo_mods(None).await?;
    let mut out: Vec<String> = items.into_iter().map(|item| item.dir_name).collect();
    out.sort();
    Ok(out)
}

#[tauri::command]
pub async fn get_repo_file(path: &str) -> Result<String, String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 3 || parts[0] != "mods" {
        return Err("BMI repo file path unsupported".to_string());
    }
    let id = parts[1];
    let file = parts[2];
    let client = BmiClient::new()?;
    let detail = client.fetch_mod(id).await?;

    match file {
        "meta.json" => {
            let meta = detail
                .meta
                .ok_or_else(|| "BMI mod missing meta".to_string())?;
            serde_json::to_string(&meta).map_err(|e| e.to_string())
        }
        "description.md" => Ok(detail.description.unwrap_or_default()),
        _ => Err(format!("Unsupported BMI repo file: {file}")),
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub async fn get_repo_thumbnail_url(dirName: String) -> Result<Option<String>, String> {
    let client = BmiClient::new()?;
    let url = client.thumbnail_url(&dirName)?;
    Ok(Some(url))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveModItem {
    pub dir_name: String,
    pub meta: ModMeta,
    pub description: String,
    pub image_url: String,
    #[serde(default)]
    pub has_thumbnail: bool,
}

// Fetch mod metadata and descriptions via the BMI HTTP service.
#[tauri::command]
pub async fn fetch_repo_mods(sort: Option<String>) -> Result<Vec<ArchiveModItem>, String> {
    let client = BmiClient::new()?;
    client.check_health().await.map_err(|e| {
        let msg = format!("BMI health check failed: {e}");
        log::warn!("{msg}");
        msg
    })?;
    let sort_mode = SortMode::from_optional(sort.clone());
    let (cache_dir, cache_file) = bmi_cache_paths(sort.as_deref())?;
    let cache = load_bmi_cache(&cache_file).await;

    let (mut items, mut latest_updated) = if let Some(existing) = cache.as_ref() {
        (existing.items.clone(), existing.last_updated_at.clone())
    } else {
        (Vec::new(), None)
    };
    if items.iter().any(|item| {
        if !item.has_thumbnail {
            return false;
        }
        let url = item.image_url.trim();
        url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://"))
    }) {
        items.clear();
        latest_updated = None;
    }
    if items.iter().any(|item| item.meta.downloads.is_none()) {
        items.clear();
        latest_updated = None;
    }

    let force_full_refresh = matches!(sort_mode, SortMode::DownloadsAsc | SortMode::DownloadsDesc);
    if force_full_refresh || items.is_empty() || latest_updated.is_none() {
        let (fresh, updated_at) = client.fetch_all_mods(sort_mode).await.map_err(|e| {
            let msg = format!("BMI fetch_all_mods failed: {e}");
            log::warn!("{msg}");
            msg
        })?;
        items = fresh;
        latest_updated = updated_at;
    } else if let Some(since) = latest_updated.clone() {
        let (changed, updated_at) =
            client
                .fetch_changed_mods(&since, sort_mode)
                .await
                .map_err(|e| {
                    let msg = format!("BMI fetch_changed_mods failed: {e}");
                    log::warn!("{msg}");
                    msg
                })?;
        if !changed.is_empty() {
            items = bmi::apply_changed(&items, changed, &client)?;
        }
        latest_updated = bmi::pick_latest_updated(latest_updated, updated_at);
    }
    if !matches!(sort_mode, SortMode::DownloadsAsc | SortMode::DownloadsDesc) {
        let _ = client.refresh_downloads(&mut items, sort_mode).await;
        sort_archive_items(&mut items, sort_mode);
    }

    // Persist cache for future incremental sync
    let cache_state = SyncCache {
        last_updated_at: latest_updated,
        items: items.clone(),
    };
    let _ = tokio::fs::create_dir_all(&cache_dir).await;
    if let Ok(json) = serde_json::to_string_pretty(&cache_state) {
        let _ = tokio::fs::write(&cache_file, json).await;
    }

    Ok(items)
}

#[tauri::command]
pub async fn fetch_repo_downloads(
    sort: Option<String>,
) -> Result<std::collections::HashMap<String, ModDownloads>, String> {
    let client = BmiClient::new()?;
    client.check_health().await?;
    let sort_mode = SortMode::from_optional(sort);
    client.fetch_downloads_map(sort_mode).await
}

fn bmi_cache_paths(sort: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
    let cache_dir = config_dir.join("Balatro").join("mod_index_cache");
    let suffix = sort
        .unwrap_or("default")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let cache_file = cache_dir.join(format!("{BMI_CACHE_FILE}_{suffix}.json"));
    Ok((cache_dir, cache_file))
}

async fn load_bmi_cache(cache_file: &PathBuf) -> Option<SyncCache> {
    tokio::fs::read_to_string(cache_file)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<SyncCache>(&s).ok())
}

fn sort_archive_items(items: &mut [ArchiveModItem], sort_mode: SortMode) {
    match sort_mode {
        SortMode::NameAsc => items.sort_by(|a, b| {
            let a_key = a.meta.title.to_lowercase();
            let b_key = b.meta.title.to_lowercase();
            match a_key.cmp(&b_key) {
                Ordering::Equal => a.dir_name.cmp(&b.dir_name),
                other => other,
            }
        }),
        SortMode::NameDesc => items.sort_by(|a, b| {
            let a_key = a.meta.title.to_lowercase();
            let b_key = b.meta.title.to_lowercase();
            match b_key.cmp(&a_key) {
                Ordering::Equal => a.dir_name.cmp(&b.dir_name),
                other => other,
            }
        }),
        SortMode::UpdatedAsc => {
            items.sort_by(|a, b| match a.meta.last_updated.cmp(&b.meta.last_updated) {
                Ordering::Equal => a.dir_name.cmp(&b.dir_name),
                other => other,
            })
        }
        SortMode::UpdatedDesc => {
            items.sort_by(|a, b| match b.meta.last_updated.cmp(&a.meta.last_updated) {
                Ordering::Equal => a.dir_name.cmp(&b.dir_name),
                other => other,
            })
        }
        SortMode::DownloadsAsc | SortMode::DownloadsDesc => {}
    }
}

async fn is_thumb_fresh_async(path: &std::path::Path) -> bool {
    let modified = match tokio::fs::metadata(path).await.and_then(|m| m.modified()) {
        Ok(ts) => ts,
        Err(_) => return false,
    };
    let age = match std::time::SystemTime::now().duration_since(modified) {
        Ok(d) => d,
        Err(_) => return false,
    };
    age.as_secs() < THUMB_CACHE_TTL_SECS
}

#[tauri::command]
pub async fn get_cached_thumbnail_by_title(title: String) -> Result<Option<String>, String> {
    let (thumbs_dir, _) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if tokio::fs::metadata(&path).await.is_err() {
        return Ok(None);
    }
    if !is_thumb_fresh_async(&path).await {
        let _ = tokio::fs::remove_file(&path).await;
        return Ok(None);
    }
    Ok(Some(
        path.to_str()
            .ok_or_else(|| format!("Failed to convert thumbnail path: {}", path.display()))?
            .to_string(),
    ))
}

/// Return a map of title -> cached thumbnail path for the given titles (if present).
#[tauri::command]
pub async fn get_cached_thumbnails_map(
    titles: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let (thumbs_dir, _) = ensure_assets_dirs_async().await?;
    let mut out = std::collections::HashMap::new();
    for title in titles {
        let slug = safe_slug(&title);
        let path = thumbs_dir.join(format!("{slug}.jpg"));
        let exists = tokio::fs::metadata(&path).await.is_ok();
        if exists && is_thumb_fresh_async(&path).await {
            if let Some(s) = path.to_str() {
                out.insert(title, s.to_string());
            }
        } else if exists {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn cache_thumbnail_from_url(
    title: String,
    url: String,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<bool, String> {
    // If present, no-op quickly
    let (thumbs_dir, _) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if tokio::fs::metadata(&path).await.is_ok() {
        if is_thumb_fresh_async(&path).await {
            return Ok(false);
        }
        let _ = tokio::fs::remove_file(&path).await;
    }

    // Enqueue background fetch with 429-aware backoff; return immediately
    log::info!("Thumbnail enqueue: title='{}' url='{}'", title, url);
    state.thumbs.enqueue(title, url);
    Ok(false)
}

#[tauri::command]
pub async fn get_cached_installed_thumbnail(
    title: String,
    dir_name: String,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Option<String>, String> {
    let installed = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installed_mods()
            .map_err(|e| e.to_string())?
            .into_iter()
            .any(|m| m.name.eq_ignore_ascii_case(&title))
    };
    if !installed {
        return Ok(None);
    }

    let (thumbs_dir, _) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if tokio::fs::metadata(&path).await.is_ok() {
        if !is_thumb_fresh_async(&path).await {
            let _ = tokio::fs::remove_file(&path).await;
        } else {
            return Ok(Some(
                path.to_str()
                    .ok_or_else(|| "Failed to convert thumbnail path".to_string())?
                    .to_string(),
            ));
        }
    }

    // Not cached yet: try to download from repo raw and store.
    let client = BmiClient::new()?;
    let url = client.thumbnail_url(&dir_name)?;
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    match client.get_bytes(parsed).await {
        Ok(bytes) => {
            let bytes = match resolve_lfs_pointer_bytes(client.http_client(), bytes).await {
                Ok(resolved) => resolved,
                Err(LfsError::Retryable(_)) => {
                    state.thumbs.enqueue(title.clone(), url.to_string());
                    return Ok(None);
                }
                Err(_) => return Ok(None),
            };
            tokio::fs::write(&path, &bytes).await.map_err(|e| {
                AppError::FileWrite {
                    path: path.clone(),
                    source: e.to_string(),
                }
                .to_string()
            })?;
            return Ok(Some(
                path.to_str()
                    .ok_or_else(|| "Failed to convert thumbnail path".to_string())?
                    .to_string(),
            ));
        }
        Err(_) => {
            // Defer retry to the background queue for rate limits or transient failures.
            state.thumbs.enqueue(title.clone(), url.to_string());
        }
    }
    Ok(None)
}

#[tauri::command]
pub async fn get_description_cached_or_remote(
    title: String,
    dir_name: String,
    _state: tauri::State<'_, crate::state::AppState>,
) -> Result<String, String> {
    let (_, descs_dir) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let path = descs_dir.join(format!("{slug}.md"));

    // Always prefer cached copy if present
    if tokio::fs::metadata(&path).await.is_ok() {
        let cached = tokio::fs::read_to_string(&path).await.map_err(|e| {
            AppError::FileRead {
                path: path.clone(),
                source: e.to_string(),
            }
            .to_string()
        })?;
        if !cached.trim().is_empty() && is_meaningful_description(&cached, &title) {
            return Ok(cached);
        }
    }

    let client = BmiClient::new()?;
    log::info!("Description fetch: title='{}' id='{}'", title, dir_name);
    let detail = client.fetch_mod(&dir_name).await?;
    let text = match detail.description_html.as_deref() {
        Some(desc) if !desc.trim().is_empty() => desc.to_string(),
        _ => match detail.description.as_deref() {
            Some(desc) if !desc.trim().is_empty() => desc.to_string(),
            _ => detail.summary.clone().unwrap_or_default(),
        },
    };
    if let Err(e) = tokio::fs::write(&path, &text).await {
        log::warn!("Failed to cache description for {}: {}", title, e);
    }
    log::info!("Description loaded: title='{}' len={}", title, text.len());
    if text.trim().is_empty() {
        log::warn!("Description empty after fetch: title='{}'", title);
    }
    Ok(text)
}

fn is_meaningful_description(text: &str, title: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let normalized_text = normalize_plaintext(trimmed);
    if normalized_text.is_empty() {
        return false;
    }
    if normalized_text.len() < 24 {
        return false;
    }
    let normalized_title = normalize_plaintext(title);
    normalized_text != normalized_title
}

fn normalize_plaintext(text: &str) -> String {
    let cleaned = strip_markdown_images_and_links(text);
    let mut out = String::with_capacity(cleaned.len());
    let mut in_tag = false;
    for ch in cleaned.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => continue,
            _ => out.push(ch),
        }
    }
    out.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_markdown_images_and_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '!' && chars.peek() == Some(&'[') {
            chars.next();
            let mut alt = String::new();
            let mut found_bracket = false;
            for c in chars.by_ref() {
                if c == ']' {
                    found_bracket = true;
                    break;
                }
                alt.push(c);
            }
            if found_bracket && chars.peek() == Some(&'(') {
                chars.next();
                let mut found_paren = false;
                for c in chars.by_ref() {
                    if c == ')' {
                        found_paren = true;
                        break;
                    }
                }
                if found_paren {
                    continue;
                }
            }
            out.push('!');
            out.push('[');
            out.push_str(&alt);
            if found_bracket {
                out.push(']');
            }
            continue;
        }
        if ch == '[' {
            let mut label = String::new();
            let mut found_bracket = false;
            for c in chars.by_ref() {
                if c == ']' {
                    found_bracket = true;
                    break;
                }
                label.push(c);
            }
            if found_bracket && chars.peek() == Some(&'(') {
                chars.next();
                let mut found_paren = false;
                for c in chars.by_ref() {
                    if c == ')' {
                        found_paren = true;
                        break;
                    }
                }
                if found_paren {
                    out.push_str(&label);
                    continue;
                }
            }
            out.push('[');
            out.push_str(&label);
            if found_bracket {
                out.push(']');
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[tauri::command]
pub async fn get_mod_requirements(dir_name: String) -> Result<(bool, bool), String> {
    let client = BmiClient::new()?;
    let detail = client.fetch_mod(&dir_name).await?;
    Ok(bmi::derive_requires(&detail))
}

#[tauri::command]
pub async fn get_mod_repo_url(dir_name: String) -> Result<Option<String>, String> {
    let client = BmiClient::new()?;
    let detail = client.fetch_mod(&dir_name).await?;
    let repo = detail
        .repo
        .clone()
        .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
        .or_else(|| {
            detail
                .homepage
                .clone()
                .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
        })
        .or_else(|| {
            detail.meta.and_then(|m| {
                if m.repo.trim().is_empty() {
                    None
                } else {
                    Some(m.repo)
                }
            })
        });
    Ok(repo)
}

#[tauri::command]
pub async fn get_cached_description_by_title(title: String) -> Result<Option<String>, String> {
    let (_, descs_dir) = ensure_assets_dirs_async().await?;
    let slug = safe_slug(&title);
    let path = descs_dir.join(format!("{slug}.md"));
    if tokio::fs::metadata(&path).await.is_err() {
        return Ok(None);
    }
    let text = tokio::fs::read_to_string(&path).await.map_err(|e| {
        AppError::FileRead {
            path: path.clone(),
            source: e.to_string(),
        }
        .to_string()
    })?;
    Ok(Some(text))
}

// ============ Batch Thumbnails (Raw URLs) ============

#[derive(Debug, Clone, Deserialize)]
pub struct ModThumbInput {
    pub title: String,
    pub dir_name: String,
}

#[tauri::command]
pub async fn batch_fetch_thumbnails_repo(inputs: Vec<ModThumbInput>) -> Result<u32, String> {
    use futures::{StreamExt, stream};

    // Ensure output directory exists early
    let (thumbs_dir, _) = ensure_assets_dirs_async().await?;

    // Filter out inputs already cached (async check)
    let mut pending: Vec<ModThumbInput> = Vec::new();
    for m in inputs {
        let slug = safe_slug(&m.title);
        let path = thumbs_dir.join(format!("{slug}.jpg"));
        if tokio::fs::metadata(&path).await.is_err() {
            pending.push(m);
        }
    }
    if pending.is_empty() {
        return Ok(0);
    }

    let client = BmiClient::new()?;

    let concurrency = 12usize;
    let saved = stream::iter(pending.into_iter())
        .map(|m| {
            let client = client.clone();
            let thumbs_dir = thumbs_dir.clone();
            async move {
                let url = match client.thumbnail_url(&m.dir_name) {
                    Ok(url) => url,
                    Err(_) => return 0u32,
                };
                let parsed = match reqwest::Url::parse(&url) {
                    Ok(url) => url,
                    Err(_) => return 0u32,
                };
                if let Ok(bytes) = client.get_bytes(parsed).await
                    && let Ok(bytes) = resolve_lfs_pointer_bytes(client.http_client(), bytes).await
                {
                    let slug = safe_slug(&m.title);
                    let path = thumbs_dir.join(format!("{slug}.jpg"));
                    if tokio::fs::write(&path, &bytes).await.is_ok() {
                        return 1u32;
                    }
                }
                0u32
            }
        })
        .buffer_unordered(concurrency)
        .fold(0u32, |acc, n| async move { acc + n })
        .await;

    Ok(saved)
}
