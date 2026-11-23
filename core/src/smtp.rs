//! Outgoing mail via lettre.

use crate::imap::{is_loopback, Credential};
use crate::model::{Account, Security};
use anyhow::{anyhow, bail, Context, Result};
use lettre::message::{header::ContentType, Mailbox, MessageBuilder};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Draft {
    pub account_id: i64,
    pub to: String,
    #[serde(default)]
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub text: String,
    /// Local id of the message being replied to, if any.
    #[serde(default)]
    pub reply_to_id: Option<i64>,
    /// Send as this address instead of the account's own. It must be one of
    /// the account's aliases; see `Config::aliases`.
    #[serde(default)]
    pub from: Option<String>,
}

/// Rejects a `from` that is neither the account's address nor one of its aliases.
pub fn check_sender(acc: &Account, aliases: &[String], d: &Draft) -> Result<()> {
    match d.from.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(from) if !from.eq_ignore_ascii_case(&acc.email) && !aliases.iter().any(|a| a.eq_ignore_ascii_case(from)) => {
            bail!("{from} is not one of {}'s addresses", acc.email)
        }
        _ => Ok(()),
    }
}

fn add_list(
    mut b: MessageBuilder,
    list: &str,
    f: fn(MessageBuilder, Mailbox) -> MessageBuilder,
) -> Result<(MessageBuilder, usize)> {
    let mut n = 0;
    for part in list.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let mb: Mailbox = part.parse().map_err(|_| anyhow!("\"{part}\" is not a valid address"))?;
        b = f(b, mb);
        n += 1;
    }
    Ok((b, n))
}

fn bracket(id: &str) -> String {
    if id.starts_with('<') {
        id.to_string()
    } else {
        format!("<{id}>")
    }
}

/// `threading` is the (Message-ID, References) of the message being answered.
pub fn build(acc: &Account, d: &Draft, threading: Option<(String, String)>) -> Result<Message> {
    let address = d.from.as_deref().map(str::trim).filter(|f| !f.is_empty()).unwrap_or(&acc.email);
    let from = Mailbox::new(
        Some(acc.name.clone()).filter(|n| !n.is_empty()),
        address.parse().map_err(|_| anyhow!("address {address} is not valid"))?,
    );
    let b = Message::builder().from(from).subject(d.subject.clone());
    let (b, to) = add_list(b, &d.to, MessageBuilder::to)?;
    let (b, cc) = add_list(b, &d.cc, MessageBuilder::cc)?;
    let (mut b, bcc) = add_list(b, &d.bcc, MessageBuilder::bcc)?;
    if to + cc + bcc == 0 {
        bail!("add at least one recipient");
    }
    if let Some((msg_id, refs)) = threading.filter(|(id, _)| !id.is_empty()) {
        let parent = bracket(&msg_id);
        let chain = refs.split_whitespace().map(bracket).chain([parent.clone()]).collect::<Vec<_>>().join(" ");
        b = b.in_reply_to(parent).references(chain);
    }
    b.header(ContentType::TEXT_PLAIN).body(d.text.clone()).context("could not build the message")
}

pub async fn send(acc: &Account, cred: &Credential, msg: Message, timeout: std::time::Duration) -> Result<()> {
    let host = acc.smtp_host.as_str();
    let builder = match acc.smtp_security {
        Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?,
        Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?,
        Security::Plain if is_loopback(host) => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        Security::Plain => bail!("unencrypted SMTP is only allowed for localhost"),
    };
    let builder = match cred {
        Credential::Password(p) => builder.credentials(Credentials::new(acc.username.clone(), p.clone())),
        Credential::AccessToken(t) => builder
            .credentials(Credentials::new(acc.username.clone(), t.clone()))
            .authentication(vec![Mechanism::Xoauth2]),
    };
    let transport = builder.port(acc.smtp_port).timeout(Some(timeout)).build();
    transport.send(msg).await.map_err(|e| anyhow!("{} would not accept the message: {e}", acc.smtp_host))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AuthKind;

    fn acc() -> Account {
        Account {
            id: 1,
            email: "me@example.com".into(),
            name: "Me".into(),
            provider: "custom".into(),
            auth: AuthKind::Password,
            username: "me".into(),
            imap_host: "h".into(),
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "h".into(),
            smtp_port: 465,
            smtp_security: Security::Tls,
        }
    }

    fn draft(from: Option<&str>) -> Draft {
        Draft {
            account_id: 1,
            to: "you@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hi".into(),
            text: "hello".into(),
            reply_to_id: None,
            from: from.map(String::from),
        }
    }

    #[test]
    fn sending_as_an_alias_needs_the_alias_to_be_listed() {
        let aliases = vec!["shop@example.com".to_string()];
        assert!(check_sender(&acc(), &aliases, &draft(None)).is_ok());
        assert!(check_sender(&acc(), &aliases, &draft(Some("ME@example.com"))).is_ok(), "own address, any case");
        assert!(check_sender(&acc(), &aliases, &draft(Some("shop@example.com"))).is_ok());
        assert!(check_sender(&acc(), &aliases, &draft(Some("boss@example.com"))).is_err(), "can't send as just anyone");

        let msg = build(&acc(), &draft(Some("shop@example.com")), None).unwrap();
        let text = String::from_utf8_lossy(&msg.formatted()).to_string();
        assert!(text.contains("From: Me <shop@example.com>"), "{text}");
    }
}
