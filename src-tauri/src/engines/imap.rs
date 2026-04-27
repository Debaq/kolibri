#![allow(dead_code)] // algunas constantes las consumen steps 6+
// ImapEngine: Gmail vía IMAP+SMTP con XOAUTH2.
//
// Step 3: read-only (list_inbox + get_message). Resto sigue stub.

use async_imap::types::Flag;
use async_native_tls::TlsConnector;
use async_trait::async_trait;
use futures_util::StreamExt;
use mail_parser::MessageParser;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::net::TcpStream;

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
    /// Access token vivo. El dispatcher lo refresca y lo inyecta por llamada.
    pub access_token: String,
}

impl ImapEngine {
    pub fn new(config: ImapConfig, access_token: String) -> Self {
        Self { config, access_token }
    }
}

// ────────── Conexión + auth XOAUTH2 ──────────

type ImapSession = async_imap::Session<async_native_tls::TlsStream<TcpStream>>;

struct XOAuth2 {
    user: String,
    token: String,
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    fn process(&mut self, _data: &[u8]) -> Self::Response {
        // SASL XOAUTH2: user=<email>\x01auth=Bearer <token>\x01\x01
        format!("user={}\x01auth=Bearer {}\x01\x01", self.user, self.token)
    }
}

/// Login expuesto para el watcher (no Result interno tipado).
pub async fn login_for_watcher(
    email: &str,
    access_token: &str,
) -> std::result::Result<ImapSession, String> {
    login(email, access_token).await.map_err(|e| e.to_string())
}

/// Para watcher: dado un sequence number (no UID), devuelve el UID.
pub async fn fetch_uid_at(
    session: &mut ImapSession,
    seq: u32,
) -> std::result::Result<Option<u32>, String> {
    let mut stream = session
        .fetch(seq.to_string(), "(UID)")
        .await
        .map_err(|e| format!("fetch uid: {}", e))?;
    let mut found = None;
    while let Some(item) = stream.next().await {
        let msg = item.map_err(|e| e.to_string())?;
        if let Some(uid) = msg.uid {
            found = Some(uid);
            break;
        }
    }
    Ok(found)
}

/// Para watcher: trae headers para rango de UIDs (descendente por fecha).
pub async fn fetch_headers_for_uids(
    session: &mut ImapSession,
    from_uid: u32,
    to_uid: u32,
) -> std::result::Result<Vec<MailHeader>, String> {
    let range = format!("{}:{}", from_uid, to_uid);
    let mut headers: Vec<MailHeader> = Vec::new();
    let mut stream = session
        .uid_fetch(
            range,
            "(UID FLAGS BODY.PEEK[HEADER.FIELDS (FROM TO SUBJECT DATE)])",
        )
        .await
        .map_err(|e| format!("uid_fetch: {}", e))?;
    while let Some(item) = stream.next().await {
        let msg = item.map_err(|e| e.to_string())?;
        if let Some(h) = parse_envelope_header(&msg) {
            headers.push(h);
        }
    }
    headers.sort_by(|a, b| b.date_ts.cmp(&a.date_ts));
    Ok(headers)
}

async fn login(email: &str, access_token: &str) -> Result<ImapSession> {
    let tcp = TcpStream::connect((IMAP_HOST, IMAP_PORT))
        .await
        .map_err(|e| MailError::Network(format!("tcp: {}", e)))?;
    let stream = TlsConnector::new()
        .connect(IMAP_HOST, tcp)
        .await
        .map_err(|e| MailError::Network(format!("tls: {}", e)))?;
    let client = async_imap::Client::new(stream);
    let auth = XOAuth2 {
        user: email.into(),
        token: access_token.into(),
    };
    let session = client
        .authenticate("XOAUTH2", auth)
        .await
        .map_err(|(e, _)| MailError::Auth(format!("xoauth2: {}", e)))?;
    Ok(session)
}

fn parse_envelope_header(fetch: &async_imap::types::Fetch) -> Option<MailHeader> {
    let uid = fetch.uid?;
    let raw = fetch.header()?;
    let parsed = MessageParser::default().parse_headers(raw)?;

    let from_first = parsed.from().and_then(|a| a.first());
    let from_addr = from_first
        .and_then(|m| m.address())
        .unwrap_or("")
        .to_string();
    let from_name = from_first
        .and_then(|m| m.name())
        .unwrap_or("")
        .to_string();
    let subject = parsed.subject().unwrap_or("").to_string();
    let date_ts = parsed.date().map(|d| d.to_timestamp()).unwrap_or(0);

    let flags: Vec<Flag> = fetch.flags().collect();
    let seen = flags.iter().any(|f| matches!(f, Flag::Seen));
    let flagged = flags.iter().any(|f| matches!(f, Flag::Flagged));

    Some(MailHeader {
        id: uid.to_string(),
        from_name: if from_name.is_empty() { from_addr.clone() } else { from_name },
        from_addr,
        subject,
        date_ts,
        snippet: String::new(),
        seen,
        flagged,
        has_attachments: false,
        thread_id: None,
    })
}

