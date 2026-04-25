mod services;
mod tray;
mod webview;

use services::AppState;
use tauri::Manager;
use webview::MountedRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .manage(MountedRegistry::default())
        .invoke_handler(tauri::generate_handler![
            services::list_services,
            services::add_service,
            services::remove_service,
            webview::switch_service,
            webview::get_active_service,
            webview::window_minimize,
            webview::window_toggle_maximize,
            webview::window_close,
        ])
        .setup(|app| {
            tray::init(app.handle())?;
            webview::setup_main_window(app.handle())?;
            services::load_from_disk(app.handle())?;

            // Resize de main → relayout (Windows lo necesita; Linux es noop).
            if let Some(window) = app.get_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Resized(_)) {
                        let _ = webview::relayout(&app_handle);
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
