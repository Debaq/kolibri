use gtk::prelude::*;
use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl,
};

use crate::services::{data_dir_for_service, Service};

use super::{label_for, scripts, BarWebViewHandle, GtkMainThreadView, BAR_HEIGHT};

pub fn setup_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // Registrar el handle compartido del WebView del bar antes de capturarlo.
    // Idempotente vía try_state: si ya está, no lo pisamos.
    if app.try_state::<BarWebViewHandle>().is_none() {
        app.manage(BarWebViewHandle::default());
    }

    let Some(window) = app.get_window("main") else {
        return Ok(());
    };

    // CSS provider en screen pero acotado por nombre de widget al window principal
    // (#kolibri-main). Antes el selector `window, box, .background` afectaba a
    // todos los GtkWindow del proceso (ej. GtkFileChooserDialog) dejando texto
    // negro sobre fondo negro.
    if let Ok(gtk_win) = window.gtk_window() {
        gtk_win.set_widget_name("kolibri-main");
    }
    let css = "#kolibri-main, #kolibri-main box, #kolibri-main .background { background-color: #1a1a1a; border: none; box-shadow: none; }";
    let provider = gtk::CssProvider::new();
    if provider.load_from_data(css.as_bytes()).is_ok() {
        if let Some(screen) = gtk::gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    let vbox = window.default_vbox()?;
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let bar_px = (BAR_HEIGHT as f64 * scale).round() as i32;
    // El primer child del vbox es el webview principal (Svelte). Lo fijamos a altura BAR_HEIGHT.
    let children = vbox.children();
    if let Some(first) = children.first() {
        vbox.set_child_packing(first, false, true, 0, gtk::PackType::Start);
        first.set_size_request(-1, bar_px);
        first.set_hexpand(true);
        first.set_halign(gtk::Align::Fill);
        first.set_valign(gtk::Align::Start);
        first.set_margin_top(0);
        first.set_margin_bottom(0);
        first.set_margin_start(0);
        first.set_margin_end(0);
    }
    // Quitar márgenes/padding del propio vbox por si el theme GTK del usuario los pone.
    vbox.set_margin_top(0);
    vbox.set_margin_bottom(0);
    vbox.set_margin_start(0);
    vbox.set_margin_end(0);
    vbox.set_spacing(0);

    // Capturar el `webkit2gtk::WebView` del bar para reutilizarlo como
    // `related_view` de cada servicio → todos comparten el mismo WebProcess.
    // El closure de `with_webview` corre en el GTK main thread.
    if let Some(main_wv) = app.get_webview_window("main") {
        let app_for_capture = app.clone();
        let _ = main_wv.with_webview(move |pw| {
            let view = pw.inner();
            if let Some(handle) = app_for_capture.try_state::<BarWebViewHandle>() {
                let mut g = handle.inner().0.lock().expect("BarWebViewHandle mutex poisoned");
                *g = Some(GtkMainThreadView(view));
            }
            // Bar capturado: ahora sí podemos montar el servicio activo con
            // related_view → comparte WebProcess con el bar.
            crate::services::mount_active_service(&app_for_capture);
        });
    }

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
    // No pasamos data_directory: con `with_related_view` los WebView comparten
    // el WebContext default y por tanto el mismo WebProcess. Las cookies
    // persisten por dominio en el dir compartido del context (gestión nativa).
    let _ = data_dir_for_service(app, svc)?;

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .disable_drag_drop_handler();
    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }
    builder = scripts::apply_common(builder, &svc.id);

    // Reutilizar el WebProcess del bar (API nativa de WebKitGTK:
    // `webkit_web_view_new_with_related_view`). Si por alguna razón aún no
    // está cacheado, montamos sin related_view (fallback → proceso separado).
    if let Some(handle) = app.try_state::<BarWebViewHandle>() {
        let bar_view = handle
            .inner()
            .0
            .lock()
            .expect("BarWebViewHandle mutex poisoned")
            .as_ref()
            .map(|v| v.0.clone());
        if let Some(view) = bar_view {
            builder = builder.with_related_view(view);
        }
    }

    // GtkBox ignora pos/size, valores son placeholders.
    window.add_child(
        builder,
        LogicalPosition::new(0.0, BAR_HEIGHT as f64),
        LogicalSize::new(800.0, 600.0),
    )?;

    // No ocultar antes de que widget se realice: WebKitGTK cancela el load
    // ("Load request cancelled") si el widget se esconde antes de mapear.
    // apply_active() ocultará los inactivos una vez que el load arrancó.
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
    for svc in services {
        let label = label_for(&svc.id);
        let Some(wv) = app.get_webview(&label) else {
            continue;
        };
        let is_active = active == Some(svc.id.as_str());
        let _ = wv.with_webview(move |pw| {
            let widget = pw.inner();
            if is_active {
                widget.show();
            } else {
                widget.hide();
            }
        });
    }
    Ok(())
}