#[async_trait]
impl MailEngine for ImapEngine {
    async fn list_inbox(&self, offset: u32, limit: u32) -> Result<Vec<MailHeader>> {
        let mut session = login(&self.config.email, &self.access_token).await?;
        let mailbox = session
            .select("INBOX")
            .await
            .map_err(|e| MailError::Network(format!("select: {}", e)))?;
        let total = mailbox.exists;
        if total == 0 {
            let _ = session.logout().await;
            return Ok(vec![]);
        }
        let limit = limit.clamp(1, 500);
        // Ventana del más reciente hacia atrás. offset=0 → últimos `limit`.
        let top = total.saturating_sub(offset).max(1);
        let bottom = top.saturating_sub(limit.saturating_sub(1)).max(1);
        if offset >= total {
            let _ = session.logout().await;
            return Ok(vec![]);
        }
        let range = format!("{}:{}", bottom, top);

        let mut headers: Vec<MailHeader> = Vec::with_capacity(limit as usize);
        {
            let mut stream = session
                .fetch(
                    range,
                    "(UID FLAGS BODY.PEEK[HEADER.FIELDS (FROM TO SUBJECT DATE)])",
                )
                .await
                .map_err(|e| MailError::Network(format!("fetch: {}", e)))?;
            while let Some(item) = stream.next().await {
                let msg = item.map_err(|e| MailError::Network(e.to_string()))?;
                if let Some(h) = parse_envelope_header(&msg) {
                    headers.push(h);
                }
            }
        }
        let _ = session.logout().await;
        headers.sort_by(|a, b| b.date_ts.cmp(&a.date_ts));
        Ok(headers)
    }

    async fn get_message(&self, id: &MessageId) -> Result<MailMessage> {
        let uid: u32 = id
            .parse()
            .map_err(|_| MailError::Parse("uid no numérico".into()))?;
        let mut session = login(&self.config.email, &self.access_token).await?;
        session
            .select("INBOX")
            .await
            .map_err(|e| MailError::Network(format!("select: {}", e)))?;

        let mut raw: Vec<u8> = Vec::new();
        {
            let mut stream = session
                .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
                .await
                .map_err(|e| MailError::Network(format!("uid_fetch: {}", e)))?;
            while let Some(item) = stream.next().await {
                let msg = item.map_err(|e| MailError::Network(e.to_string()))?;
                if let Some(body) = msg.body() {
                    raw = body.to_vec();
                    break;
                }
            }
        }
        let _ = session.logout().await;

        if raw.is_empty() {
            return Err(MailError::NotFound);
        }
        let parsed = MessageParser::default()
            .parse(&raw)
            .ok_or_else(|| MailError::Parse("rfc822".into()))?;

        let from_first = parsed.from().and_then(|a| a.first());
        let from_addr = from_first
            .and_then(|m| m.address())
            .unwrap_or("")
            .to_string();
        let from_name = from_first
            .and_then(|m| m.name())
            .unwrap_or("")
            .to_string();
        let to: Vec<String> = parsed
            .to()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.address().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let cc: Vec<String> = parsed
            .cc()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.address().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let subject = parsed.subject().unwrap_or("").to_string();
        let date_ts = parsed.date().map(|d| d.to_timestamp()).unwrap_or(0);
        let body_text = parsed.body_text(0).map(|c| c.to_string()).unwrap_or_default();
        let body_html = parsed.body_html(0).map(|c| c.to_string());
        let has_attachments = parsed.attachments().next().is_some();

        Ok(MailMessage {
            id: uid.to_string(),
            message_id: parsed.message_id().map(|s| s.to_string()),
            from_name: if from_name.is_empty() { from_addr.clone() } else { from_name },
            from_addr,
            to,
            cc,
            subject,
            date_ts,
            body_text,
            body_html,
            has_attachments,
            thread_id: None,
        })
    }

    async fn search(&self, _query: &str, _offset: u32, _limit: u32) -> Result<Vec<MailHeader>> {
        Err(MailError::Other("imap.search not implemented (step 8)".into()))
    }

