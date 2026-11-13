//! Everything that touches the network: sync passes, body fetches, flag and
//! move operations, sending. All of it runs on the engine's runtime and
//! reports back through events; nothing here is called on a UI thread.

use crate::imap::{self, Credential, Session};
use crate::model::{Account, AuthKind, Folder};
use crate::oauth::{self, ClientCreds, Tokens};
use crate::providers::{self, OAuthEndpoints};
use crate::secrets::{self, SecretStore};
use crate::store::Store;
use crate::items::Item;
use crate::{claude, mime, smtp};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard};

/// Messages fetched when a folder is first seen.
const INITIAL_WINDOW: u32 = 300;
/// UIDs of history examined per pass after that.
const BACKFILL_CHUNK: u32 = 500;
/// How far back flag changes and deletions are checked on a routine pass.
const RECONCILE_WINDOW: u32 = 2000;
/// Inbox bodies downloaded ahead of time so opening them is instant.
const PREFETCH_BODIES: i64 = 30;
const STEP_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_secs(120);

pub type Emit = Arc<dyn Fn(String) + Send + Sync>;

pub struct Conn {
    session: Option<Session>,
    last_used: Instant,
}

/// Runs one IMAP exchange with a timeout. A failure drops the connection so
/// the next caller reconnects instead of reusing a broken session.
macro_rules! step {
    ($conn:ident, |$s:ident| $body:expr) => {{
        let r: Result<_> = match tokio::time::timeout(STEP_TIMEOUT, async {
            let $s = $conn.session.as_mut().ok_or_else(|| anyhow!("not connected"))?;
            $body
        })
        .await
        {
            Ok(r) => r,
            Err(_) => Err(anyhow!("the mail server stopped responding")),
        };
        if r.is_err() {
            $conn.session = None;
        }
        $conn.last_used = Instant::now();
        r
    }};
}

pub struct Inner {
    pub store: Store,
    pub secrets: Arc<dyn SecretStore>,
    emit: Emit,
    conns: Mutex<HashMap<i64, Arc<AsyncMutex<Conn>>>>,
    tokens: Mutex<HashMap<i64, Tokens>>,
    syncing: Mutex<HashSet<i64>>,
    pub wake: Notify,
}

