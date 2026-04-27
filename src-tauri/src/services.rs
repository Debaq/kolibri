use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use url::Url;

use crate::webview;

/// Engine que renderiza el servicio.
/// - `Webview` (default): WebView del SO. Modelo histórico (WhatsApp queda acá siempre).
/// - `Imap`: cliente IMAP+SMTP nativo con XOAUTH2. Para Gmail.
/// - `Graph`: REST Microsoft Graph con OAuth2. Para Outlook empresarial (M365).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceEngine {
    Webview,
    Imap,
    Graph,
}

impl Default for ServiceEngine {
    fn default() -> Self {
        ServiceEngine::Webview
    }
}

/// Config para engine IMAP (Gmail). Tokens OAuth2 viven en keyring del SO
/// (kind="imap", account=service_id), NO en services.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImapConfig {
    pub email: String,
    /// Client ID OAuth registrado por el user en Google Cloud Console.
    pub client_id: String,
    pub client_secret: String,
    /// True cuando el flow OAuth ya se completó (tokens en keyring).
    #[serde(default)]
    pub authorized: bool,
}

/// Config para engine Graph (Outlook M365). Tokens en keyring (kind="graph").
/// Default `client_id` = Microsoft Graph PowerShell (pre-aprobado, sin Azure setup).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphConfig {
    pub email: String,
    pub client_id: String,
    /// Tenant: "common" multi-tenant, GUID específico para empresarial fijo.
    #[serde(default = "graph_default_tenant")]
    pub tenant: String,
    #[serde(default)]
    pub authorized: bool,
}

fn graph_default_tenant() -> String {
    "common".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub user_agent: Option<String>,
    /// Slot dentro del host (0 = primero, 1 = segundo del mismo host, etc.).
    /// Servicios con el mismo host comparten data_dir solo si comparten slot.
    /// Para que N servicios de hosts distintos compartan WebProcess, usamos slot=0
    /// salvo colisión de host (segundo whatsapp.com → slot=1, etc.).
    #[serde(default)]
    pub session_slot: u32,
    /// Si está en true, la pestaña no se suspende por inactividad.
    #[serde(default)]
    pub keep_alive: bool,
    /// Si está en true, el servicio se monta con `data_directory` propio →
    /// WebContext + NetworkProcess + WebProcess aislados (vuelve al modelo v0.1.0
    /// para ese servicio). Necesario para Google/Microsoft que rechazan login si
    /// detectan WebContext compartido. Trade-off: +1 WebProcess en RAM.
    #[serde(default)]
    pub isolated_session: bool,
    #[serde(default)]
    pub engine: ServiceEngine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imap: Option<ImapConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphConfig>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PersistedState {
    pub services: Vec<Service>,
    pub active_id: Option<String>,
    #[serde(default)]
    pub toggle_shortcut: Option<String>,
    /// Minutos de inactividad tras los que se suspende una pestaña.
    /// 0 = nunca suspender. Default = 5.
    #[serde(default = "default_suspend_minutes")]
    pub inactive_suspend_minutes: u32,
}

fn default_suspend_minutes() -> u32 {
    5
}

#[derive(Default)]
pub struct AppState {
    pub inner: Mutex<PersistedState>,
}

const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Hosts que requieren WebContext aislado por default (Google/Microsoft rechazan
/// login en WebContext compartido → "browser may not be secure").
fn needs_isolated_session(host: &str) -> bool {
    const ISOLATED_DOMAINS: &[&str] = &[
        "google.com",
        "gmail.com",
        "googleusercontent.com",
        "live.com",
        "outlook.com",
        "office.com",
        "microsoft.com",
        "microsoftonline.com",
    ];
    ISOLATED_DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{}", d)))
}

fn config_path<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("services.json"))
}

