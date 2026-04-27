// Watchers de servicios mail nativos.
//
// - Gmail (IMAP): conexión persistente con IDLE cada ~28 min, reconnect on
//   error con backoff. Cuando IDLE devuelve EXISTS → FETCH del último UID
//   y emit `kolibri:mail:new`.
// - Graph (Outlook): polling cada 60s del más reciente; si cambió el id,
//   emit `kolibri:mail:new`.
//
// Auto-start: en setup() para todos los servicios autorizados, y al
// completar `*_oauth_authorize`. Auto-stop al `*_oauth_revoke` o al
// remove_service.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;

use super::{graph, imap as imap_engine, MailHeader};
use crate::services::{AppState, ServiceEngine};

pub struct WatcherRegistry {
    pub inner: Mutex<HashMap<String, Handle>>,
}

impl Default for WatcherRegistry {
    fn default() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }
}

pub struct Handle {
    /// Drop = cancel.
    pub stop: Option<oneshot::Sender<()>>,
    pub task: JoinHandle<()>,
}

pub fn stop<R: Runtime>(app: &AppHandle<R>, service_id: &str) {
    let registry = app.state::<WatcherRegistry>();
    let mut map = registry.inner.lock().expect("watcher mutex poisoned");
    if let Some(h) = map.remove(service_id) {
        if let Some(tx) = h.stop {
            let _ = tx.send(());
        }
        h.task.abort();
    }
}

pub fn start<R: Runtime>(app: &AppHandle<R>, service_id: &str) {
    // Si ya hay watcher, detenerlo primero (reauth puede invalidar token).
    stop(app, service_id);

    let svc = {
        let state = app.state::<AppState>();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        g.services.iter().find(|s| s.id == service_id).cloned()
    };
    let Some(svc) = svc else { return };

    let app_clone = app.clone();
    let id = service_id.to_string();
    let (tx, rx) = oneshot::channel::<()>();

    let task = match svc.engine {
        ServiceEngine::Imap => {
            let app2 = app_clone.clone();
            let id2 = id.clone();
            tauri::async_runtime::spawn(async move {
                imap_watch_loop(app2, id2, rx).await;
            })
        }
        ServiceEngine::Graph => {
            let app2 = app_clone.clone();
            let id2 = id.clone();
            tauri::async_runtime::spawn(async move {
                graph_watch_loop(app2, id2, rx).await;
            })
        }
        ServiceEngine::Webview => return,
    };

    let registry = app.state::<WatcherRegistry>();
    let mut map = registry.inner.lock().expect("watcher mutex poisoned");
    map.insert(
        service_id.to_string(),
        Handle {
            stop: Some(tx),
            task,
        },
    );
}

pub fn start_all_authorized<R: Runtime>(app: &AppHandle<R>) {
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        g.services
            .iter()
            .filter_map(|s| match s.engine {
                ServiceEngine::Imap if s.imap.as_ref().is_some_and(|c| c.authorized) => {
                    Some(s.id.clone())
                }
                ServiceEngine::Graph if s.graph.as_ref().is_some_and(|c| c.authorized) => {
                    Some(s.id.clone())
                }
                _ => None,
            })
            .collect()
    };
    for id in ids {
        start(app, &id);
    }
}

// ────────── Gmail IDLE loop ──────────
//
// async-imap session.idle() devuelve un IDLE handle. Esperamos con timeout
// de 28 min (Gmail desconecta tras 30). Cuando se interrumpe por EXISTS o
// timeout, salimos, hacemos done() y FETCH del último mensaje.

async fn imap_watch_loop<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
    mut cancel: oneshot::Receiver<()>,
) {
    let mut last_uid: u32 = 0;
    let mut backoff_secs: u64 = 5;

    loop {
        // Check cancel.
        if cancel.try_recv().is_ok() {
            return;
        }

        let result = imap_run_round(&app, &service_id, &mut last_uid).await;
        match result {
            Ok(()) => {
                backoff_secs = 5; // reset on success
            }
            Err(e) => {
                eprintln!("[watcher imap {}] error: {}", service_id, e);
                let wait = backoff_secs.min(300);
                backoff_secs = (backoff_secs * 2).min(300);
                tokio::select! {
                    _ = &mut cancel => return,
                    _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                }
            }
        }
    }
}

