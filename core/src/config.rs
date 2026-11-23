//! Brick's configuration file, `config.toml`.
//!
//! Every setting lives here: the engine's tunables (sync windows, timeouts,
//! the Claude model), OAuth client IDs, per-account aliases and signatures, and
//! front-end preferences. Secrets do not: passwords, refresh tokens and the
//! Anthropic key stay in the secret store (see `secrets.rs`).
//!
//! The file is `config.toml` in the data directory, or wherever `BRICK_CONFIG`
//! points. A commented template listing every key with its default is written
//! the first time. The engine re-reads the file whenever it changes on disk, so
//! hand edits apply without a restart (except `secrets.store`, read at startup).
//! `settings.set` edits the file in place through `toml_edit`, so the user's
//! comments and layout survive.
//!
//! Precedence: environment variable (`BRICK_SECRETS`, `BRICK_ANTHROPIC_BASE_URL`)
//! over the file over the built-in defaults.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use toml_edit::{DocumentMut, Item, Table, Value};

pub const FILE_NAME: &str = "config.toml";

// ---- schema ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub sync: SyncConfig,
    pub network: NetworkConfig,
    pub claude: ClaudeConfig,
    pub secrets: SecretsConfig,
    pub ui: UiConfig,
    /// OAuth client credentials by provider id ("google", "microsoft").
    pub oauth: BTreeMap<String, OAuthClient>,
    /// Per-account options, keyed by account id.
    pub accounts: BTreeMap<String, AccountConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SyncConfig {
    pub poll_interval_secs: u64,
    pub initial_window: u32,
    pub backfill_chunk: u32,
    pub reconcile_window: u32,
    pub prefetch_bodies: u32,
    pub step_timeout_secs: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 120,
            initial_window: 300,
            backfill_chunk: 500,
            reconcile_window: 2000,
            prefetch_bodies: 30,
            step_timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub imap_connect_timeout_secs: u64,
    pub smtp_timeout_secs: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self { imap_connect_timeout_secs: 20, smtp_timeout_secs: 60 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClaudeConfig {
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
    pub max_body_chars: usize,
    pub timeout_secs: u64,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            model: crate::claude::DEFAULT_MODEL.into(),
            base_url: "https://api.anthropic.com".into(),
            max_tokens: 16000,
            max_body_chars: 100_000,
            timeout_secs: 180,
        }
    }
}

