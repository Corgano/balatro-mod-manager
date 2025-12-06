use std::path::PathBuf;

use crate::models::ModMeta;
use base64::Engine as _;
use bmm_lib::errors::AppError;
use serde::{Deserialize, Serialize};

const GITLAB_PROJECT: &str = "balatro-mod-index/repo";
const GITLAB_BASE: &str = "https://gitlab.com/balatro-mod-index/repo";
const GITLAB_RAW_MAIN: &str = "https://gitlab.com/balatro-mod-index/repo/-/raw/main";
const GITLAB_RAW_MASTER: &str = "https://gitlab.com/balatro-mod-index/repo/-/raw/master";
const GITLAB_LFS_BATCH: &str =
    "https://gitlab.com/balatro-mod-index/repo.git/info/lfs/objects/batch";

#[derive(Deserialize)]
struct GitLabTreeEntry {
    name: String,
    #[allow(dead_code)]
    r#type: String,
}

#[tauri::command]
pub async fn list_gitlab_mods() -> Result<Vec<String>, String> {
    let project = urlencoding::encode(GITLAB_PROJECT);
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/tree?path=mods&ref=main&per_page=500",
        project
    );
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("GitLab API error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitLab API status: {}", resp.status()));
    }
    let entries: Vec<GitLabTreeEntry> = resp
        .json()
        .await
        .map_err(|e| format!("Parse GitLab tree failed: {}", e))?;
    Ok(entries
        .into_iter()
        .filter(|e| e.r#type == "tree")
        .map(|e| e.name)
        .collect())
}

#[tauri::command]
pub async fn get_gitlab_file(path: &str) -> Result<String, String> {
    use tokio::time::{Duration, sleep};

    // Encode path by segments so slashes remain
    let encoded: String = path
        .split('/')
        .map(urlencoding::encode)
        .map(|s| s.into_owned())
        .collect::<Vec<_>>()
        .join("/");

    let urls = [
        format!("{}/-/raw/main/{}", GITLAB_BASE, encoded),
        format!("{}/-/raw/master/{}", GITLAB_BASE, encoded),
    ];

    let client = reqwest::Client::new();
    let mut delay = Duration::from_millis(250);
    for attempt in 0..4 {
        for u in &urls {
            let resp = client.get(u).send().await.map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                return resp.text().await.map_err(|e| e.to_string());
            }
            let code = resp.status().as_u16();
            // 404/410: not found — no point retrying this URL
            if code == 404 || code == 410 {
                continue;
            }
            // 429/5xx: temporary, retry after delay
        }
        if attempt < 3 {
            sleep(delay).await;
            delay = delay.saturating_mul(2);
        }
    }
    Err(format!("Failed to fetch {} after retries", path))
}

