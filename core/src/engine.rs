//! The request API front ends use.
//!
//! [`Engine::call`] takes `{"method": "...", "params": {...}}` and returns
//! `{"ok": ...}` or `{"error": "..."}`. Calls only read or write the local
//! database, so they are safe to make on a UI thread; anything that needs the
//! network is started in the background and reported through events.

use crate::imap::Credential;
use crate::model::{Account, AuthKind, Security};
use crate::secrets::{self, MemoryStore, SecretStore};
use crate::store::{Scope, Store};
use crate::sync::{self, Emit, Inner};
use crate::items::{self, Item};
use crate::config::{self, ConfigFile, SecretStoreKind};
use crate::{claude, ics, mime, oauth, providers, smtp};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub struct Engine {
    runtime: tokio::runtime::Runtime,
    inner: Arc<Inner>,
}

fn param<T: for<'de> Deserialize<'de>>(params: &Value, key: &str) -> Result<T> {
    let v = params.get(key).ok_or_else(|| anyhow!("missing parameter \"{key}\""))?;
    T::deserialize(v).map_err(|e| anyhow!("bad parameter \"{key}\": {e}"))
}

fn opt<T: for<'de> Deserialize<'de>>(params: &Value, key: &str) -> Option<T> {
    params.get(key).filter(|v| !v.is_null()).and_then(|v| T::deserialize(v).ok())
}

impl Engine {
    /// Opens (or creates) the mail store in `data_dir` and starts background sync.
    /// Settings come from `config.toml` there (or `BRICK_CONFIG`); see config.rs.
    pub fn open(data_dir: &Path, emit: Emit) -> Result<Self> {
        std::fs::create_dir_all(data_dir).with_context(|| format!("could not create {}", data_dir.display()))?;
        let config = Arc::new(ConfigFile::open(&config::default_path(data_dir))?);
        // BRICK_SECRETS overrides secrets.store, so tools can run against a throwaway store.
        let kind = match std::env::var("BRICK_SECRETS") {
            Ok(v) if !v.trim().is_empty() => v.parse::<SecretStoreKind>().context("BRICK_SECRETS")?,
            _ => config.get().secrets.store,
        };
        let secrets: Arc<dyn SecretStore> = match kind {
            SecretStoreKind::Memory => Arc::new(MemoryStore::default()),
            SecretStoreKind::Config => {
                // Earlier versions kept them in secrets.json on Linux.
                if let Err(e) = config.adopt_secrets_file(&data_dir.join("secrets.json")) {
                    log::warn!("{e:#}");
                }
                config.clone()
            }
        };
        Self::start(data_dir, config, secrets, emit)
    }

    pub fn open_with(data_dir: &Path, secrets: Arc<dyn SecretStore>, emit: Emit) -> Result<Self> {
        std::fs::create_dir_all(data_dir).with_context(|| format!("could not create {}", data_dir.display()))?;
        let config = Arc::new(ConfigFile::open(&config::default_path(data_dir))?);
        Self::start(data_dir, config, secrets, emit)
    }

