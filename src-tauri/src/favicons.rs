use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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
    if bytes.len() < 16 {
        return false;
    }
    let head = &bytes[..bytes.len().min(256)];
    let s = String::from_utf8_lossy(head).to_ascii_lowercase();
    if s.contains("<html") || s.contains("<!doctype") || s.contains("<head") {
        return false;
    }
    true
}

fn cache_fresh(path: &std::path::Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < CACHE_TTL)
        .unwrap_or(false)
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

    let bytes = if cache_path.exists() && cache_fresh(&cache_path) {
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
        let bytes = match found {
            Some(b) => b,
            None => {
                if cache_path.exists() {
                    fs::read(&cache_path).map_err(|e| e.to_string())?
                } else {
                    return Err("no favicon found".to_string());
                }
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mime_png() {
        assert_eq!(detect_mime(b"\x89PNG\r\n\x1a\nrest"), "image/png");
    }

    #[test]
    fn detect_mime_gif() {
        assert_eq!(detect_mime(b"GIF89a..."), "image/gif");
    }

    #[test]
    fn detect_mime_jpeg() {
        assert_eq!(detect_mime(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]), "image/jpeg");
    }

    #[test]
    fn detect_mime_webp() {
        let mut v = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        v.extend_from_slice(b"VP8 ");
        assert_eq!(detect_mime(&v), "image/webp");
    }

    #[test]
    fn detect_mime_svg() {
        assert_eq!(detect_mime(b"<?xml ?><svg xmlns='...'></svg>"), "image/svg+xml");
    }

    #[test]
    fn detect_mime_fallback_ico() {
        assert_eq!(detect_mime(&[0, 0, 1, 0, 1, 0, 16, 16]), "image/x-icon");
    }

    #[test]
    fn looks_valid_rejects_too_small() {
        assert!(!looks_valid(b"short"));
    }

    #[test]
    fn looks_valid_rejects_html() {
        assert!(!looks_valid(b"<!doctype html><html><head></head></html>"));
        assert!(!looks_valid(b"<HTML><body>error page </body></HTML>"));
    }

    #[test]
    fn looks_valid_accepts_png_bytes() {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 32]);
        assert!(looks_valid(&v));
    }

    #[test]
    fn safe_name_replaces_unsafe() {
        assert_eq!(safe_name("web.whatsapp.com"), "web.whatsapp.com");
        assert_eq!(safe_name("foo:8080"), "foo_8080");
    }

    #[test]
    fn host_from_extracts() {
        assert_eq!(host_from("https://web.whatsapp.com/x"), Some("web.whatsapp.com".to_string()));
        assert_eq!(host_from("not-a-url"), None);
    }
}