impl ClaudeConfig {
    /// The model to use; a blank `model` means the default.
    pub fn model(&self) -> &str {
        match self.model.trim() {
            "" => crate::claude::DEFAULT_MODEL,
            m => m,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretStoreKind {
    /// The Keychain on macOS, `secrets.json` elsewhere.
    #[default]
    Auto,
    File,
    Memory,
    Keychain,
}

impl std::str::FromStr for SecretStoreKind {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.trim() {
            "auto" | "" => Self::Auto,
            "file" => Self::File,
            "memory" => Self::Memory,
            "keychain" => Self::Keychain,
            other => bail!("unknown secret store \"{other}\" (use auto, file, memory or keychain)"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecretsConfig {
    pub store: SecretStoreKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    /// Group the message list into conversations.
    pub threaded: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { threaded: true }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountConfig {
    /// Extra addresses the account may send as.
    pub aliases: Vec<String>,
    /// Appended to new messages by the front ends.
    pub signature: String,
}

impl Config {
    fn parse(text: &str) -> Result<Self> {
        let cfg: Config = toml_edit::de::from_str(text).map_err(|e| anyhow!("{}", e.to_string().trim_end()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        let positive = [
            ("sync.poll_interval_secs", self.sync.poll_interval_secs),
            ("sync.initial_window", self.sync.initial_window.into()),
            ("sync.backfill_chunk", self.sync.backfill_chunk.into()),
            ("sync.reconcile_window", self.sync.reconcile_window.into()),
            ("sync.step_timeout_secs", self.sync.step_timeout_secs),
            ("network.imap_connect_timeout_secs", self.network.imap_connect_timeout_secs),
            ("network.smtp_timeout_secs", self.network.smtp_timeout_secs),
            ("claude.max_tokens", self.claude.max_tokens.into()),
            ("claude.max_body_chars", self.claude.max_body_chars as u64),
            ("claude.timeout_secs", self.claude.timeout_secs),
        ];
        for (key, v) in positive {
            if v == 0 {
                bail!("{key} must be at least 1");
            }
        }
        for id in self.accounts.keys() {
            if id.parse::<i64>().is_err() {
                bail!("[accounts.{id}]: account tables are named by account id, a number");
            }
        }
        Ok(())
    }

    pub fn account(&self, account_id: i64) -> Option<&AccountConfig> {
        self.accounts.get(&account_id.to_string())
    }

    /// Addresses the account may send as, besides its own.
    pub fn aliases(&self, account_id: i64) -> Vec<String> {
        self.account(account_id).map(|a| split_aliases(&a.aliases.join("\n"))).unwrap_or_default()
    }
}

fn split_aliases(raw: &str) -> Vec<String> {
    raw.split([',', ';', '\n', ' ', '\t']).map(str::trim).filter(|a| !a.is_empty()).map(String::from).collect()
}

/// Written the first time Brick runs. Every `# key = value` line is the
/// built-in default (a test checks they agree).
pub const TEMPLATE: &str = r##"# Brick configuration.
#
# Every setting is listed with its default, commented out: remove the "# " in
# front of a line to change it. Brick notices when this file changes, so edits
# apply without a restart (except secrets.store, which is read at startup).
# The app's settings screens write here too, and keep your comments.
#
# Passwords, OAuth tokens and the Anthropic API key are not kept here.

[sync]
# Seconds between checks for new mail.
# poll_interval_secs = 120
# Newest messages fetched when a folder is first seen.
# initial_window = 300
# Older messages examined per pass while filling in a folder's history.
# backfill_chunk = 500
# How many of the newest messages are checked for flag changes and deletions.
# reconcile_window = 2000
# Inbox messages downloaded ahead of time so opening them is instant.
# prefetch_bodies = 30
# Seconds to wait for the mail server to answer one command.
# step_timeout_secs = 120

[network]
# imap_connect_timeout_secs = 20
# smtp_timeout_secs = 60

[claude]
# Model used to find to-dos and events in an email.
# model = "claude-opus-5-5"
# base_url = "https://api.anthropic.com"
# max_tokens = 16000
# Longer emails are cut to this many characters before they are sent.
# max_body_chars = 100000
# timeout_secs = 180

[secrets]
# Where passwords, OAuth tokens and the API key are kept: "auto" (the Keychain
# on macOS, secrets.json in the data directory elsewhere), "file"
# (secrets.json), "keychain" (macOS only) or "memory" (forgotten on exit).
# store = "auto"

[ui]
# Group the message list into conversations.
# threaded = true

# OAuth client credentials for browser sign-in, one table per provider:
#   [oauth.google]
#   client_id = "1234-abcd.apps.googleusercontent.com"
#   client_secret = "..."
[oauth]

# Per-account options. The table name is the account's id:
#   [accounts.1]
#   aliases = ["me@work.example", "shop@example.com"]
#   signature = """
#   Best wishes,
#   A. Person"""
[accounts]
"##;

/// `BRICK_CONFIG`, or `config.toml` in the data directory.
pub fn default_path(data_dir: &Path) -> PathBuf {
    match std::env::var_os("BRICK_CONFIG") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => data_dir.join(FILE_NAME),
    }
}

// ---- the file ---------------------------------------------------------------

pub struct ConfigFile {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    doc: DocumentMut,
    config: Arc<Config>,
    stamp: Option<(SystemTime, u64)>,
    /// Why the file on disk was last rejected; the previous good config stays in use.
    broken: Option<String>,
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn load(path: &Path) -> Result<(DocumentMut, Config)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| anyhow!("{}", e.to_string().trim_end()))?;
    let cfg = Config::parse(&text)?;
    Ok((doc, cfg))
}

impl ConfigFile {
    /// Opens the file, writing the template first if there is none. A file
    /// that is not valid is an error here, so a typo is noticed at startup.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
            }
            write_atomic(path, TEMPLATE)?;
        }
        let (doc, cfg) = load(path).with_context(|| format!("{} is not valid", path.display()))?;
        let state = State { doc, config: Arc::new(cfg), stamp: stamp(path), broken: None };
        Ok(Self { path: path.to_path_buf(), state: Mutex::new(state) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The current configuration, re-read first if the file changed.
    pub fn get(&self) -> Arc<Config> {
        let mut st = self.state.lock().unwrap();
        self.refresh(&mut st);
        st.config.clone()
    }

    /// The problem with the file on disk, if it was edited into an invalid state.
    pub fn error(&self) -> Option<String> {
        let mut st = self.state.lock().unwrap();
        self.refresh(&mut st);
        st.broken.clone()
    }

    fn refresh(&self, st: &mut State) {
        let now = stamp(&self.path);
        if now == st.stamp {
            return;
        }
        st.stamp = now;
        if now.is_none() {
            // Deleted: keep what we have; the next settings.set writes it back.
            st.broken = None;
            return;
        }
        match load(&self.path) {
            Ok((doc, cfg)) => {
                st.doc = doc;
                st.config = Arc::new(cfg);
                st.broken = None;
            }
            Err(e) => {
                log::warn!("ignoring changes to {}: {e:#}", self.path.display());
                st.broken = Some(format!("{e:#}"));
            }
        }
    }

    /// A setting as it is written in the file, or None when it is left at its
    /// default. See [`resolve`] for the accepted key names.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        let path = resolve(key)?;
        let mut st = self.state.lock().unwrap();
        self.refresh(&mut st);
        Ok(lookup(st.doc.as_item(), &path).and_then(render))
    }

    /// Sets a setting from its text form; a blank value removes it, restoring
    /// the default. Numbers and booleans are parsed; anything that would make
    /// the file invalid is refused and nothing is written.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let path = resolve(key)?;
        self.edit(|doc| {
            if value.trim().is_empty() {
                remove(doc.as_table_mut(), &path);
                return Ok(());
            }
            let new = typed_value(&path, value)?;
            if lookup(doc.as_item(), &path).is_none() {
                if let Some(d) = uncomment_in_place(&doc.to_string(), &path, &new) {
                    *doc = d;
                    return Ok(());
                }
            }
            let (last, parents) = path.split_last().unwrap();
            let table = table_at(doc.as_table_mut(), parents)?;
            let decor = table.get(last).and_then(Item::as_value).map(|v| v.decor().clone());
            let mut new = new;
            if let Some(d) = decor {
                *new.decor_mut() = d;
            }
            table.insert(last, Item::Value(new));
            Ok(())
        })
        .with_context(|| format!("could not set {}", path.join(".")))
    }

    /// Drops an account's table.
    pub fn remove_account(&self, account_id: i64) -> Result<()> {
        let id = account_id.to_string();
        let present = self.get().accounts.contains_key(&id);
        if !present {
            return Ok(());
        }
        self.edit(|doc| {
            remove(doc.as_table_mut(), &["accounts".into(), id.clone()]);
            Ok(())
        })
    }

    /// Moves settings from the old SQLite `settings` table into the file. A
    /// value already in the file wins. Returns the keys it took care of, which
    /// the caller can delete from the database.
    pub fn adopt_legacy(&self, rows: &[(String, String)]) -> Vec<String> {
        let mut done = Vec::new();
        for (key, value) in rows {
            if resolve(key).is_err() {
                log::warn!("old setting {key} has no place in {}; left in the database", FILE_NAME);
                continue;
            }
            let ok = match self.setting(key) {
                Ok(Some(_)) => true,
                Ok(None) => self.set_setting(key, value).map_err(|e| log::warn!("{e:#}")).is_ok(),
                Err(_) => false,
            };
            if ok {
                done.push(key.clone());
            }
        }
        done
    }

    fn edit(&self, f: impl FnOnce(&mut DocumentMut) -> Result<()>) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        self.refresh(&mut st);
        if let Some(e) = &st.broken {
            bail!("{} has an error; fix it first: {e}", self.path.display());
        }
        let mut doc = st.doc.clone();
        f(&mut doc)?;
        let text = doc.to_string();
        let cfg = Config::parse(&text)?;
        write_atomic(&self.path, &text)?;
        st.doc = doc;
        st.config = Arc::new(cfg);
        st.stamp = stamp(&self.path);
        Ok(())
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("toml.tmp");
    let write = || -> std::io::Result<()> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        // OAuth client secrets live here; keep the file to its owner.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().with_context(|| format!("could not save {}", path.display()))
}

// ---- keys -----------------------------------------------------------------

/// Turns a setting name into a path in the file. Dotted paths
/// (`sync.poll_interval_secs`) are used as they are; the flat names the front
/// ends have always used are still accepted:
///
/// * `claude_model` → `claude.model`
/// * `<provider>_client_id`, `<provider>_client_secret` → `oauth.<provider>.client_id`, …
/// * `aliases_<id>`, `signature_<id>` → `accounts.<id>.aliases`, …
pub fn resolve(key: &str) -> Result<Vec<String>> {
    let key = key.trim();
    let v = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if key == "claude_model" {
        return Ok(v(&["claude", "model"]));
    }
    for (prefix, field) in [("aliases_", "aliases"), ("signature_", "signature")] {
        if let Some(id) = key.strip_prefix(prefix) {
            if id.parse::<i64>().is_ok() {
                return Ok(v(&["accounts", id, field]));
            }
        }
    }
    if !key.contains('.') {
        for field in ["client_id", "client_secret"] {
            if let Some(provider) = key.strip_suffix(field).and_then(|p| p.strip_suffix('_')) {
                if !provider.is_empty() {
                    return Ok(v(&["oauth", provider, field]));
                }
            }
        }
        bail!("unknown setting \"{key}\"");
    }
    let parts: Vec<String> = key.split('.').map(|s| s.trim().to_string()).collect();
    if parts.iter().any(String::is_empty) {
        bail!("unknown setting \"{key}\"");
    }
    Ok(parts)
}

fn lookup<'a>(item: &'a Item, path: &[String]) -> Option<&'a Item> {
    path.iter().try_fold(item, |it, seg| it.as_table_like()?.get(seg))
}

/// Settings travel as text: arrays are one entry per line.
fn render(item: &Item) -> Option<String> {
    let v = item.as_value()?;
    Some(match v {
        Value::String(s) => s.value().clone(),
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n"),
        Value::Integer(i) => i.value().to_string(),
        Value::Boolean(b) => b.value().to_string(),
        Value::Float(f) => f.value().to_string(),
        other => other.to_string().trim().to_string(),
    })
}

/// Parses `raw` into the type the schema has at `path`.
fn typed_value(path: &[String], raw: &str) -> Result<Value> {
    if path.last().map(String::as_str) == Some("aliases") {
        return Ok(Value::Array(split_aliases(raw).into_iter().collect()));
    }
    let defaults = toml_edit::ser::to_document(&Config::default()).expect("defaults serialise");
    let key = path.join(".");
    Ok(match lookup(defaults.as_item(), path).and_then(Item::as_value) {
        Some(Value::Integer(_)) => {
            let n: i64 = raw.trim().parse().map_err(|_| anyhow!("{key} must be a whole number, not \"{}\"", raw.trim()))?;
            Value::from(n)
        }
        Some(Value::Boolean(_)) => Value::from(match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => true,
            "false" | "no" | "off" | "0" => false,
            other => bail!("{key} must be true or false, not \"{other}\""),
        }),
        _ if path.last().map(String::as_str) == Some("signature") => Value::from(raw),
        _ => Value::from(raw.trim()),
    })
}

/// Sets a key the template lists as a commented-out default by replacing that
/// `# key = default` line, so the value stays under its explanation. None if
/// there is no such line in the right section.
fn uncomment_in_place(text: &str, path: &[String], new: &Value) -> Option<DocumentMut> {
    let (last, parents) = path.split_last()?;
    let header = format!("[{}]", parents.join("."));
    let mut in_section = parents.is_empty();
    let mut done = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        if t.starts_with('[') {
            in_section = t == header;
        }
        let commented = t.strip_prefix("# ").and_then(|r| r.split_once(" = ")).is_some_and(|(k, _)| k == last);
        if in_section && commented && !done {
            out.push_str(&format!("{last} = {}\n", new.to_string().trim()));
            done = true;
        } else {
            out.push_str(line);
        }
    }
    if !done {
        return None;
    }
    let doc: DocumentMut = out.parse().ok()?;
    let placed = lookup(doc.as_item(), path).and_then(render);
    (placed.is_some() && placed == render(&Item::Value(new.clone()))).then_some(doc)
}

/// The table at `path`, creating missing ones. New nested tables are placed
/// right after their parent so `[accounts.3]` lands under `[accounts]`.
fn table_at<'a>(root: &'a mut Table, path: &[String]) -> Result<&'a mut Table> {
    let mut t = root;
    for (i, seg) in path.iter().enumerate() {
        let pos = t.position();
        let item = t.entry(seg).or_insert_with(|| {
            let mut nt = Table::new();
            nt.set_implicit(true);
            if let Some(p) = pos {
                nt.set_position(p);
            }
            Item::Table(nt)
        });
        t = item.as_table_mut().ok_or_else(|| anyhow!("{} is not a table", path[..=i].join(".")))?;
    }
    Ok(t)
}

