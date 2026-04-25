use tauri::{AppHandle, Runtime};

use crate::services::Service;

pub fn setup_main_window<R: Runtime>(_app: &AppHandle<R>) -> tauri::Result<()> {
    Ok(())
}

pub fn mount_child<R: Runtime>(_app: &AppHandle<R>, _svc: &Service) -> tauri::Result<()> {
    Err(tauri::Error::Anyhow(anyhow::anyhow!(
        "mount_child no implementado en este OS — Kolibri solo soporta Linux y Windows por ahora"
    )))
}

pub fn unmount_child<R: Runtime>(_app: &AppHandle<R>, _label: &str) -> tauri::Result<()> {
    Ok(())
}

pub fn apply_active<R: Runtime>(
    _app: &AppHandle<R>,
    _services: &[Service],
    _active: Option<&str>,
) -> tauri::Result<()> {
    Ok(())
}
