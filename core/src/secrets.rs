//! Credential storage. Passwords and refresh tokens never touch SQLite.

#[cfg(target_os = "macos")]
use anyhow::anyhow;
use anyhow::Result;
#[cfg(unix)]
use anyhow::Context;
use std::collections::HashMap;
#[cfg(unix)]
use std::collections::BTreeMap;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// The macOS Keychain. Not built on Linux: the Secret Service (GNOME Keyring,
/// KWallet) is deliberately not used there.
#[cfg(target_os = "macos")]
pub struct KeyringStore {
    service: String,
}

#[cfg(target_os = "macos")]
impl KeyringStore {
    pub fn new(service: &str) -> Self {
        Self { service: service.to_string() }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(|e| anyhow!("keychain unavailable: {e}"))
    }
}

#[cfg(target_os = "macos")]
impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!("could not read from the keychain: {e}")),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| anyhow!("could not save to the keychain: {e}"))
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow!("could not delete from the keychain: {e}")),
        }
    }
}

/// Secrets in a JSON file that only the owner can read or write (mode 0600),
/// like `~/.netrc` or msmtp's config. Used on Linux instead of the Secret
/// Service, so Brick works on any desktop or none.
///
/// The file is not encrypted: anything running as the user can read it, and
/// backups copy it as plain text. Every change rewrites the whole file through a
/// temporary file and a rename, so a crash never leaves it half-written.
#[cfg(unix)]
pub struct FileStore {
    path: PathBuf,
    map: Mutex<BTreeMap<String, String>>,
}

#[cfg(unix)]
impl FileStore {
    /// Loads `path`, or starts empty if it does not exist. A file other users
    /// can read or write has its permissions narrowed to 0600.
    pub fn open(path: &Path) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let map = match std::fs::read_to_string(path) {
            Ok(text) => {
                let mode = std::fs::metadata(path)?.permissions().mode();
                if mode & 0o077 != 0 {
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                        .with_context(|| format!("could not restrict permissions on {}", path.display()))?;
                }
                serde_json::from_str(&text).with_context(|| format!("{} is not a valid secrets file", path.display()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
        };
        Ok(Self { path: path.to_path_buf(), map: Mutex::new(map) })
    }

    fn save(&self, map: &BTreeMap<String, String>) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let tmp = self.path.with_extension("tmp");
        let write = || -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
            f.write_all(serde_json::to_string_pretty(map)?.as_bytes())?;
            f.sync_all()?;
            std::fs::rename(&tmp, &self.path)
        };
        write().with_context(|| format!("could not save {}", self.path.display()))
    }

    fn change(&self, f: impl FnOnce(&mut BTreeMap<String, String>) -> bool) -> Result<()> {
        let mut map = self.map.lock().unwrap();
        let mut next = map.clone();
        if f(&mut next) {
            self.save(&next)?;
            *map = next;
        }
        Ok(())
    }
}

#[cfg(unix)]
impl SecretStore for FileStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.change(|m| m.insert(key.to_string(), value.to_string()).as_deref() != Some(value))
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.change(|m| m.remove(key).is_some())
    }
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
    format!("account-{account_id}-password")
}

pub fn refresh_token_key(account_id: i64) -> String {
    format!("account-{account_id}-refresh-token")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn file_store_persists_with_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        let s = FileStore::open(&path).unwrap();
        assert_eq!(s.get("a").unwrap(), None);
        assert!(!path.exists(), "nothing is written until a secret is set");

        s.set("a", "one").unwrap();
        s.set("b", "two").unwrap();
        s.delete("b").unwrap();
        s.delete("missing").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!path.with_extension("tmp").exists());

        let reopened = FileStore::open(&path).unwrap();
        assert_eq!(reopened.get("a").unwrap().as_deref(), Some("one"));
        assert_eq!(reopened.get("b").unwrap(), None);
    }

    #[test]
    fn file_store_narrows_loose_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        std::fs::write(&path, r#"{"k":"v"}"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let s = FileStore::open(&path).unwrap();
        assert_eq!(s.get("k").unwrap().as_deref(), Some("v"));
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
