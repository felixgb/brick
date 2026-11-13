//! Built-in server settings for the providers Brick knows about.

use crate::model::{AuthKind, Security};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OAuthEndpoints {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static str,
    /// Host used in the loopback redirect URI. Google wants the literal IP,
    /// Microsoft only ignores the port for `localhost`.
    pub redirect_host: &'static str,
    /// Settings keys holding the client id and (optional) secret.
    pub client_id_key: &'static str,
    pub client_secret_key: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub auth: AuthKind,
    pub imap_host: &'static str,
    pub imap_port: u16,
    pub imap_security: Security,
    pub smtp_host: &'static str,
    pub smtp_port: u16,
    pub smtp_security: Security,
    pub oauth: Option<OAuthEndpoints>,
    /// The server files sent mail itself, so we must not append a second copy.
    pub server_saves_sent: bool,
    pub hint: &'static str,
}

pub fn all() -> Vec<Provider> {
    vec![
        Provider {
            id: "fastmail",
            label: "Fastmail",
            auth: AuthKind::Password,
            imap_host: "imap.fastmail.com",
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "smtp.fastmail.com",
            smtp_port: 465,
            smtp_security: Security::Tls,
            oauth: None,
            server_saves_sent: false,
            hint: "Use an app password from Settings → Privacy & Security → Connected apps & API tokens.",
        },
        Provider {
            id: "gmail",
            label: "Gmail",
            auth: AuthKind::OAuth,
            imap_host: "imap.gmail.com",
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "smtp.gmail.com",
            smtp_port: 465,
            smtp_security: Security::Tls,
            oauth: Some(OAuthEndpoints {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_url: "https://oauth2.googleapis.com/token",
                scopes: "https://mail.google.com/",
                redirect_host: "127.0.0.1",
                client_id_key: "google_client_id",
                client_secret_key: "google_client_secret",
            }),
            server_saves_sent: true,
            hint: "Signs in with Google in your browser.",
        },
        Provider {
            id: "outlook",
            label: "Outlook",
            auth: AuthKind::OAuth,
            imap_host: "outlook.office365.com",
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "smtp-mail.outlook.com",
            smtp_port: 587,
            smtp_security: Security::StartTls,
            oauth: Some(OAuthEndpoints {
                auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                scopes: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access",
                redirect_host: "localhost",
                client_id_key: "microsoft_client_id",
                client_secret_key: "microsoft_client_secret",
            }),
            server_saves_sent: true,
            hint: "Signs in with Microsoft in your browser.",
        },
        Provider {
            id: "custom",
            label: "Other (IMAP)",
            auth: AuthKind::Password,
            imap_host: "",
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "",
            smtp_port: 465,
            smtp_security: Security::Tls,
            oauth: None,
            server_saves_sent: false,
            hint: "Enter your server details.",
        },
    ]
}

pub fn get(id: &str) -> Option<Provider> {
    all().into_iter().find(|p| p.id == id)
}
