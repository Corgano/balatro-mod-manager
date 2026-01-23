use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use reqwest::StatusCode;

use crate::assets::{ensure_assets_dirs, safe_slug};
use crate::lfs::{LfsError, resolve_lfs_pointer_bytes};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::{Duration, sleep};

/// Background thumbnail fetch request
#[derive(Clone, Debug)]
struct ThumbReq {
    title: String,
    url: String,
    attempts: u32,
    priority: bool, // High priority requests (visible thumbnails) are processed first
}

/// Manager that rate-limits and retries thumbnail downloads in the background.
/// It honors 429 Retry-After when present, and uses exponential backoff for 5xx/network errors.
/// Supports priority queueing for visible thumbnails.
#[derive(Clone)]
pub struct ThumbnailManager {
    tx_high: mpsc::Sender<ThumbReq>, // High priority channel (visible thumbnails)
    tx_low: mpsc::Sender<ThumbReq>,  // Low priority channel (background prefetch)
    // Prevent duplicate queueing per title within a session
    enqueued: Arc<Mutex<HashSet<String>>>,
}

const THUMB_CACHE_TTL_SECS: u64 = 60 * 60 * 24 * 7;

impl ThumbnailManager {
    pub fn new() -> Self {
        // Smaller bounded queues to avoid memory spikes - high priority gets processed first
        let (tx_high, mut rx_high) = mpsc::channel::<ThumbReq>(128);
        let (tx_low, mut rx_low) = mpsc::channel::<ThumbReq>(256);
        let enqueued: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        // Limit concurrent downloads to avoid rate limits
        // Using 6 concurrent downloads for faster thumbnail loading
        let semaphore = Arc::new(Semaphore::new(6));
        let client = reqwest::Client::builder()
            .user_agent("balatro-mod-manager/1.0")
            .timeout(Duration::from_secs(10))
            // Accept invalid certs for direct IP connections (fallback mode)
            .danger_accept_invalid_certs(true)
            .build()
            .expect("reqwest client");

        // Spawn dispatcher task that prioritizes high-priority requests
        let enq_for_task = enqueued.clone();
        let tx_high_for_dispatch = tx_high.clone();
        let tx_low_for_dispatch = tx_low.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                // Try high priority first (with timeout), then fall back to low priority
                let req = tokio::select! {
                    biased;
                    Some(req) = rx_high.recv() => Some(req),
                    Some(req) = rx_low.recv() => Some(req),
                    else => None,
                };

                let Some(mut req) = req else {
                    // Both channels closed
                    break;
                };

                // Skip if file already exists or has been de-duped
                if file_exists_for_title_async(&req.title).await {
                    // Remove from enqueued set so future explicit requests are allowed
                    if let Ok(mut set) = enq_for_task.lock() {
                        set.remove(&req.title);
                    }
                    continue;
                }

                let semaphore = semaphore.clone();
                let client = client.clone();
                let enq_set = enq_for_task.clone();
                let tx_retry = if req.priority {
                    tx_high_for_dispatch.clone()
                } else {
                    tx_low_for_dispatch.clone()
                };
                tauri::async_runtime::spawn(async move {
                    // Acquire permit inside the spawned task to avoid blocking the dispatcher
                    let _permit = match semaphore.acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => return, // Semaphore closed
                    };
                    match fetch_and_store(&client, &req.title, &req.url).await {
                        Ok(true) => {
                            if let Ok(mut set) = enq_set.lock() {
                                set.remove(&req.title);
                            }
                        }
                        Ok(false) => {
                            // Non-retryable (e.g., 404/unsupported), drop and clear
                            if let Ok(mut set) = enq_set.lock() {
                                set.remove(&req.title);
                            }
                        }
                        Err(Backoff::RetryAfter(delay)) => {
                            // schedule retry after delay
                            req.attempts = req.attempts.saturating_add(1);
                            if req.attempts > 3 {
                                if let Ok(mut set) = enq_set.lock() {
                                    set.remove(&req.title);
                                }
                                return;
                            }
                            let title = req.title.clone();
                            tauri::async_runtime::spawn(async move {
                                sleep(delay).await;
                                // Put back into queue, keep enqueued flag as-is
                                let _ = tx_retry.send(req).await;
                                // If send fails, allow future enqueue by clearing mark
                                if let Ok(mut set) = enq_set.lock() {
                                    set.remove(&title);
                                }
                            });
                        }
                    }
                });
            }
        });

        Self {
            tx_high,
            tx_low,
            enqueued,
        }
    }

    /// Enqueue a single thumbnail request if not already present and not already cached.
    /// Use `priority = true` for visible thumbnails to download them first.
    pub fn enqueue(&self, title: String, url: String) {
        self.enqueue_with_priority(title, url, false);
    }

    /// Enqueue a single thumbnail with explicit priority.
    /// High priority thumbnails (visible on screen) are processed before low priority ones.
    pub fn enqueue_with_priority(&self, title: String, url: String, priority: bool) {
        // Use sync file check here since we're in a sync context
        if file_exists_for_title_sync(&title) {
            return;
        }
        if let Ok(mut set) = self.enqueued.lock()
            && !set.insert(title.clone())
        {
            return; // already queued
        }
        let req = ThumbReq {
            title,
            url,
            attempts: 0,
            priority,
        };
        if priority {
            let _ = self.tx_high.try_send(req);
        } else {
            let _ = self.tx_low.try_send(req);
        }
    }

    /// Enqueue multiple thumbnail requests.
    pub fn enqueue_many(&self, items: impl IntoIterator<Item = (String, String)>) {
        for (title, url) in items {
            self.enqueue(title, url);
        }
    }

    /// Enqueue multiple thumbnail requests with priority support.
    /// Items are tuples of (title, url, priority).
    pub fn enqueue_many_with_priority(
        &self,
        items: impl IntoIterator<Item = (String, String, bool)>,
    ) {
        for (title, url, priority) in items {
            self.enqueue_with_priority(title, url, priority);
        }
    }
}

