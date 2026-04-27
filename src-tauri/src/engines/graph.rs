#![allow(dead_code)] // algunas constantes consumen steps 6+
// GraphEngine: Outlook M365 vía Microsoft Graph REST.
//
// Step 5: read-only (list_inbox + get_message). Resto sigue stub hasta step 6.
//
// Auth: PKCE OAuth2 contra https://login.microsoftonline.com. Public client
// (sin secret). Default `client_id` = Microsoft Graph PowerShell público —
// pre-aprobado por Microsoft, evita registrar app en Azure AD si el tenant
// del usuario lo bloquea.

use async_trait::async_trait;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use super::oauth;
use super::{MailEngine, MailError, MailHeader, MailMessage, MessageId, OutgoingMessage, Result};
use crate::services::{save, AppState, GraphConfig};

pub const API_BASE: &str = "https://graph.microsoft.com/v1.0";
pub const SCOPES: &[&str] = &[
    "Mail.ReadWrite",
    "Mail.Send",
    "User.Read",
    "offline_access",
];

/// Microsoft Graph PowerShell — client_id público pre-aprobado por Microsoft.
pub const PUBLIC_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

pub const KEYRING_KIND: &str = "graph";

pub fn auth_url(tenant: &str) -> String {
    format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
        tenant
    )
}

pub fn token_url(tenant: &str) -> String {
    format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        tenant
    )
}

pub struct GraphEngine {
    pub config: GraphConfig,
    pub access_token: String,
}

impl GraphEngine {
    pub fn new(config: GraphConfig, access_token: String) -> Self {
        Self {
            config,
            access_token,
        }
    }

    fn http() -> reqwest::Client {
        reqwest::Client::new()
    }
}

// ────────── Helpers de parseo ──────────

/// "2026-04-27T15:30:00Z" → unix seconds. Devuelve 0 si no parsea.
fn iso_to_ts(s: &str) -> i64 {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339)
        .map(|d| d.unix_timestamp())
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct GraphList<T> {
    value: Vec<T>,
}

#[derive(Deserialize)]
struct GraphRecipient {
    #[serde(rename = "emailAddress")]
    email_address: GraphEmailAddress,
}

