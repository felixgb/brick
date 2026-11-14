//! Message parsing and HTML sanitising.

use crate::model::{Attachment, Body, NewMessage};
use base64::Engine as _;
use mail_parser::{Address, HeaderValue, MessageParser, MimeHeaders};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn join_addresses(a: Option<&Address>) -> String {
    let Some(a) = a else { return String::new() };
    a.iter()
        .filter_map(|x| {
            let addr = x.address.as_deref()?;
            Some(match x.name.as_deref() {
                Some(n) if !n.is_empty() => format!("{n} <{addr}>"),
                _ => addr.to_string(),
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn header_ids(v: &HeaderValue) -> String {
    match v {
        HeaderValue::Text(t) => t.to_string(),
        HeaderValue::TextList(l) => l.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// Builds list-view metadata from a raw header block.
pub fn parse_headers(raw: &[u8]) -> NewMessage {
    let mut m = NewMessage::default();
    let Some(msg) = MessageParser::default().parse(raw) else { return m };
    m.subject = msg.subject().unwrap_or("").trim().to_string();
    if let Some(first) = msg.from().and_then(|f| f.first()) {
        m.from_name = first.name.as_deref().unwrap_or("").to_string();
        m.from_addr = first.address.as_deref().unwrap_or("").to_string();
    }
    m.to = join_addresses(msg.to());
    m.cc = join_addresses(msg.cc());
    m.message_id = msg.message_id().unwrap_or("").to_string();
    m.in_reply_to = header_ids(msg.in_reply_to());
    m.references = header_ids(msg.references());
    m.date = msg.date().map(|d| d.to_timestamp()).unwrap_or(0);
    m.has_attachments = msg
        .content_type()
        .map(|ct| ct.c_type.eq_ignore_ascii_case("multipart") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("mixed")))
        .unwrap_or(false);
    m
}

pub struct Parsed {
    pub text: String,
    pub snippet: String,
    pub has_attachments: bool,
}

/// Extracts searchable text and a list-view snippet from a full message.
pub fn summarise(raw: &[u8]) -> Parsed {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return Parsed { text: String::new(), snippet: String::new(), has_attachments: false };
    };
    let text = msg.body_text(0).map(|t| t.into_owned()).unwrap_or_default();
    let snippet: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
    Parsed { text, snippet, has_attachments: msg.attachment_count() > 0 && has_real_attachment(&msg) }
}

fn has_real_attachment(msg: &mail_parser::Message) -> bool {
    msg.attachments().any(|p| p.content_id().is_none() || p.attachment_name().is_some())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn wrap_document(inner: &str, allow_remote: bool) -> String {
    // The policy is the real guarantee: no scripts, no frames, no network
    // except images when the user has opted in for this message.
    let img = if allow_remote { "data: https: http:" } else { "data:" };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src {img}; style-src 'unsafe-inline'; font-src data:\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <style>html{{background:#fff;color:#1b1b1b}}body{{margin:16px;font:14px/1.5 -apple-system,'Noto Sans','Segoe UI',sans-serif;overflow-wrap:anywhere}}\
         img{{max-width:100%;height:auto}}pre{{white-space:pre-wrap;font:13px/1.5 ui-monospace,'DejaVu Sans Mono',monospace}}\
         blockquote{{margin:0 0 0 .8em;padding-left:.8em;border-left:2px solid #c8c8c8;color:#555}}</style>\
         </head><body>{inner}</body></html>"
    )
}

/// Renders a stored message for display. `allow_remote` lets remote images load.
pub fn render(raw: &[u8], allow_remote: bool) -> Body {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return Body {
            html: wrap_document("<p>This message could not be read.</p>", false),
            text: String::new(),
            attachments: vec![],
            blocked_remote: false,
            invitation: false,
        };
    };

    let text = msg.body_text(0).map(|t| t.into_owned()).unwrap_or_default();

    // Inline images referenced as cid: become data: URIs.
    let mut cids: HashMap<String, String> = HashMap::new();
    let mut attachments = Vec::new();
    for (index, part) in msg.attachments().enumerate() {
        let mime = part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.c_type, sub),
                None => ct.c_type.to_string(),
            })
            .unwrap_or_else(|| "application/octet-stream".into());
        if let Some(cid) = part.content_id() {
            if mime.starts_with("image/") && part.contents().len() < 4 * 1024 * 1024 {
                let data = base64::engine::general_purpose::STANDARD.encode(part.contents());
                cids.insert(cid.trim_matches(|c| c == '<' || c == '>').to_string(), format!("data:{mime};base64,{data}"));
            }
        }
        if part.content_id().is_none() || part.attachment_name().is_some() {
            attachments.push(Attachment {
                index,
                name: part.attachment_name().unwrap_or("attachment").to_string(),
                mime,
                size: part.contents().len(),
            });
        }
    }

    let blocked = Arc::new(AtomicBool::new(false));
    let cids = Arc::new(cids);
    let inner = match msg.body_html(0) {
        // body_html synthesises HTML from text/plain when there is no HTML part,
        // so check that a real one exists before trusting its markup.
        Some(html) if msg.html_body_count() > 0 && is_html_part(&msg) => sanitise(&html, cids.clone(), allow_remote, blocked.clone()),
        _ => format!("<pre>{}</pre>", escape(&text)),
    };

    Body {
        html: wrap_document(&inner, allow_remote),
        text,
        attachments,
        blocked_remote: blocked.load(Ordering::Relaxed),
        invitation: !invitations(raw).is_empty(),
    }
}

fn is_html_part(msg: &mail_parser::Message) -> bool {
    msg.html_bodies().next().is_some_and(|p| matches!(p.body, mail_parser::PartType::Html(_)))
}

fn sanitise(html: &str, cids: Arc<HashMap<String, String>>, allow_remote: bool, blocked: Arc<AtomicBool>) -> String {
    let mut b = ammonia::Builder::default();
    b.add_tags(["style", "font", "center"]);
    b.rm_clean_content_tags(["style"]);
    b.add_generic_attributes([
        "style", "class", "align", "valign", "bgcolor", "width", "height", "dir", "lang", "border", "cellpadding",
        "cellspacing", "color", "face", "size",
    ]);
    b.url_schemes(HashSet::from(["http", "https", "mailto", "cid", "data"]));
    b.link_rel(Some("noopener noreferrer"));
    b.attribute_filter(move |element, attribute, value| {
        let lower = value.trim_start().to_ascii_lowercase();
        match (element, attribute) {
            ("img", "src") => {
                if let Some(cid) = lower.strip_prefix("cid:") {
                    // Content-IDs are case-sensitive; look up with the original casing.
                    let original = &value.trim_start()[4..];
                    return cids.get(original).or_else(|| cids.get(cid)).map(|d| Cow::Owned(d.clone()));
                }
                if lower.starts_with("data:image/") {
                    return Some(Cow::Borrowed(value));
                }
                if allow_remote && (lower.starts_with("https:") || lower.starts_with("http:")) {
                    return Some(Cow::Borrowed(value));
                }
                blocked.store(true, Ordering::Relaxed);
                None
            }
            // Only images may use data: or cid: URLs.
            (_, "href") | (_, "src") | (_, "cite") if lower.starts_with("data:") || lower.starts_with("cid:") => None,
            (_, "srcset") => None,
            (_, "style") if !allow_remote && lower.contains("url(") => {
                blocked.store(true, Ordering::Relaxed);
                None
            }
            _ => Some(Cow::Borrowed(value)),
        }
    });
    b.clean(html).to_string()
}

/// The text of every calendar invitation in a message: text/calendar parts,
/// whether inline or attached, and attachments named *.ics.
pub fn invitations(raw: &[u8]) -> Vec<String> {
    let Some(msg) = MessageParser::default().parse(raw) else { return vec![] };
    msg.parts
        .iter()
        .filter(|p| {
            let calendar = p
                .content_type()
                .is_some_and(|ct| ct.c_type.eq_ignore_ascii_case("text") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("calendar")));
            let named = p.attachment_name().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
            calendar || named
        })
        .map(|p| String::from_utf8_lossy(p.contents()).into_owned())
        .filter(|t| t.contains("BEGIN:VCALENDAR"))
        .collect()
}

/// Returns (file name, bytes) for the attachment at `index`.
pub fn attachment(raw: &[u8], index: usize) -> Option<(String, Vec<u8>)> {
    let msg = MessageParser::default().parse(raw)?;
    let part = msg.attachments().nth(index)?;
    Some((part.attachment_name().unwrap_or("attachment").to_string(), part.contents().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML_MAIL: &str = "From: =?UTF-8?Q?Tom=C3=A1s_Vega?= <tomas@example.com>\r\n\
To: me@example.com, Other <o@example.com>\r\n\
Subject: =?UTF-8?B?SW52b2ljZSDigJMgMjI5MQ==?=\r\n\
Message-ID: <abc@example.com>\r\n\
References: <one@example.com> <two@example.com>\r\n\
Date: Tue, 10 Nov 2026 08:15:00 +0000\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<html><body onload=\"evil()\"><script>alert(1)</script>\
<p style=\"color:red\">Hello <b>there</b></p>\
<img src=\"https://tracker.example.com/p.gif\">\
<a href=\"javascript:alert(1)\">x</a><a href=\"https://example.com\">ok</a>\
<iframe src=\"https://example.com\"></iframe></body></html>\r\n";

    #[test]
    fn headers_decode_encoded_words() {
        let m = parse_headers(HTML_MAIL.as_bytes());
        assert_eq!(m.subject, "Invoice – 2291");
        assert_eq!(m.from_name, "Tomás Vega");
        assert_eq!(m.from_addr, "tomas@example.com");
        assert_eq!(m.to, "me@example.com, Other <o@example.com>");
        assert_eq!(m.message_id, "abc@example.com");
        assert_eq!(m.references, "one@example.com two@example.com");
        assert_eq!(m.date, 1794298500);
    }

    #[test]
    fn sanitiser_strips_active_content_and_trackers() {
        let body = render(HTML_MAIL.as_bytes(), false);
        assert!(!body.html.contains("<script"));
        assert!(!body.html.contains("alert(1)"));
        assert!(!body.html.contains("onload"));
        assert!(!body.html.contains("javascript:"));
        assert!(!body.html.contains("<iframe"));
        assert!(!body.html.contains("tracker.example.com"));
        assert!(body.blocked_remote);
        assert!(body.html.contains("color:red"));
        assert!(body.html.contains("href=\"https://example.com\""));
        assert!(body.html.contains("img-src data:;"));

        let body = render(HTML_MAIL.as_bytes(), true);
        assert!(body.html.contains("tracker.example.com"));
        assert!(!body.blocked_remote);
        assert!(!body.html.contains("<script"));
    }

    #[test]
    fn plain_text_is_escaped() {
        let raw = b"From: a@example.com\r\nSubject: hi\r\n\r\n<script>alert(1)</script> & more\r\n";
        let body = render(raw, false);
        assert!(body.html.contains("&lt;script&gt;"));
        assert!(!body.html.contains("<script>"));
        let s = summarise(raw);
        assert!(s.snippet.contains("& more"));
        assert!(!s.has_attachments);
    }

    #[test]
    fn finds_invitations_inline_and_attached() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Review\r\nDTSTART:20261201T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let inline = format!(
            "From: a@example.com\r\nSubject: Invite\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n\
--b\r\nContent-Type: text/plain\r\n\r\nJoin us\r\n--b\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\n{ics}--b--\r\n"
        );
        assert_eq!(invitations(inline.as_bytes()).len(), 1);
        assert!(render(inline.as_bytes(), false).invitation);

        let attached = format!(
            "From: a@example.com\r\nSubject: Invite\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n\
--b\r\nContent-Type: text/plain\r\n\r\nSee attached\r\n--b\r\nContent-Type: application/octet-stream; name=\"invite.ics\"\r\n\
Content-Disposition: attachment; filename=\"invite.ics\"\r\n\r\n{ics}--b--\r\n"
        );
        assert_eq!(invitations(attached.as_bytes()).len(), 1);

        assert!(invitations(HTML_MAIL.as_bytes()).is_empty());
        assert!(!render(HTML_MAIL.as_bytes(), false).invitation);
    }
}