/// Saneo del host para usarlo como nombre de directorio (evita ':', '/', etc.).
fn sanitize_host(host: &str) -> String {
    host.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .map(|h| sanitize_host(&h))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Carpeta de datos compartida por host+slot. Servicios del mismo (host, slot)
/// comparten WebContext → un único WebProcess.
pub fn data_dir_for_service<R: Runtime>(app: &AppHandle<R>, svc: &Service) -> tauri::Result<PathBuf> {
    let host = host_of(&svc.url);
    let dir = app
        .path()
        .app_data_dir()?
        .join("sessions")
        .join("by-host")
        .join(host)
        .join(svc.session_slot.to_string());
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Mantengo wrapper viejo por compatibilidad (no usado, llamadas migradas).
#[allow(dead_code)]
pub fn data_dir_for<R: Runtime>(app: &AppHandle<R>, id: &str) -> tauri::Result<PathBuf> {
    let dir = app.path().app_data_dir()?.join("sessions").join(id);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn save<R: Runtime>(app: &AppHandle<R>, state: &PersistedState) -> tauri::Result<()> {
    let path = config_path(app)?;
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
    fs::write(path, json)?;
    Ok(())
}

/// Asigna `session_slot` a un servicio nuevo en función de los existentes
/// del mismo host. Slots pequeños se reusan si quedaron libres.
fn allocate_slot(existing: &[Service], host: &str) -> u32 {
    let used: HashSet<u32> = existing
        .iter()
        .filter(|s| host_of(&s.url) == host)
        .map(|s| s.session_slot)
        .collect();
    let mut slot = 0u32;
    while used.contains(&slot) {
        slot += 1;
    }
    slot
}

/// Migración del esquema viejo `sessions/<uuid>/`: si encontramos al menos un
/// directorio cuyo nombre parece UUID, eliminamos todo `sessions/` y emitimos
/// evento al frontend para avisar al usuario.
fn migrate_old_sessions<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<bool> {
    let sessions_dir = app.path().app_data_dir()?.join("sessions");
    if !sessions_dir.exists() {
        return Ok(false);
    }
    let mut found_old = false;
    for entry in fs::read_dir(&sessions_dir)?.flatten() {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s == "by-host" {
            continue;
        }
        // Cualquier dir top-level distinto de `by-host` es esquema viejo
        // (UUIDs, timestamps `svc_177...`, `1777...`, etc.)
        if entry.path().is_dir() {
            let _ = fs::remove_dir_all(entry.path());
            found_old = true;
        }
    }
    Ok(found_old)
}

pub fn load_from_disk<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // 1) Migración: detectar/borrar sesiones viejas (esquema por UUID).
    let migrated = migrate_old_sessions(app).unwrap_or(false);

    let path = config_path(app)?;
    if !path.exists() {
        if migrated {
            let _ = app.emit("kolibri:sessions_migrated", ());
        }
        return Ok(());
    }
    let raw = fs::read_to_string(&path)?;
    let mut parsed: PersistedState = serde_json::from_str(&raw).unwrap_or_default();

    // 2) Asignar session_slot a servicios sin slot definido (orden estable).
    //    Recorremos en orden y para cada uno calculamos el slot mirando los previos.
    let mut assigned: Vec<Service> = Vec::with_capacity(parsed.services.len());
    for svc in parsed.services.iter() {
        let mut s = svc.clone();
        // Migración: hosts que requieren WebContext aislado (Google/Microsoft).
        // Si el servicio persistido no estaba marcado, lo marcamos ahora.
        let host = host_of(&s.url);
        if !s.isolated_session && needs_isolated_session(&host) {
            s.isolated_session = true;
        }
        // Si todos los servicios del mismo host tienen slot 0 por default y este es
        // el primero, queda 0. Si ya hay otro con slot 0 del mismo host → asigna 1, etc.
        let used: HashSet<u32> = assigned
            .iter()
            .filter(|x| host_of(&x.url) == host)
            .map(|x| x.session_slot)
            .collect();
        if used.contains(&s.session_slot) {
            // colisión: re-asignar el menor libre
            let mut slot = 0u32;
            while used.contains(&slot) {
                slot += 1;
            }
            s.session_slot = slot;
        }
        assigned.push(s);
    }
    parsed.services = assigned;

    let active_id = parsed.active_id.clone();
    let state: State<AppState> = app.state();
    {
        let mut guard = state.inner.lock().expect("AppState mutex poisoned");
        *guard = parsed;
    }

    // Persistir slots reasignados.
    {
        let g = state.inner.lock().expect("AppState mutex poisoned").clone();
        save(app, &g)?;
    }

    // 3) NO montamos el activo aquí: necesitamos esperar a que el WebView
    //    del bar esté capturado (ver setup_main_window → with_webview), de lo
    //    contrario el primer servicio se monta sin `related_view` y abre un
    //    WebProcess separado. El mount lo dispara `setup_main_window` desde
    //    dentro del callback de captura, llamando `mount_active_service`.
    let _ = active_id;

    if migrated {
        let _ = app.emit("kolibri:sessions_migrated", ());
    }
    Ok(())
}

#[tauri::command]
pub fn list_services(state: State<'_, AppState>) -> Vec<Service> {
    state.inner.lock().expect("AppState mutex poisoned").services.clone()
}

#[tauri::command]
pub fn add_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    url: String,
    icon: Option<String>,
    color: Option<String>,
) -> Result<Service, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let host = host_of(&url);
    let slot = {
        let g = state.inner.lock().expect("AppState mutex poisoned");
        allocate_slot(&g.services, &host)
    };
    let isolated = needs_isolated_session(&host);
    let svc = Service {
        id: id.clone(),
        name,
        url,
        icon,
        color,
        user_agent: Some(CHROME_UA.to_string()),
        session_slot: slot,
        keep_alive: false,
        isolated_session: isolated,
        engine: ServiceEngine::default(),
        imap: None,
        graph: None,
    };
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        g.services.push(svc.clone());
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    webview::ensure_mounted(&app, &svc).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:services_changed", ());
    Ok(svc)
}

