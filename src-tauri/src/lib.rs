mod services;
mod tray;
mod webview;

use services::AppState;
use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            services::list_services,
            services::add_service,
            services::remove_service,
            services::set_active_service,
            services::set_sidebar_collapsed,
            webview::update_content_bounds,
            webview::window_minimize,
            webview::window_toggle_maximize,
            webview::window_close,
        ])
        .setup(|app| {
            tray::init(app.handle())?;
            services::load_from_disk(app.handle())?;

            if let Some(window) = app.get_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Resized(_) = event {
                        let app = app_handle.clone();
                        let active = app
                            .state::<AppState>()
                            .inner
                            .lock()
                            .unwrap()
                            .active_id
                            .clone();
                        let _ = webview::set_active(&app, active.as_deref());
                    }
                });
            }

            // Forzar recálculo de bounds después de que la ventana se muestre y reporte su tamaño real.
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let active = app_handle
                    .state::<AppState>()
                    .inner
                    .lock()
                    .unwrap()
                    .active_id
                    .clone();
                let _ = webview::set_active(&app_handle, active.as_deref());
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                // placeholder por si queremos interceptar cierre futuro
            }
        });
}
