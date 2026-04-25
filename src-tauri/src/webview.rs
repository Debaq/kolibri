use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, State,
    WebviewUrl,
};

use crate::services::{data_dir_for, AppState, Service};

pub const BAR_HEIGHT: u32 = 44;
#[cfg(target_os = "windows")]
const OFFSCREEN_X: f64 = -20000.0;

fn label_for(id: &str) -> String {
    format!("svc__{}", id)
}

pub fn ensure_mounted<R: Runtime>(app: &AppHandle<R>, svc: &Service) -> tauri::Result<()> {
    let label = label_for(&svc.id);
    if app.webviews().contains_key(&label) {
        return Ok(());
    }
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };

    let url = svc
        .url
        .parse()
        .map_err(|e: url::ParseError| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
    let data_dir = data_dir_for(app, &svc.id)?;

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .data_directory(data_dir);
    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }

    #[cfg(target_os = "windows")]
    {
        let (pos, size) = win_service_bounds(app);
        window.add_child(builder, pos, size)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Linux: position is ignored by GtkBox layout. Use a placeholder.
        window.add_child(
            builder,
            LogicalPosition::new(0.0, BAR_HEIGHT as f64),
            LogicalSize::new(800.0, 600.0),
        )?;
    }

    Ok(())
}

pub fn unmount<R: Runtime>(app: &AppHandle<R>, id: &str) -> tauri::Result<()> {
    let label = label_for(id);
    if let Some(wv) = app.get_webview(&label) {
        wv.close()?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn win_window_size<R: Runtime>(app: &AppHandle<R>) -> (f64, f64) {
    app.get_window("main")
        .and_then(|w| {
            let inner = w.inner_size().ok()?;
            let scale = w.scale_factor().unwrap_or(1.0);
            Some((inner.width as f64 / scale, inner.height as f64 / scale))
        })
        .unwrap_or((1200.0, 800.0))
}

#[cfg(target_os = "windows")]
fn win_service_bounds<R: Runtime>(app: &AppHandle<R>) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let (w, h) = win_window_size(app);
    (
        LogicalPosition::new(0.0, BAR_HEIGHT as f64),
        LogicalSize::new(w.max(100.0), (h - BAR_HEIGHT as f64).max(100.0)),
    )
}

#[cfg(target_os = "windows")]
fn win_bar_bounds<R: Runtime>(app: &AppHandle<R>) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let (w, _h) = win_window_size(app);
    (
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(w.max(100.0), BAR_HEIGHT as f64),
    )
}

pub fn set_active<R: Runtime>(app: &AppHandle<R>, id: Option<&str>) -> tauri::Result<()> {
    let state: State<AppState> = app.state();
    let services = state.inner.lock().unwrap().services.clone();

    #[cfg(target_os = "linux")]
    {
        linux_apply_layout(app, &services, id)?;
    }

    #[cfg(target_os = "windows")]
    {
        let main = app.get_webview_window("main");
        if let Some(main) = main {
            let (pos, size) = win_bar_bounds(app);
            let _ = main.set_position(pos);
            let _ = main.set_size(size);
        }
        let (svc_pos, svc_size) = win_service_bounds(app);
        for svc in services.iter() {
            let label = label_for(&svc.id);
            let Some(wv) = app.get_webview(&label) else {
                continue;
            };
            let is_active = id == Some(svc.id.as_str());
            if is_active {
                let _ = wv.set_size(svc_size);
                let _ = wv.set_position(svc_pos);
            } else {
                let _ = wv.set_position(LogicalPosition::new(OFFSCREEN_X, svc_pos.y));
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_apply_layout<R: Runtime>(
    app: &AppHandle<R>,
    services: &[Service],
    active: Option<&str>,
) -> tauri::Result<()> {
    use gtk::prelude::*;

    let Some(window) = app.get_window("main") else {
        return Ok(());
    };
    let vbox = window.default_vbox()?;

    // Children are in insertion order: index 0 = main bar webview, 1.. = services.
    let children = vbox.children();
    if let Some(first) = children.first() {
        // Pin bar to natural height, no expand/fill — services take the rest.
        vbox.set_child_packing(first, false, false, 0, gtk::PackType::Start);
        first.set_size_request(-1, BAR_HEIGHT as i32);
    }

    // Show only the active service widget; hide all others.
    for svc in services.iter() {
        let label = label_for(&svc.id);
        let Some(wv) = app.get_webview(&label) else {
            continue;
        };
        let is_active = active == Some(svc.id.as_str());
        let _ = wv.with_webview(move |platform_wv| {
            let widget = platform_wv.inner();
            if is_active {
                widget.show();
            } else {
                widget.hide();
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub fn update_content_bounds<R: Runtime>(
    app: AppHandle<R>,
    _x: f64,
    _y: f64,
    _width: f64,
    _height: f64,
) -> Result<(), String> {
    let app_state: State<AppState> = app.state();
    let active = app_state.inner.lock().unwrap().active_id.clone();
    set_active(&app, active.as_deref()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_minimize<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(w) = app.get_window("main") {
        w.minimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_toggle_maximize<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(w) = app.get_window("main") {
        if w.is_maximized().unwrap_or(false) {
            w.unmaximize().map_err(|e| e.to_string())?;
        } else {
            w.maximize().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn window_close<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(w) = app.get_window("main") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}