#[allow(non_snake_case)]
#[tauri::command]
pub async fn get_gitlab_thumbnail_url(dirName: String) -> Result<Option<String>, String> {
    // Try unencoded then encoded, on main then master
    let enc = urlencoding::encode(&dirName);
    let candidates = [
        format!("{}/-/raw/main/mods/{}/thumbnail.jpg", GITLAB_BASE, dirName),
        format!("{}/-/raw/main/mods/{}/thumbnail.jpg", GITLAB_BASE, enc),
        format!(
            "{}/-/raw/master/mods/{}/thumbnail.jpg",
            GITLAB_BASE, dirName
        ),
        format!("{}/-/raw/master/mods/{}/thumbnail.jpg", GITLAB_BASE, enc),
    ];

    use reqwest::header::RANGE;
    let client = reqwest::Client::new();
    for url in candidates {
        let resp = client
            .get(&url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            return Ok(Some(url));
        }
    }
    Ok(None)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveModItem {
    pub dir_name: String,
    pub meta: ModMeta,
    pub description: String,
    pub image_url: String,
}

// Fetch all mod metadata and descriptions via a single GitLab archive request.
// LFS images are not included (only pointer files), so we avoid downloading thumbnails here.
#[tauri::command]
pub async fn fetch_gitlab_mods_archive() -> Result<Vec<ArchiveModItem>, String> {
    use flate2::read::GzDecoder;
    use std::collections::HashMap;
    use std::io::Read;
    use std::time::Instant;
    use tar::Archive;

    // Cache location in config dir (created lazily when writing)
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
    let cache_dir = config_dir.join("Balatro").join("mod_index_cache");
    let cache_file = cache_dir.join("mod_index_archive.json");

    #[derive(Serialize, Deserialize)]
    struct ArchiveCache {
        etag: Option<String>,
        branch: String,
        items: Vec<ArchiveModItem>,
    }

    // Try to load existing cache to get ETag and for offline fallback
    let mut existing_cache: Option<ArchiveCache> = std::fs::File::open(&cache_file)
        .ok()
        .and_then(|f| serde_json::from_reader::<_, ArchiveCache>(f).ok());

    // Build archive URL for main branch; fall back to master if needed.
    let project = urlencoding::encode(GITLAB_PROJECT);
    // Limit archive to the `mods` directory only to reduce size significantly
    let urls = [
        format!(
            "https://gitlab.com/api/v4/projects/{}/repository/archive.tar.gz?sha=main&path=mods",
            project
        ),
        format!(
            "https://gitlab.com/api/v4/projects/{}/repository/archive.tar.gz?sha=master&path=mods",
            project
        ),
    ];

    // Download archive stream
    use reqwest::header::IF_NONE_MATCH;
    let client = reqwest::Client::new();
    let mut used_branch: Option<&'static str> = None;
    let mut received_etag: Option<String> = None;
    let mut response_stream: Option<reqwest::Response> = None;
    for (idx, u) in urls.iter().enumerate() {
        let mut req = client.get(u.clone());
        if let Some(c) = &existing_cache
            && let Some(et) = &c.etag
        {
            req = req.header(IF_NONE_MATCH, et);
        }
        match req.send().await {
            Ok(resp) => {
                if resp.status().as_u16() == 304 {
                    if let Some(c) = existing_cache {
                        log::info!(
                            "GitLab archive 304 Not Modified (branch: {}), using cached items: {}",
                            c.branch,
                            c.items.len()
                        );
                        return Ok(c.items);
                    }
                    // No cache to use; try next URL branch without ETag
                    existing_cache = None;
                    continue;
                }
                if resp.status().is_success() {
                    if let Some(et) = resp.headers().get(reqwest::header::ETAG)
                        && let Ok(s) = et.to_str()
                    {
                        received_etag = Some(s.to_string());
                    }
                    used_branch = Some(if idx == 0 { "main" } else { "master" });
                    response_stream = Some(resp);
                    break;
                } else {
                    log::debug!("GitLab archive status {} for {}", resp.status(), u);
                    continue;
                }
            }
            Err(e) => {
                log::debug!("GitLab archive request error for {}: {}", u, e);
                continue;
            }
        }
    }

    let resp = match response_stream {
        Some(r) => r,
        None => return Err("Failed to fetch GitLab archive for main/master".into()),
    };

    // Stream download to a temp file to avoid holding the full archive in memory.
    let tmp_path = cache_dir.join("mods_archive.tmp");
    {
        use tokio::io::AsyncWriteExt;
        use tokio_stream::StreamExt;
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("Failed to create temp archive file: {e}"))?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("Download error: {e}"))?;
            file.write_all(&bytes)
                .await
                .map_err(|e| format!("Failed writing archive: {e}"))?;
        }
        file.sync_all()
            .await
            .map_err(|e| format!("Failed to flush archive: {e}"))?;
    }

    // Decompress and iterate entries from disk
    let parse_start = Instant::now();
    let file =
        std::fs::File::open(&tmp_path).map_err(|e| format!("Failed to reopen archive: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    // Collect per-mod pieces we care about
    #[derive(Default)]
    struct Parts {
        meta: Option<ModMeta>,
        desc: Option<String>,
    }
    let mut map: HashMap<String, Parts> = HashMap::new();

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = match entry.path() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Expect paths like: <top> / mods / <dir> / (meta.json|description.md)
        let mut comps = path.components();
        match comps.next() {
            Some(_) => (),
            None => continue,
        };
        let mods_dir = match comps.next() {
            Some(c) => c,
            None => continue,
        };
        if mods_dir.as_os_str() != "mods" {
            continue;
        }
        let dir_os = match comps.next() {
            Some(c) => c.as_os_str().to_owned(),
            None => continue,
        };
        let dir_name = match dir_os.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let filename = match comps.next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => continue,
        };

        if filename == "meta.json" {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            if let Ok(meta) = serde_json::from_str::<ModMeta>(&buf) {
                map.entry(dir_name).or_default().meta = Some(meta);
            }
        } else if filename == "description.md" {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            map.entry(dir_name).or_default().desc = Some(buf);
        } else {
            // Skip other files (including images/LFS pointers)
            continue;
        }
    }

    // Build result. Use branch-based raw URL for thumbnails (no extra requests).
    let image_branch = used_branch.unwrap_or("main"); // build URLs against the branch we fetched
    let mut out: Vec<ArchiveModItem> = Vec::with_capacity(map.len());
    for (dir, parts) in map.into_iter() {
        if let Some(meta) = parts.meta {
            let description = parts.desc.unwrap_or_default();
            let image_url = format!(
                "{}/-/raw/{}/mods/{}/thumbnail.jpg",
                GITLAB_BASE, image_branch, dir
            );
            out.push(ArchiveModItem {
                dir_name: dir,
                meta,
                description,
                image_url,
            });
        }
    }

    // Sort by title asc for stability
    out.sort_by(|a, b| {
        a.meta
            .title
            .to_lowercase()
            .cmp(&b.meta.title.to_lowercase())
    });

    log::info!(
        "Parsed {} mods from archive in {} ms",
        out.len(),
        parse_start.elapsed().as_millis()
    );

    // Save cache with ETag for future 304 validations
    let cache = ArchiveCache {
        etag: received_etag,
        branch: image_branch.to_string(),
        items: out.clone(),
    };
    // Ensure directory exists before writing
    let _ = std::fs::create_dir_all(&cache_dir);
    if let Ok(f) = std::fs::File::create(&cache_file) {
        let _ = serde_json::to_writer_pretty(f, &cache);
    }

    // Clean up temp archive
    let _ = std::fs::remove_file(&tmp_path);

    Ok(out)
}

