mod services;
mod tray;
mod webview;

use services::AppState;
use tauri::Manager;
use webview::GeometryState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .manage(GeometryState::default())
        .invoke_handler(tauri::generate_handler![
            services::list_services,
            services::add_service,
            services::remove_service,
            services::set_active_service,
            services::set_sidebar_collapsed,
            webview::switch_service,
            webview::get_active_service,
            webview::open_add_dialog,
            webview::open_settings,
            webview::window_minimize,
            webview::window_toggle_maximize,
            webview::window_close,
        ])
        .setup(|app| {
            tray::init(app.handle())?;
            services::load_from_disk(app.handle())?;

            // Listener de geometría en main para sincronizar a otras ventanas.
            if let Some(window) = app.get_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
                    ) {
                        if let Some(w) = app_handle.get_window("main") {
                            if let Some(g) = webview::snapshot_geometry(&w) {
                                webview::store_geometry(&app_handle, g);
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