/// Crea un servicio engine=Imap. No abre webview. Tras esto la UI debe
/// disparar `imap_oauth_authorize` para completar la auth.
#[tauri::command]
pub fn add_imap_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    client_id: String,
    client_secret: String,
    color: Option<String>,
) -> Result<Service, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let svc = Service {
        id: id.clone(),
        name,
        url: String::new(),
        icon: None,
        color,
        user_agent: None,
        session_slot: 0,
        keep_alive: false,
        isolated_session: false,
        engine: ServiceEngine::Imap,
        imap: Some(ImapConfig {
            email: String::new(),
            client_id,
            client_secret,
            authorized: false,
        }),
        graph: None,
    };
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        g.services.push(svc.clone());
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(svc)
}

#[tauri::command]
pub fn update_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    url: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    keep_alive: Option<bool>,
    isolated_session: Option<bool>,
) -> Result<Service, String> {
    let updated;
    let url_changed;
    let isolated_changed;
    let was_active;
    let new_slot_needed;
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        // Calcular si cambió host antes del borrow mut
        let prev_host;
        let prev_slot;
        let new_host_opt;
        {
            let svc_ref = g
                .services
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| "service not found".to_string())?;
            prev_host = host_of(&svc_ref.url);
            prev_slot = svc_ref.session_slot;
            new_host_opt = url.as_ref().map(|u| host_of(u));
        }
        let host_changed = new_host_opt
            .as_ref()
            .map(|h| h != &prev_host)
            .unwrap_or(false);
        // Si cambia host, asignar nuevo slot mirando los OTROS servicios del nuevo host.
        new_slot_needed = if host_changed {
            let new_host = new_host_opt.clone().unwrap();
            let used: HashSet<u32> = g
                .services
                .iter()
                .filter(|s| s.id != id && host_of(&s.url) == new_host)
                .map(|s| s.session_slot)
                .collect();
            let mut slot = 0u32;
            while used.contains(&slot) {
                slot += 1;
            }
            Some(slot)
        } else {
            None
        };

        let svc = g
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "service not found".to_string())?;
        url_changed = url.as_ref().map(|u| u != &svc.url).unwrap_or(false);
        if let Some(n) = name {
            svc.name = n;
        }
        if let Some(u) = url {
            svc.url = u;
        }
        if let Some(i) = icon {
            if !i.is_empty() {
                svc.icon = Some(i);
            }
        }
        if let Some(c) = color {
            if !c.is_empty() {
                svc.color = Some(c);
            }
        }
        if let Some(ka) = keep_alive {
            svc.keep_alive = ka;
        }
        isolated_changed = isolated_session
            .map(|new| new != svc.isolated_session)
            .unwrap_or(false);
        if let Some(iso) = isolated_session {
            svc.isolated_session = iso;
        }
        if let Some(slot) = new_slot_needed {
            svc.session_slot = slot;
        }
        updated = svc.clone();
        was_active = g.active_id.as_deref() == Some(&id);
        save(&app, &g).map_err(|e| e.to_string())?;
        let _ = prev_slot;
    }
    if url_changed || isolated_changed {
        webview::unmount(&app, &id).map_err(|e| e.to_string())?;
        webview::ensure_mounted(&app, &updated).map_err(|e| e.to_string())?;
        if was_active {
            webview::set_active(&app, Some(&id)).map_err(|e| e.to_string())?;
        }
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(updated)
}

#[tauri::command]
pub fn clear_service_icon<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "service not found".to_string())?;
        svc.icon = None;
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_service_color<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "service not found".to_string())?;
        svc.color = None;
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