#[derive(Serialize, Deserialize)]
struct IndexFileV1 {
    version: Option<u32>,
    mods: Vec<ArchiveModItem>,
}

// Try a lightweight index.json first; fall back to archive if missing.
#[tauri::command]
pub async fn fetch_gitlab_mods() -> Result<Vec<ArchiveModItem>, String> {
    use reqwest::header::{ETAG, IF_NONE_MATCH};

    // Cache location (created lazily only when writing)
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
    let cache_dir = config_dir.join("Balatro").join("mod_index_cache");
    let cache_file = cache_dir.join("index_v1.json");
    let etag_file = cache_dir.join("index_v1.etag");

    let urls = [
        format!("{}/index.json", GITLAB_RAW_MAIN),
        format!("{}/index.json", GITLAB_RAW_MASTER),
    ];

    let client = reqwest::Client::new();
    let mut etag: Option<String> = std::fs::read_to_string(&etag_file).ok();

    for url in urls {
        let mut req = client.get(&url);
        if let Some(ref v) = etag {
            req = req.header(IF_NONE_MATCH, v);
        }
        match req.send().await {
            Ok(resp) => {
                if resp.status().as_u16() == 304 {
                    if let Ok(f) = std::fs::File::open(&cache_file)
                        && let Ok(parsed) = serde_json::from_reader::<_, IndexFileV1>(f)
                    {
                        return Ok(parsed.mods);
                    }
                    // No cache to use; try next URL without etag
                    etag = None;
                    continue;
                }
                if resp.status().is_success() {
                    let new_etag = resp
                        .headers()
                        .get(ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                    let parsed: IndexFileV1 = serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Failed to parse index.json: {}", e))?;
                    // Save cache and etag (ensure directory exists)
                    let _ = std::fs::create_dir_all(&cache_dir);
                    if let Ok(f) = std::fs::File::create(&cache_file) {
                        let _ = serde_json::to_writer_pretty(f, &parsed);
                    }
                    if let Some(et) = new_etag {
                        let _ = std::fs::create_dir_all(&cache_dir);
                        let _ = std::fs::write(&etag_file, et);
                    }
                    return Ok(parsed.mods);
                }
            }
            Err(_) => continue,
        }
    }

    // Fallback: meta-only (fast, multi-request) then archive (single, heavy)
    match fetch_gitlab_mods_meta_only().await {
        Ok(items) if !items.is_empty() => Ok(items),
        _ => fetch_gitlab_mods_archive().await,
    }
}

#[tauri::command]
pub async fn fetch_gitlab_mods_meta_only() -> Result<Vec<ArchiveModItem>, String> {
    use std::time::Instant;
    let t0 = Instant::now();
    // 1) List mod directories via GitLab API (main, then master)
    #[derive(Deserialize)]
    struct GitLabTreeEntry {
        name: String,
        r#type: String,
    }

    let client = reqwest::Client::new();
    let project = urlencoding::encode(GITLAB_PROJECT);
    let mut branch_used: Option<&'static str> = None;
    let mut mod_dirs: Vec<String> = Vec::new();
    for (i, branch) in ["main", "master"].iter().enumerate() {
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/tree?path=mods&ref={}&per_page=500",
            project, branch
        );
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let entries: Vec<GitLabTreeEntry> = resp.json().await.map_err(|e| e.to_string())?;
                mod_dirs = entries
                    .into_iter()
                    .filter(|e| e.r#type == "tree")
                    .map(|e| e.name)
                    .collect();
                branch_used = Some(if i == 0 { "main" } else { "master" });
                break;
            }
            _ => continue,
        }
    }
    if mod_dirs.is_empty() {
        return Err("No mod directories found from GitLab API".into());
    }
    let image_branch = branch_used.unwrap_or("main");

    // 2) Concurrently fetch only meta.json for each mod (description is deferred)
    use futures::{StreamExt, stream};
    let concurrency = 12usize;
    let results = stream::iter(mod_dirs.into_iter())
        .map(|dir| {
            let client = client.clone();
            let dir_clone = dir.clone();
            async move {
                // Try main then master for meta.json
                let paths = [
                    format!(
                        "{}/mods/{}/meta.json",
                        GITLAB_RAW_MAIN,
                        urlencoding::encode(&dir_clone)
                    ),
                    format!(
                        "{}/mods/{}/meta.json",
                        GITLAB_RAW_MASTER,
                        urlencoding::encode(&dir_clone)
                    ),
                ];
                for url in &paths {
                    if let Ok(resp) = client.get(url).send().await
                        && resp.status().is_success()
                        && let Ok(text) = resp.text().await
                        && let Ok(meta) = serde_json::from_str::<ModMeta>(&text)
                    {
                        let image_url = format!(
                            "{}/mods/{}/thumbnail.jpg",
                            if image_branch == "main" {
                                GITLAB_RAW_MAIN
                            } else {
                                GITLAB_RAW_MASTER
                            },
                            urlencoding::encode(&dir_clone)
                        );
                        return Some(ArchiveModItem {
                            dir_name: dir_clone,
                            meta,
                            description: String::new(), // defer heavy descriptions
                            image_url,
                        });
                    }
                }
                None
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

    let mut items: Vec<ArchiveModItem> = results.into_iter().flatten().collect();
    items.sort_by(|a, b| {
        a.meta
            .title
            .to_lowercase()
            .cmp(&b.meta.title.to_lowercase())
    });
    log::info!(
        "Fetched meta for {} mods via API in {} ms",
        items.len(),
        t0.elapsed().as_millis()
    );
    Ok(items)
}

fn is_legal_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '('
                | ')'
                | '+'
                | ','
                | '-'
                | '='
                | ';'
                | '@'
                | '['
                | ']'
                | '^'
                | '_'
                | '`'
                | '{'
                | '}'
                | '~'
        )
}

