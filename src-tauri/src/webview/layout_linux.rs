use gtk::prelude::*;
use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, Runtime,
    WebviewUrl,
};

use crate::services::{data_dir_for, Service};

use super::{label_for, BAR_HEIGHT};

pub fn setup_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };
    let vbox = window.default_vbox()?;
    // El primer child del vbox es el webview principal (Svelte). Lo fijamos a altura BAR_HEIGHT.
    let children = vbox.children();
    if let Some(first) = children.first() {
        vbox.set_child_packing(first, false, false, 0, gtk::PackType::Start);
        first.set_size_request(-1, BAR_HEIGHT as i32);
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
    let data_dir = data_dir_for(app, &svc.id)?;

    let mut builder =
        WebviewBuilder::new(&label, WebviewUrl::External(url)).data_directory(data_dir);
    if let Some(ua) = svc.user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }
    // Inyección del navigator spoof — útil para WhatsApp y otros que detectan SO.
    builder = builder.initialization_script(NAV_SPOOF);

    // GtkBox ignora pos/size, valores son placeholders.
    window.add_child(
        builder,
        LogicalPosition::new(0.0, BAR_HEIGHT as f64),
        LogicalSize::new(800.0, 600.0),
    )?;

    // Recién montado: oculto por defecto. apply_active lo mostrará si corresponde.
    if let Some(wv) = app.get_webview(&label) {
        let _ = wv.with_webview(|pw| {
            pw.inner().hide();
        });
    }
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