    async fn mark_read(&self, id: &MessageId, read: bool) -> Result<()> {
        let uid: u32 = id
            .parse()
            .map_err(|_| MailError::Parse("uid no numérico".into()))?;
        let mut session = login(&self.config.email, &self.access_token).await?;
        session
            .select("INBOX")
            .await
            .map_err(|e| MailError::Network(format!("select: {}", e)))?;
        let cmd = if read {
            "+FLAGS (\\Seen)"
        } else {
            "-FLAGS (\\Seen)"
        };
        {
            let mut stream = session
                .uid_store(uid.to_string(), cmd)
                .await
                .map_err(|e| MailError::Network(format!("uid_store: {}", e)))?;
            while let Some(_) = stream.next().await {}
        }
        let _ = session.logout().await;
        Ok(())
    }

    /// Archivar en Gmail = sacar el label `\\Inbox`. El mensaje queda en
    /// All Mail. Usa la extensión X-GM-LABELS.
    async fn archive(&self, id: &MessageId) -> Result<()> {
        let uid: u32 = id
            .parse()
            .map_err(|_| MailError::Parse("uid no numérico".into()))?;
        let mut session = login(&self.config.email, &self.access_token).await?;
        session
            .select("INBOX")
            .await
            .map_err(|e| MailError::Network(format!("select: {}", e)))?;
        {
            let mut stream = session
                .uid_store(uid.to_string(), "-X-GM-LABELS (\\\\Inbox)")
                .await
                .map_err(|e| MailError::Network(format!("uid_store labels: {}", e)))?;
            while let Some(_) = stream.next().await {}
        }
        let _ = session.logout().await;
        Ok(())
    }

    /// Mover a [Gmail]/Trash usando UID MOVE (extensión Gmail).
    async fn delete(&self, id: &MessageId) -> Result<()> {
        let uid: u32 = id
            .parse()
            .map_err(|_| MailError::Parse("uid no numérico".into()))?;
        let mut session = login(&self.config.email, &self.access_token).await?;
        session
            .select("INBOX")
            .await
            .map_err(|e| MailError::Network(format!("select: {}", e)))?;
        session
            .uid_mv(uid.to_string(), "[Gmail]/Trash")
            .await
            .map_err(|e| MailError::Network(format!("uid_mv: {}", e)))?;
        let _ = session.logout().await;
        Ok(())
    }

    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        use lettre::message::header::ContentType;
        use lettre::transport::smtp::authentication::{Credentials, Mechanism};
        use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

        let from_addr: lettre::Address = self
            .config
            .email
            .parse()
            .map_err(|e: lettre::address::AddressError| MailError::Parse(e.to_string()))?;
        let mut builder = Message::builder()
            .from(lettre::message::Mailbox::new(None, from_addr))
            .subject(&msg.subject);
        for t in &msg.to {
            let addr: lettre::Address = t
                .parse()
                .map_err(|e: lettre::address::AddressError| MailError::Parse(e.to_string()))?;
            builder = builder.to(lettre::message::Mailbox::new(None, addr));
        }
        for c in &msg.cc {
            let addr: lettre::Address = c
                .parse()
                .map_err(|e: lettre::address::AddressError| MailError::Parse(e.to_string()))?;
            builder = builder.cc(lettre::message::Mailbox::new(None, addr));
        }
        if let Some(irt) = &msg.in_reply_to {
            // El frontend pasa el `message_id` (RFC822 Message-ID) cuando
            // dispara reply, no el UID. Si por error mandó un UID (numérico),
            // ignorar para no romper el header.
            if irt.contains('@') {
                builder = builder.in_reply_to(irt.clone());
            }
        }
        let email = builder
            .header(ContentType::TEXT_PLAIN)
            .body(msg.body_text.clone())
            .map_err(|e| MailError::Other(format!("build msg: {}", e)))?;

        // XOAUTH2 SMTP. lettre arma el SASL string desde (user, access_token)
        // cuando el mecanismo es Xoauth2.
        let creds = Credentials::new(self.config.email.clone(), self.access_token.clone());
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(SMTP_HOST)
            .map_err(|e| MailError::Network(format!("smtp relay: {}", e)))?
            .port(SMTP_PORT)
            .credentials(creds)
            .authentication(vec![Mechanism::Xoauth2])
            .build();
        mailer
            .send(email)
            .await
            .map_err(|e| MailError::Network(format!("smtp send: {}", e)))?;
        Ok(())
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

    // Arrancar watcher IDLE en background.
    super::watcher::start(&app, &service_id);

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
    super::watcher::stop(&app, &service_id);
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
