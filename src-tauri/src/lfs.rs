use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

const LFS_API_BASE: &str =
    "http://smallgit.dasguney.com:3000/skyline/balatro-mod-index.git/info/lfs";

#[derive(Debug)]
pub enum LfsError {
    NotFound,
    Retryable(String),
    Other(String),
}

impl std::fmt::Display for LfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LfsError::NotFound => write!(f, "LFS object not found"),
            LfsError::Retryable(msg) => write!(f, "LFS retryable error: {msg}"),
            LfsError::Other(msg) => write!(f, "LFS error: {msg}"),
        }
    }
}

#[derive(Debug)]
struct LfsPointer {
    oid: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct LfsBatchResponse {
    objects: Vec<LfsObject>,
}

#[derive(Debug, Deserialize)]
struct LfsObject {
    oid: String,
    actions: Option<LfsActions>,
    error: Option<LfsObjectError>,
}

#[derive(Debug, Deserialize)]
struct LfsActions {
    download: Option<LfsAction>,
}

#[derive(Debug, Deserialize)]
struct LfsAction {
    href: String,
}

#[derive(Debug, Deserialize)]
struct LfsObjectError {
    code: Option<i32>,
    message: Option<String>,
}

fn parse_lfs_pointer(bytes: &[u8]) -> Option<LfsPointer> {
    if bytes.len() > 2048 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.starts_with("version https://git-lfs.github.com/spec/v1") {
        return None;
    }
    let mut oid: Option<String> = None;
    let mut size: Option<u64> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("oid sha256:") {
            if rest.len() == 64 {
                oid = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("size ")
            && let Ok(parsed) = rest.trim().parse::<u64>()
        {
            size = Some(parsed);
        }
    }
    match (oid, size) {
        (Some(oid), Some(size)) => Some(LfsPointer { oid, size }),
        _ => None,
    }
}

fn status_to_error(status: StatusCode, context: &str) -> LfsError {
    if status == StatusCode::NOT_FOUND || status == StatusCode::GONE {
        return LfsError::NotFound;
    }
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return LfsError::Retryable(format!("{context} returned {}", status.as_u16()));
    }
    LfsError::Other(format!("{context} returned {}", status.as_u16()))
}

async fn download_lfs_object(
    client: &reqwest::Client,
    pointer: &LfsPointer,
) -> Result<Vec<u8>, LfsError> {
    let url = format!("{}/objects/batch", LFS_API_BASE);
    let body = json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{
            "oid": pointer.oid,
            "size": pointer.size,
        }]
    });
    let resp = client
        .post(url)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| LfsError::Retryable(format!("LFS batch request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(status_to_error(resp.status(), "LFS batch"));
    }
    let batch: LfsBatchResponse = resp
        .json()
        .await
        .map_err(|e| LfsError::Other(format!("LFS batch decode failed: {e}")))?;
    let obj = batch
        .objects
        .into_iter()
        .find(|o| o.oid == pointer.oid)
        .ok_or_else(|| LfsError::Other("LFS batch missing object".to_string()))?;
    if let Some(err) = obj.error {
        if matches!(err.code, Some(404 | 410)) {
            return Err(LfsError::NotFound);
        }
        let msg = err
            .message
            .unwrap_or_else(|| "LFS object error".to_string());
        return Err(LfsError::Other(msg));
    }
    let href = obj
        .actions
        .and_then(|a| a.download)
        .map(|a| a.href)
        .ok_or_else(|| LfsError::Other("LFS download action missing".to_string()))?;
    let resp = client
        .get(href)
        .send()
        .await
        .map_err(|e| LfsError::Retryable(format!("LFS download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(status_to_error(resp.status(), "LFS download"));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| LfsError::Other(format!("LFS download bytes failed: {e}")))
}

pub async fn resolve_lfs_pointer_bytes(
    client: &reqwest::Client,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, LfsError> {
    if let Some(pointer) = parse_lfs_pointer(&bytes) {
        return download_lfs_object(client, &pointer).await;
    }
    Ok(bytes)
}
