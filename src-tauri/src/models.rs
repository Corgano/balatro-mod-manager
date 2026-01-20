use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Serialize)]
pub struct Payload {
    pub args: Vec<String>,
    pub cwd: String,
}

/// Event payload for installed-mods-changed event.
/// Contains delta information about what changed.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ModsChangedEvent {
    /// Mod names that were added
    pub added: Vec<String>,
    /// Mod names that were removed
    pub removed: Vec<String>,
    /// Whether this is a full refresh (added/removed may be incomplete)
    pub full_refresh: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModMeta {
    #[serde(rename = "requires-steamodded", alias = "requires_steamodded", default)]
    pub requires_steamodded: bool,
    #[serde(rename = "requires-talisman", alias = "requires_talisman", default)]
    pub requires_talisman: bool,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "downloadURL")]
    pub download_url: Option<String>,
    #[serde(rename = "folderName", default)]
    pub folder_name: String,
    #[serde(default)]
    pub version: String,
    #[serde(rename = "automatic-version-check", default)]
    pub automatic_version_check: bool,
    #[serde(rename = "last-updated", default)]
    pub last_updated: u64,
    #[serde(default)]
    pub downloads: Option<ModDownloads>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct ModDownloads {
    pub total: u64,
    pub today: u64,
}

impl<'de> Deserialize<'de> for ModDownloads {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(parse_downloads_value(&value))
    }
}

fn parse_downloads_value(value: &serde_json::Value) -> ModDownloads {
    match value {
        serde_json::Value::Number(num) => ModDownloads {
            total: num.as_u64().unwrap_or(0),
            today: 0,
        },
        serde_json::Value::String(s) => ModDownloads {
            total: s.parse::<u64>().unwrap_or(0),
            today: 0,
        },
        serde_json::Value::Object(map) => {
            let total = map
                .get("total")
                .or_else(|| map.get("total_downloads"))
                .or_else(|| map.get("totalDownloads"))
                .or_else(|| map.get("downloads"))
                .or_else(|| map.get("count"))
                .and_then(read_u64)
                .unwrap_or(0);
            let today = map
                .get("today")
                .or_else(|| map.get("today_downloads"))
                .or_else(|| map.get("todayDownloads"))
                .and_then(read_u64)
                .unwrap_or(0);
            ModDownloads { total, today }
        }
        _ => ModDownloads::default(),
    }
}

fn read_u64(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(num) => num.as_u64(),
        serde_json::Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mod_downloads_accepts_number() {
        let parsed: ModDownloads = serde_json::from_value(json!(123)).expect("parse");
        assert_eq!(parsed.total, 123);
        assert_eq!(parsed.today, 0);
    }

    #[test]
    fn mod_downloads_accepts_alias_fields() {
        let parsed: ModDownloads =
            serde_json::from_value(json!({"total_downloads": 456, "today_downloads": 7}))
                .expect("parse");
        assert_eq!(parsed.total, 456);
        assert_eq!(parsed.today, 7);
    }
}