#[derive(Deserialize)]
struct GraphEmailAddress {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

#[derive(Deserialize)]
struct GraphFlag {
    #[serde(rename = "flagStatus")]
    flag_status: Option<String>,
}

#[derive(Deserialize)]
struct GraphBody {
    #[serde(rename = "contentType")]
    content_type: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMessageSummary {
    id: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    from: Option<GraphRecipient>,
    #[serde(rename = "receivedDateTime", default)]
    received: Option<String>,
    #[serde(rename = "isRead", default)]
    is_read: Option<bool>,
    #[serde(default)]
    flag: Option<GraphFlag>,
    #[serde(rename = "bodyPreview", default)]
    body_preview: Option<String>,
    #[serde(rename = "hasAttachments", default)]
    has_attachments: Option<bool>,
    #[serde(rename = "conversationId", default)]
    conversation_id: Option<String>,
}

#[derive(Deserialize)]
struct GraphMessageFull {
    id: String,
    #[serde(rename = "internetMessageId", default)]
    internet_message_id: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    from: Option<GraphRecipient>,
    #[serde(rename = "toRecipients", default)]
    to_recipients: Vec<GraphRecipient>,
    #[serde(rename = "ccRecipients", default)]
    cc_recipients: Vec<GraphRecipient>,
    #[serde(rename = "receivedDateTime", default)]
    received: Option<String>,
    #[serde(default)]
    body: Option<GraphBody>,
    #[serde(rename = "hasAttachments", default)]
    has_attachments: Option<bool>,
    #[serde(rename = "conversationId", default)]
    conversation_id: Option<String>,
}

/// Re-export público para watcher.rs (que parsea respuestas de polling).
pub type GraphMessageSummaryPub = GraphMessageSummary;
pub fn summary_to_header_pub(m: GraphMessageSummary) -> MailHeader {
    summary_to_header(m)
}

fn summary_to_header(m: GraphMessageSummary) -> MailHeader {
    let (from_name, from_addr) = match m.from {
        Some(r) => (
            r.email_address.name.unwrap_or_default(),
            r.email_address.address.unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    let date_ts = m.received.as_deref().map(iso_to_ts).unwrap_or(0);
    let flagged = m
        .flag
        .and_then(|f| f.flag_status)
        .map(|s| s == "flagged")
        .unwrap_or(false);
    MailHeader {
        id: m.id,
        from_name: if from_name.is_empty() {
            from_addr.clone()
        } else {
            from_name
        },
        from_addr,
        subject: m.subject.unwrap_or_default(),
        date_ts,
        snippet: m.body_preview.unwrap_or_default(),
        seen: m.is_read.unwrap_or(false),
        flagged,
        has_attachments: m.has_attachments.unwrap_or(false),
        thread_id: m.conversation_id,
    }
}

fn full_to_message(m: GraphMessageFull) -> MailMessage {
    let (from_name, from_addr) = match m.from {
        Some(r) => (
            r.email_address.name.unwrap_or_default(),
            r.email_address.address.unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    let to: Vec<String> = m
        .to_recipients
        .into_iter()
        .filter_map(|r| r.email_address.address)
        .collect();
    let cc: Vec<String> = m
        .cc_recipients
        .into_iter()
        .filter_map(|r| r.email_address.address)
        .collect();
    let date_ts = m.received.as_deref().map(iso_to_ts).unwrap_or(0);
    let (body_text, body_html) = match m.body {
        Some(b) => {
            let content = b.content.unwrap_or_default();
            let is_html = b
                .content_type
                .map(|t| t.eq_ignore_ascii_case("html"))
                .unwrap_or(false);
            if is_html {
                (String::new(), Some(content))
            } else {
                (content, None)
            }
        }
        None => (String::new(), None),
    };
    MailMessage {
        id: m.id,
        message_id: m.internet_message_id,
        from_name: if from_name.is_empty() {
            from_addr.clone()
        } else {
            from_name
        },
        from_addr,
        to,
        cc,
        subject: m.subject.unwrap_or_default(),
        date_ts,
        body_text,
        body_html,
        has_attachments: m.has_attachments.unwrap_or(false),
        thread_id: m.conversation_id,
    }
}

// ────────── Trait impl ──────────

#[async_trait]
impl MailEngine for GraphEngine {
    async fn list_inbox(&self, offset: u32, limit: u32) -> Result<Vec<MailHeader>> {
        let limit = limit.clamp(1, 1000);
        let url = format!(
            "{}/me/mailFolders/inbox/messages?\
             $top={}&$skip={}&\
             $orderby=receivedDateTime%20desc&\
             $select=id,subject,from,receivedDateTime,isRead,flag,bodyPreview,hasAttachments,conversationId",
            API_BASE, limit, offset
        );
        let resp = Self::http()
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MailError::Network(format!("graph {}: {}", s, body)));
        }
        let list: GraphList<GraphMessageSummary> = resp
            .json()
            .await
            .map_err(|e| MailError::Parse(format!("graph json: {}", e)))?;
        Ok(list.value.into_iter().map(summary_to_header).collect())
    }

    async fn get_message(&self, id: &MessageId) -> Result<MailMessage> {
        let url = format!(
            "{}/me/messages/{}?$select=id,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,body,hasAttachments,conversationId",
            API_BASE, id
        );
        let resp = Self::http()
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(MailError::NotFound);
        }
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MailError::Network(format!("graph {}: {}", s, body)));
        }
        let full: GraphMessageFull = resp
            .json()
            .await
            .map_err(|e| MailError::Parse(format!("graph json: {}", e)))?;
        Ok(full_to_message(full))
    }

    /// Búsqueda con KQL (Microsoft) o términos libres. p.ej.
    /// `from:foo@bar.com hasAttachment:true`. Graph $search NO soporta
    /// $orderby ni $skip — resultado ranked por relevancia. MVP: ignorar
    /// offset, devolver primer batch de $top.
    async fn search(&self, query: &str, _offset: u32, limit: u32) -> Result<Vec<MailHeader>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let limit = limit.clamp(1, 250);
        let url = format!("{}/me/messages", API_BASE);
        let resp = Self::http()
            .get(&url)
            .bearer_auth(&self.access_token)
            // ConsistencyLevel=eventual es requerido por Graph $search.
            .header("ConsistencyLevel", "eventual")
            .query(&[
                ("$search", format!("\"{}\"", query)),
                ("$top", limit.to_string()),
                (
                    "$select",
                    "id,subject,from,receivedDateTime,isRead,flag,bodyPreview,hasAttachments,conversationId"
                        .to_string(),
                ),
            ])
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MailError::Network(format!("graph search {}: {}", s, body)));
        }
        let list: GraphList<GraphMessageSummary> = resp
            .json()
            .await
            .map_err(|e| MailError::Parse(format!("graph search json: {}", e)))?;
        Ok(list.value.into_iter().map(summary_to_header).collect())
    }

    async fn mark_read(&self, id: &MessageId, read: bool) -> Result<()> {
        let url = format!("{}/me/messages/{}", API_BASE, id);
        let resp = Self::http()
            .patch(&url)
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({ "isRead": read }))
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        check_ok(resp).await
    }

    async fn archive(&self, id: &MessageId) -> Result<()> {
        let url = format!("{}/me/messages/{}/move", API_BASE, id);
        let resp = Self::http()
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({ "destinationId": "archive" }))
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        check_ok(resp).await
    }

    /// Mover a Deleted Items (no DELETE definitivo, alineado con el botón
    /// "Eliminar" de Outlook web que también va a Trash).
    async fn delete(&self, id: &MessageId) -> Result<()> {
        let url = format!("{}/me/messages/{}/move", API_BASE, id);
        let resp = Self::http()
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({ "destinationId": "deleteditems" }))
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        check_ok(resp).await
    }

    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        let to_recipients: Vec<_> = msg
            .to
            .iter()
            .map(|a| serde_json::json!({ "emailAddress": { "address": a } }))
            .collect();
        let cc_recipients: Vec<_> = msg
            .cc
            .iter()
            .map(|a| serde_json::json!({ "emailAddress": { "address": a } }))
            .collect();
        let payload = serde_json::json!({
            "message": {
                "subject": msg.subject,
                "body": { "contentType": "Text", "content": msg.body_text },
                "toRecipients": to_recipients,
                "ccRecipients": cc_recipients,
            },
            "saveToSentItems": true,
        });
        let url = format!("{}/me/sendMail", API_BASE);
        let resp = Self::http()
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| MailError::Network(e.to_string()))?;
        check_ok(resp).await
    }
}

