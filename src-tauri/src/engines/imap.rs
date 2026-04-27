#![allow(dead_code)] // step 1: las constantes/campos los usan steps 2-3+
// ImapEngine: Gmail vía IMAP+SMTP con XOAUTH2.
//
// Step 1: skeleton, sin lógica. Implementación real en step 3.
//
// Hosts hardcoded a Gmail:
//   IMAP: imap.gmail.com:993 (TLS)
//   SMTP: smtp.gmail.com:465 (TLS)
// XOAUTH2 SASL string: "user=<email>\x01auth=Bearer <token>\x01\x01"

use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use super::oauth;
use super::{MailEngine, MailError, MailHeader, MailMessage, MessageId, OutgoingMessage, Result};
use crate::services::{save, AppState, ImapConfig};

pub const IMAP_HOST: &str = "imap.gmail.com";
pub const IMAP_PORT: u16 = 993;
pub const SMTP_HOST: &str = "smtp.gmail.com";
pub const SMTP_PORT: u16 = 465;

pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const SCOPE: &str = "https://mail.google.com/";
pub const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

pub const KEYRING_KIND: &str = "imap";

pub struct ImapEngine {
    pub config: ImapConfig,
}

impl ImapEngine {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MailEngine for ImapEngine {
    async fn list_inbox(&self, _offset: u32, _limit: u32) -> Result<Vec<MailHeader>> {
        Err(MailError::Other("imap.list_inbox not implemented (step 3)".into()))
    }

    async fn get_message(&self, _id: &MessageId) -> Result<MailMessage> {
        Err(MailError::Other("imap.get_message not implemented (step 3)".into()))
    }

    async fn search(&self, _query: &str, _offset: u32, _limit: u32) -> Result<Vec<MailHeader>> {
        Err(MailError::Other("imap.search not implemented (step 8)".into()))
    }

    async fn mark_read(&self, _id: &MessageId, _read: bool) -> Result<()> {
        Err(MailError::Other("imap.mark_read not implemented (step 6)".into()))
    }

    async fn archive(&self, _id: &MessageId) -> Result<()> {
        Err(MailError::Other("imap.archive not implemented (step 6)".into()))
    }

    async fn delete(&self, _id: &MessageId) -> Result<()> {
        Err(MailError::Other("imap.delete not implemented (step 6)".into()))
    }

    async fn send(&self, _msg: &OutgoingMessage) -> Result<()> {
        Err(MailError::Other("imap.send not implemented (step 6)".into()))
    }
}

// ────────── OAuth flow + token management ──────────

#[derive(serde::Serialize)]
pub struct AuthorizeResult {
    pub email: String,
    pub scope: String,
    pub expires_at: i64,
}

/// Dispara flow PKCE Gmail. Persiste tokens en keyring y actualiza
/// `Service.imap.email` + `authorized=true` en services.json.
///
/// Pre-condición: el servicio debe existir con engine=imap y client_id/secret
/// ya en su `ImapConfig`. UI flow: crear servicio → llamar a este comando.
#[tauri::command]
pub async fn imap_oauth_authorize<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
) -> std::result::Result<AuthorizeResult, String> {
    let (client_id, client_secret) = {
        let state: State<AppState> = app.state();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter()
            .find(|s| s.id == service_id)
            .ok_or_else(|| format!("service {} not found", service_id))?;
        let cfg = svc
            .imap
            .as_ref()
            .ok_or_else(|| "no imap config".to_string())?;
        (cfg.client_id.clone(), cfg.client_secret.clone())
    };
    if client_id.is_empty() {
        return Err("client_id vacío".into());
    }

    let result = oauth::run_loopback_pkce(
        AUTH_URL,
        TOKEN_URL,
        &client_id,
        Some(&client_secret),
        &[SCOPE],
        // Google: forzar refresh_token + reconfirmar consent.
        &[
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
        Some(USERINFO_URL),
    )
    .await?;

    oauth::keyring_save(KEYRING_KIND, &service_id, &result.tokens)?;

    // Persistir email + authorized=true.
    {
        let state: State<AppState> = app.state();
        let mut g = state.inner.lock().expect("AppState mutex poisoned");
        if let Some(svc) = g.services.iter_mut().find(|s| s.id == service_id) {
            if let Some(cfg) = svc.imap.as_mut() {
                cfg.email = result.email.clone();
                cfg.authorized = true;
            }
        }
        save(&app, &g).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("kolibri:services_changed", ());

    Ok(AuthorizeResult {
        email: result.email,
        scope: result.tokens.scope,
        expires_at: result.tokens.expires_at,
    })
}

/// Revoca tokens locales (borra del keyring + marca authorized=false).
/// No revoca server-side; user puede revocar en Google Account → Security.
#[tauri::command]
pub async fn imap_oauth_revoke<R: Runtime>(
    app: AppHandle<R>,
    service_id: String,
) -> std::result::Result<(), String> {
    let _ = oauth::keyring_delete(KEYRING_KIND, &service_id);
    let state: State<AppState> = app.state();
    let mut g = state.inner.lock().expect("AppState mutex poisoned");
    if let Some(svc) = g.services.iter_mut().find(|s| s.id == service_id) {
        if let Some(cfg) = svc.imap.as_mut() {
            cfg.authorized = false;
        }
    }
    save(&app, &g).map_err(|e| e.to_string())?;
    let _ = app.emit("kolibri:services_changed", ());
    Ok(())
}

/// Helper que steps 3+ usan: trae access token vivo (refresca si expiró).
pub async fn current_access_token<R: Runtime>(
    app: &AppHandle<R>,
    service_id: &str,
) -> std::result::Result<String, String> {
    let (client_id, client_secret) = {
        let state: State<AppState> = app.state();
        let g = state.inner.lock().expect("AppState mutex poisoned");
        let svc = g
            .services
            .iter()
            .find(|s| s.id == service_id)
            .ok_or_else(|| format!("service {} not found", service_id))?;
        let cfg = svc.imap.as_ref().ok_or_else(|| "no imap config".to_string())?;
        (cfg.client_id.clone(), cfg.client_secret.clone())
    };
    oauth::ensure_fresh_access(
        KEYRING_KIND,
        service_id,
        TOKEN_URL,
        &client_id,
        Some(&client_secret),
    )
    .await
}
