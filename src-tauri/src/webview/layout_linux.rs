use gtk::prelude::*;
use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl,
};

use crate::services::{data_dir_for, Service};

use super::{label_for, BAR_HEIGHT};

pub fn setup_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };

    // CSS provider global: pinta cualquier área no cubierta por webview en negro.
    // Evita que el bg blanco del tema GTK se vea como "borde" en la ventana.
    let css = "window, box, .background { background-color: #1a1a1a; border: none; box-shadow: none; }";
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
    // El primer child del vbox es el webview principal (Svelte). Lo fijamos a altura BAR_HEIGHT.
    let children = vbox.children();
    if let Some(first) = children.first() {
        vbox.set_child_packing(first, false, true, 0, gtk::PackType::Start);
        first.set_size_request(-1, BAR_HEIGHT as i32);
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
    let data_dir = data_dir_for(app, &svc.id)?;

    let mut builder =
        WebviewBuilder::new(&label, WebviewUrl::External(url)).data_directory(data_dir);
    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }
    // Inyección del navigator spoof — útil para WhatsApp y otros que detectan SO.
    builder = builder.initialization_script(NAV_SPOOF);

    let svc_id = svc.id.clone();
    builder = builder.on_page_load(move |webview, payload| {
        let phase = match payload.event() {
            PageLoadEvent::Started => "started",
            PageLoadEvent::Finished => "finished",
        };
        let _ = webview
            .app_handle()
            .emit("kolibri:page_load", (svc_id.clone(), phase));
    });

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

const NAV_SPOOF: &str = r#"
(function () {
  if (window.__KOLIBRI_NAV__) return;
  window.__KOLIBRI_NAV__ = true;
  try {
    var def = function (prop, value) {
      try { Object.defineProperty(navigator, prop, { get: function () { return value; }, configurable: true }); } catch (e) {}
    };
    def('platform', 'Linux x86_64');
    def('vendor', 'Google Inc.');
    def('vendorSub', '');
    def('product', 'Gecko');
    def('productSub', '20030107');
    def('appName', 'Netscape');
    def('appCodeName', 'Mozilla');
    def('appVersion', '5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36');
    def('maxTouchPoints', 0);
  } catch (e) {}
})();
"#;