async fn imap_run_round<R: Runtime>(
    app: &AppHandle<R>,
    service_id: &str,
    last_uid: &mut u32,
) -> std::result::Result<(), String> {
    let token = imap_engine::current_access_token(app, service_id).await?;
    let email = {
        let state = app.state::<AppState>();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        g.services
            .iter()
            .find(|s| s.id == service_id)
            .and_then(|s| s.imap.as_ref().map(|c| c.email.clone()))
            .ok_or_else(|| "no imap config".to_string())?
    };

    let mut session = imap_engine::login_for_watcher(&email, &token)
        .await
        .map_err(|e| e.to_string())?;
    let mailbox = session
        .select("INBOX")
        .await
        .map_err(|e| format!("select: {}", e))?;
    let initial_exists = mailbox.exists;

    // Si es la primera ronda, sincronizar last_uid al UID del último mensaje.
    if *last_uid == 0 && initial_exists > 0 {
        if let Some(uid) = imap_engine::fetch_uid_at(&mut session, initial_exists).await? {
            *last_uid = uid;
        }
    }

    // IDLE con keepalive: cada 28 min cerramos y volvemos a empezar.
    let mut idle = session.idle();
    idle.init().await.map_err(|e| format!("idle init: {}", e))?;

    let timeout = Duration::from_secs(28 * 60);
    let (response_fut, _interrupt) = idle.wait();
    let response = tokio::time::timeout(timeout, response_fut).await;
    let mut session = idle
        .done()
        .await
        .map_err(|e| format!("idle done: {}", e))?;

    // Determinar si hubo cambio. EXISTS o RECENT en la respuesta.
    let triggered = match response {
        Ok(_) => true, // alguna respuesta del server (EXISTS, EXPUNGE, etc.)
        Err(_) => false, // timeout puro: re-conectar sin notif
    };

    if triggered {
        // Re-seleccionar para refrescar mailbox state.
        let mailbox = session
            .select("INBOX")
            .await
            .map_err(|e| format!("re-select: {}", e))?;
        let total = mailbox.exists;
        if total > 0 {
            // Buscar UIDs > last_uid.
            if let Some(uid) = imap_engine::fetch_uid_at(&mut session, total).await? {
                if uid > *last_uid {
                    if let Ok(new_headers) =
                        imap_engine::fetch_headers_for_uids(&mut session, *last_uid + 1, uid).await
                    {
                        for h in &new_headers {
                            emit_new_mail(app, service_id, h);
                        }
                    }
                    *last_uid = uid;
                }
            }
        }
    }

    let _ = session.logout().await;
    Ok(())
}

// ────────── Graph polling loop ──────────

const GRAPH_POLL_SECS: u64 = 60;

async fn graph_watch_loop<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
    mut cancel: oneshot::Receiver<()>,
) {
    let mut last_id: Option<String> = None;
    let mut backoff_secs: u64 = GRAPH_POLL_SECS;

    loop {
        if cancel.try_recv().is_ok() {
            return;
        }

        let result = graph_poll_round(&app, &service_id, &mut last_id).await;
        let wait = match result {
            Ok(()) => {
                backoff_secs = GRAPH_POLL_SECS;
                GRAPH_POLL_SECS
            }
            Err(e) => {
                eprintln!("[watcher graph {}] error: {}", service_id, e);
                let w = backoff_secs.min(600);
                backoff_secs = (backoff_secs * 2).min(600);
                w
            }
        };

        tokio::select! {
            _ = &mut cancel => return,
            _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
        }
    }
}

async fn graph_poll_round<R: Runtime>(
    app: &AppHandle<R>,
    service_id: &str,
    last_id: &mut Option<String>,
) -> std::result::Result<(), String> {
    let token = graph::current_access_token(app, service_id).await?;
    let url = format!(
        "{}/me/mailFolders/inbox/messages?\
         $top=10&\
         $orderby=receivedDateTime%20desc&\
         $select=id,subject,from,receivedDateTime,isRead,bodyPreview,hasAttachments,conversationId",
        graph::API_BASE
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("graph {}: {}", s, body));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let arr = json
        .get("value")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if arr.is_empty() {
        return Ok(());
    }

    // Tomamos el más reciente (primer elemento).
    let latest_id = arr
        .first()
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if last_id.is_none() {
        // Primera ronda: sólo registrar, no notificar (evita spam al arrancar).
        *last_id = latest_id;
        return Ok(());
    }

    if last_id == &latest_id {
        return Ok(());
    }

    // Hay novedad: emitir todos los que aparezcan antes del last_id conocido.
    for item in &arr {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if id == *last_id {
            break;
        }
        if let Ok(summary) =
            serde_json::from_value::<graph::GraphMessageSummaryPub>(item.clone())
        {
            let header = graph::summary_to_header_pub(summary);
            emit_new_mail(app, service_id, &header);
        }
    }
    *last_id = latest_id;
    Ok(())
}

// ────────── Notificación + emit ──────────

fn emit_new_mail<R: Runtime>(app: &AppHandle<R>, service_id: &str, header: &MailHeader) {
    let _ = app.emit(
        "kolibri:mail:new",
        serde_json::json!({
            "service_id": service_id,
            "header": header,
        }),
    );

    // Notificación nativa.
    let title = if header.from_name.is_empty() {
        header.from_addr.clone()
    } else {
        header.from_name.clone()
    };
    let body = if header.subject.is_empty() {
        "(sin asunto)".to_string()
    } else {
        header.subject.clone()
    };
    let _ = app
        .notification()
        .builder()
        .title(&title)
        .body(&body)
        .show();
}