fn safe_slug(input: &str) -> String {
    let mut s = input.trim().to_lowercase();
    s = s
        .chars()
        .map(|c| if is_legal_char(c) { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
}

fn ensure_assets_dirs() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        AppError::DirNotFound(std::path::PathBuf::from("config directory")).to_string()
    })?;
    let base = config_dir.join("Balatro").join("mod_assets");
    let thumbs = base.join("thumbnails");
    let descs = base.join("descriptions");
    std::fs::create_dir_all(&thumbs).map_err(|e| {
        AppError::DirCreate {
            path: thumbs.clone(),
            source: e.to_string(),
        }
        .to_string()
    })?;
    std::fs::create_dir_all(&descs).map_err(|e| {
        AppError::DirCreate {
            path: descs.clone(),
            source: e.to_string(),
        }
        .to_string()
    })?;
    Ok((thumbs, descs))
}

#[tauri::command]
pub async fn get_cached_thumbnail_by_title(title: String) -> Result<Option<String>, String> {
    let (thumbs_dir, _) = ensure_assets_dirs()?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(
        path.to_str()
            .ok_or_else(|| format!("Failed to convert thumbnail path: {}", path.display()))?
            .to_string(),
    ))
}

#[tauri::command]
pub async fn cache_thumbnail_from_url(
    title: String,
    url: String,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<bool, String> {
    // If present, no-op quickly
    let (thumbs_dir, _) = ensure_assets_dirs()?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if path.exists() {
        return Ok(false);
    }

    // Enqueue background fetch with 429-aware backoff; return immediately
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
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_installed_mods()
            .map_err(|e| e.to_string())?
            .into_iter()
            .any(|m| m.name.eq_ignore_ascii_case(&title))
    };
    if !installed {
        return Ok(None);
    }

    let (thumbs_dir, _) = ensure_assets_dirs()?;
    let slug = safe_slug(&title);
    let path = thumbs_dir.join(format!("{slug}.jpg"));
    if path.exists() {
        return Ok(Some(
            path.to_str()
                .ok_or_else(|| "Failed to convert thumbnail path".to_string())?
                .to_string(),
        ));
    }

    // Not cached yet: try to download from GitLab raw and store.
    let client = reqwest::Client::new();
    let enc = urlencoding::encode(&dir_name);
    let candidates = [
        format!("{}/mods/{}/thumbnail.jpg", GITLAB_RAW_MAIN, enc),
        format!("{}/mods/{}/thumbnail.jpg", GITLAB_RAW_MASTER, enc),
    ];
    for url in &candidates {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    // Persist and return
                    std::fs::write(&path, &bytes).map_err(|e| {
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
            } else if resp.status().as_u16() == 429 {
                // Handle rate limiting in the background; keep UI unblocked
                state.thumbs.enqueue(title.clone(), url.to_string());
            }
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
    let (_, descs_dir) = ensure_assets_dirs()?;
    let slug = safe_slug(&title);
    let path = descs_dir.join(format!("{slug}.md"));

    // Always prefer cached copy if present
    if path.exists() {
        return std::fs::read_to_string(&path).map_err(|e| {
            AppError::FileRead {
                path,
                source: e.to_string(),
            }
            .to_string()
        });
    }

    // Fetch from GitLab raw (main then master)
    let client = reqwest::Client::new();
    let enc = urlencoding::encode(&dir_name);
    let candidates = [
        format!("{}/mods/{}/description.md", GITLAB_RAW_MAIN, enc),
        format!("{}/mods/{}/description.md", GITLAB_RAW_MASTER, enc),
    ];
    for url in &candidates {
        if let Ok(resp) = client.get(url).send().await
            && resp.status().is_success()
            && let Ok(text) = resp.text().await
        {
            // Cache for future sessions regardless of install state
            if let Err(e) = std::fs::write(&path, &text) {
                log::warn!("Failed to cache description for {}: {}", title, e);
            }
            return Ok(text);
        }
    }
    Err(format!("Description not found for {}", dir_name))
}

#[tauri::command]
pub async fn get_cached_description_by_title(title: String) -> Result<Option<String>, String> {
    let (_, descs_dir) = ensure_assets_dirs()?;
    let slug = safe_slug(&title);
    let path = descs_dir.join(format!("{slug}.md"));
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| {
        AppError::FileRead {
            path: path.clone(),
            source: e.to_string(),
        }
        .to_string()
    })?;
    Ok(Some(text))
}

// ============ LFS Batch Thumbnails ============

#[derive(Debug, Clone, Deserialize)]
pub struct ModThumbInput {
    pub title: String,
    pub dir_name: String,
}

#[derive(Deserialize)]
struct GitLabFileContent {
    content: String,
    encoding: String,
}

#[derive(Serialize)]
struct LfsBatchReq<'a> {
    operation: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    transfers: Option<&'a [&'a str]>,
    objects: Vec<LfsObjectSpec>,
}

