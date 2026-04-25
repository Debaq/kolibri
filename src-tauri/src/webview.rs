use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    webview::WebviewWindowBuilder, AppHandle, Emitter, Manager, Runtime, State, WebviewUrl,
    WebviewWindow,
};

use crate::services::{data_dir_for, AppState, Service};

const BAR_JS: &str = include_str!("bar.js");

pub fn label_for(id: &str) -> String {
    format!("svc-{}", id)
}

#[derive(Default)]
pub struct GeometryState {
    pub inner: Mutex<Geometry>,
}

#[derive(Default, Clone, Copy, Serialize, Deserialize)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub fn snapshot_geometry<R: Runtime>(window: &tauri::Window<R>) -> Option<Geometry> {
    let pos = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let maximized = window.is_maximized().unwrap_or(false);
    Some(Geometry {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized,
    })
}

pub fn store_geometry<R: Runtime>(app: &AppHandle<R>, g: Geometry) {
    if let Some(state) = app.try_state::<GeometryState>() {
        *state.inner.lock().unwrap() = g;
    }
}

pub fn apply_geometry_to<R: Runtime>(window: &WebviewWindow<R>, g: Geometry) {
    if g.width > 0 && g.height > 0 {
        let _ = window.set_size(tauri::PhysicalSize::new(g.width, g.height));
        let _ = window.set_position(tauri::PhysicalPosition::new(g.x, g.y));
    }
    if g.maximized {
        let _ = window.maximize();
    }
}

pub fn ensure_service_window<R: Runtime>(
    app: &AppHandle<R>,
    svc: &Service,
) -> tauri::Result<WebviewWindow<R>> {
    let label = label_for(&svc.id);
    if let Some(w) = app.get_webview_window(&label) {
        return Ok(w);
    }

    let url = svc
        .url
        .parse()
        .map_err(|e: url::ParseError| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
    let data_dir = data_dir_for(app, &svc.id)?;

    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(&svc.name)
        .data_directory(data_dir)
        .decorations(false)
        .shadow(false)
        .visible(false)
        .skip_taskbar(true)
        .min_inner_size(600.0, 400.0)
        .inner_size(1200.0, 800.0)
        .initialization_script(BAR_JS);

    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }

    // Hereda geometría de la ventana visible actual si la hay.
    let geom = app
        .try_state::<GeometryState>()
        .map(|s| *s.inner.lock().unwrap())
        .unwrap_or_default();

    let window = builder.build()?;

    if geom.width > 0 && geom.height > 0 {
        apply_geometry_to(&window, geom);
    }

    let app_handle: AppHandle<R> = app.clone();
    let label_owned = label.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_) => {
            if let Some(w) = app_handle.get_webview_window(&label_owned) {
                let inner = w.as_ref().window().clone();
                if let Some(g) = snapshot_geometry(&inner) {
                    store_geometry(&app_handle, g);
                }
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Some(w) = app_handle.get_webview_window(&label_owned) {
                let _ = w.hide();
            }
        }
        _ => {}
    });

    Ok(window)
}

pub fn destroy_service_window<R: Runtime>(app: &AppHandle<R>, id: &str) -> tauri::Result<()> {
    let label = label_for(id);
    if let Some(w) = app.get_webview_window(&label) {
        // Permitir cierre real esta vez
        w.destroy()?;
    }
    Ok(())
}

pub fn show_only<R: Runtime>(app: &AppHandle<R>, target: Option<&str>) -> tauri::Result<()> {
    let services_state: State<AppState> = app.state();
    let services = services_state.inner.lock().unwrap().services.clone();

    // Capturar geometría actual antes de switch.
    if let Some(visible) = current_visible_window(app, &services) {
        let inner = visible.as_ref().window().clone();
        if let Some(g) = snapshot_geometry(&inner) {
            store_geometry(app, g);
        }
    }

    let geom = app
        .try_state::<GeometryState>()
        .map(|s| *s.inner.lock().unwrap())
        .unwrap_or_default();

    // Mostrar primero target, luego ocultar resto (evita flicker).
    let target_label = match target {
        Some(id) => label_for(id),
        None => "main".to_string(),
    };

    if let Some(w) = app.get_webview_window(&target_label) {
        if geom.width > 0 && geom.height > 0 {
            apply_geometry_to(&w, geom);
        }
        let _ = w.show();
        let _ = w.set_focus();
    }

    // Ocultar todas las demás
    if target_label != "main" {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
    }
    for svc in services.iter() {
        let lbl = label_for(&svc.id);
        if lbl == target_label {
            continue;
        }
        if let Some(w) = app.get_webview_window(&lbl) {
            let _ = w.hide();
        }
    }

    let _ = app.emit("kolibri:active_changed", target);
    Ok(())
}

fn current_visible_window<R: Runtime>(
    app: &AppHandle<R>,
    services: &[Service],
) -> Option<WebviewWindow<R>> {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            return Some(w);
        }
    }
    for svc in services {
        if let Some(w) = app.get_webview_window(&label_for(&svc.id)) {
            if w.is_visible().unwrap_or(false) {
                return Some(w);
            }
        }
    }
    None
}

#[tauri::command]
pub fn switch_service<R: Runtime>(app: AppHandle<R>, id: Option<String>) -> Result<(), String> {
    {
        let state: State<AppState> = app.state();
        let mut g = state.inner.lock().unwrap();
        g.active_id = id.clone();
        crate::services::save(&app, &g).map_err(|e| e.to_string())?;
    }
    show_only(&app, id.as_deref()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_active_service(state: State<'_, AppState>) -> Option<String> {
    state.inner.lock().unwrap().active_id.clone()
}

#[tauri::command]
pub fn open_add_dialog<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    show_only(&app, None).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:open_add_dialog", ());
    Ok(())
}

#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    show_only(&app, None).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:open_settings", ());
    Ok(())
}

#[tauri::command]
pub fn window_minimize<R: Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    eprintln!("[KOLIBRI] window_minimize from label={}", window.label());
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_toggle_maximize<R: Runtime>(window: tauri::Window<R>) -> Result<(), String> {
    let max = window.is_maximized().unwrap_or(false);
    eprintln!(
        "[KOLIBRI] window_toggle_maximize from label={} (currently maximized={})",
        window.label(),
        max
    );
    if max {
        window.unmaximize().map_err(|e| e.to_string())?;
    } else {
        window.maximize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_close<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>) -> Result<(), String> {
    eprintln!("[KOLIBRI] window_close from label={}", window.label());
    if window.label() != "main" {
        window.hide().map_err(|e| e.to_string())?;
        show_only(&app, None).map_err(|e| e.to_string())?;
    } else {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}
