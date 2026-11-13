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
    let from = Mailbox::new(
        Some(acc.name.clone()).filter(|n| !n.is_empty()),
        acc.email.parse().map_err(|_| anyhow!("account address {} is not valid", acc.email))?,
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

pub async fn send(acc: &Account, cred: &Credential, msg: Message) -> Result<()> {
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
    let transport = builder.port(acc.smtp_port).timeout(Some(std::time::Duration::from_secs(60))).build();
    transport.send(msg).await.map_err(|e| anyhow!("{} would not accept the message: {e}", acc.smtp_host))?;
    Ok(())
}
