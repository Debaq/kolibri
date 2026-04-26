use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use tauri::{AppHandle, Manager, Runtime};

fn favicons_dir<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    let dir = app.path().app_data_dir()?.join("favicons");
    fs::create_dir_all(&dir).ok();
    Ok(dir)
}

fn host_from(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(|s| s.to_lowercase())
}

fn safe_name(host: &str) -> String {
    host.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect()
}

fn detect_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if {
        let head = &bytes[..bytes.len().min(256)];
        let s = String::from_utf8_lossy(head).to_ascii_lowercase();
        s.contains("<svg")
    } {
        "image/svg+xml"
    } else {
        "image/x-icon"
    }
}

fn looks_valid(bytes: &[u8]) -> bool {
    if bytes.len() < 64 {
        return false;
    }
    let head = &bytes[..bytes.len().min(256)];
    let s = String::from_utf8_lossy(head).to_ascii_lowercase();
    if s.contains("<html") || s.contains("<!doctype") || s.contains("<head") {
        return false;
    }
    true
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?.to_vec();
    if looks_valid(&bytes) {
        Some(bytes)
    } else {
        None
    }
}

#[tauri::command]
pub async fn get_favicon<R: Runtime>(
    app: AppHandle<R>,
    url: String,
) -> Result<String, String> {
    let host = host_from(&url).ok_or_else(|| "invalid url".to_string())?;
    let dir = favicons_dir(&app).map_err(|e| e.to_string())?;
    let cache_path = dir.join(format!("{}.bin", safe_name(&host)));

    let bytes = if cache_path.exists() {
        fs::read(&cache_path).map_err(|e| e.to_string())?
    } else {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Kolibri")
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|e| e.to_string())?;
        let candidates = [
            format!("https://www.google.com/s2/favicons?domain={}&sz=64", host),
            format!("https://icons.duckduckgo.com/ip3/{}.ico", host),
            format!("https://{}/favicon.ico", host),
        ];
        let mut found: Option<Vec<u8>> = None;
        for c in &candidates {
            if let Some(b) = fetch_one(&client, c).await {
                found = Some(b);
                break;
            }
        }
        let bytes = found.ok_or_else(|| "no favicon found".to_string())?;
        fs::write(&cache_path, &bytes).ok();
        bytes
    };

    let mime = detect_mime(&bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

#[tauri::command]
pub async fn clear_favicon_cache<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = favicons_dir(&app).map_err(|e| e.to_string())?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
