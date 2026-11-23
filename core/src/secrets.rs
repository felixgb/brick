//! Credential storage. Passwords and refresh tokens never touch SQLite.
//!
//! They live in `config.toml` with every other setting: [`ConfigFile`]
//! implements [`SecretStore`], and the keys below are paths into that file.
//! The file is kept to its owner (mode 0600) but is not encrypted. There is no
//! Keychain or Secret Service backend.
//!
//! [`ConfigFile`]: crate::config::ConfigFile

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Keeps secrets in memory only. Used by tests and when `BRICK_SECRETS=memory`.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.map.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.map.lock().unwrap().remove(key);
        Ok(())
    }
}

pub fn password_key(account_id: i64) -> String {
    format!("accounts.{account_id}.password")
}

pub fn refresh_token_key(account_id: i64) -> String {
    format!("accounts.{account_id}.refresh_token")
}

/// The Anthropic API key.
pub const CLAUDE_KEY: &str = "claude.api_key";

/// Maps a key from the old `secrets.json` onto its place in `config.toml`.
pub fn legacy_key(old: &str) -> Option<String> {
    if old == "anthropic-api-key" {
        return Some(CLAUDE_KEY.to_string());
    }
    let rest = old.strip_prefix("account-")?;
    let (id, kind) = rest.split_once('-')?;
    let id: i64 = id.parse().ok()?;
    match kind {
        "password" => Some(password_key(id)),
        "refresh-token" => Some(refresh_token_key(id)),
        _ => None,
    }
}
