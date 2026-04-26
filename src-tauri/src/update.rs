use serde::{Deserialize, Serialize};

const REPO: &str = "Debaq/kolibri";
const API: &str = "https://api.github.com/repos/Debaq/kolibri/releases/latest";

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
    pub repo: String,
}

fn parse_version(s: &str) -> Vec<u64> {
    s.trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<u64>().ok())
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let a = parse_version(latest);
    let b = parse_version(current);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x > y { return true; }
        if x < y { return false; }
    }
    false
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = reqwest::Client::builder()
        .user_agent(format!("kolibri/{}", current))
        .build()
        .map_err(|e| e.to_string())?;
    let rel: GhRelease = client
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let latest_tag = rel.tag_name.clone();
    let available = !rel.draft && !rel.prerelease && is_newer(&latest_tag, &current);
    let url = rel.html_url;
    Ok(UpdateInfo {
        available,
        current,
        latest: rel.name.unwrap_or(latest_tag),
        url,
        repo: REPO.to_string(),
    })
}
