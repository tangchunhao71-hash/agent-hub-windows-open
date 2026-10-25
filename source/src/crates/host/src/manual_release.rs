//! A build-time, user-opened download directory. No polling or updater service.
#[cfg(target_os = "windows")]
#[path = "manual_release/windows.rs"]
mod target;
#[cfg(not(target_os = "windows"))]
#[path = "manual_release/unsupported.rs"]
mod target;

fn download_url(config: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value = serde_json::from_str(config).map_err(|e| e.to_string())?;
    let Some(value) = value.get("downloadUrl").filter(|v| !v.is_null()) else { return Ok(None) };
    let text = value.as_str().ok_or("Download URL must be a string")?;
    if text.trim().is_empty() { return Ok(None); }
    let url = tauri::Url::parse(text).map_err(|_| "Invalid download URL")?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("Download URL must be HTTPS without credentials".into());
    }
    Ok(Some(url.to_string()))
}

#[tauri::command]
pub(crate) fn manual_download_url() -> Result<Option<String>, String> {
    download_url(include_str!("../../../release-channel.json"))
}

#[tauri::command]
pub(crate) fn open_manual_download() -> Result<(), String> {
    // Never accept a renderer-supplied URL or shell command.
    let url = manual_download_url()?.ok_or("Download directory is not configured")?;
    target::open(&url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_and_valid_channels() {
        assert_eq!(download_url(r#"{"downloadUrl":null}"#).unwrap(), None);
        assert_eq!(download_url(r#"{"downloadUrl":" "}"#).unwrap(), None);
        assert_eq!(download_url(r#"{"downloadUrl":"https://pan.example.org/s/abc#code"}"#).unwrap(), Some("https://pan.example.org/s/abc#code".into()));
    }
    #[test]
    fn rejects_unsafe_or_malformed_channels() {
        for url in ["http://example.org/", "file:///C:/test", "javascript:alert(1)", "https://user:secret@example.org/", "bad"] {
            assert!(download_url(&serde_json::json!({"downloadUrl":url}).to_string()).is_err());
        }
        assert!(download_url(r#"{"downloadUrl":23}"#).is_err());
        assert!(download_url("invalid json").is_err());
    }
}
