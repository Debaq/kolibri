#![allow(dead_code)] // step 1: constantes/campos los usan steps 2-5+
// GraphEngine: Outlook M365 vía Microsoft Graph REST.
//
// Step 1: skeleton, sin lógica. Implementación real en step 5.
//
// Endpoints base:
//   AUTH:  https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize
//   TOKEN: https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token
//   API:   https://graph.microsoft.com/v1.0
//
// Default client_id: Microsoft Graph PowerShell (14d82eec-...) ya pre-aprobado
// por Microsoft, evita registrar app propia en Azure AD si el tenant lo bloquea.

use async_trait::async_trait;

use super::{MailEngine, MailError, MailHeader, MailMessage, MessageId, OutgoingMessage, Result};
use crate::services::GraphConfig;

pub const API_BASE: &str = "https://graph.microsoft.com/v1.0";
pub const SCOPE: &str = "Mail.ReadWrite Mail.Send offline_access";

/// Microsoft Graph PowerShell — client_id público pre-aprobado por Microsoft.
/// Útil cuando el tenant del user bloquea registrar apps propias en Azure AD.
pub const PUBLIC_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

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
}

impl GraphEngine {
    pub fn new(config: GraphConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MailEngine for GraphEngine {
    async fn list_inbox(&self, _offset: u32, _limit: u32) -> Result<Vec<MailHeader>> {
        Err(MailError::Other("graph.list_inbox not implemented (step 5)".into()))
    }

    async fn get_message(&self, _id: &MessageId) -> Result<MailMessage> {
        Err(MailError::Other("graph.get_message not implemented (step 5)".into()))
    }

    async fn search(&self, _query: &str, _offset: u32, _limit: u32) -> Result<Vec<MailHeader>> {
        Err(MailError::Other("graph.search not implemented (step 8)".into()))
    }

    async fn mark_read(&self, _id: &MessageId, _read: bool) -> Result<()> {
        Err(MailError::Other("graph.mark_read not implemented (step 6)".into()))
    }

    async fn archive(&self, _id: &MessageId) -> Result<()> {
        Err(MailError::Other("graph.archive not implemented (step 6)".into()))
    }

    async fn delete(&self, _id: &MessageId) -> Result<()> {
        Err(MailError::Other("graph.delete not implemented (step 6)".into()))
    }

    async fn send(&self, _msg: &OutgoingMessage) -> Result<()> {
        Err(MailError::Other("graph.send not implemented (step 6)".into()))
    }
}
