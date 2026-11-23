//! Everything that touches the network: sync passes, body fetches, flag and
//! move operations, sending. All of it runs on the engine's runtime and
//! reports back through events; nothing here is called on a UI thread.

use crate::imap::{self, Credential, Session};
use crate::model::{Account, AuthKind, Folder};
use crate::oauth::{self, ClientCreds, Tokens};
use crate::providers::{self, OAuthEndpoints};
use crate::config::ConfigFile;
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

// Sync windows, timeouts and the poll interval come from config.toml ([sync],
// [network]); see config.rs for what each one means.

pub type Emit = Arc<dyn Fn(String) + Send + Sync>;

/// Messages with a local change (flag or move) not yet pushed to the server,
/// by `(folder_id, uid)`. Sync leaves these alone, so a pass that read the
/// server just before the push doesn't undo what the user did. Mostly matters
/// with IDLE, which syncs the inbox the moment anything there changes.
#[derive(Default, Clone)]
pub struct Pending(Arc<Mutex<HashMap<(i64, u32), usize>>>);

/// Marks a message as pending until dropped.
pub struct PendingGuard {
    pending: Pending,
    key: (i64, u32),
}

impl Pending {
    pub fn hold(&self, folder_id: i64, uid: u32) -> PendingGuard {
        *self.0.lock().unwrap().entry((folder_id, uid)).or_default() += 1;
        PendingGuard { pending: self.clone(), key: (folder_id, uid) }
    }

    fn uids(&self, folder_id: i64) -> HashSet<u32> {
        self.0.lock().unwrap().keys().filter(|(f, _)| *f == folder_id).map(|(_, u)| *u).collect()
    }
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        let mut map = self.pending.0.lock().unwrap();
        if let Some(n) = map.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                map.remove(&self.key);
            }
        }
    }
}

pub struct Conn {
    session: Option<Session>,
    last_used: Instant,
    /// sync.step_timeout_secs, refreshed each time the connection is handed out.
    timeout: Duration,
}