#[derive(Serialize, Deserialize, Clone)]
struct LfsObjectSpec {
    oid: String,
    size: u64,
}

#[derive(Deserialize)]
struct LfsBatchResp {
    #[allow(dead_code)]
    transfer: Option<String>,
    objects: Vec<LfsObjectResp>,
}

#[derive(Deserialize)]
struct LfsObjectResp {
    oid: String,
    size: u64,
    #[serde(default)]
    actions: Option<LfsObjActions>,
}

#[derive(Deserialize, Clone)]
struct LfsObjActions {
    download: LfsAction,
}

#[derive(Deserialize, Clone)]
struct LfsAction {
    href: String,
    #[serde(default)]
    header: Option<std::collections::HashMap<String, String>>,
}

fn parse_lfs_pointer(ptr_text: &str) -> Option<(String, u64)> {
    let mut oid: Option<String> = None;
    let mut size: Option<u64> = None;
    for line in ptr_text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("oid ") {
            // Accept formats like: "sha256:<hex>" or already just the hex
            let val = rest.trim();
            if let Some(h) = val.strip_prefix("sha256:") {
                oid = Some(h.to_string());
            } else {
                oid = Some(val.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("size ")
            && let Ok(n) = rest.trim().parse::<u64>()
        {
            size = Some(n);
        }
    }
    match (oid, size) {
        (Some(o), Some(s)) => Some((o, s)),
        _ => None,
    }
}

async fn fetch_pointer_for_thumb(
    client: &reqwest::Client,
    project_enc: &str,
    dir_name: &str,
) -> Option<(String, u64)> {
    // file_path must encode slashes (i.e., use one-shot encode of full path)
    let file_path = format!("mods/{}/thumbnail.jpg", dir_name);
    let file_enc = urlencoding::encode(&file_path);
    let branches = ["main", "master"]; // try main first, then master
    for b in branches {
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/files/{}/?ref={}",
            project_enc, file_enc, b
        );
        if let Ok(resp) = client.get(&url).send().await
            && resp.status().is_success()
            && let Ok(meta) = resp.json::<GitLabFileContent>().await
            && meta.encoding.to_lowercase() == "base64"
            && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(meta.content)
            && let Ok(text) = String::from_utf8(bytes)
            && let Some((oid, size)) = parse_lfs_pointer(&text)
        {
            return Some((oid, size));
        }
    }
    None
}

#[tauri::command]
pub async fn batch_fetch_thumbnails_lfs(inputs: Vec<ModThumbInput>) -> Result<u32, String> {
    use futures::{StreamExt, stream};

    // Ensure output directory exists early
    let (thumbs_dir, _) = ensure_assets_dirs()?;

    // Filter out inputs already cached
    let pending: Vec<ModThumbInput> = inputs
        .into_iter()
        .filter(|m| {
            let slug = safe_slug(&m.title);
            !thumbs_dir.join(format!("{slug}.jpg")).exists()
        })
        .collect();
    if pending.is_empty() {
        return Ok(0);
    }

    // HTTP client (reused across all requests)
    let client = reqwest::Client::builder()
        .user_agent("balatro-mod-manager/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    // Organize titles by dir and compute the set we need
    use std::collections::{HashMap, HashSet};
    let mut dir_to_titles: HashMap<String, Vec<String>> = HashMap::new();
    for m in &pending {
        dir_to_titles
            .entry(m.dir_name.clone())
            .or_default()
            .push(m.title.clone());
    }
    let needed_dirs: HashSet<String> = dir_to_titles.keys().cloned().collect();

    // 1) Try to get all pointer oids in-memory by downloading a single archive of `mods/`.
    let project = urlencoding::encode(GITLAB_PROJECT);
    let archive_urls = [
        format!(
            "https://gitlab.com/api/v4/projects/{}/repository/archive.tar.gz?sha=main&path=mods",
            project
        ),
        format!(
            "https://gitlab.com/api/v4/projects/{}/repository/archive.tar.gz?sha=master&path=mods",
            project
        ),
    ];

    let mut oid_to_titles: HashMap<String, Vec<String>> = HashMap::new();
    let mut objects: Vec<LfsObjectSpec> = Vec::new();
    let mut seen_oid: HashSet<String> = HashSet::new();

    'outer: for url in &archive_urls {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.bytes().await {
                    Ok(b) => {
                        use flate2::read::GzDecoder;
                        use std::io::Read;
                        use tar::Archive;
                        let mut dec = GzDecoder::new(b.as_ref());
                        let mut tar_bytes = Vec::with_capacity(b.len());
                        if dec.read_to_end(&mut tar_bytes).is_err() {
                            continue;
                        }
                        let mut archive = Archive::new(std::io::Cursor::new(tar_bytes));
                        let mut found = 0usize;
                        let total_needed = needed_dirs.len();
                        if let Ok(mut entries) = archive.entries() {
                            'entries: for e in entries.by_ref() {
                                let mut entry = match e {
                                    Ok(v) => v,
                                    Err(_) => continue,
                                };
                                let path = match entry.path() {
                                    Ok(p) => p.into_owned(),
                                    Err(_) => continue,
                                };
                                // Path looks like: <root>/mods/<dir>/thumbnail.jpg
                                let comps: Vec<_> = path.components().collect();
                                let mut mods_idx = None;
                                for (i, c) in comps.iter().enumerate() {
                                    if c.as_os_str() == std::ffi::OsStr::new("mods") {
                                        mods_idx = Some(i);
                                        break;
                                    }
                                }
                                let mi = match mods_idx {
                                    Some(i) => i,
                                    None => continue,
                                };
                                if comps.len() < mi + 3 {
                                    continue;
                                }
                                let dir_name_os = match &comps[mi + 1] {
                                    std::path::Component::Normal(n) => n,
                                    _ => continue,
                                };
                                let file_os = match &comps[mi + 2] {
                                    std::path::Component::Normal(n) => n,
                                    _ => continue,
                                };
                                if file_os.to_string_lossy() != "thumbnail.jpg" {
                                    continue;
                                }
                                let dir_name = dir_name_os.to_string_lossy().to_string();
                                if !needed_dirs.contains(&dir_name) {
                                    continue;
                                }

                                let mut s = String::new();
                                if entry.read_to_string(&mut s).is_err() {
                                    continue;
                                }
                                if let Some((oid, size)) = parse_lfs_pointer(&s) {
                                    oid_to_titles.entry(oid.clone()).or_default().extend(
                                        dir_to_titles.get(&dir_name).cloned().unwrap_or_default(),
                                    );
                                    if seen_oid.insert(oid.clone()) {
                                        objects.push(LfsObjectSpec { oid, size });
                                    }
                                    found += 1;
                                    if found >= total_needed {
                                        break 'entries;
                                    }
                                }
                            }
                        }
                        // break outer if we collected anything from this branch
                        if !objects.is_empty() {
                            break 'outer;
                        }
                    }
                    Err(_) => continue,
                }
            }
            _ => continue,
        }
    }

    // Fallback to per-file pointer fetch if archive path yielded nothing
    if objects.is_empty() {
        let project_enc = urlencoding::encode(GITLAB_PROJECT).to_string();
        let pointer_results = stream::iter(pending.into_iter())
            .map(|m| {
                let client = client.clone();
                let project_enc = project_enc.clone();
                let dir = m.dir_name;
                let title = m.title;
                async move {
                    let p = fetch_pointer_for_thumb(&client, &project_enc, &dir).await;
                    p.map(|(oid, size)| (title, oid, size))
                }
            })
            .buffer_unordered(12)
            .collect::<Vec<_>>()
            .await;

        for item in pointer_results.into_iter().flatten() {
            let (title, oid, size) = item;
            oid_to_titles.entry(oid.clone()).or_default().push(title);
            if seen_oid.insert(oid.clone()) {
                objects.push(LfsObjectSpec { oid, size });
            }
        }
        if objects.is_empty() {
            return Ok(0);
        }
    }

    // 2) Perform LFS batch requests (chunked, e.g., 50 objects per request)
    let chunk_size = 50usize;
    let mut total_saved = 0u32;
    for chunk in objects.chunks(chunk_size) {
        let req_body = LfsBatchReq {
            operation: "download",
            transfers: Some(&["basic"]),
            objects: chunk.to_vec(),
        };
        let resp = client
            .post(GITLAB_LFS_BATCH)
            .header(
                reqwest::header::ACCEPT,
                "application/vnd.git-lfs+json; charset=utf-8",
            )
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/vnd.git-lfs+json; charset=utf-8",
            )
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("LFS batch error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("LFS batch status: {}", resp.status()));
        }
        let batch: LfsBatchResp = resp
            .json()
            .await
            .map_err(|e| format!("Parse LFS batch response failed: {}", e))?;

        // 3) Download each object and store under each mapped title
        let concurrency = 8usize;
        let saved = stream::iter(batch.objects.into_iter())
            .filter_map(|o| {
                let actions = o.actions.clone();
                let titles = oid_to_titles.get(&o.oid).cloned();
                async move {
                    match (actions, titles) {
                        (Some(a), Some(ts)) => Some((o.oid, o.size, a.download, ts)),
                        _ => None,
                    }
                }
            })
            .map(|(_oid, _size, act, titles)| {
                let client = client.clone();
                let thumbs_dir_cloned = thumbs_dir.clone();
                async move {
                    let mut req = client.get(&act.href);
                    if let Some(h) = act.header {
                        for (k, v) in h {
                            req = req.header(k, v);
                        }
                    }
                    if let Ok(resp) = req.send().await
                        && resp.status().is_success()
                        && let Ok(bytes) = resp.bytes().await
                    {
                        let mut count = 0u32;
                        for t in &titles {
                            let slug = safe_slug(t);
                            let path = thumbs_dir_cloned.join(format!("{slug}.jpg"));
                            if std::fs::write(&path, &bytes).is_ok() {
                                count += 1;
                            }
                        }
                        return count;
                    }
                    0u32
                }
            })
            .buffer_unordered(concurrency)
            .fold(0u32, |acc, n| async move { acc + n })
            .await;
        total_saved += saved;
    }

    Ok(total_saved)
}
