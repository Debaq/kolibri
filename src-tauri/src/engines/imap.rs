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

use super::{MailEngine, MailError, MailHeader, MailMessage, MessageId, OutgoingMessage, Result};
use crate::services::ImapConfig;

pub const IMAP_HOST: &str = "imap.gmail.com";
pub const IMAP_PORT: u16 = 993;
pub const SMTP_HOST: &str = "smtp.gmail.com";
pub const SMTP_PORT: u16 = 465;

/// OAuth2 endpoints + scope. Step 2 los usa.
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const SCOPE: &str = "https://mail.google.com/";

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
