//! IMAP connection handling on top of async-imap.

use crate::mime;
use crate::model::{Account, NewMessage, Security};
use crate::oauth;
use anyhow::{anyhow, bail, Context, Result};
use futures::TryStreamExt;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send + Debug {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Debug> Io for T {}

pub type Session = async_imap::Session<Box<dyn Io>>;

/// What we present to the server when logging in.
#[derive(Clone)]
pub enum Credential {
    Password(String),
    AccessToken(String),
}

struct XOAuth2 {
    response: String,
    sent: bool,
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> String {
        // A second challenge is the server's error payload; answer with an
        // empty line so it can fail the command cleanly.
        if self.sent {
            String::new()
        } else {
            self.sent = true;
            self.response.clone()
        }
    }
}

pub fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

pub fn tls_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("rustls protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
}

pub async fn connect(acc: &Account, cred: &Credential) -> Result<Session> {
    let addr = (acc.imap_host.as_str(), acc.imap_port);
    let tcp = tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(addr))
        .await
        .map_err(|_| anyhow!("timed out connecting to {}", acc.imap_host))?
        .with_context(|| format!("could not connect to {}", acc.imap_host))?;
    tcp.set_nodelay(true).ok();

    let stream: Box<dyn Io> = match acc.imap_security {
        Security::Tls => {
            let name = rustls::pki_types::ServerName::try_from(acc.imap_host.clone())
                .map_err(|_| anyhow!("invalid server name {}", acc.imap_host))?;
            let tls = tokio_rustls::TlsConnector::from(tls_config())
                .connect(name, tcp)
                .await
                .with_context(|| format!("TLS handshake with {} failed", acc.imap_host))?;
            Box::new(tls)
        }
        Security::Plain if is_loopback(&acc.imap_host) => Box::new(tcp),
        Security::Plain => bail!("unencrypted IMAP is only allowed for localhost"),
        Security::StartTls => bail!("IMAP STARTTLS is not supported; use port 993"),
    };

    let mut client = async_imap::Client::new(stream);
    client
        .read_response()
        .await
        .ok_or_else(|| anyhow!("{} closed the connection", acc.imap_host))?
        .context("no greeting from server")?;

    let session = match cred {
        Credential::Password(p) => client
            .login(&acc.username, p)
            .await
            .map_err(|(e, _)| anyhow!("sign-in to {} failed: {e}", acc.imap_host))?,
        Credential::AccessToken(t) => client
            .authenticate("XOAUTH2", XOAuth2 { response: oauth::xoauth2(&acc.username, t), sent: false })
            .await
            .map_err(|(e, _)| anyhow!("sign-in to {} failed: {e}", acc.imap_host))?,
    };
    Ok(session)
}

#[derive(Debug, Clone)]
pub struct RemoteFolder {
    pub path: String,
    pub name: String,
    pub role: String,
    pub selectable: bool,
}

fn role_for(path: &str, attrs: &str) -> &'static str {
    if path.eq_ignore_ascii_case("INBOX") {
        return "inbox";
    }
    // SPECIAL-USE attributes (RFC 6154), matched on their debug form so this
    // survives changes to the library's enum.
    for (needle, role) in [
        ("sent", "sent"),
        ("drafts", "drafts"),
        ("trash", "trash"),
        ("junk", "junk"),
        ("archive", "archive"),
        ("flagged", "flagged"),
        ("all", "all"),
    ] {
        if attrs.contains(needle) {
            return role;
        }
    }
    let leaf = path.rsplit(['/', '.']).next().unwrap_or(path).to_ascii_lowercase();
    match leaf.as_str() {
        "sent" | "sent items" | "sent mail" | "sent messages" => "sent",
        "drafts" => "drafts",
        "trash" | "deleted items" | "deleted messages" | "bin" => "trash",
        "junk" | "spam" | "junk email" | "junk mail" => "junk",
        "archive" | "archives" => "archive",
        _ => "",
    }
}