/// Pure: aplica el orden `ids` a `services`. Los IDs no presentes en `ids`
/// quedan al final (orden original relativo no garantizado por el HashMap).
pub fn apply_reorder(services: Vec<Service>, ids: &[String]) -> Vec<Service> {
    let mut by_id: std::collections::HashMap<String, Service> =
        services.into_iter().map(|s| (s.id.clone(), s)).collect();
    let mut out: Vec<Service> = Vec::with_capacity(by_id.len());
    for id in ids.iter() {
        if let Some(s) = by_id.remove(id) {
            out.push(s);
        }
    }
    for (_, s) in by_id.drain() {
        out.push(s);
    }
    out
}

#[tauri::command]
pub fn reorder_services<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let mut g = state.inner.lock().expect("AppState mutex poisoned");
    let services = std::mem::take(&mut g.services);
    g.services = apply_reorder(services, &ids);
    save(&app, &g).map_err(|e| e.to_string())?;
    drop(g);
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

#[tauri::command]
pub fn remove_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let was_active;
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        g.services.retain(|s| s.id != id);
        was_active = g.active_id.as_deref() == Some(&id);
        if was_active {
            g.active_id = None;
        }
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    webview::unmount(&app, &id).map_err(|e| e.to_string())?;
    if was_active {
        webview::set_active(&app, None).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

/// Monta el servicio activo (si hay) y aplica visibilidad. Llamado desde
/// el callback de captura del bar webview, garantizando que `related_view`
/// esté disponible antes de crear el primer webview de servicio.
pub fn mount_active_service<R: Runtime>(app: &AppHandle<R>) {
    let state: State<AppState> = app.state();
    let (active, svc_opt) = {
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let active = g.active_id.clone();
        let svc = active
            .as_deref()
            .and_then(|id| g.services.iter().find(|s| s.id == id).cloned());
        (active, svc)
    };
    if let Some(svc) = svc_opt {
        let _ = webview::ensure_mounted(app, &svc);
    }
    if active.is_some() {
        let _ = webview::set_active(app, active.as_deref());
    }
}

#[tauri::command]
pub fn get_inactive_suspend_minutes(state: State<'_, AppState>) -> u32 {
    state
        .inner
        .lock()
        .expect("AppState mutex poisoned")
        .inactive_suspend_minutes
}

#[tauri::command]
pub fn set_inactive_suspend_minutes<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    minutes: u32,
) -> Result<(), String> {
    let mut g = state.inner.lock().expect("AppState mutex poisoned");
    g.inactive_suspend_minutes = minutes;
    save(&app, &g).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_keep_alive<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    keep_alive: bool,
) -> Result<(), String> {
    {
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "service not found".to_string())?;
        svc.keep_alive = keep_alive;
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

/// Limpia cookies/cache de un servicio borrando su data_dir (host+slot).
/// Si otros servicios comparten ese dir (mismo host+slot), también los desmonta
/// para que el próximo mount cree estado limpio. Devuelve la lista de IDs afectados.
#[tauri::command]
pub fn clear_service_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, String> {
    let (target, sharing, was_active_id) = {
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let target = g
            .services
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| "service not found".to_string())?;
        let host = host_of(&target.url);
        let slot = target.session_slot;
        let sharing: Vec<Service> = g
            .services
            .iter()
            .filter(|s| s.id != id && host_of(&s.url) == host && s.session_slot == slot)
            .cloned()
            .collect();
        (target, sharing, g.active_id.clone())
    };

    let dir = data_dir_for_service(&app, &target).map_err(|e| e.to_string())?;
    let mut affected = vec![target.id.clone()];
    webview::unmount(&app, &target.id).map_err(|e| e.to_string())?;
    for s in sharing.iter() {
        affected.push(s.id.clone());
        webview::unmount(&app, &s.id).map_err(|e| e.to_string())?;
    }

    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    if let Some(active) = was_active_id.as_deref() {
        if affected.iter().any(|i| i == active) {
            // Remontar el activo en estado limpio
            let g = state.inner.lock().expect("AppState mutex poisoned");
            if let Some(svc) = g.services.iter().find(|s| s.id == active).cloned() {
                drop(g);
                webview::ensure_mounted(&app, &svc).map_err(|e| e.to_string())?;
                webview::set_active(&app, Some(active)).map_err(|e| e.to_string())?;
            }
        }
    }

    let _ = app.emit("kolibri:services_changed", ());
    Ok(affected)
}

