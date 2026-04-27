use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, Runtime,
    WebviewUrl,
};

use crate::services::{data_dir_for_service, Service};

use super::{label_for, scripts, BAR_HEIGHT, PLACEHOLDER_LABEL};

const OFFSCREEN_X: f64 = -20000.0;

fn window_size<R: Runtime>(app: &AppHandle<R>) -> (f64, f64) {
    app.get_window("main")
        .and_then(|w| {
            let inner = w.inner_size().ok()?;
            let scale = w.scale_factor().unwrap_or(1.0);
            Some((inner.width as f64 / scale, inner.height as f64 / scale))
        })
        .unwrap_or((1200.0, 800.0))
}

fn service_bounds<R: Runtime>(app: &AppHandle<R>) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let (w, h) = window_size(app);
    (
        LogicalPosition::new(0.0, BAR_HEIGHT as f64),
        LogicalSize::new(w.max(100.0), (h - BAR_HEIGHT as f64).max(100.0)),
    )
}

pub fn setup_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // Importante: no redimensionar la WebviewWindow — eso encoge la ventana
    // del SO entera a 56px (síntoma: solo barra superior visible, app
    // inutilizable). Hay que ajustar el sub-bounds del Webview dentro de la
    // ventana, igual que con los children.
    if let Some(main) = app.get_webview("main") {
        let (w, _h) = window_size(app);
        let _ = main.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = main.set_size(LogicalSize::new(w, BAR_HEIGHT as f64));
    }
    mount_placeholder(app)?;
    Ok(())
}

fn mount_placeholder<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };
    if app.get_webview(PLACEHOLDER_LABEL).is_some() {
        return Ok(());
    }
    let builder = WebviewBuilder::new(
        PLACEHOLDER_LABEL,
        WebviewUrl::App("placeholder.html".into()),
    )
    .disable_drag_drop_handler();
    let (pos, size) = service_bounds(app);
    window.add_child(builder, pos, size)?;
    Ok(())
}

pub fn mount_child<R: Runtime>(app: &AppHandle<R>, svc: &Service) -> tauri::Result<()> {
    let label = label_for(&svc.id);
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };
    let url = svc
        .url
        .parse()
        .map_err(|e: url::ParseError| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
    let data_dir = data_dir_for_service(app, svc)?;

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .data_directory(data_dir)
        .disable_drag_drop_handler();
    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }
    builder = scripts::apply_common(builder, &svc.id);

    let (pos, size) = service_bounds(app);
    // Mounted offscreen by default; apply_active lo lleva on-screen si corresponde.
    window.add_child(
        builder,
        LogicalPosition::new(OFFSCREEN_X, pos.y),
        size,
    )?;
    Ok(())
}

pub fn unmount_child<R: Runtime>(app: &AppHandle<R>, label: &str) -> tauri::Result<()> {
    if let Some(wv) = app.get_webview(label) {
        wv.close()?;
    }
    Ok(())
}

pub fn apply_active<R: Runtime>(
    app: &AppHandle<R>,
    services: &[Service],
    active: Option<&str>,
) -> tauri::Result<()> {
    let (pos, size) = service_bounds(app);
    for svc in services {
        let label = label_for(&svc.id);
        let Some(wv) = app.get_webview(&label) else {
            continue;
        };
        let is_active = active == Some(svc.id.as_str());
        if is_active {
            let _ = wv.set_size(size);
            let _ = wv.set_position(pos);
        } else {
            let _ = wv.set_position(LogicalPosition::new(OFFSCREEN_X, pos.y));
        }
    }
    // Placeholder: visible cuando no hay svc activo, offscreen si lo hay.
    if let Some(ph) = app.get_webview(PLACEHOLDER_LABEL) {
        let _ = ph.set_size(size);
        if active.is_some() {
            let _ = ph.set_position(LogicalPosition::new(OFFSCREEN_X, pos.y));
        } else {
            let _ = ph.set_position(pos);
        }
    }
    // También reposicionar bar (main webview) por si la ventana cambió de tamaño.
    if let Some(main) = app.get_webview("main") {
        let (w, _h) = window_size(app);
        let _ = main.set_size(LogicalSize::new(w, BAR_HEIGHT as f64));
        let _ = main.set_position(LogicalPosition::new(0.0, 0.0));
    }
    Ok(())
}