async fn check_ok(resp: reqwest::Response) -> Result<()> {
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(MailError::Network(format!("graph {}: {}", status, body)))
}

// ────────── OAuth flow + token management ──────────

#[derive(serde::Serialize)]
pub struct AuthorizeResult {
    pub email: String,
    pub scope: String,
    pub expires_at: i64,
}

/// Resuelve client_id y tenant, defaulteando al public client.
fn resolve_client(cfg: &GraphConfig) -> (String, String) {
    let client_id = if cfg.client_id.is_empty() {
        PUBLIC_CLIENT_ID.to_string()
    } else {
        cfg.client_id.clone()
    };
    let tenant = if cfg.tenant.is_empty() {
        "common".to_string()
    } else {
        cfg.tenant.clone()
    };
    (client_id, tenant)
}

#[tauri::command]
pub async fn graph_oauth_authorize<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
) -> std::result::Result<AuthorizeResult, String> {
    let (client_id, tenant) = {
        let state: State<AppState> = app.state();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter()
            .find(|s| s.id == service_id)
            .ok_or_else(|| format!("service {} not found", service_id))?;
        let cfg = svc
            .graph
            .as_ref()
            .ok_or_else(|| "no graph config".to_string())?;
        resolve_client(cfg)
    };

    let result = oauth::run_loopback_pkce(
        &auth_url(&tenant),
        &token_url(&tenant),
        &client_id,
        None, // public client
        SCOPES,
        &[("prompt", "select_account")],
        Some(&format!("{}/me", API_BASE)),
    )
    .await?;

    oauth::keyring_save(KEYRING_KIND, &service_id, &result.tokens)?;

    {
        let state: State<AppState> = app.state();
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        if let Some(svc) = g.services.iter_mut().find(|s| s.id == service_id) {
            if let Some(cfg) = svc.graph.as_mut() {
                cfg.email = result.email.clone();
                cfg.authorized = true;
                if cfg.client_id.is_empty() {
                    cfg.client_id = client_id.clone();
                }
            }
        }
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());

    super::watcher::start(&app, &service_id);

    Ok(AuthorizeResult {
        email: result.email,
        scope: result.tokens.scope,
        expires_at: result.tokens.expires_at,
    })
}

#[tauri::command]
pub async fn graph_oauth_revoke<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
) -> std::result::Result<(), String> {
    super::watcher::stop(&app, &service_id);
    let _ = oauth::keyring_delete(KEYRING_KIND, &service_id);
    let state: State<AppState> = app.state();
    let mut g = state.inner.lock().expect("AppState mutex poisoned");
    if let Some(svc) = g.services.iter_mut().find(|s| s.id == service_id) {
        if let Some(cfg) = svc.graph.as_mut() {
            cfg.authorized = false;
        }
    }
    save(&app, &g).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

pub async fn current_access_token<R: Runtime>(
    app: &AppHandle<R>,
    service_id: &str,
) -> std::result::Result<String, String> {
    let (client_id, tenant) = {
        let state: State<AppState> = app.state();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter()
            .find(|s| s.id == service_id)
            .ok_or_else(|| format!("service {} not found", service_id))?;
        let cfg = svc.graph.as_ref().ok_or_else(|| "no graph config".to_string())?;
        resolve_client(cfg)
    };
    oauth::ensure_fresh_access(KEYRING_KIND, service_id, &token_url(&tenant), &client_id, None)
        .await
}
