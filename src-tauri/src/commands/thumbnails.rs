use serde::Deserialize;

#[derive(Deserialize)]
pub struct ThumbItem {
    pub title: String,
    pub url: String,
}

#[tauri::command]
pub async fn enqueue_thumbnails(
    items: Vec<ThumbItem>,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<u32, String> {
    let count = items.len() as u32;
    let pairs = items.into_iter().map(|i| (i.title, i.url));
    state.thumbs.enqueue_many(pairs);
    Ok(count)
}

#[tauri::command]
pub async fn enqueue_thumbnail(
    title: String,
    url: String,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<(), String> {
    state.thumbs.enqueue(title, url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumb_item_deserialize() {
        let json = r#"{"title": "Test Mod", "url": "https://example.com/thumb.jpg"}"#;
        let item: ThumbItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.title, "Test Mod");
        assert_eq!(item.url, "https://example.com/thumb.jpg");
    }

    #[test]
    fn test_thumb_item_deserialize_array() {
        let json = r#"[
            {"title": "Mod1", "url": "https://example.com/1.jpg"},
            {"title": "Mod2", "url": "https://example.com/2.jpg"}
        ]"#;
        let items: Vec<ThumbItem> = serde_json::from_str(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Mod1");
        assert_eq!(items[1].title, "Mod2");
    }

    #[test]
    fn test_thumb_item_deserialize_empty_strings() {
        let json = r#"{"title": "", "url": ""}"#;
        let item: ThumbItem = serde_json::from_str(json).unwrap();
        assert!(item.title.is_empty());
        assert!(item.url.is_empty());
    }

    #[test]
    fn test_thumb_item_deserialize_unicode() {
        let json = r#"{"title": "日本語モッド", "url": "https://example.com/日本語.jpg"}"#;
        let item: ThumbItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.title, "日本語モッド");
    }

    #[test]
    fn test_thumb_item_missing_field_fails() {
        let json = r#"{"title": "Test Mod"}"#;
        let result: Result<ThumbItem, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