/// Removes the key at `path`, then any nested table it leaves empty.
fn remove(root: &mut Table, path: &[String]) {
    fn go(t: &mut Table, path: &[String], depth: usize) {
        match path {
            [] => {}
            [last] => {
                t.remove(last);
            }
            [first, rest @ ..] => {
                if let Some(child) = t.get_mut(first).and_then(Item::as_table_mut) {
                    go(child, rest, depth + 1);
                    // Keep top-level sections such as [accounts]; drop emptied [accounts.3].
                    if depth >= 1 && child.is_empty() {
                        t.remove(first);
                    }
                }
            }
        }
    }
    go(root, path, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> (tempfile::TempDir, ConfigFile) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ConfigFile::open(&dir.path().join(FILE_NAME)).unwrap();
        (dir, cfg)
    }

    fn text(c: &ConfigFile) -> String {
        std::fs::read_to_string(c.path()).unwrap()
    }

    #[test]
    fn template_is_written_and_its_commented_defaults_match_the_code() {
        let (_d, c) = file();
        assert_eq!(text(&c), TEMPLATE);
        assert_eq!(*c.get(), Config::default());

        // Uncomment every "# key = value" line: the result must be the defaults.
        let uncommented: String = TEMPLATE
            .lines()
            .map(|l| match l.strip_prefix("# ") {
                Some(rest) if rest.split_once(" = ").is_some_and(|(k, _)| k.chars().all(|c| c.is_ascii_lowercase() || c == '_')) => rest,
                _ => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(uncommented, TEMPLATE);
        assert_eq!(Config::parse(&uncommented).unwrap(), Config::default());
    }

    #[test]
    fn old_flat_keys_map_onto_sections() {
        let r = |k: &str| resolve(k).unwrap().join(".");
        assert_eq!(r("claude_model"), "claude.model");
        assert_eq!(r("google_client_id"), "oauth.google.client_id");
        assert_eq!(r("microsoft_client_secret"), "oauth.microsoft.client_secret");
        assert_eq!(r("aliases_12"), "accounts.12.aliases");
        assert_eq!(r("signature_3"), "accounts.3.signature");
        assert_eq!(r("sync.poll_interval_secs"), "sync.poll_interval_secs");
        assert!(resolve("whatever").is_err());
        assert!(resolve("sync..x").is_err());
    }

    #[test]
    fn set_keeps_comments_and_types_values() {
        let (_d, c) = file();
        c.set_setting("sync.poll_interval_secs", "300").unwrap();
        c.set_setting("ui.threaded", "false").unwrap();
        c.set_setting("claude_model", " claude-sonnet-5-5 ").unwrap();
        c.set_setting("aliases_4", "a@x.example, b@x.example\nc@x.example").unwrap();
        c.set_setting("signature_4", "Best,\nA. Person").unwrap();

        let cfg = c.get();
        assert_eq!(cfg.sync.poll_interval_secs, 300);
        assert!(!cfg.ui.threaded);
        assert_eq!(cfg.claude.model(), "claude-sonnet-5-5");
        assert_eq!(cfg.aliases(4), ["a@x.example", "b@x.example", "c@x.example"]);
        assert_eq!(cfg.account(4).unwrap().signature, "Best,\nA. Person");

        let t = text(&c);
        assert!(t.contains("# Seconds between checks for new mail."), "comments survive:\n{t}");
        assert!(
            t.contains("# Seconds between checks for new mail.\npoll_interval_secs = 300\n"),
            "set in place of its commented default:\n{t}"
        );
        assert!(!t.contains("# poll_interval_secs = 120"));
        assert!(t.find("[accounts]").unwrap() < t.find("[accounts.4]").unwrap(), "{t}");

        assert_eq!(c.setting("aliases_4").unwrap().unwrap(), "a@x.example\nb@x.example\nc@x.example");
        assert_eq!(c.setting("sync.poll_interval_secs").unwrap().as_deref(), Some("300"));
        assert_eq!(c.setting("sync.initial_window").unwrap(), None, "unset means default");

        // Blank removes, and an emptied account table goes away.
        c.set_setting("aliases_4", "").unwrap();
        c.set_setting("signature_4", "").unwrap();
        assert!(!text(&c).contains("[accounts.4]"));
        assert!(c.get().account(4).is_none());
    }

    #[test]
    fn bad_values_are_refused_and_nothing_is_written() {
        let (_d, c) = file();
        let before = text(&c);
        assert!(format!("{:#}", c.set_setting("sync.poll_interval_secs", "soon").unwrap_err()).contains("whole number"));
        assert!(c.set_setting("sync.poll_interval_secs", "0").is_err());
        assert!(c.set_setting("sync.poll_interval_secs", "-5").is_err());
        assert!(c.set_setting("sync.bogus", "1").is_err());
        assert!(c.set_setting("ui.threaded", "maybe").is_err());
        assert!(c.set_setting("secrets.store", "vault").is_err());
        assert_eq!(text(&c), before);
    }

    #[test]
    fn hand_edits_are_picked_up_and_broken_edits_ignored() {
        let (_d, c) = file();
        let edited = TEMPLATE.replace("# prefetch_bodies = 30", "prefetch_bodies = 5");
        std::fs::write(c.path(), &edited).unwrap();
        // mtime resolution can be coarse; the length differs anyway.
        assert_eq!(c.get().sync.prefetch_bodies, 5);

        std::fs::write(c.path(), format!("{edited}\n[sync\n")).unwrap();
        assert_eq!(c.get().sync.prefetch_bodies, 5, "last good config stays in use");
        assert!(c.error().is_some());
        assert!(format!("{:#}", c.set_setting("ui.threaded", "false").unwrap_err()).contains("fix it first"));
    }

    #[test]
    fn an_invalid_file_is_an_error_at_open() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(FILE_NAME);
        std::fs::write(&p, "[sync]\npoll_interval_secs = \"often\"\n").unwrap();
        let err = format!("{:#}", ConfigFile::open(&p).err().unwrap());
        assert!(err.contains("poll_interval_secs"), "{err}");
    }

    #[test]
    fn legacy_rows_are_adopted_but_the_file_wins() {
        let (_d, c) = file();
        c.set_setting("claude_model", "from-file").unwrap();
        let rows = vec![
            ("claude_model".to_string(), "from-db".to_string()),
            ("google_client_id".to_string(), "abc".to_string()),
            ("mystery".to_string(), "x".to_string()),
        ];
        let done = c.adopt_legacy(&rows);
        assert_eq!(done, ["claude_model", "google_client_id"]);
        let cfg = c.get();
        assert_eq!(cfg.claude.model, "from-file");
        assert_eq!(cfg.oauth["google"].client_id, "abc");
    }

    #[test]
    fn removing_an_account_drops_its_table() {
        let (_d, c) = file();
        c.set_setting("signature_9", "hi").unwrap();
        c.remove_account(9).unwrap();
        assert!(c.get().account(9).is_none());
        assert!(text(&c).contains("[accounts]"));
    }
}