/// Decodes an IMAP folder name from modified UTF-7 (RFC 3501 section 5.1.3), so
/// "Entw&APw-rfe" reads as "Entwürfe". Anything malformed is returned as it came.
pub fn decode_modified_utf7(s: &str) -> String {
    use base64::Engine as _;
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else { return s.to_string() };
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&'); // "&-" is a literal ampersand
        } else {
            let standard = encoded.replace(',', "/");
            let Ok(bytes) = base64::engine::general_purpose::STANDARD_NO_PAD.decode(standard) else {
                return s.to_string();
            };
            if bytes.len() % 2 != 0 {
                return s.to_string();
            }
            let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            match String::from_utf16(&units) {
                Ok(text) => out.push_str(&text),
                Err(_) => return s.to_string(),
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub async fn list_folders(s: &mut Session) -> Result<Vec<RemoteFolder>> {
    let names: Vec<_> = s.list(Some(""), Some("*")).await?.try_collect().await?;
    let mut out = Vec::new();
    for n in names {
        let attrs = format!("{:?}", n.attributes()).to_ascii_lowercase();
        let selectable = !attrs.contains("noselect") && !attrs.contains("nonexistent");
        let path = n.name().to_string();
        let name = match n.delimiter() {
            Some(d) if !d.is_empty() => path.rsplit(d).next().unwrap_or(&path).to_string(),
            _ => path.clone(),
        };
        let role = role_for(&path, &attrs).to_string();
        let name = if role == "inbox" { "Inbox".to_string() } else { decode_modified_utf7(&name) };
        out.push(RemoteFolder { path, name, role, selectable });
    }
    Ok(out)
}

pub struct Selected {
    pub exists: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
}

pub async fn select(s: &mut Session, path: &str) -> Result<Selected> {
    let mb = s.select(path).await.with_context(|| format!("could not open folder {path}"))?;
    Ok(Selected {
        exists: mb.exists,
        uid_validity: mb.uid_validity.unwrap_or(0),
        uid_next: mb.uid_next.unwrap_or(0),
    })
}

fn flags_of(f: &async_imap::types::Fetch) -> (bool, bool, bool) {
    use async_imap::types::Flag;
    let (mut seen, mut flagged, mut answered) = (false, false, false);
    for flag in f.flags() {
        match flag {
            Flag::Seen => seen = true,
            Flag::Flagged => flagged = true,
            Flag::Answered => answered = true,
            _ => {}
        }
    }
    (seen, flagged, answered)
}

/// Fetches list-view metadata for a UID set such as `120:*` or `5:80`.
pub async fn fetch_headers(s: &mut Session, uid_set: &str) -> Result<Vec<NewMessage>> {
    let fetches: Vec<_> = s
        .uid_fetch(uid_set, "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER])")
        .await?
        .try_collect()
        .await?;
    let mut out = Vec::with_capacity(fetches.len());
    for f in &fetches {
        let Some(uid) = f.uid else { continue };
        let mut m = f.header().map(mime::parse_headers).unwrap_or_default();
        m.uid = uid;
        m.size = f.size.unwrap_or(0);
        (m.seen, m.flagged, m.answered) = flags_of(f);
        if m.date == 0 {
            m.date = f.internal_date().map(|d| d.timestamp()).unwrap_or(0);
        }
        out.push(m);
    }
    Ok(out)
}

/// Fetches `(uid, seen, flagged, answered)` for a UID range.
pub async fn fetch_flags(s: &mut Session, lo: u32, hi: u32) -> Result<Vec<(u32, bool, bool, bool)>> {
    let fetches: Vec<_> = s.uid_fetch(format!("{lo}:{hi}"), "(UID FLAGS)").await?.try_collect().await?;
    Ok(fetches
        .iter()
        .filter_map(|f| {
            let (a, b, c) = flags_of(f);
            // `n:*` always returns the last message even when it is below `n`.
            f.uid.filter(|u| *u >= lo && *u <= hi).map(|u| (u, a, b, c))
        })
        .collect())
}

pub async fn fetch_body(s: &mut Session, uid: u32) -> Result<Vec<u8>> {
    let fetches: Vec<_> = s.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])").await?.try_collect().await?;
    fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .and_then(|f| f.body())
        .map(|b| b.to_vec())
        .ok_or_else(|| anyhow!("the server no longer has this message"))
}

pub async fn store_flag(s: &mut Session, uid: u32, flag: &str, on: bool) -> Result<()> {
    let op = if on { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
    let _: Vec<_> = s.uid_store(uid.to_string(), format!("{op} ({flag})")).await?.try_collect().await?;
    Ok(())
}

/// Moves a message, falling back to copy + delete where MOVE is unavailable.
pub async fn move_message(s: &mut Session, uid: u32, dest: &str) -> Result<()> {
    let set = uid.to_string();
    if s.uid_mv(&set, dest).await.is_ok() {
        return Ok(());
    }
    s.uid_copy(&set, dest).await?;
    store_flag(s, uid, "\\Deleted", true).await?;
    let _: Vec<_> = s.expunge().await?.try_collect().await?;
    Ok(())
}

pub async fn delete_forever(s: &mut Session, uid: u32) -> Result<()> {
    store_flag(s, uid, "\\Deleted", true).await?;
    let _: Vec<_> = s.expunge().await?.try_collect().await?;
    Ok(())
}

pub async fn append(s: &mut Session, folder: &str, raw: &[u8]) -> Result<()> {
    s.append(folder, Some("(\\Seen)"), None, raw).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_decode_from_modified_utf7() {
        assert_eq!(decode_modified_utf7("Inbox"), "Inbox");
        assert_eq!(decode_modified_utf7("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(decode_modified_utf7("~peter/mail/&ZeVnLIqe-/&U,BTFw-"), "~peter/mail/日本語/台北");
        assert_eq!(decode_modified_utf7("Sent &- Items"), "Sent & Items");
        assert_eq!(decode_modified_utf7("&Jjo-"), "☺");
        assert_eq!(decode_modified_utf7("Broken &AB"), "Broken &AB", "unterminated: left alone");
        assert_eq!(decode_modified_utf7("Bad &!!!!-"), "Bad &!!!!-", "bad base64: left alone");
    }
}