/// Runs one IMAP exchange with a timeout. A failure drops the connection so
/// the next caller reconnects instead of reusing a broken session.
macro_rules! step {
    ($conn:ident, |$s:ident| $body:expr) => {{
        let r: Result<_> = match tokio::time::timeout($conn.timeout, async {
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
    pub config: Arc<ConfigFile>,
    emit: Emit,
    conns: Mutex<HashMap<i64, Arc<AsyncMutex<Conn>>>>,
    tokens: Mutex<HashMap<i64, Tokens>>,
    syncing: Mutex<HashSet<i64>>,
    pub pending: Pending,
    /// One lock per folder, so an IDLE-triggered inbox sync and a full pass
    /// never work on the same folder at once.
    folder_locks: Mutex<HashMap<i64, Arc<AsyncMutex<()>>>>,
    /// The IDLE watcher task of each account.
    idlers: Mutex<HashMap<i64, tokio::task::JoinHandle<()>>>,
    pub wake: Notify,
}

impl Inner {
    pub fn new(store: Store, secrets: Arc<dyn SecretStore>, config: Arc<ConfigFile>, emit: Emit) -> Self {
        Self {
            store,
            secrets,
            config,
            emit,
            conns: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            syncing: Mutex::new(HashSet::new()),
            pending: Pending::default(),
            folder_locks: Mutex::new(HashMap::new()),
            idlers: Mutex::new(HashMap::new()),
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
        // Reading and writing config.toml is blocking file I/O.
        let secrets = self.secrets.clone();
        tokio::task::spawn_blocking(move || secrets.get(&key)).await?
    }

    pub async fn set_secret(&self, key: String, value: String) -> Result<()> {
        let secrets = self.secrets.clone();
        tokio::task::spawn_blocking(move || secrets.set(&key, &value)).await?
    }

    pub fn client_creds(&self, ep: &OAuthEndpoints) -> Result<ClientCreds> {
        let id = self
            .config
            .setting(ep.client_id_key)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("no OAuth client ID is configured for this provider; add one in Settings"))?;
        let secret = self.config.setting(ep.client_secret_key)?.filter(|s| !s.trim().is_empty());
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
            .or_insert_with(|| Arc::new(AsyncMutex::new(Conn { session: None, last_used: Instant::now(), timeout: Duration::ZERO })))
            .clone();
        let mut conn = slot.lock_owned().await;
        let cfg = self.config.get();
        conn.timeout = Duration::from_secs(cfg.sync.step_timeout_secs);

        if conn.session.is_some() && conn.last_used.elapsed() > Duration::from_secs(45) {
            let alive = tokio::time::timeout(Duration::from_secs(10), conn.session.as_mut().unwrap().noop()).await;
            if !matches!(alive, Ok(Ok(()))) {
                conn.session = None;
            }
        }
        if conn.session.is_none() {
            let cred = self.credential(acc).await?;
            match imap::connect(acc, &cred, Duration::from_secs(cfg.network.imap_connect_timeout_secs)).await {
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
        if let Some(task) = self.idlers.lock().unwrap().remove(&account_id) {
            task.abort();
        }
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
        let lock = self.folder_locks.lock().unwrap().entry(folder.id).or_default().clone();
        let _held = lock.lock().await;
        let store = &self.store;
        let cfg = self.config.get();
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
                changed |= store.reconcile(folder.id, 1, u32::MAX, &[], &self.pending.uids(folder.id))?;
            }
            store.set_backfill_uid(folder.id, 1)?;
        } else {
            // New mail (or the newest window on first visit).
            let lo = if max > 0 {
                max + 1
            } else if sel.uid_next > cfg.sync.initial_window {
                sel.uid_next - cfg.sync.initial_window
            } else {
                1
            };
            let fresh = step!(conn, |s| imap::fetch_headers(s, &format!("{lo}:*")).await)?;
            // A message moved away locally but not yet on the server isn't new mail.
            let in_flight = self.pending.uids(folder.id);
            let fresh: Vec<_> = fresh.into_iter().filter(|m| m.uid >= lo && !in_flight.contains(&m.uid)).collect();
            changed |= store.insert_messages(acc.id, folder.id, &fresh)? > 0;
            if max == 0 {
                store.set_backfill_uid(folder.id, lo)?;
            }

            // Flag changes and deletions made elsewhere.
            if max > 0 {
                let floor = store.min_uid(folder.id)?;
                let from = if thorough { floor } else { floor.max(max.saturating_sub(cfg.sync.reconcile_window)) };
                let flags = step!(conn, |s| imap::fetch_flags(s, from, max).await)?;
                // Checked now, not before the fetch: pushes need this connection,
                // which is still held, so none can have finished in between.
                changed |= store.reconcile(folder.id, from, max, &flags, &self.pending.uids(folder.id))?;
            }

            // Older history, one chunk per pass.
            let cursor = store.backfill_uid(folder.id)?;
            if cursor > 1 {
                let lo = cursor.saturating_sub(cfg.sync.backfill_chunk).max(1);
                let older = step!(conn, |s| imap::fetch_headers(s, &format!("{lo}:{}", cursor - 1)).await)?;
                let in_flight = self.pending.uids(folder.id);
                let older: Vec<_> =
                    older.into_iter().filter(|m| m.uid >= lo && m.uid < cursor && !in_flight.contains(&m.uid)).collect();
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
            for (id, uid) in store.missing_bodies(folder.id, cfg.sync.prefetch_bodies.into())? {
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

    /// Syncs only the account's inbox: what IDLE asks for when the server
    /// announces a change there.
    pub async fn sync_inbox(&self, account_id: i64) -> Result<()> {
        let acc = self.store.account(account_id)?.ok_or_else(|| anyhow!("account was removed"))?;
        let Some(inbox) = self.store.folders(acc.id)?.into_iter().find(|f| f.role == "inbox") else { return Ok(()) };
        self.sync_folder(&acc, &inbox, false).await.map(|_| ())
    }

    // ---- IDLE -------------------------------------------------------------

    /// Starts an IDLE watcher for every account that lacks one, and stops
    /// those of accounts that are gone or whose watcher has ended.
    fn ensure_idlers(self: &Arc<Self>, accounts: &[Account]) {
        let mut idlers = self.idlers.lock().unwrap();
        idlers.retain(|id, task| {
            let keep = !task.is_finished() && accounts.iter().any(|a| a.id == *id);
            if !keep {
                task.abort();
            }
            keep
        });
        for acc in accounts {
            if !idlers.contains_key(&acc.id) {
                let inner = self.clone();
                let id = acc.id;
                idlers.insert(id, tokio::spawn(async move { inner.idle_loop(id).await }));
            }
        }
    }

    /// Keeps an IDLE connection open on the account's inbox, reconnecting with
    /// a growing delay after failures. Ends if the server has no IDLE (the poll
    /// covers that) or the account is removed. While `sync.idle` is off it
    /// only waits, checking the setting again each minute.
    async fn idle_loop(self: Arc<Self>, account_id: i64) {
        let mut backoff = Duration::from_secs(15);
        loop {
            if !self.config.get().sync.idle {
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
            let started = Instant::now();
            match self.idle_session(account_id).await {
                Ok(IdleEnd::Stop) => return,
                Ok(IdleEnd::Retry) => {}
                Err(e) => log::info!("IDLE for account {account_id}: {e:#}"),
            }
            // A session that lasted a while was healthy; start the delay over.
            if started.elapsed() > Duration::from_secs(300) {
                backoff = Duration::from_secs(15);
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(900));
        }
    }

    /// One IDLE connection, from connecting until it fails or has to restart.
    async fn idle_session(&self, account_id: i64) -> Result<IdleEnd> {
        use async_imap::extensions::idle::IdleResponse;

        let Some(acc) = self.store.account(account_id)? else { return Ok(IdleEnd::Stop) };
        // The first full pass lists the folders; until then there's nothing to watch.
        let Some(inbox) = self.store.folders(acc.id)?.into_iter().find(|f| f.role == "inbox") else {
            return Ok(IdleEnd::Retry);
        };
        let cfg = self.config.get();
        let step = Duration::from_secs(cfg.sync.step_timeout_secs);
        let restart = Duration::from_secs(cfg.sync.idle_restart_secs);
        let timed = |what: &'static str| move |_| anyhow!("the mail server stopped responding ({what})");

        let cred = self.credential(&acc).await?;
        let mut session = imap::connect(&acc, &cred, Duration::from_secs(cfg.network.imap_connect_timeout_secs)).await?;
        if !tokio::time::timeout(step, imap::supports_idle(&mut session)).await.map_err(timed("capabilities"))?? {
            log::info!("{} does not support IDLE; new mail is found by polling", acc.imap_host);
            let _ = session.logout().await;
            return Ok(IdleEnd::Stop);
        }
        tokio::time::timeout(step, imap::select(&mut session, &inbox.path)).await.map_err(timed("select"))??;
        // Anything already announced is old news: the inbox was just synced or is about to be.
        while session.unsolicited_responses.try_recv().is_ok() {}

        loop {
            if !self.config.get().sync.idle {
                let _ = session.logout().await;
                return Ok(IdleEnd::Retry);
            }
            let mut handle = session.idle();
            tokio::time::timeout(step, handle.init()).await.map_err(timed("IDLE"))??;
            let (wait, _stop) = handle.wait_with_timeout(restart);
            let response = wait.await?;
            session = tokio::time::timeout(step, handle.done()).await.map_err(timed("DONE"))??;

            // Changes reported while IDLE was being started land in the
            // session's side channel rather than in `response`.
            let mut changed = matches!(response, IdleResponse::NewData(_));
            while session.unsolicited_responses.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                if let Err(e) = self.sync_inbox(acc.id).await {
                    self.emit_error(Some(acc.id), &e.context(format!("syncing {}", inbox.name)));
                }
            }
        }
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
        let cfg = self.config.get();
        smtp::check_sender(&acc, &cfg.aliases(acc.id), &draft)?;
        let msg = smtp::build(&acc, &draft, threading)?;
        let raw = msg.formatted();
        let cred = self.credential(&acc).await?;
        smtp::send(&acc, &cred, msg, Duration::from_secs(cfg.network.smtp_timeout_secs)).await?;

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
                let _held = self.pending.hold(folder.id, uid);
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
            .secret(secrets::CLAUDE_KEY.to_string())
            .await?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| anyhow!("Add your Anthropic API key in Settings to use this."))?;
        let cfg = self.config.get();

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
        let mut out = claude::extract(key.trim(), &cfg.claude, &email).await?;

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
        let timeout = Duration::from_secs(self.config.get().network.imap_connect_timeout_secs);
        let mut s = imap::connect(acc, cred, timeout).await?;
        let _ = s.logout().await;
        Ok(())
    }
}

enum IdleEnd {
    /// Don't try again: no IDLE on this server, or the account is gone.
    Stop,
    /// Reconnect after a pause.
    Retry,
}

/// Background loop: sync every account now, then on a timer or when woken.
/// Each account also gets an IDLE watcher that syncs its inbox as soon as the
/// server reports a change, so the timer mostly matters for other folders.
pub async fn run(inner: Arc<Inner>) {
    let mut first = true;
    loop {
        let accounts = inner.store.accounts().unwrap_or_default();
        let mut tasks = Vec::new();
        for acc in &accounts {
            let inner = inner.clone();
            let thorough = first;
            let id = acc.id;
            tasks.push(tokio::spawn(async move { inner.sync_account(id, thorough).await.unwrap_or(false) }));
        }
        let mut more = false;
        for t in tasks {
            more |= t.await.unwrap_or(false);
        }
        first = false;
        // After the pass, so a new account's folders (and inbox) are known.
        inner.ensure_idlers(&accounts);

        let wait = if more { Duration::from_secs(1) } else { Duration::from_secs(inner.config.get().sync.poll_interval_secs) };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = inner.wake.notified() => {}
        }
    }
}