/// Suspende (unmount real) un servicio si NO es el activo y NO tiene keep_alive.
#[tauri::command]
pub fn suspend_service<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let g = state.inner.lock().expect("AppState mutex poisoned");
    if g.active_id.as_deref() == Some(&id) {
        return Ok(false);
    }
    if let Some(svc) = g.services.iter().find(|s| s.id == id) {
        if svc.keep_alive {
            return Ok(false);
        }
    } else {
        return Ok(false);
    }
    drop(g);
    webview::unmount(&app, &id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(id: &str, url: &str, slot: u32) -> Service {
        Service {
            id: id.to_string(),
            name: id.to_string(),
            url: url.to_string(),
            icon: None,
            color: None,
            user_agent: None,
            session_slot: slot,
            keep_alive: false,
            isolated_session: false,
        }
    }

    #[test]
    fn sanitize_host_strips_unsafe() {
        assert_eq!(sanitize_host("web.whatsapp.com"), "web.whatsapp.com");
        assert_eq!(sanitize_host("foo:8080"), "foo_8080");
        assert_eq!(sanitize_host("a/b\\c"), "a_b_c");
    }

    #[test]
    fn host_of_extracts() {
        assert_eq!(host_of("https://web.whatsapp.com/x"), "web.whatsapp.com");
        assert_eq!(host_of("not-a-url"), "unknown");
        assert_eq!(host_of("https://foo:8080/"), "foo");
    }

    #[test]
    fn allocate_slot_first_is_zero() {
        let v: Vec<Service> = vec![];
        assert_eq!(allocate_slot(&v, "web.whatsapp.com"), 0);
    }

    #[test]
    fn allocate_slot_picks_next_free() {
        let v = vec![
            svc("a", "https://web.whatsapp.com/", 0),
            svc("b", "https://web.whatsapp.com/", 1),
        ];
        assert_eq!(allocate_slot(&v, "web.whatsapp.com"), 2);
    }

    #[test]
    fn allocate_slot_reuses_gap() {
        let v = vec![
            svc("a", "https://web.whatsapp.com/", 0),
            svc("b", "https://web.whatsapp.com/", 2),
        ];
        assert_eq!(allocate_slot(&v, "web.whatsapp.com"), 1);
    }

    #[test]
    fn allocate_slot_isolated_per_host() {
        let v = vec![svc("a", "https://gmail.com/", 0)];
        assert_eq!(allocate_slot(&v, "web.whatsapp.com"), 0);
    }

    #[test]
    fn apply_reorder_respects_ids() {
        let v = vec![
            svc("a", "https://a.com/", 0),
            svc("b", "https://b.com/", 0),
            svc("c", "https://c.com/", 0),
        ];
        let ids = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        let out = apply_reorder(v, &ids);
        let order: Vec<&str> = out.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(order, vec!["c", "a", "b"]);
    }

    #[test]
    fn apply_reorder_unknown_ids_appended() {
        let v = vec![
            svc("a", "https://a.com/", 0),
            svc("b", "https://b.com/", 0),
        ];
        let ids = vec!["b".to_string()];
        let out = apply_reorder(v, &ids);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "b");
        assert_eq!(out[1].id, "a");
    }

    #[test]
    fn apply_reorder_ignores_unknown_in_ids() {
        let v = vec![svc("a", "https://a.com/", 0)];
        let ids = vec!["zzz".to_string(), "a".to_string()];
        let out = apply_reorder(v, &ids);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "a");
    }

    #[test]
    fn isolated_session_matches_google_microsoft() {
        assert!(needs_isolated_session("mail.google.com"));
        assert!(needs_isolated_session("accounts.google.com"));
        assert!(needs_isolated_session("google.com"));
        assert!(needs_isolated_session("outlook.live.com"));
        assert!(needs_isolated_session("login.live.com"));
        assert!(needs_isolated_session("login.microsoftonline.com"));
        assert!(needs_isolated_session("outlook.office.com"));
    }

    #[test]
    fn isolated_session_does_not_match_unrelated() {
        assert!(!needs_isolated_session("web.whatsapp.com"));
        assert!(!needs_isolated_session("slack.com"));
        assert!(!needs_isolated_session("notion.so"));
        // Sufijos engañosos no deben matchear (solo `.google.com` o exact).
        assert!(!needs_isolated_session("notgoogle.com"));
        assert!(!needs_isolated_session("malicious-google.com.evil.com"));
    }

    #[test]
    fn host_of_handles_subdomains_and_ports() {
        assert_eq!(host_of("https://mail.google.com"), "mail.google.com");
        assert_eq!(host_of("https://outlook.live.com:443/mail"), "outlook.live.com");
        assert_eq!(host_of("https://web.whatsapp.com/?param=x"), "web.whatsapp.com");
    }

    #[test]
    fn host_of_invalid_url_returns_unknown() {
        assert_eq!(host_of(""), "unknown");
        assert_eq!(host_of("not a url"), "unknown");
        assert_eq!(host_of("javascript:alert(1)"), "unknown");
    }

    #[test]
    fn host_of_normalizes_uppercase() {
        // Url crate normaliza host a minúsculas (per RFC 3986).
        assert_eq!(host_of("https://Mail.Google.COM/"), "mail.google.com");
    }

    #[test]
    fn sanitize_host_preserves_safe_chars() {
        assert_eq!(sanitize_host("a-b_c.d.e"), "a-b_c.d.e");
        assert_eq!(sanitize_host("ABC123"), "ABC123");
    }

    #[test]
    fn sanitize_host_replaces_unicode_and_symbols() {
        // 'é' es 1 char Unicode → 1 underscore.
        assert_eq!(sanitize_host("café.com"), "caf_.com");
        assert_eq!(sanitize_host("a b@c"), "a_b_c");
    }

    #[test]
    fn allocate_slot_empty_existing() {
        let v: Vec<Service> = vec![];
        assert_eq!(allocate_slot(&v, "any.host"), 0);
    }

    #[test]
    fn allocate_slot_fills_large_gaps() {
        let v = vec![
            svc("a", "https://x.com/", 0),
            svc("b", "https://x.com/", 1),
            svc("c", "https://x.com/", 5),
        ];
        // Slot libre más bajo es 2.
        assert_eq!(allocate_slot(&v, "x.com"), 2);
    }

    #[test]
    fn apply_reorder_empty_inputs() {
        let out = apply_reorder(vec![], &[]);
        assert!(out.is_empty());

        let v = vec![svc("a", "https://a.com/", 0)];
        let out = apply_reorder(v, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "a");
    }

    #[test]
    fn apply_reorder_duplicate_ids_taken_once() {
        let v = vec![
            svc("a", "https://a.com/", 0),
            svc("b", "https://b.com/", 0),
        ];
        let ids = vec!["a".to_string(), "a".to_string(), "b".to_string()];
        let out = apply_reorder(v, &ids);
        assert_eq!(out.len(), 2);
        let order: Vec<&str> = out.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(order, vec!["a", "b"]);
    }

    #[test]
    fn default_suspend_minutes_is_five() {
        assert_eq!(default_suspend_minutes(), 5);
    }

    #[test]
    fn isolated_session_case_insensitive_via_host_of() {
        // host_of normaliza a minúsculas → match consistente.
        let host = host_of("https://Mail.Google.com/");
        assert!(needs_isolated_session(&host));
    }

    #[test]
    fn persisted_state_serde_roundtrip() {
        let original = PersistedState {
            services: vec![Service {
                id: "abc".into(),
                name: "Test".into(),
                url: "https://test.com".into(),
                icon: Some("T".into()),
                color: Some("#fff".into()),
                user_agent: None,
                session_slot: 2,
                keep_alive: true,
                isolated_session: true,
            }],
            active_id: Some("abc".into()),
            toggle_shortcut: Some("Ctrl+Alt+K".into()),
            inactive_suspend_minutes: 10,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.services.len(), 1);
        assert_eq!(parsed.services[0].id, "abc");
        assert_eq!(parsed.services[0].session_slot, 2);
        assert!(parsed.services[0].keep_alive);
        assert!(parsed.services[0].isolated_session);
        assert_eq!(parsed.active_id.as_deref(), Some("abc"));
        assert_eq!(parsed.inactive_suspend_minutes, 10);
    }

    #[test]
    fn persisted_state_loads_old_config_without_new_fields() {
        // Un config persistido en 0.1.3 (sin isolated_session) debe parsear OK
        // y dejar el campo en false (default serde).
        let old_json = r#"{
            "services": [{
                "id": "x",
                "name": "X",
                "url": "https://x.com",
                "icon": null,
                "color": null,
                "user_agent": null,
                "session_slot": 0,
                "keep_alive": false
            }],
            "active_id": null
        }"#;
        let parsed: PersistedState = serde_json::from_str(old_json).unwrap();
        assert_eq!(parsed.services.len(), 1);
        assert!(!parsed.services[0].isolated_session);
        assert_eq!(parsed.inactive_suspend_minutes, 5);
    }
}
