use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    Password,
    OAuth,
}

impl AuthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthKind::Password => "password",
            AuthKind::OAuth => "oauth",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "oauth" {
            AuthKind::OAuth
        } else {
            AuthKind::Password
        }
    }
}

/// How a connection to a mail server is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// TLS from the first byte (IMAP 993, SMTP 465).
    Tls,
    /// Upgrade with STARTTLS (SMTP 587).
    StartTls,
    /// No encryption. Only accepted for loopback hosts; used by the test suite.
    Plain,
}

impl Security {
    pub fn as_str(self) -> &'static str {
        match self {
            Security::Tls => "tls",
            Security::StartTls => "starttls",
            Security::Plain => "plain",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "starttls" => Security::StartTls,
            "plain" => Security::Plain,
            _ => Security::Tls,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub provider: String,
    pub auth: AuthKind,
    pub username: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: Security,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: Security,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub path: String,
    pub name: String,
    /// inbox, sent, drafts, archive, trash, junk, all, flagged, or "" for ordinary folders.
    pub role: String,
    pub selectable: bool,
    pub unread: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: u32,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub to: String,
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachments: bool,
    pub snippet: String,
}

/// Header-level data for a message as it arrives from the server.
#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub uid: u32,
    pub message_id: String,
    pub in_reply_to: String,
    pub references: String,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub to: String,
    pub cc: String,
    pub date: i64,
    pub size: u32,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    pub index: usize,
    pub name: String,
    pub mime: String,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Body {
    /// Complete, sanitised HTML document ready to hand to a web view.
    pub html: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    /// True when remote images were present and stripped.
    pub blocked_remote: bool,
    /// True when the message carries a calendar invitation (see `items.import_invitation`).
    #[serde(default)]
    pub invitation: bool,
}
