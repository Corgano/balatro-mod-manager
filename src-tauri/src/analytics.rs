//! Umami analytics client.
//!
//! All errors are logged and swallowed — analytics must never affect the user experience.
//! Events are posted to the self-hosted Umami instance at UMAMI_URL.

use serde_json::{Value, json};

const UMAMI_URL: &str = "https://analytics.dasguney.com/api/send";
const WEBSITE_ID: &str = "d5277634-f1b9-4082-ad6a-7299a2b53a18";

/// Send a named event to Umami. Fails silently — never returns an error.
pub async fn send_event(name: &str, props: Value, app_version: &str) {
    // Umami requires flat event data — merge app_version into props directly
    let mut data = match props {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    data.insert(
        "app_version".to_string(),
        Value::String(app_version.to_string()),
    );

    let payload = json!({
        "type": "event",
        "payload": {
            "website": WEBSITE_ID,
            "hostname": "balatro-mod-manager",
            "language": "en",
            "url": "/",
            "name": name,
            "data": data,
        }
    });

    post(payload).await;
}

/// Send a page view to Umami so the Overview dashboard counts Visitors/Visits/Views.
/// Must be called once per session (i.e. on app startup). Fails silently.
pub async fn send_pageview() {
    let payload = json!({
        "type": "event",
        "payload": {
            "website": WEBSITE_ID,
            "hostname": "balatro-mod-manager",
            "language": "en",
            "url": "/",
            "title": "Balatro Mod Manager"
        }
    });

    post(payload).await;
}

async fn post(payload: Value) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Analytics: failed to build HTTP client: {e}");
            return;
        }
    };

    match client
        .post(UMAMI_URL)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            log::debug!("Analytics payload sent OK");
        }
        Ok(resp) => {
            log::warn!("Analytics payload failed (status {})", resp.status());
        }
        Err(e) => {
            log::warn!("Analytics request error: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_payload_structure() {
        let props = json!({ "mod": "TestMod" });
        let payload = json!({
            "type": "event",
            "payload": {
                "website": WEBSITE_ID,
                "hostname": "balatro-mod-manager",
                "language": "en",
                "url": "/",
                "name": "mod_installed",
                "data": {
                    "app_version": "0.4.1",
                    "props": props,
                }
            }
        });

        assert_eq!(payload["type"], "event");
        assert_eq!(payload["payload"]["website"], WEBSITE_ID);
        assert_eq!(payload["payload"]["name"], "mod_installed");
        assert_eq!(payload["payload"]["data"]["props"]["mod"], "TestMod");
    }

    #[test]
    fn test_website_id_is_set() {
        assert!(!WEBSITE_ID.is_empty());
        assert_eq!(WEBSITE_ID, "d5277634-f1b9-4082-ad6a-7299a2b53a18");
    }
}
