//! OAuth 2.0 authorization-code flow with PKCE and a loopback redirect,
//! which is what Google and Microsoft both require of desktop mail clients.

use crate::providers::OAuthEndpoints;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

const B64URL: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Debug, Clone)]
pub struct ClientCreds {
    pub id: String,
    /// Google issues a (non-confidential) secret to desktop clients and requires it.
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

fn random_token(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    B64URL.encode(buf)
}

pub fn pkce_challenge(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

/// An authorization attempt waiting for the browser to come back.
pub struct PendingFlow {
    listener: TcpListener,
    redirect_uri: String,
    verifier: String,
    state: String,
    pub auth_url: String,
}

/// Binds a loopback port and builds the URL the user must open in their browser.
pub async fn begin(ep: &OAuthEndpoints, creds: &ClientCreds, login_hint: &str) -> Result<PendingFlow> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.context("could not open a local port for sign-in")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://{}:{}", ep.redirect_host, port);
    let verifier = random_token(48);
    let state = random_token(16);

    let mut url = Url::parse(ep.auth_url)?;
    url.query_pairs_mut()
        .append_pair("client_id", &creds.id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", ep.scopes)
        .append_pair("state", &state)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("login_hint", login_hint);

    Ok(PendingFlow { listener, redirect_uri, verifier, state, auth_url: url.into() })
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Brick</title>\
<body style=\"font:16px system-ui;margin:20vh auto;max-width:28em;text-align:center\">\
<h2>You're signed in</h2><p>You can close this tab and return to Brick.</p>";

const FAIL_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Brick</title>\
<body style=\"font:16px system-ui;margin:20vh auto;max-width:28em;text-align:center\">\
<h2>Sign-in didn't complete</h2><p>Return to Brick and try again.</p>";

impl PendingFlow {
    /// Waits for the browser redirect, then swaps the code for tokens.
    pub async fn finish(self, ep: &OAuthEndpoints, creds: &ClientCreds, timeout: Duration) -> Result<Tokens> {
        let code = tokio::time::timeout(timeout, self.wait_for_code())
            .await
            .map_err(|_| anyhow!("sign-in timed out"))??;

        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", creds.id.clone()),
            ("code_verifier", self.verifier.clone()),
        ];
        if let Some(s) = &creds.secret {
            form.push(("client_secret", s.clone()));
        }
        let tokens = token_request(ep.token_url, &form).await?;
        if tokens.refresh_token.is_none() {
            bail!("the provider did not return a refresh token");
        }
        Ok(tokens)
    }

    async fn wait_for_code(&self) -> Result<String> {
        loop {
            let (mut sock, _) = self.listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]);
            let target = head.split_whitespace().nth(1).unwrap_or("/");
            let parsed = Url::parse(&format!("http://localhost{target}")).ok();
            let param = |k: &str| {
                parsed
                    .as_ref()
                    .and_then(|u| u.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned()))
            };

            let outcome = if let Some(err) = param("error") {
                Some(Err(anyhow!("sign-in was refused: {}", param("error_description").unwrap_or(err))))
            } else if let Some(code) = param("code") {
                if param("state").as_deref() == Some(self.state.as_str()) {
                    Some(Ok(code))
                } else {
                    Some(Err(anyhow!("sign-in response did not match this request")))
                }
            } else {
                None // favicon and other stray requests
            };

            let (status, page) = match &outcome {
                Some(Ok(_)) => ("200 OK", DONE_PAGE),
                Some(Err(_)) => ("400 Bad Request", FAIL_PAGE),
                None => ("404 Not Found", ""),
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;

            if let Some(result) = outcome {
                return result;
            }
        }
    }
}

pub async fn refresh(ep: &OAuthEndpoints, creds: &ClientCreds, refresh_token: &str) -> Result<Tokens> {
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", creds.id.clone()),
    ];
    if let Some(s) = &creds.secret {
        form.push(("client_secret", s.clone()));
    }
    token_request(ep.token_url, &form).await
}

async fn token_request(token_url: &str, form: &[(&str, String)]) -> Result<Tokens> {
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(form).finish();
    let resp = reqwest::Client::new()
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("could not reach the sign-in server")?;
    let parsed: TokenResponse = resp.json().await.context("unexpected reply from the sign-in server")?;
    if let Some(err) = parsed.error {
        bail!("sign-in server said: {}", parsed.error_description.unwrap_or(err));
    }
    let access_token = parsed.access_token.ok_or_else(|| anyhow!("no access token in reply"))?;
    // Refresh a minute early so a token never expires mid-command.
    let ttl = parsed.expires_in.unwrap_or(3600).saturating_sub(60);
    Ok(Tokens {
        access_token,
        refresh_token: parsed.refresh_token,
        expires_at: Instant::now() + Duration::from_secs(ttl),
    })
}

/// SASL XOAUTH2 initial response (before base64).
pub fn xoauth2(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn loopback_flow_exchanges_code() {
        // A stand-in token endpoint.
        let token_srv = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let token_url: &'static str =
            Box::leak(format!("http://127.0.0.1:{}/token", token_srv.local_addr().unwrap().port()).into_boxed_str());
        tokio::spawn(async move {
            let (mut s, _) = token_srv.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(req.contains("code=abc123"));
            assert!(req.contains("code_verifier="));
            let body = r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#;
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            s.write_all(resp.as_bytes()).await.unwrap();
        });

        let ep = OAuthEndpoints {
            auth_url: "https://example.com/auth",
            token_url,
            scopes: "mail",
            redirect_host: "127.0.0.1",
            client_id_key: "k",
            client_secret_key: "s",
        };
        let creds = ClientCreds { id: "cid".into(), secret: None };
        let flow = begin(&ep, &creds, "me@example.com").await.unwrap();
        let url = Url::parse(&flow.auth_url).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["code_challenge_method"], "S256");
        let redirect = q["redirect_uri"].clone();
        let state = q["state"].clone();

        // Pretend to be the browser: one stray request, then the real redirect.
        tokio::spawn(async move {
            let _ = reqwest::get(format!("{redirect}/favicon.ico")).await;
            let _ = reqwest::get(format!("{redirect}/?code=abc123&state={state}")).await;
        });

        let tokens = flow.finish(&ep, &creds, Duration::from_secs(10)).await.unwrap();
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
    }
}
