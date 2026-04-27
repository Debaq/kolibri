#[cfg(target_os = "linux")]
mod desktop_install;
mod engines;
mod favicons;
mod ram_log;
mod services;
mod sysmem;
mod tray;
mod update;
mod webview;

use services::AppState;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{Manager, Emitter};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use webview::MountedRegistry;

#[derive(Default)]
pub struct UnreadState {
    pub counts: Mutex<HashMap<String, u32>>,
    pub seeded: Mutex<HashSet<String>>,
}

pub const DEFAULT_TOGGLE_SHORTCUT: &str = "Ctrl+Alt+K";

// ──────────── Mail dispatcher ────────────
//
// Comandos Tauri unificados que dispatchan al engine correcto según
// `Service.engine`. UI Svelte llama solo estos; no conoce IMAP vs Graph.

use engines::{MailEngine, MailHeader, MailMessage, OutgoingMessage};
use services::ServiceEngine;

/// Construye el engine adecuado, refrescando access_token desde keyring
/// si hace falta. Async porque el refresh hace HTTP.
async fn engine_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    service_id: &str,
) -> std::result::Result<Box<dyn MailEngine>, String> {
    let svc = {
        let state: tauri::State<AppState> = app.state();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        g.services
            .iter()
            .find(|s| s.id == service_id)
            .cloned()
            .ok_or_else(|| format!("service {} not found", service_id))?
    };
    match svc.engine {
        ServiceEngine::Webview => Err("service is webview, no mail engine".into()),
        ServiceEngine::Imap => {
            let cfg = svc
                .imap
                .clone()
                .ok_or_else(|| "imap config missing".to_string())?;
            if !cfg.authorized {
                return Err("servicio sin autorizar (corré imap_oauth_authorize)".into());
            }
            let token = engines::imap::current_access_token(app, service_id).await?;
            Ok(Box::new(engines::imap::ImapEngine::new(cfg, token)))
        }
        ServiceEngine::Graph => {
            let cfg = svc
                .graph
                .clone()
                .ok_or_else(|| "graph config missing".to_string())?;
            if !cfg.authorized {
                return Err("servicio Graph sin autorizar (corré graph_oauth_authorize)".into());
            }
            let token = engines::graph::current_access_token(app, service_id).await?;
            Ok(Box::new(engines::graph::GraphEngine::new(cfg, token)))
        }
    }
}

#[tauri::command]
async fn mail_list_inbox<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
) -> std::result::Result<Vec<MailHeader>, String> {
    let engine = engine_for(&app, &service_id).await?;
    engine
        .list_inbox(offset.unwrap_or(0), limit.unwrap_or(50))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_get_message<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    id: String,
) -> std::result::Result<MailMessage, String> {
    let engine = engine_for(&app, &service_id).await?;
    engine.get_message(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_search<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
) -> std::result::Result<Vec<MailHeader>, String> {
    let engine = engine_for(&app, &service_id).await?;
    engine
        .search(&query, offset.unwrap_or(0), limit.unwrap_or(50))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_mark_read<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    id: String,
    read: bool,
) -> std::result::Result<(), String> {
    let engine = engine_for(&app, &service_id).await?;
    engine.mark_read(&id, read).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_archive<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    id: String,
) -> std::result::Result<(), String> {
    let engine = engine_for(&app, &service_id).await?;
    engine.archive(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_delete<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    id: String,
) -> std::result::Result<(), String> {
    let engine = engine_for(&app, &service_id).await?;
    engine.delete(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mail_send<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    service_id: String,
    msg: OutgoingMessage,
) -> std::result::Result<(), String> {
    let engine = engine_for(&app, &service_id).await?;
    engine.send(&msg).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_toggle_shortcut(state: tauri::State<'_, AppState>) -> String {
    state
        .inner
        .lock()
        .expect("state mutex poisoned")
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
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
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
    let is_seed = state
        .seeded
        .lock()
        .expect("UnreadState seeded mutex poisoned")
        .insert(service_id.clone());
    let (prev, total, name) = {
        let mut g = state.counts.lock().expect("UnreadState mutex poisoned");
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
            .expect("AppState mutex poisoned")
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
    if !is_seed && count > prev && !name.is_empty() {
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

#[tauri::command]
fn reinstall_desktop_entry<R: tauri::Runtime>(_app: tauri::AppHandle<R>) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        return desktop_install::install(true).map_err(|e| e.to_string());
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(false)
    }
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
        .manage(sysmem::SysState::default())
        .invoke_handler(tauri::generate_handler![
            services::list_services,
            services::add_service,
            services::add_imap_service,
            services::add_graph_service,
            services::remove_service,
            services::reorder_services,
            services::update_service,
            services::clear_service_icon,
            services::clear_service_color,
            services::get_inactive_suspend_minutes,
            services::set_inactive_suspend_minutes,
            services::set_keep_alive,
            services::suspend_service,
            services::clear_service_session,
            reinstall_desktop_entry,
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
            sysmem::get_memory_usage,
            update::check_update,
            mail_list_inbox,
            mail_get_message,
            mail_mark_read,
            mail_archive,
            mail_delete,
            mail_send,
            mail_search,
            engines::imap::imap_oauth_authorize,
            engines::imap::imap_oauth_revoke,
            engines::graph::graph_oauth_authorize,
            engines::graph::graph_oauth_revoke,
        ])
        .setup(|app| {
            #[cfg(target_os = "linux")]
            {
                let _ = desktop_install::install(false);
            }
            tray::init(app.handle())?;

            // RAM logger: activado vía `--log-ram` (env KOLIBRI_LOG_RAM=1).
            if std::env::var_os("KOLIBRI_LOG_RAM").is_some() {
                if let Ok(dir) = app.path().app_data_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let log_path = dir.join("ram.log");
                    match ram_log::init(log_path.clone()) {
                        Ok(()) => {
                            eprintln!("[kolibri] RAM log: {}", log_path.display());
                            ram_log::start_ticker(app.handle().clone());
                            ram_log::snapshot(app.handle(), "startup");
                        }
                        Err(e) => eprintln!("[kolibri] RAM log init failed: {}", e),
                    }
                }
            }

            webview::setup_main_window(app.handle())?;
            services::load_from_disk(app.handle())?;

            let accel = app
                .state::<AppState>()
                .inner
                .lock()
                .expect("AppState mutex poisoned")
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