    fn start(data_dir: &Path, config: Arc<ConfigFile>, secrets: Arc<dyn SecretStore>, emit: Emit) -> Result<Self> {
        let store = Store::open(&data_dir.join("brick.sqlite"))?;
        // Older versions kept settings in the database; move them to config.toml.
        let legacy = store.legacy_settings()?;
        if !legacy.is_empty() {
            let moved = config.adopt_legacy(&legacy);
            store.delete_legacy_settings(&moved)?;
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("brick-core")
            .enable_all()
            .build()?;
        let inner = Arc::new(Inner::new(store, secrets, config, emit));
        runtime.spawn(sync::run(inner.clone()));
        Ok(Self { runtime, inner })
    }

    pub fn call(&self, request: &str) -> String {
        let result = serde_json::from_str::<Value>(request)
            .map_err(|e| anyhow!("malformed request: {e}"))
            .and_then(|req| {
                let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                let params = req.get("params").cloned().unwrap_or(Value::Null);
                self.dispatch(&method, &params)
            });
        match result {
            Ok(v) => json!({ "ok": v }).to_string(),
            Err(e) => json!({ "error": format!("{e:#}") }).to_string(),
        }
    }

    fn dispatch(&self, method: &str, p: &Value) -> Result<Value> {
        let store = &self.inner.store;
        Ok(match method {
            "providers.list" => json!(providers::all()),

            "settings.get" => json!(self.inner.config.setting(&param::<String>(p, "key")?)?),
            "settings.set" => {
                self.inner.config.set_setting(&param::<String>(p, "key")?, &param::<String>(p, "value")?)?;
                Value::Null
            }
            "config.status" => {
                let c = &self.inner.config;
                json!({ "path": c.path().display().to_string(), "error": c.error() })
            }

            "accounts.list" => json!(store.accounts()?),
            "accounts.add" => self.add_password_account(p)?,
            "accounts.oauth" => self.add_oauth_account(p)?,
            "accounts.remove" => {
                let id: i64 = param(p, "id")?;
                store.remove_account(id)?;
                self.inner.forget_account(id);
                if let Err(e) = self.inner.config.remove_account(id) {
                    log::warn!("{e:#}");
                }
                let _ = self.inner.secrets.delete(&secrets::password_key(id));
                let _ = self.inner.secrets.delete(&secrets::refresh_token_key(id));
                self.inner.emit(json!({ "type": "accounts_changed" }));
                Value::Null
            }

            "folders.list" => json!(store.folders(param(p, "account_id")?)?),

            // One folder, or with "role" instead of "folder_id" every folder of that
            // role across accounts (role "inbox" is the unified inbox).
            "messages.list" => {
                let offset = opt::<i64>(p, "offset").unwrap_or(0).max(0);
                let limit = opt::<i64>(p, "limit").unwrap_or(200).clamp(1, 1000);
                let scope = match opt::<String>(p, "role") {
                    Some(role) => Scope::Role(role),
                    None => Scope::Folder(param(p, "folder_id")?),
                };
                if opt::<bool>(p, "threaded").unwrap_or(false) {
                    // One row per conversation; each carries thread_count and thread_unread.
                    json!({
                        "total": store.count_threads(&scope)?,
                        "offset": offset,
                        "rows": store.list_threads(&scope, offset, limit)?,
                    })
                } else {
                    match scope {
                        Scope::Role(role) => json!({
                            "total": store.count_messages_by_role(&role)?,
                            "offset": offset,
                            "rows": store.list_messages_by_role(&role, offset, limit)?,
                        }),
                        Scope::Folder(folder_id) => json!({
                            "total": store.count_messages(folder_id)?,
                            "offset": offset,
                            "rows": store.list_messages(folder_id, offset, limit)?,
                        }),
                    }
                }
            }
            // Every message of the conversation a message is in, oldest first.
            // Pass folder_id or role to keep only the messages in that folder or role.
            "messages.thread" => {
                let scope = match (opt::<i64>(p, "folder_id"), opt::<String>(p, "role")) {
                    (Some(id), _) => Some(Scope::Folder(id)),
                    (None, Some(role)) => Some(Scope::Role(role)),
                    _ => None,
                };
                json!({ "rows": store.thread_of(param(p, "id")?, scope.as_ref())? })
            }
            "messages.search" => {
                let q: String = param(p, "query")?;
                json!({ "rows": store.search(&q, opt(p, "account_id"), opt::<i64>(p, "limit").unwrap_or(200))? })
            }
            "messages.get" => self.get_message(p)?,
            "messages.set_flags" => self.set_flags(p)?,
            "messages.move" => self.move_messages(p)?,
            "attachments.save" => {
                let id: i64 = param(p, "id")?;
                let index: usize = param(p, "index")?;
                let path: String = param(p, "path")?;
                let raw = store.body(id)?.ok_or_else(|| anyhow!("message has not been downloaded yet"))?;
                let (_, bytes) = mime::attachment(&raw, index).ok_or_else(|| anyhow!("attachment not found"))?;
                std::fs::write(&path, bytes).with_context(|| format!("could not write {path}"))?;
                Value::Null
            }

            "send" => {
                let draft: smtp::Draft = serde_json::from_value(p.clone()).map_err(|e| anyhow!("bad draft: {e}"))?;
                let token = opt::<String>(p, "token").unwrap_or_default();
                // Catch address mistakes now, while the compose window is still open.
                let acc = store.account(draft.account_id)?.ok_or_else(|| anyhow!("choose an account to send from"))?;
                smtp::check_sender(&acc, &self.inner.config.get().aliases(acc.id), &draft)?;
                smtp::build(&acc, &draft, None)?;
                let inner = self.inner.clone();
                self.runtime.spawn(async move {
                    match inner.send(draft).await {
                        Ok(()) => inner.emit(json!({ "type": "send_done", "token": token })),
                        Err(e) => inner.emit(json!({ "type": "send_error", "token": token, "message": format!("{e:#}") })),
                    }
                });
                Value::Null
            }

            "items.list" => json!(store.with_db(items::list)?),
            "items.save" => {
                let item: Item = param(p, "item")?;
                let saved = store.with_db(|db| items::save(db, item))?;
                self.inner.emit(json!({ "type": "items_changed" }));
                json!(saved)
            }
            "items.set_done" => {
                let (id, done): (i64, bool) = (param(p, "id")?, param(p, "done")?);
                store.with_db(|db| items::set_done(db, id, done))?;
                self.inner.emit(json!({ "type": "items_changed" }));
                Value::Null
            }
            "items.delete" => {
                let id: i64 = param(p, "id")?;
                store.with_db(|db| items::delete(db, id))?;
                self.inner.emit(json!({ "type": "items_changed" }));
                Value::Null
            }
            // The whole list as an iCalendar file's text.
            "items.export_ics" => json!(ics::export(&store.with_db(items::list)?)),
            // Adds the items in iCalendar text. Items the list already has are skipped.
            "items.import_ics" => {
                let text: String = param(p, "text")?;
                let (found, dropped) = ics::import(&text);
                json!(self.import_items(found, dropped, None)?)
            }
            // Adds the calendar invitations carried by an email, linked back to it.
            "items.import_invitation" => {
                let id: i64 = param(p, "id")?;
                let row = store.message(id)?.ok_or_else(|| anyhow!("message no longer exists"))?;
                let raw = store.body(id)?.ok_or_else(|| anyhow!("message has not been downloaded yet"))?;
                let header_id = store.threading(id)?.map(|t| t.0).unwrap_or_default();
                let (mut found, mut dropped) = (Vec::new(), 0);
                for text in mime::invitations(&raw) {
                    let (i, d) = ics::import(&text);
                    found.extend(i);
                    dropped += d;
                }
                if found.is_empty() {
                    bail!("this email has no calendar invitation");
                }
                let from = if row.from_name.is_empty() { row.from_addr.clone() } else { row.from_name.clone() };
                json!(self.import_items(found, dropped, Some((row.account_id, header_id, row.subject.clone(), from)))?)
            }
            // Items already made from this email.
            "items.for_message" => {
                let header_id = store.threading(param(p, "id")?)?.map(|t| t.0).unwrap_or_default();
                json!(store.with_db(|db| items::for_source(db, &header_id))?)
            }
            // Where the email an item came from lives now, or null if it is gone.
            "items.source" => {
                let id: i64 = param(p, "id")?;
                let item = store.with_db(|db| items::get(db, id))?.ok_or_else(|| anyhow!("that item no longer exists"))?;
                match store.find_by_message_id(item.source_account_id, &item.source_message_id)? {
                    Some((message_id, folder_id)) => json!({ "message_id": message_id, "folder_id": folder_id }),
                    None => Value::Null,
                }
            }
            // Asks Claude what is in an email. Proposals come back in an
            // `items_extracted` event; nothing is saved until the UI saves it.
            "items.extract" => {
                let id: i64 = param(p, "id")?;
                let token = opt::<String>(p, "token").unwrap_or_default();
                if store.message(id)?.is_none() {
                    bail!("message no longer exists");
                }
                let inner = self.inner.clone();
                self.runtime.spawn(async move {
                    match inner.extract_items(id).await {
                        Ok(out) => inner.emit(json!({
                            "type": "items_extracted", "token": token, "message_id": id,
                            "items": out.items, "summary": out.summary, "truncated": out.truncated,
                        })),
                        Err(e) => inner.emit(json!({
                            "type": "items_extract_error", "token": token, "message_id": id,
                            "message": format!("{e:#}"),
                        })),
                    }
                });
                Value::Null
            }

            "claude.status" => {
                let configured = self.inner.secrets.get(secrets::CLAUDE_KEY)?.is_some_and(|k| !k.trim().is_empty());
                let model = self.inner.config.get().claude.model().to_string();
                json!({ "configured": configured, "model": model, "default_model": claude::DEFAULT_MODEL })
            }
            "claude.set_key" => {
                let key = param::<String>(p, "key")?.trim().to_string();
                if key.is_empty() {
                    self.inner.secrets.delete(secrets::CLAUDE_KEY)?;
                } else {
                    self.inner.secrets.set(secrets::CLAUDE_KEY, &key)?;
                }
                Value::Null
            }

            "sync.now" => {
                self.inner.wake.notify_one();
                Value::Null
            }

            other => bail!("unknown method \"{other}\""),
        })
    }

    // ---- accounts -------------------------------------------------------

    fn account_from_params(&self, p: &Value) -> Result<Account> {
        let provider_id: String = param(p, "provider")?;
        let provider = providers::get(&provider_id).ok_or_else(|| anyhow!("unknown provider {provider_id}"))?;
        let email: String = param::<String>(p, "email")?.trim().to_string();
        if !email.contains('@') {
            bail!("enter a full email address");
        }
        let sec = |key: &str, default: Security| opt::<String>(p, key).map(|s| Security::parse(&s)).unwrap_or(default);
        Ok(Account {
            id: 0,
            name: opt::<String>(p, "name").unwrap_or_default().trim().to_string(),
            username: opt::<String>(p, "username").filter(|u| !u.is_empty()).unwrap_or_else(|| email.clone()),
            email,
            provider: provider.id.to_string(),
            auth: provider.auth,
            imap_host: opt::<String>(p, "imap_host").filter(|h| !h.is_empty()).unwrap_or_else(|| provider.imap_host.into()),
            imap_port: opt(p, "imap_port").unwrap_or(provider.imap_port),
            imap_security: sec("imap_security", provider.imap_security),
            smtp_host: opt::<String>(p, "smtp_host").filter(|h| !h.is_empty()).unwrap_or_else(|| provider.smtp_host.into()),
            smtp_port: opt(p, "smtp_port").unwrap_or(provider.smtp_port),
            smtp_security: sec("smtp_security", provider.smtp_security),
        })
    }

    /// Verifies the password against the server, then saves the account.
    /// Result arrives as an `account_added` or `account_error` event.
    fn add_password_account(&self, p: &Value) -> Result<Value> {
        let mut acc = self.account_from_params(p)?;
        if acc.auth != AuthKind::Password {
            bail!("{} accounts sign in through the browser", acc.provider);
        }
        let password: String = param(p, "password")?;
        if password.is_empty() {
            bail!("enter a password");
        }
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            let result: Result<i64> = async {
                inner.verify(&acc, &Credential::Password(password.clone())).await?;
                acc.id = inner.store.add_account(&acc)?;
                if let Err(e) = inner.set_secret(secrets::password_key(acc.id), password).await {
                    inner.store.remove_account(acc.id)?;
                    return Err(e);
                }
                Ok(acc.id)
            }
            .await;
            finish_add(&inner, result);
        });
        Ok(json!({ "pending": true }))
    }

    /// Starts browser sign-in. Returns the URL to open; the outcome arrives as
    /// an `account_added` or `account_error` event.
    fn add_oauth_account(&self, p: &Value) -> Result<Value> {
        let mut acc = self.account_from_params(p)?;
        let ep = providers::get(&acc.provider)
            .and_then(|p| p.oauth)
            .ok_or_else(|| anyhow!("{} does not use browser sign-in", acc.provider))?;
        let creds = self.inner.client_creds(&ep)?;
        // Only binds a local port; no network round-trip.
        let flow = self.runtime.block_on(oauth::begin(&ep, &creds, &acc.email))?;
        let auth_url = flow.auth_url.clone();

        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            let result: Result<i64> = async {
                let tokens = flow.finish(&ep, &creds, Duration::from_secs(300)).await?;
                inner.verify(&acc, &Credential::AccessToken(tokens.access_token.clone())).await?;
                acc.id = inner.store.add_account(&acc)?;
                let refresh = tokens.refresh_token.clone().unwrap_or_default();
                if let Err(e) = inner.set_secret(secrets::refresh_token_key(acc.id), refresh).await {
                    inner.store.remove_account(acc.id)?;
                    return Err(e);
                }
                inner.cache_tokens(acc.id, tokens);
                Ok(acc.id)
            }
            .await;
            finish_add(&inner, result);
        });
        Ok(json!({ "auth_url": auth_url }))
    }

    // ---- messages -------------------------------------------------------

    /// Saves imported items, skipping any the list already has.
    fn import_items(&self, found: Vec<Item>, dropped: usize, source: Option<(i64, String, String, String)>) -> Result<Value> {
        let store = &self.inner.store;
        let existing = store.with_db(items::list)?;
        let (mut added, mut skipped) = (0, 0);
        for mut item in found {
            if let Some((account, message_id, subject, from)) = &source {
                item.source_account_id = Some(*account);
                item.source_message_id = message_id.clone();
                item.source_subject = subject.clone();
                item.source_from = from.clone();
            }
            let item = item.normalise();
            if existing.iter().any(|e| ics::same(e, &item)) {
                skipped += 1;
                continue;
            }
            store.with_db(|db| items::save(db, item))?;
            added += 1;
        }
        if added > 0 {
            self.inner.emit(json!({ "type": "items_changed" }));
        }
        Ok(json!({ "added": added, "skipped": skipped, "repeats_dropped": dropped }))
    }

    fn get_message(&self, p: &Value) -> Result<Value> {
        let store = &self.inner.store;
        let id: i64 = param(p, "id")?;
        let allow_remote = opt::<bool>(p, "allow_remote").unwrap_or(false);
        let row = store.message(id)?.ok_or_else(|| anyhow!("message no longer exists"))?;
        let cc = store.threading(id)?.map(|t| t.2).unwrap_or_default();

        let body = match store.body(id)? {
            Some(raw) => Some(mime::render(&raw, allow_remote)),
            None => {
                let inner = self.inner.clone();
                self.runtime.spawn(async move {
                    if let Err(e) = inner.fetch_body(id).await {
                        inner.emit(json!({ "type": "body_error", "id": id, "message": format!("{e:#}") }));
                    }
                });
                None
            }
        };
        Ok(json!({ "message": row, "cc": cc, "body": body }))
    }

    fn set_flags(&self, p: &Value) -> Result<Value> {
        let ids: Vec<i64> = param(p, "ids")?;
        let seen: Option<bool> = opt(p, "seen");
        let flagged: Option<bool> = opt(p, "flagged");
        let store = &self.inner.store;
        let mut folders = std::collections::BTreeSet::new();

        for id in ids {
            let Some(m) = store.message(id)? else { continue };
            let (Some(acc), Some(folder)) = (store.account(m.account_id)?, store.folder(m.folder_id)?) else { continue };
            let mut ops: Vec<(&'static str, bool)> = Vec::new();
            if let Some(v) = seen.filter(|v| *v != m.seen) {
                store.set_seen(id, v)?;
                ops.push(("\\Seen", v));
            }
            if let Some(v) = flagged.filter(|v| *v != m.flagged) {
                store.set_flagged(id, v)?;
                ops.push(("\\Flagged", v));
            }
            if ops.is_empty() {
                continue;
            }
            folders.insert((acc.id, folder.id));
            let inner = self.inner.clone();
            self.runtime.spawn(async move {
                for (flag, on) in ops {
                    if let Err(e) = inner.push_flag(acc.clone(), folder.clone(), m.uid, flag, on).await {
                        inner.emit_error(Some(acc.id), &e.context("could not update the message on the server"));
                        inner.wake.notify_one(); // resync to show the server's truth
                        break;
                    }
                }
            });
        }
        for (account_id, folder_id) in folders {
            store.refresh_counts(folder_id)?;
            self.inner.emit(json!({ "type": "messages_changed", "account_id": account_id, "folder_id": folder_id }));
            self.inner.emit(json!({ "type": "folders_changed", "account_id": account_id }));
        }
        Ok(Value::Null)
    }

    fn move_messages(&self, p: &Value) -> Result<Value> {
        let ids: Vec<i64> = param(p, "ids")?;
        let to: Value = p.get("to").cloned().ok_or_else(|| anyhow!("missing parameter \"to\""))?;
        let store = &self.inner.store;
        let mut folders = std::collections::BTreeSet::new();

        for id in ids {
            let Some(m) = store.message(id)? else { continue };
            let (Some(acc), Some(folder)) = (store.account(m.account_id)?, store.folder(m.folder_id)?) else { continue };

            // None means delete permanently (deleting something already in Trash).
            let dest = match &to {
                Value::String(role) if role == "trash" && folder.role == "trash" => None,
                Value::String(role) if role == "archive" => Some(
                    store
                        .folder_by_role(acc.id, "archive")?
                        .or(store.folder_by_role(acc.id, "all")?)
                        .ok_or_else(|| anyhow!("{} has no archive folder", acc.email))?,
                ),
                Value::String(role) => Some(
                    store.folder_by_role(acc.id, role)?.ok_or_else(|| anyhow!("{} has no {role} folder", acc.email))?,
                ),
                Value::Number(n) => Some(
                    store
                        .folder(n.as_i64().unwrap_or(0))?
                        .filter(|f| f.account_id == acc.id)
                        .ok_or_else(|| anyhow!("destination folder not found"))?,
                ),
                _ => bail!("bad parameter \"to\""),
            };
            if dest.as_ref().is_some_and(|d| d.id == folder.id) {
                continue;
            }

            // Optimistic: the row disappears now; the destination picks it up on the next sync.
            store.delete_message(id)?;
            folders.insert((acc.id, folder.id));
            let inner = self.inner.clone();
            self.runtime.spawn(async move {
                let account_id = acc.id;
                if let Err(e) = inner.push_move(acc, folder, m.uid, dest).await {
                    inner.emit_error(Some(account_id), &e.context("could not move the message on the server"));
                }
                inner.wake.notify_one();
            });
        }
        for (account_id, folder_id) in folders {
            store.refresh_counts(folder_id)?;
            self.inner.emit(json!({ "type": "messages_changed", "account_id": account_id, "folder_id": folder_id }));
            self.inner.emit(json!({ "type": "folders_changed", "account_id": account_id }));
        }
        Ok(Value::Null)
    }
}

fn finish_add(inner: &Arc<Inner>, result: Result<i64>) {
    match result {
        Ok(id) => {
            inner.emit(json!({ "type": "account_added", "account_id": id }));
            inner.emit(json!({ "type": "accounts_changed" }));
            inner.wake.notify_one();
        }
        Err(e) => inner.emit(json!({ "type": "account_error", "message": format!("{e:#}") })),
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Dropping the runtime would block on in-flight network calls.
        let rt = std::mem::replace(
            &mut self.runtime,
            tokio::runtime::Builder::new_current_thread().build().expect("runtime"),
        );
        rt.shutdown_background();
    }
}
