mod favicons;
mod services;
mod tray;
mod webview;

use services::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Manager, Emitter};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use webview::MountedRegistry;

#[derive(Default)]
pub struct UnreadState {
    pub counts: Mutex<HashMap<String, u32>>,
}

pub const DEFAULT_TOGGLE_SHORTCUT: &str = "Ctrl+Alt+K";

#[tauri::command]
fn get_toggle_shortcut(state: tauri::State<'_, AppState>) -> String {
    state
        .inner
        .lock()
        .unwrap()
        .toggle_shortcut
        .clone()
        .unwrap_or_else(|| DEFAULT_TOGGLE_SHORTCUT.to_string())
}

#[tauri::command]
fn set_toggle_shortcut<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    accelerator: String,
) -> Result<(), String> {
    let prev = {
        let mut g = state.inner.lock().unwrap();
        let prev = g
            .toggle_shortcut
            .clone()
            .unwrap_or_else(|| DEFAULT_TOGGLE_SHORTCUT.to_string());
        g.toggle_shortcut = Some(accelerator.clone());
        services::save(&app, &g).map_err(|e| e.to_string())?;
        prev
    };
    let gs = app.global_shortcut();
    let _ = gs.unregister(prev.as_str());
    gs.register(accelerator.as_str()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn emit_unread<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, UnreadState>,
    svc_state: tauri::State<'_, AppState>,
    service_id: String,
    count: u32,
) -> Result<(), String> {
    let (prev, total, name) = {
        let mut g = state.counts.lock().unwrap();
        let prev = g.get(&service_id).copied().unwrap_or(0);
        if count == 0 {
            g.remove(&service_id);
        } else {
            g.insert(service_id.clone(), count);
        }
        let total: u32 = g.values().sum();
        let name = svc_state
            .inner
            .lock()
            .unwrap()
            .services
            .iter()
            .find(|s| s.id == service_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        (prev, total, name)
    };
    let _ = app.emit(
        "kolibri:unread",
        serde_json::json!({"service_id": service_id, "count": count, "total": total}),
    );
    if let Some(tray) = app.tray_by_id("kolibri-tray") {
        let tip = if total > 0 {
            format!("Kolibri — {} sin leer", total)
        } else {
            "Kolibri".to_string()
        };
        let _ = tray.set_tooltip(Some(&tip));
    }
    if count > prev && !name.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let body = format!("{} mensajes sin leer", count);
        let _ = app
            .notification()
            .builder()
            .title(&name)
            .body(&body)
            .show();
    }
    Ok(())
}

fn toggle_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false) {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .manage(AppState::default())
        .manage(MountedRegistry::default())
        .manage(UnreadState::default())
        .invoke_handler(tauri::generate_handler![
            services::list_services,
            services::add_service,
            services::remove_service,
            services::reorder_services,
            services::update_service,
            webview::switch_service,
            webview::get_active_service,
            webview::reload_service,
            webview::window_minimize,
            webview::window_toggle_maximize,
            webview::window_close,
            get_toggle_shortcut,
            set_toggle_shortcut,
            emit_unread,
            favicons::get_favicon,
            favicons::clear_favicon_cache,
        ])
        .setup(|app| {
            tray::init(app.handle())?;
            webview::setup_main_window(app.handle())?;
            services::load_from_disk(app.handle())?;

            let accel = app
                .state::<AppState>()
                .inner
                .lock()
                .unwrap()
                .toggle_shortcut
                .clone()
                .unwrap_or_else(|| DEFAULT_TOGGLE_SHORTCUT.to_string());
            let _ = app.global_shortcut().register(accel.as_str());

            if let Some(icon) = app.default_window_icon().cloned() {
                if let Some(w) = app.get_window("main") {
                    let _ = w.set_icon(icon);
                }
            }

            if let Some(window) = app.get_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Resized(_)) {
                        let _ = webview::relayout(&app_handle);
                    }
                });
            }
            let _ = app.emit("kolibri:ready", ());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