impl Inner {
    pub fn new(store: Store, secrets: Arc<dyn SecretStore>, emit: Emit) -> Self {
        Self {
            store,
            secrets,
            emit,
            conns: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashSet::new()),
            wake: Notify::new(),
        }
    }

    pub fn emit(&self, event: Value) {
        (self.emit)(event.to_string());
    }

    pub fn emit_error(&self, account_id: Option<i64>, err: &anyhow::Error) {
        self.emit(json!({ "type": "error", "account_id": account_id, "message": format!("{err:#}") }));
    }

    // ---- credentials ----------------------------------------------------

    async fn secret(&self, key: String) -> Result<Option<String>> {
        // The keychain backends block, and some refuse to run on a runtime thread.
        let secrets = self.secrets.clone();
        tokio::task::spawn_blocking(move || secrets.get(&key)).await?
    }

    pub async fn set_secret(&self, key: String, value: String) -> Result<()> {
        let secrets = self.secrets.clone();
        tokio::task::spawn_blocking(move || secrets.set(&key, &value)).await?
    }

    pub fn client_creds(&self, ep: &OAuthEndpoints) -> Result<ClientCreds> {
        let id = self
            .store
            .setting(ep.client_id_key)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("no OAuth client ID is configured for this provider; add one in Settings"))?;
        let secret = self.store.setting(ep.client_secret_key)?.filter(|s| !s.trim().is_empty());
        Ok(ClientCreds { id: id.trim().to_string(), secret: secret.map(|s| s.trim().to_string()) })
    }

    pub fn cache_tokens(&self, account_id: i64, tokens: Tokens) {
        self.tokens.lock().unwrap().insert(account_id, tokens);
    }

    pub async fn credential(&self, acc: &Account) -> Result<Credential> {
        match acc.auth {
            AuthKind::Password => {
                let p = self
                    .secret(secrets::password_key(acc.id))
                    .await?
                    .ok_or_else(|| anyhow!("no saved password for {}; remove and re-add the account", acc.email))?;
                Ok(Credential::Password(p))
            }
            AuthKind::OAuth => {
                if let Some(t) = self.tokens.lock().unwrap().get(&acc.id) {
                    if t.expires_at > Instant::now() {
                        return Ok(Credential::AccessToken(t.access_token.clone()));
                    }
                }
                let ep = providers::get(&acc.provider)
                    .and_then(|p| p.oauth)
                    .ok_or_else(|| anyhow!("{} does not support browser sign-in", acc.provider))?;
                let creds = self.client_creds(&ep)?;
                let refresh = self
                    .secret(secrets::refresh_token_key(acc.id))
                    .await?
                    .ok_or_else(|| anyhow!("{} needs to sign in again", acc.email))?;
                let tokens = oauth::refresh(&ep, &creds, &refresh).await?;
                if let Some(rotated) = &tokens.refresh_token {
                    self.set_secret(secrets::refresh_token_key(acc.id), rotated.clone()).await?;
                }
                let access = tokens.access_token.clone();
                self.cache_tokens(acc.id, tokens);
                Ok(Credential::AccessToken(access))
            }
        }
    }

    // ---- connections ----------------------------------------------------

    /// Hands out the account's IMAP connection, opening or reviving it as needed.
    /// Holding the guard serialises commands on that connection.
    async fn acquire(&self, acc: &Account) -> Result<OwnedMutexGuard<Conn>> {
        let slot = self
            .conns
            .lock()
            .unwrap()
            .entry(acc.id)
            .or_insert_with(|| Arc::new(AsyncMutex::new(Conn { session: None, last_used: Instant::now() })))
            .clone();
        let mut conn = slot.lock_owned().await;

        if conn.session.is_some() && conn.last_used.elapsed() > Duration::from_secs(45) {
            let alive = tokio::time::timeout(Duration::from_secs(10), conn.session.as_mut().unwrap().noop()).await;
            if !matches!(alive, Ok(Ok(()))) {
                conn.session = None;
            }
        }
        if conn.session.is_none() {
            let cred = self.credential(acc).await?;
            match imap::connect(acc, &cred).await {
                Ok(s) => conn.session = Some(s),
                Err(e) => {
                    // A rejected token may have been revoked; force a refresh next time.
                    self.tokens.lock().unwrap().remove(&acc.id);
                    return Err(e);
                }
            }
        }
        conn.last_used = Instant::now();
        Ok(conn)
    }

    pub fn forget_account(&self, account_id: i64) {
        self.conns.lock().unwrap().remove(&account_id);
        self.tokens.lock().unwrap().remove(&account_id);
    }

    // ---- sync -----------------------------------------------------------

    /// One full pass over an account. Returns true when history is still being
    /// filled in and another pass should follow soon.
    pub async fn sync_account(self: &Arc<Self>, account_id: i64, thorough: bool) -> Result<bool> {
        if !self.syncing.lock().unwrap().insert(account_id) {
            return Ok(false); // already running
        }
        self.emit(json!({ "type": "sync_started", "account_id": account_id }));
        let result = self.sync_account_inner(account_id, thorough).await;
        self.syncing.lock().unwrap().remove(&account_id);
        self.emit(json!({ "type": "sync_finished", "account_id": account_id, "ok": result.is_ok() }));
        if let Err(e) = &result {
            self.emit_error(Some(account_id), e);
        }
        result
    }

    async fn sync_account_inner(self: &Arc<Self>, account_id: i64, thorough: bool) -> Result<bool> {
        let acc = self.store.account(account_id)?.ok_or_else(|| anyhow!("account was removed"))?;

        let remote = {
            let mut conn = self.acquire(&acc).await?;
            step!(conn, |s| imap::list_folders(s).await)?
        };
        let mut paths = Vec::new();
        for f in &remote {
            self.store.upsert_folder(acc.id, &f.path, &f.name, &f.role, f.selectable)?;
            paths.push(f.path.clone());
        }
        self.store.retain_folders(acc.id, &paths)?;
        self.emit(json!({ "type": "folders_changed", "account_id": acc.id }));

        let mut more = false;
        // store.folders() already orders by role: inbox first.
        for folder in self.store.folders(acc.id)? {
            // Gmail's Starred view duplicates messages that live elsewhere.
            if !folder.selectable || folder.role == "flagged" {
                continue;
            }
            match self.sync_folder(&acc, &folder, thorough).await {
                Ok(pending) => more |= pending,
                Err(e) => self.emit_error(Some(acc.id), &e.context(format!("syncing {}", folder.name))),
            }
        }
        Ok(more)
    }

    async fn sync_folder(&self, acc: &Account, folder: &Folder, thorough: bool) -> Result<bool> {
        let store = &self.store;
        let mut changed = false;
        let mut pending = false;

        // The connection is held for one folder's bounded batch of commands and
        // released before body prefetch, so a click in the UI never waits long.
        let mut conn = self.acquire(acc).await?;
        let sel = step!(conn, |s| imap::select(s, &folder.path).await)?;

        let known = store.folder_uidvalidity(folder.id)?;
        if known != sel.uid_validity {
            // UIDs were renumbered (or this is the first visit): start over.
            store.reset_folder(folder.id, sel.uid_validity)?;
            changed = known != 0;
        }

        let max = store.max_uid(folder.id)?;
        if sel.exists == 0 {
            if max > 0 {
                changed |= store.reconcile(folder.id, 1, u32::MAX, &[])?;
            }
            store.set_backfill_uid(folder.id, 1)?;
        } else {
            // New mail (or the newest window on first visit).
            let lo = if max > 0 {
                max + 1
            } else if sel.uid_next > INITIAL_WINDOW {
                sel.uid_next - INITIAL_WINDOW
            } else {
                1
            };
            let fresh = step!(conn, |s| imap::fetch_headers(s, &format!("{lo}:*")).await)?;
            let fresh: Vec<_> = fresh.into_iter().filter(|m| m.uid >= lo).collect();
            changed |= store.insert_messages(acc.id, folder.id, &fresh)? > 0;
            if max == 0 {
                store.set_backfill_uid(folder.id, lo)?;
            }

            // Flag changes and deletions made elsewhere.
            if max > 0 {
                let floor = store.min_uid(folder.id)?;
                let from = if thorough { floor } else { floor.max(max.saturating_sub(RECONCILE_WINDOW)) };
                let flags = step!(conn, |s| imap::fetch_flags(s, from, max).await)?;
                changed |= store.reconcile(folder.id, from, max, &flags)?;
            }

            // Older history, one chunk per pass.
            let cursor = store.backfill_uid(folder.id)?;
            if cursor > 1 {
                let lo = cursor.saturating_sub(BACKFILL_CHUNK).max(1);
                let older = step!(conn, |s| imap::fetch_headers(s, &format!("{lo}:{}", cursor - 1)).await)?;
                let older: Vec<_> = older.into_iter().filter(|m| m.uid >= lo && m.uid < cursor).collect();
                changed |= store.insert_messages(acc.id, folder.id, &older)? > 0;
                store.set_backfill_uid(folder.id, lo)?;
                pending = lo > 1;
            }
        }
        drop(conn);

        store.refresh_counts(folder.id)?;
        if changed {
            self.emit(json!({ "type": "messages_changed", "account_id": acc.id, "folder_id": folder.id }));
            self.emit(json!({ "type": "folders_changed", "account_id": acc.id }));
        }

        if folder.role == "inbox" {
            let mut got = false;
            for (id, uid) in store.missing_bodies(folder.id, PREFETCH_BODIES)? {
                let mut conn = self.acquire(acc).await?;
                step!(conn, |s| imap::select(s, &folder.path).await)?;
                match step!(conn, |s| imap::fetch_body(s, uid).await) {
                    Ok(raw) => {
                        self.save_body(id, &raw)?;
                        got = true;
                    }
                    // Deleted since we listed it; the next pass will reconcile.
                    Err(_) if conn.session.is_some() => continue,
                    Err(e) => return Err(e),
                }
            }
            if got {
                self.emit(json!({ "type": "messages_changed", "account_id": acc.id, "folder_id": folder.id }));
            }
        }
        Ok(pending)
    }

    fn save_body(&self, message_id: i64, raw: &[u8]) -> Result<()> {
        let p = mime::summarise(raw);
        self.store.save_body(message_id, raw, &p.text, &p.snippet, p.has_attachments)
    }

    // ---- user actions ---------------------------------------------------

    fn locate(&self, message_id: i64) -> Result<(Account, Folder, u32)> {
        let m = self.store.message(message_id)?.ok_or_else(|| anyhow!("message no longer exists"))?;
        let acc = self.store.account(m.account_id)?.ok_or_else(|| anyhow!("account was removed"))?;
        let folder = self.store.folder(m.folder_id)?.ok_or_else(|| anyhow!("folder was removed"))?;
        Ok((acc, folder, m.uid))
    }

    pub async fn fetch_body(&self, message_id: i64) -> Result<()> {
        let (acc, folder, uid) = self.locate(message_id)?;
        let raw = {
            let mut conn = self.acquire(&acc).await?;
            step!(conn, |s| imap::select(s, &folder.path).await)?;
            step!(conn, |s| imap::fetch_body(s, uid).await)?
        };
        self.save_body(message_id, &raw)?;
        self.emit(json!({ "type": "body_ready", "id": message_id, "folder_id": folder.id }));
        Ok(())
    }

    /// Pushes a flag change that has already been applied locally.
    pub async fn push_flag(&self, acc: Account, folder: Folder, uid: u32, flag: &'static str, on: bool) -> Result<()> {
        let mut conn = self.acquire(&acc).await?;
        step!(conn, |s| imap::select(s, &folder.path).await)?;
        step!(conn, |s| imap::store_flag(s, uid, flag, on).await)
    }

    /// Pushes a move (or permanent delete when `dest` is None) already applied locally.
    pub async fn push_move(&self, acc: Account, folder: Folder, uid: u32, dest: Option<Folder>) -> Result<()> {
        let mut conn = self.acquire(&acc).await?;
        step!(conn, |s| imap::select(s, &folder.path).await)?;
        match &dest {
            Some(d) => step!(conn, |s| imap::move_message(s, uid, &d.path).await),
            None => step!(conn, |s| imap::delete_forever(s, uid).await),
        }
    }

    pub async fn send(&self, draft: smtp::Draft) -> Result<()> {
        let acc = self.store.account(draft.account_id)?.ok_or_else(|| anyhow!("choose an account to send from"))?;
        let threading = match draft.reply_to_id {
            Some(id) => self.store.threading(id)?.map(|(mid, refs, _)| (mid, refs)),
            None => None,
        };
        let msg = smtp::build(&acc, &draft, threading)?;
        let raw = msg.formatted();
        let cred = self.credential(&acc).await?;
        smtp::send(&acc, &cred, msg).await?;

        // From here the message is sent; follow-up failures must not look like a failed send.
        let saves_sent = providers::get(&acc.provider).map(|p| p.server_saves_sent).unwrap_or(false);
        if !saves_sent {
            if let Some(sent) = self.store.folder_by_role(acc.id, "sent")? {
                let filed: Result<()> = async {
                    let mut conn = self.acquire(&acc).await?;
                    step!(conn, |s| imap::append(s, &sent.path, &raw).await)
                }
                .await;
                if let Err(e) = filed {
                    self.emit_error(Some(acc.id), &e.context("the message was sent, but could not be saved to Sent"));
                }
            }
        }
        if let Some(id) = draft.reply_to_id {
            if let Ok((acc, folder, uid)) = self.locate(id) {
                self.store.set_answered(id)?;
                self.emit(json!({ "type": "messages_changed", "account_id": acc.id, "folder_id": folder.id }));
                let _ = self.push_flag(acc, folder, uid, "\\Answered", true).await;
            }
        }
        Ok(())
    }

    /// Sends an email to Claude and returns the items it proposes, each tagged
    /// with where it came from. Nothing is saved.
    pub async fn extract_items(&self, message_id: i64) -> Result<claude::Extraction> {
        let row = self.store.message(message_id)?.ok_or_else(|| anyhow!("message no longer exists"))?;
        let key = self
            .secret(claude::KEY_SECRET.to_string())
            .await?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| anyhow!("Add your Anthropic API key in Settings to use this."))?;
        let model = self
            .store
            .setting(claude::MODEL_SETTING)?
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| claude::DEFAULT_MODEL.to_string());

        if self.store.body(message_id)?.is_none() {
            self.fetch_body(message_id).await?;
        }
        let raw = self.store.body(message_id)?.ok_or_else(|| anyhow!("the message could not be downloaded"))?;
        let text = mime::summarise(&raw).text;
        let from = if row.from_name.is_empty() {
            row.from_addr.clone()
        } else {
            format!("{} <{}>", row.from_name, row.from_addr)
        };

        let email = claude::Email { from: &from, to: &row.to, subject: &row.subject, sent: row.date, text: &text };
        let mut out = claude::extract(key.trim(), model.trim(), &email).await?;

        let header_id = self.store.threading(message_id)?.map(|t| t.0).unwrap_or_default();
        out.items = out
            .items
            .into_iter()
            .map(|i| Item {
                source_account_id: Some(row.account_id),
                source_message_id: header_id.clone(),
                source_subject: row.subject.clone(),
                source_from: from.clone(),
                ..i
            })
            .collect();
        Ok(out)
    }

    /// Checks that the account's credentials are accepted by its IMAP server.
    pub async fn verify(&self, acc: &Account, cred: &Credential) -> Result<()> {
        if acc.imap_host.trim().is_empty() {
            bail!("enter an IMAP server");
        }
        let mut s = imap::connect(acc, cred).await?;
        let _ = s.logout().await;
        Ok(())
    }
}

/// Background loop: sync every account now, then on a timer or when woken.
pub async fn run(inner: Arc<Inner>) {
    let mut first = true;
    loop {
        let accounts = inner.store.accounts().unwrap_or_default();
        let mut tasks = Vec::new();
        for acc in accounts {
            let inner = inner.clone();
            let thorough = first;
            tasks.push(tokio::spawn(async move { inner.sync_account(acc.id, thorough).await.unwrap_or(false) }));
        }
        let mut more = false;
        for t in tasks {
            more |= t.await.unwrap_or(false);
        }
        first = false;

        let wait = if more { Duration::from_secs(1) } else { POLL_INTERVAL };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = inner.wake.notified() => {}
        }
    }
}
