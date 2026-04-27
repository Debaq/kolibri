// Engines de mail nativo.
//
// Trait MailEngine abstrae las dos backends:
// - ImapEngine: Gmail vía IMAP/XOAUTH2.
// - GraphEngine: Outlook M365 vía Microsoft Graph REST.
//
// La UI Svelte solo conoce los tipos de este módulo + comandos Tauri que
// dispatch al engine correcto según `Service.engine`. WhatsApp se queda en
// webview siempre.

pub mod graph;
pub mod imap;
pub mod oauth;
pub mod watcher;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("auth: {0}")]
    Auth(String),
    #[error("network: {0}")]
    Network(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Other(String),
}

impl From<MailError> for String {
    fn from(e: MailError) -> Self {
        e.to_string()
    }
}

pub type Result<T> = std::result::Result<T, MailError>;

/// ID opaco del mensaje. En IMAP es UID stringificado, en Graph es el id REST.
pub type MessageId = String;

/// Header resumido para listado de bandeja.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailHeader {
    pub id: MessageId,
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    /// UNIX seconds.
    pub date_ts: i64,
    pub snippet: String,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: MessageId,
    pub message_id: Option<String>,
    pub from_name: String,
    pub from_addr: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub date_ts: i64,
    pub body_text: String,
    pub body_html: Option<String>,
    pub has_attachments: bool,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    /// Si está set, el envío se encadena como reply (In-Reply-To / replyTo).
    pub in_reply_to: Option<MessageId>,
}

#[async_trait]
pub trait MailEngine: Send + Sync {
    /// Lista mensajes de INBOX. Paginación por offset desde el más reciente
    /// (offset=0 → últimos `limit`).
    async fn list_inbox(&self, offset: u32, limit: u32) -> Result<Vec<MailHeader>>;

    /// Detalle de un mensaje por id.
    async fn get_message(&self, id: &MessageId) -> Result<MailMessage>;

    /// Búsqueda server-side. Sintaxis depende del engine (X-GM-RAW vs KQL).
    async fn search(&self, query: &str, offset: u32, limit: u32) -> Result<Vec<MailHeader>>;

    /// Marcar leído / no-leído.
    async fn mark_read(&self, id: &MessageId, read: bool) -> Result<()>;

    /// Archivar (Gmail: remove INBOX label; Graph: move a Archive).
    async fn archive(&self, id: &MessageId) -> Result<()>;

    /// Borrar (mover a Trash).
    async fn delete(&self, id: &MessageId) -> Result<()>;

    /// Enviar mensaje. En MVP texto plano sin adjuntos.
    async fn send(&self, msg: &OutgoingMessage) -> Result<()>;
}