impl Default for ThumbnailManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that indicate we should retry later with a delay
enum Backoff {
    RetryAfter(Duration),
}

async fn fetch_and_store(
    client: &reqwest::Client,
    title: &str,
    url: &str,
) -> Result<bool, Backoff> {
    log::info!("Thumbnail fetch start: title='{}' url='{}'", title, url);
    // Don't waste network if already cached
    if file_exists_for_title_async(title).await {
        return Ok(false);
    }

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return Err(Backoff::RetryAfter(jitter(Duration::from_secs(3)))),
    };

    match resp.status() {
        StatusCode::OK => {
            let bytes = match resp.bytes().await {
                Ok(b) => b.to_vec(),
                Err(_) => return Ok(false),
            };
            let bytes = match resolve_lfs_pointer_bytes(client, bytes).await {
                Ok(resolved) => resolved,
                Err(LfsError::Retryable(_)) => {
                    return Err(Backoff::RetryAfter(jitter(Duration::from_secs(4))));
                }
                Err(_) => return Ok(false),
            };
            if write_thumbnail_async(title, &bytes).await.is_err() {
                // Disk error; drop silently
                return Ok(false);
            }
            log::info!("Thumbnail saved: title='{}'", title);
            Ok(true)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            let delay =
                retry_after_delay(resp.headers()).unwrap_or_else(|| jitter(Duration::from_secs(5)));
            Err(Backoff::RetryAfter(delay))
        }
        s if s.is_server_error() => Err(Backoff::RetryAfter(jitter(Duration::from_secs(4)))),
        StatusCode::NOT_FOUND | StatusCode::GONE => Ok(false),
        _ => Ok(false),
    }
}

fn jitter(base: Duration) -> Duration {
    // Small jitter based on current time millis; avoids extra deps
    use std::time::{SystemTime, UNIX_EPOCH};
    let base_ms = base.as_millis() as u64;
    let wiggle_base = (base_ms / 3).max(1);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let wiggle = now_ms % wiggle_base;
    Duration::from_millis(base_ms + wiggle)
}

fn retry_after_delay(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    use reqwest::header::RETRY_AFTER;
    if let Some(val) = headers.get(RETRY_AFTER)
        && let Ok(s) = val.to_str()
    {
        // Either seconds or HTTP-date
        if let Ok(secs) = s.trim().parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
        if let Ok(target) = httpdate::parse_http_date(s) {
            // Convert to duration from now; guard against past
            if let Ok(diff) = target.duration_since(std::time::SystemTime::now()) {
                return Some(diff);
            }
        }
    }
    None
}

fn file_exists_for_title_sync(title: &str) -> bool {
    let slug = safe_slug(title);
    if let Ok((thumbs, _)) = ensure_assets_dirs() {
        let p = thumbs.join(format!("{slug}.jpg"));
        if !p.exists() {
            return false;
        }
        if !is_thumb_fresh(&p) {
            let _ = std::fs::remove_file(&p);
            return false;
        }
        return true;
    }
    false
}

async fn file_exists_for_title_async(title: &str) -> bool {
    let slug = safe_slug(title);
    if let Ok((thumbs, _)) = ensure_assets_dirs() {
        let p = thumbs.join(format!("{slug}.jpg"));
        match tokio::fs::metadata(&p).await {
            Ok(meta) => {
                if !is_thumb_fresh_from_metadata(&meta) {
                    let _ = tokio::fs::remove_file(&p).await;
                    return false;
                }
                true
            }
            Err(_) => false,
        }
    } else {
        false
    }
}

fn is_thumb_fresh(path: &std::path::Path) -> bool {
    let modified = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(ts) => ts,
        Err(_) => return false,
    };
    let age = match std::time::SystemTime::now().duration_since(modified) {
        Ok(d) => d,
        Err(_) => return false,
    };
    age.as_secs() < THUMB_CACHE_TTL_SECS
}

fn is_thumb_fresh_from_metadata(meta: &std::fs::Metadata) -> bool {
    let modified = match meta.modified() {
        Ok(ts) => ts,
        Err(_) => return false,
    };
    let age = match std::time::SystemTime::now().duration_since(modified) {
        Ok(d) => d,
        Err(_) => return false,
    };
    age.as_secs() < THUMB_CACHE_TTL_SECS
}

async fn write_thumbnail_async(title: &str, bytes: &[u8]) -> Result<(), String> {
    let slug = safe_slug(title);
    let (thumbs, _) = ensure_assets_dirs()?;
    let path = thumbs.join(format!("{slug}.jpg"));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| e.to_string())
}
