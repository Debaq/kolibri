use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::webview;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PersistedState {
    pub services: Vec<Service>,
    pub active_id: Option<String>,
}

#[derive(Default)]
pub struct AppState {
    pub inner: Mutex<PersistedState>,
}

const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

fn config_path<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir).ok();
    Ok(dir.join("services.json"))
}

pub fn data_dir_for<R: Runtime>(app: &AppHandle<R>, id: &str) -> tauri::Result<PathBuf> {
    let dir = app.path().app_data_dir()?.join("sessions").join(id);
    fs::create_dir_all(&dir).ok();
    Ok(dir)
}

pub fn save<R: Runtime>(app: &AppHandle<R>, state: &PersistedState) -> tauri::Result<()> {
    let path = config_path(app)?;
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_from_disk<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&path)?;
    let parsed: PersistedState = serde_json::from_str(&raw).unwrap_or_default();
    let state: State<AppState> = app.state();
    {
        let mut guard = state.inner.lock().unwrap();
        *guard = parsed;
    }
    let snapshot = state.inner.lock().unwrap().clone();
    for svc in snapshot.services.iter() {
        let _ = webview::ensure_mounted(app, svc);
    }
    let _ = webview::set_active(app, snapshot.active_id.as_deref());
    Ok(())
}

#[tauri::command]
pub fn list_services(state: State<'_, AppState>) -> Vec<Service> {
    state.inner.lock().unwrap().services.clone()
}

#[tauri::command]
pub fn add_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    url: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<Service, String> {
    let id = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let svc = Service {
        id: id.clone(),
        name,
        url,
        icon,
        color,
        user_agent: Some(CHROME_UA.to_string()),
    };
    {
        let mut g = state.inner.lock().unwrap();
        g.services.push(svc.clone());
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    webview::ensure_mounted(&app, &svc).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:services_changed", ());
    Ok(svc)
}

#[tauri::command]
pub fn remove_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let was_active;
    {
        let mut g = state.inner.lock().unwrap();
        g.services.retain(|s| s.id != id);
        was_active = g.active_id.as_deref() == Some(&id);
        if was_active {
            g.active_id = None;
        }
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    webview::unmount(&app, &id).map_err(|e| e.to_string())?;
    if was_active {
        webview::set_active(&app, None).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}
