use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::services::{AppState, Service};

pub const BAR_HEIGHT: u32 = 56;

pub fn label_for(id: &str) -> String {
    format!("svc__{}", id)
}

#[cfg(target_os = "linux")]
mod layout_linux;
#[cfg(target_os = "linux")]
use layout_linux as platform;

#[cfg(target_os = "windows")]
mod layout_windows;
#[cfg(target_os = "windows")]
use layout_windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod layout_other;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
use layout_other as platform;

/// Mapping de service_id → label de webview montado.
#[derive(Default)]
pub struct MountedRegistry {
    pub inner: Mutex<Vec<String>>,
}

pub fn setup_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    platform::setup_main_window(app)
}

pub fn ensure_mounted<R: Runtime>(app: &AppHandle<R>, svc: &Service) -> tauri::Result<()> {
    let label = label_for(&svc.id);
    let registry = app.state::<MountedRegistry>();
    if registry.inner.lock().unwrap().iter().any(|l| l == &label) {
        return Ok(());
    }
    platform::mount_child(app, svc)?;
    registry.inner.lock().unwrap().push(label);
    Ok(())
}

pub fn unmount<R: Runtime>(app: &AppHandle<R>, svc_id: &str) -> tauri::Result<()> {
    let label = label_for(svc_id);
    platform::unmount_child(app, &label)?;
    let registry = app.state::<MountedRegistry>();
    registry.inner.lock().unwrap().retain(|l| l != &label);
    Ok(())
}

pub fn set_active<R: Runtime>(app: &AppHandle<R>, svc_id: Option<&str>) -> tauri::Result<()> {
    let services = app.state::<AppState>().inner.lock().unwrap().services.clone();
    platform::apply_active(app, &services, svc_id)?;
    let _ = app.emit("kolibri:active_changed", svc_id);
    Ok(())
}

pub fn relayout<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let services = app.state::<AppState>().inner.lock().unwrap().services.clone();
    let active = app.state::<AppState>().inner.lock().unwrap().active_id.clone();
    platform::apply_active(app, &services, active.as_deref())
}

// ============== Tauri commands ==============

#[tauri::command]
pub fn switch_service<R: Runtime>(app: AppHandle<R>, id: Option<String>) -> Result<(), String> {
    {
        let state: State<AppState> = app.state();
        let mut g = state.inner.lock().unwrap();
        g.active_id = id.clone();
        crate::services::save(&app, &g).map_err(|e| e.to_string())?;
    }
    set_active(&app, id.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_active_service(state: State<'_, AppState>) -> Option<String> {
    state.inner.lock().unwrap().active_id.clone()
}

#[tauri::command]
pub fn window_minimize<R: Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_toggle_maximize<R: Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| e.to_string())?;
    } else {
        window.maximize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_close<R: Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}
