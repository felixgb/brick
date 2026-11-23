//! Local SQLite cache. Every read the UI makes is served from here.

use crate::model::*;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS accounts (
    id            INTEGER PRIMARY KEY,
    email         TEXT NOT NULL,
    name          TEXT NOT NULL,
    provider      TEXT NOT NULL,
    auth          TEXT NOT NULL,
    username      TEXT NOT NULL,
    imap_host     TEXT NOT NULL,
    imap_port     INTEGER NOT NULL,
    imap_security TEXT NOT NULL,
    smtp_host     TEXT NOT NULL,
    smtp_port     INTEGER NOT NULL,
    smtp_security TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS folders (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    path        TEXT NOT NULL,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT '',
    selectable  INTEGER NOT NULL DEFAULT 1,
    uidvalidity INTEGER NOT NULL DEFAULT 0,
    -- Lowest UID already examined while filling in history; 1 means complete.
    backfill_uid INTEGER NOT NULL DEFAULT 0,
    unread      INTEGER NOT NULL DEFAULT 0,
    total       INTEGER NOT NULL DEFAULT 0,
    UNIQUE(account_id, path)
);
CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    folder_id       INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid             INTEGER NOT NULL,
    message_id      TEXT NOT NULL DEFAULT '',
    in_reply_to     TEXT NOT NULL DEFAULT '',
    refs            TEXT NOT NULL DEFAULT '',
    subject         TEXT NOT NULL DEFAULT '',
    from_name       TEXT NOT NULL DEFAULT '',
    from_addr       TEXT NOT NULL DEFAULT '',
    to_text         TEXT NOT NULL DEFAULT '',
    cc_text         TEXT NOT NULL DEFAULT '',
    date            INTEGER NOT NULL DEFAULT 0,
    size            INTEGER NOT NULL DEFAULT 0,
    seen            INTEGER NOT NULL DEFAULT 0,
    flagged         INTEGER NOT NULL DEFAULT 0,
    answered        INTEGER NOT NULL DEFAULT 0,
    has_attachments INTEGER NOT NULL DEFAULT 0,
    snippet         TEXT NOT NULL DEFAULT '',
    UNIQUE(folder_id, uid)
);
CREATE INDEX IF NOT EXISTS messages_by_date ON messages(folder_id, date DESC, id DESC);
CREATE TABLE IF NOT EXISTS bodies (
    message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    raw        BLOB NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject, sender, recipients, body,
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;
"#;

const ROW_COLS: &str = "id, account_id, folder_id, uid, subject, from_name, from_addr, to_text, \
                        date, seen, flagged, answered, has_attachments, snippet";

/// Which messages a list covers.
pub enum Scope {
    Folder(i64),
    /// Every folder with this role, across accounts.
    Role(String),
}

impl Scope {
    /// SQL condition on messages aliased `alias`; the value is parameter `?n`.
    fn sql(&self, alias: &str, n: usize) -> String {
        match self {
            Scope::Folder(_) => format!("{alias}.folder_id = ?{n}"),
            Scope::Role(_) => format!("{alias}.folder_id IN (SELECT id FROM folders WHERE role = ?{n})"),
        }
    }

    fn key(&self) -> rusqlite::types::Value {
        match self {
            Scope::Folder(id) => (*id).into(),
            Scope::Role(role) => role.clone().into(),
        }
    }
}

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

fn row_to_message(r: &Row) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        folder_id: r.get(2)?,
        uid: r.get(3)?,
        subject: r.get(4)?,
        from_name: r.get(5)?,
        from_addr: r.get(6)?,
        to: r.get(7)?,
        date: r.get(8)?,
        seen: r.get(9)?,
        flagged: r.get(10)?,
        answered: r.get(11)?,
        has_attachments: r.get(12)?,
        snippet: r.get(13)?,
    })
}

fn row_to_account(r: &Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        email: r.get(1)?,
        name: r.get(2)?,
        provider: r.get(3)?,
        auth: AuthKind::parse(&r.get::<_, String>(4)?),
        username: r.get(5)?,
        imap_host: r.get(6)?,
        imap_port: r.get(7)?,
        imap_security: Security::parse(&r.get::<_, String>(8)?),
        smtp_host: r.get(9)?,
        smtp_port: r.get(10)?,
        smtp_security: Security::parse(&r.get::<_, String>(11)?),
    })
}

fn row_to_folder(r: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(0)?,
        account_id: r.get(1)?,
        path: r.get(2)?,
        name: r.get(3)?,
        role: r.get(4)?,
        selectable: r.get(5)?,
        unread: r.get(6)?,
        total: r.get(7)?,
    })
}

const FOLDER_COLS: &str = "id, account_id, path, name, role, selectable, unread, total";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(crate::items::SCHEMA)?;
        crate::items::migrate(&conn)?;
        crate::threads::migrate(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `f` with the database connection. Used by modules that keep their
    /// own SQL, such as `items`.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        f(&self.db())
    }

    // ---- legacy settings ------------------------------------------------
    // Settings now live in config.toml (see config.rs). The table is only read
    // so values saved by older versions can be moved there.

    pub fn legacy_settings(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_legacy_settings(&self, keys: &[String]) -> Result<()> {
        let db = self.db();
        for k in keys {
            db.execute("DELETE FROM settings WHERE key = ?1", [k])?;
        }
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.db().execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---- accounts -------------------------------------------------------

    pub fn add_account(&self, a: &Account) -> Result<i64> {
        let db = self.db();
        db.execute(
            "INSERT INTO accounts(email, name, provider, auth, username, imap_host, imap_port,
                                  imap_security, smtp_host, smtp_port, smtp_security)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                a.email,
                a.name,
                a.provider,
                a.auth.as_str(),
                a.username,
                a.imap_host,
                a.imap_port,
                a.imap_security.as_str(),
                a.smtp_host,
                a.smtp_port,
                a.smtp_security.as_str()
            ],
        )?;
        Ok(db.last_insert_rowid())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT id, email, name, provider, auth, username, imap_host, imap_port,
                    imap_security, smtp_host, smtp_port, smtp_security
             FROM accounts ORDER BY id",
        )?;
        let rows = st.query_map([], row_to_account)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn account(&self, id: i64) -> Result<Option<Account>> {
        Ok(self.accounts()?.into_iter().find(|a| a.id == id))
    }

    pub fn remove_account(&self, id: i64) -> Result<()> {
        self.db().execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- folders --------------------------------------------------------

    pub fn folders(&self, account_id: i64) -> Result<Vec<Folder>> {
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT {FOLDER_COLS} FROM folders WHERE account_id = ?1
             ORDER BY CASE role
                 WHEN 'inbox' THEN 0 WHEN 'flagged' THEN 1 WHEN 'drafts' THEN 2
                 WHEN 'sent' THEN 3 WHEN 'archive' THEN 4 WHEN 'all' THEN 5
                 WHEN 'junk' THEN 6 WHEN 'trash' THEN 7 ELSE 8 END,
               path COLLATE NOCASE"
        ))?;
        let rows = st.query_map([account_id], row_to_folder)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn folder(&self, id: i64) -> Result<Option<Folder>> {
        Ok(self
            .db()
            .query_row(&format!("SELECT {FOLDER_COLS} FROM folders WHERE id = ?1"), [id], row_to_folder)
            .optional()?)
    }

    pub fn folder_by_role(&self, account_id: i64, role: &str) -> Result<Option<Folder>> {
        Ok(self
            .db()
            .query_row(
                &format!("SELECT {FOLDER_COLS} FROM folders WHERE account_id = ?1 AND role = ?2 LIMIT 1"),
                params![account_id, role],
                row_to_folder,
            )
            .optional()?)
    }

    /// Inserts or updates a folder and returns its id.
    pub fn upsert_folder(
        &self,
        account_id: i64,
        path: &str,
        name: &str,
        role: &str,
        selectable: bool,
    ) -> Result<i64> {
        let db = self.db();
        db.execute(
            "INSERT INTO folders(account_id, path, name, role, selectable) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, path) DO UPDATE
               SET name = excluded.name, role = excluded.role, selectable = excluded.selectable",
            params![account_id, path, name, role, selectable],
        )?;
        Ok(db.query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND path = ?2",
            params![account_id, path],
            |r| r.get(0),
        )?)
    }

    /// Drops folders that no longer exist on the server.
    pub fn retain_folders(&self, account_id: i64, paths: &[String]) -> Result<()> {
        let existing: Vec<(i64, String)> = {
            let db = self.db();
            let mut st = db.prepare("SELECT id, path FROM folders WHERE account_id = ?1")?;
            let v = st
                .query_map([account_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        let db = self.db();
        for (id, path) in existing {
            if !paths.contains(&path) {
                db.execute("DELETE FROM folders WHERE id = ?1", [id])?;
            }
        }
        Ok(())
    }

    pub fn folder_uidvalidity(&self, folder_id: i64) -> Result<u32> {
        Ok(self.db().query_row("SELECT uidvalidity FROM folders WHERE id = ?1", [folder_id], |r| r.get(0))?)
    }

    /// Records a new UIDVALIDITY. All cached messages for the folder are invalid and removed.
    pub fn reset_folder(&self, folder_id: i64, uidvalidity: u32) -> Result<()> {
        let db = self.db();
        db.execute("DELETE FROM messages WHERE folder_id = ?1", [folder_id])?;
        db.execute(
            "UPDATE folders SET uidvalidity = ?2, backfill_uid = 0 WHERE id = ?1",
            params![folder_id, uidvalidity],
        )?;
        Ok(())
    }

    pub fn backfill_uid(&self, folder_id: i64) -> Result<u32> {
        Ok(self.db().query_row("SELECT backfill_uid FROM folders WHERE id = ?1", [folder_id], |r| r.get(0))?)
    }

    pub fn set_backfill_uid(&self, folder_id: i64, uid: u32) -> Result<()> {
        self.db().execute("UPDATE folders SET backfill_uid = ?2 WHERE id = ?1", params![folder_id, uid])?;
        Ok(())
    }

    pub fn refresh_counts(&self, folder_id: i64) -> Result<()> {
        self.db().execute(
            "UPDATE folders SET
                total  = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1),
                unread = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1 AND seen = 0)
             WHERE id = ?1",
            [folder_id],
        )?;
        Ok(())
    }

    // ---- messages -------------------------------------------------------

    pub fn max_uid(&self, folder_id: i64) -> Result<u32> {
        Ok(self.db().query_row(
            "SELECT COALESCE(MAX(uid), 0) FROM messages WHERE folder_id = ?1",
            [folder_id],
            |r| r.get(0),
        )?)
    }

    pub fn min_uid(&self, folder_id: i64) -> Result<u32> {
        Ok(self.db().query_row(
            "SELECT COALESCE(MIN(uid), 0) FROM messages WHERE folder_id = ?1",
            [folder_id],
            |r| r.get(0),
        )?)
    }

    pub fn insert_messages(&self, account_id: i64, folder_id: i64, msgs: &[NewMessage]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut inserted = 0;
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO messages(account_id, folder_id, uid, message_id, in_reply_to, refs,
                     subject, from_name, from_addr, to_text, cc_text, date, size, seen, flagged, answered,
                     has_attachments)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )?;
            let mut fts = tx.prepare(
                "INSERT INTO messages_fts(rowid, subject, sender, recipients, body) VALUES(?1, ?2, ?3, ?4, '')",
            )?;
            for m in msgs {
                let n = ins.execute(params![
                    account_id,
                    folder_id,
                    m.uid,
                    m.message_id,
                    m.in_reply_to,
                    m.references,
                    m.subject,
                    m.from_name,
                    m.from_addr,
                    m.to,
                    m.cc,
                    m.date,
                    m.size,
                    m.seen,
                    m.flagged,
                    m.answered,
                    m.has_attachments
                ])?;
                if n > 0 {
                    inserted += 1;
                    let id = tx.last_insert_rowid();
                    crate::threads::assign(&tx, id, account_id, &m.message_id, &m.in_reply_to, &m.references)?;
                    fts.execute(params![
                        id,
                        m.subject,
                        format!("{} {}", m.from_name, m.from_addr),
                        format!("{} {}", m.to, m.cc)
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Applies the server's view of flags for a UID range and removes messages
    /// in that range the server no longer has. Returns true if anything changed.
    pub fn reconcile(
        &self,
        folder_id: i64,
        lo: u32,
        hi: u32,
        server: &[(u32, bool, bool, bool)],
    ) -> Result<bool> {
        use std::collections::HashMap;
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut changed = false;
        {
            let local: HashMap<u32, (bool, bool, bool)> = {
                let mut st = tx.prepare(
                    "SELECT uid, seen, flagged, answered FROM messages
                     WHERE folder_id = ?1 AND uid BETWEEN ?2 AND ?3",
                )?;
                let m = st
                    .query_map(params![folder_id, lo, hi], |r| {
                        Ok((r.get::<_, u32>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                m
            };
            let mut present = std::collections::HashSet::new();
            let mut upd = tx.prepare(
                "UPDATE messages SET seen = ?3, flagged = ?4, answered = ?5 WHERE folder_id = ?1 AND uid = ?2",
            )?;
            for &(uid, seen, flagged, answered) in server {
                present.insert(uid);
                if let Some(&cur) = local.get(&uid) {
                    if cur != (seen, flagged, answered) {
                        upd.execute(params![folder_id, uid, seen, flagged, answered])?;
                        changed = true;
                    }
                }
            }
            let mut del = tx.prepare("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
            for uid in local.keys() {
                if !present.contains(uid) {
                    del.execute(params![folder_id, uid])?;
                    changed = true;
                }
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn list_messages(&self, folder_id: i64, offset: i64, limit: i64) -> Result<Vec<MessageRow>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT {ROW_COLS} FROM messages WHERE folder_id = ?1
             ORDER BY date DESC, id DESC LIMIT ?3 OFFSET ?2"
        ))?;
        let rows = st
            .query_map(params![folder_id, offset, limit], row_to_message)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Messages in every folder with this role, across accounts: the unified inbox.
    pub fn list_messages_by_role(&self, role: &str, offset: i64, limit: i64) -> Result<Vec<MessageRow>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT {ROW_COLS} FROM messages
             WHERE folder_id IN (SELECT id FROM folders WHERE role = ?1)
             ORDER BY date DESC, id DESC LIMIT ?3 OFFSET ?2"
        ))?;
        let rows = st.query_map(params![role, offset, limit], row_to_message)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn count_messages_by_role(&self, role: &str) -> Result<i64> {
        Ok(self.db().query_row(
            "SELECT COUNT(*) FROM messages WHERE folder_id IN (SELECT id FROM folders WHERE role = ?1)",
            [role],
            |r| r.get(0),
        )?)
    }

    /// One row per conversation in a folder (or in every folder of a role),
    /// newest conversation first. Each row is the conversation's newest message.
    pub fn list_threads(&self, scope: &Scope, offset: i64, limit: i64) -> Result<Vec<ThreadRow>> {
        let (m, x) = (scope.sql("m", 1), scope.sql("x", 1));
        let cols = ROW_COLS.split(',').map(|c| format!("m.{}", c.trim())).collect::<Vec<_>>().join(", ");
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT {cols}, m.thread_id,
                    (SELECT COUNT(*) FROM messages x WHERE x.thread_id = m.thread_id AND {x}),
                    (SELECT COUNT(*) FROM messages x WHERE x.thread_id = m.thread_id AND x.seen = 0 AND {x})
             FROM messages m
             WHERE {m} AND m.id = (SELECT x.id FROM messages x WHERE x.thread_id = m.thread_id AND {x}
                                   ORDER BY x.date DESC, x.id DESC LIMIT 1)
             ORDER BY m.date DESC, m.id DESC LIMIT ?3 OFFSET ?2"
        ))?;
        let rows = st
            .query_map(params![scope.key(), offset, limit], |r| {
                Ok(ThreadRow {
                    message: row_to_message(r)?,
                    thread_id: r.get(14)?,
                    thread_count: r.get(15)?,
                    thread_unread: r.get(16)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn count_threads(&self, scope: &Scope) -> Result<i64> {
        Ok(self.db().query_row(
            &format!("SELECT COUNT(DISTINCT m.thread_id) FROM messages m WHERE {}", scope.sql("m", 1)),
            [scope.key()],
            |r| r.get(0),
        )?)
    }

    /// Every message in the conversation `id` belongs to, oldest first: in
    /// every folder of its account, or only those a scope covers.
    pub fn thread_of(&self, id: i64, scope: Option<&Scope>) -> Result<Vec<MessageRow>> {
        let db = self.db();
        let limit = scope.map(|s| format!(" AND {}", s.sql("messages", 2))).unwrap_or_default();
        let mut st = db.prepare(&format!(
            "SELECT {ROW_COLS} FROM messages
             WHERE thread_id = (SELECT thread_id FROM messages WHERE id = ?1){limit} ORDER BY date, id"
        ))?;
        let rows = match scope {
            Some(scope) => st.query_map(params![id, scope.key()], row_to_message)?.collect::<rusqlite::Result<Vec<_>>>()?,
            None => st.query_map([id], row_to_message)?.collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(rows)
    }

    pub fn count_messages(&self, folder_id: i64) -> Result<i64> {
        Ok(self.db().query_row("SELECT COUNT(*) FROM messages WHERE folder_id = ?1", [folder_id], |r| r.get(0))?)
    }

    pub fn message(&self, id: i64) -> Result<Option<MessageRow>> {
        Ok(self
            .db()
            .query_row(&format!("SELECT {ROW_COLS} FROM messages WHERE id = ?1"), [id], row_to_message)
            .optional()?)
    }

    /// Message-ID, References and Cc for building a reply.
    pub fn threading(&self, id: i64) -> Result<Option<(String, String, String)>> {
        Ok(self
            .db()
            .query_row("SELECT message_id, refs, cc_text FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?)
    }

    /// Finds a message by its Message-ID header: (id, folder id). A message
    /// can sit in several folders; the inbox copy wins, then the newest.
    pub fn find_by_message_id(&self, account_id: Option<i64>, message_id: &str) -> Result<Option<(i64, i64)>> {
        if message_id.is_empty() {
            return Ok(None);
        }
        Ok(self
            .db()
            .query_row(
                "SELECT m.id, m.folder_id FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.message_id = ?1 AND (?2 IS NULL OR m.account_id = ?2)
                 ORDER BY f.role = 'inbox' DESC, m.id DESC LIMIT 1",
                params![message_id, account_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn set_seen(&self, id: i64, seen: bool) -> Result<()> {
        self.db().execute("UPDATE messages SET seen = ?2 WHERE id = ?1", params![id, seen])?;
        Ok(())
    }

    pub fn set_flagged(&self, id: i64, flagged: bool) -> Result<()> {
        self.db().execute("UPDATE messages SET flagged = ?2 WHERE id = ?1", params![id, flagged])?;
        Ok(())
    }

    pub fn set_answered(&self, id: i64) -> Result<()> {
        self.db().execute("UPDATE messages SET answered = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_message(&self, id: i64) -> Result<()> {
        self.db().execute("DELETE FROM messages WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- bodies ---------------------------------------------------------

    pub fn body(&self, message_id: i64) -> Result<Option<Vec<u8>>> {
        Ok(self
            .db()
            .query_row("SELECT raw FROM bodies WHERE message_id = ?1", [message_id], |r| r.get(0))
            .optional()?)
    }

    pub fn save_body(&self, message_id: i64, raw: &[u8], text: &str, snippet: &str, has_attachments: bool) -> Result<()> {
        let db = self.db();
        db.execute(
            "INSERT OR REPLACE INTO bodies(message_id, raw) VALUES(?1, ?2)",
            params![message_id, raw],
        )?;
        db.execute(
            "UPDATE messages SET snippet = ?2, has_attachments = ?3 WHERE id = ?1",
            params![message_id, snippet, has_attachments],
        )?;
        db.execute("UPDATE messages_fts SET body = ?2 WHERE rowid = ?1", params![message_id, text])?;
        Ok(())
    }

    /// Newest messages in a folder that have no cached body yet.
    pub fn missing_bodies(&self, folder_id: i64, limit: i64) -> Result<Vec<(i64, u32)>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT m.id, m.uid FROM messages m
             WHERE m.folder_id = ?1 AND NOT EXISTS (SELECT 1 FROM bodies b WHERE b.message_id = m.id)
             ORDER BY m.date DESC LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![folder_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---- search ---------------------------------------------------------

    pub fn search(&self, query: &str, account_id: Option<i64>, limit: i64) -> Result<Vec<MessageRow>> {
        let fts = fts_query(query);
        if fts.is_empty() {
            return Ok(vec![]);
        }
        let db = self.db();
        let cols = ROW_COLS
            .split(',')
            .map(|c| format!("m.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut st = db.prepare(&format!(
            "SELECT {cols} FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?1 AND (?2 IS NULL OR m.account_id = ?2)
             ORDER BY m.date DESC LIMIT ?3"
        ))?;
        let rows = st
            .query_map(params![fts, account_id, limit], row_to_message)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

/// Turns free text into a safe FTS5 query: every word is quoted and prefix-matched.
fn fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> Account {
        Account {
            id: 0,
            email: "a@example.com".into(),
            name: "A".into(),
            provider: "custom".into(),
            auth: AuthKind::Password,
            username: "a@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: 993,
            imap_security: Security::Tls,
            smtp_host: "localhost".into(),
            smtp_port: 465,
            smtp_security: Security::Tls,
        }
    }

    fn msg(uid: u32, subject: &str) -> NewMessage {
        NewMessage { uid, subject: subject.into(), from_addr: "x@example.com".into(), date: uid as i64, ..Default::default() }
    }

    #[test]
    fn insert_list_reconcile_search() {
        let s = Store::open_in_memory().unwrap();
        let acc = s.add_account(&account()).unwrap();
        let f = s.upsert_folder(acc, "INBOX", "Inbox", "inbox", true).unwrap();
        assert_eq!(s.upsert_folder(acc, "INBOX", "Inbox", "inbox", true).unwrap(), f);

        let n = s.insert_messages(acc, f, &[msg(1, "Quarterly report"), msg(2, "Lunch?"), msg(3, "Re: Lunch?")]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(s.insert_messages(acc, f, &[msg(3, "dup")]).unwrap(), 0);
        assert_eq!(s.max_uid(f).unwrap(), 3);

        let rows = s.list_messages(f, 0, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.uid).collect::<Vec<_>>(), vec![3, 2, 1]);

        // Server says: uid 1 now read, uid 2 gone, uid 3 unchanged.
        let changed = s.reconcile(f, 1, 3, &[(1, true, false, false), (3, false, false, false)]).unwrap();
        assert!(changed);
        let rows = s.list_messages(f, 0, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().find(|r| r.uid == 1).unwrap().seen);

        s.refresh_counts(f).unwrap();
        let folder = s.folder(f).unwrap().unwrap();
        assert_eq!((folder.total, folder.unread), (2, 1));

        assert_eq!(s.search("quart", None, 10).unwrap().len(), 1);
        assert_eq!(s.search("lunch", Some(acc), 10).unwrap().len(), 1);
        assert!(s.search("\"", None, 10).unwrap().is_empty());

        let id = rows[0].id;
        s.save_body(id, b"raw", "the body mentions zebras", "the body…", false).unwrap();
        assert_eq!(s.search("zebra", None, 10).unwrap().len(), 1);
        assert_eq!(s.body(id).unwrap().unwrap(), b"raw");

        s.remove_account(acc).unwrap();
        assert!(s.search("zebra", None, 10).unwrap().is_empty());
    }

    #[test]
    fn unified_inbox_spans_accounts() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_account(&account()).unwrap();
        let b = s.add_account(&Account { email: "b@example.com".into(), ..account() }).unwrap();
        let (ia, ib) = (
            s.upsert_folder(a, "INBOX", "Inbox", "inbox", true).unwrap(),
            s.upsert_folder(b, "INBOX", "Inbox", "inbox", true).unwrap(),
        );
        let sent = s.upsert_folder(a, "Sent", "Sent", "sent", true).unwrap();
        s.insert_messages(a, ia, &[msg(1, "one"), msg(3, "three")]).unwrap();
        s.insert_messages(b, ib, &[msg(2, "two")]).unwrap();
        s.insert_messages(a, sent, &[msg(9, "sent")]).unwrap();

        assert_eq!(s.count_messages_by_role("inbox").unwrap(), 3);
        let rows = s.list_messages_by_role("inbox", 0, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.uid).collect::<Vec<_>>(), vec![3, 2, 1], "newest first across accounts");
        assert_eq!(s.list_messages_by_role("inbox", 1, 1).unwrap()[0].uid, 2, "paging works");

        s.set_setting("claude_model", "m").unwrap();
        assert_eq!(s.legacy_settings().unwrap(), [("claude_model".to_string(), "m".to_string())]);
        s.delete_legacy_settings(&["claude_model".into()]).unwrap();
        assert!(s.legacy_settings().unwrap().is_empty());
    }

    fn threaded(uid: u32, id: &str, reply_to: &str, refs: &str) -> NewMessage {
        NewMessage {
            message_id: id.into(),
            in_reply_to: reply_to.into(),
            references: refs.into(),
            ..msg(uid, &format!("m{uid}"))
        }
    }

    #[test]
    fn replies_join_their_conversation_whatever_order_they_arrive_in() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_account(&account()).unwrap();
        let inbox = s.upsert_folder(a, "INBOX", "Inbox", "inbox", true).unwrap();
        let sent = s.upsert_folder(a, "Sent", "Sent", "sent", true).unwrap();

        // The reply is synced before the message it answers; a stranger sits between.
        s.insert_messages(a, inbox, &[threaded(2, "b@x", "a@x", "a@x")]).unwrap();
        s.insert_messages(a, inbox, &[threaded(3, "other@x", "", "")]).unwrap();
        s.insert_messages(a, inbox, &[threaded(1, "a@x", "", "")]).unwrap();
        // My own answer is in another folder; it still belongs to the conversation.
        s.insert_messages(a, sent, &[threaded(5, "c@x", "b@x", "a@x b@x")]).unwrap();

        let rows = s.list_threads(&Scope::Folder(inbox), 0, 10).unwrap();
        assert_eq!(s.count_threads(&Scope::Folder(inbox)).unwrap(), 2);
        assert_eq!(rows.len(), 2);
        let convo = rows.iter().find(|r| r.thread_count == 2).expect("two messages in this folder");
        assert_eq!(convo.message.uid, 2, "the newest message in the folder represents it");
        assert_eq!(convo.thread_unread, 2);
        assert_eq!(rows.iter().find(|r| r.thread_count == 1).unwrap().message.uid, 3);

        let all = s.thread_of(convo.message.id, None).unwrap();
        assert_eq!(all.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![1, 2, 5], "oldest first, across folders");
        assert_eq!(s.thread_of(all[2].id, None).unwrap().len(), 3, "same from any member");
        let here = s.thread_of(convo.message.id, Some(&Scope::Folder(inbox))).unwrap();
        assert_eq!(here.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![1, 2], "only the folder asked about");
        assert_eq!(s.thread_of(convo.message.id, Some(&Scope::Role("sent".into()))).unwrap().len(), 1);
    }

    #[test]
    fn a_message_that_bridges_two_conversations_merges_them() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_account(&account()).unwrap();
        let f = s.upsert_folder(a, "INBOX", "Inbox", "inbox", true).unwrap();
        s.insert_messages(a, f, &[threaded(1, "a@x", "", ""), threaded(2, "z@x", "", "")]).unwrap();
        assert_eq!(s.count_threads(&Scope::Folder(f)).unwrap(), 2);
        s.insert_messages(a, f, &[threaded(3, "m@x", "z@x", "a@x z@x")]).unwrap();
        assert_eq!(s.count_threads(&Scope::Folder(f)).unwrap(), 1);
        assert_eq!(s.list_threads(&Scope::Folder(f), 0, 10).unwrap()[0].thread_count, 3);
    }

    #[test]
    fn conversations_never_cross_accounts_or_guess_from_subjects() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_account(&account()).unwrap();
        let b = s.add_account(&Account { email: "b@example.com".into(), ..account() }).unwrap();
        let (fa, fb) = (
            s.upsert_folder(a, "INBOX", "Inbox", "inbox", true).unwrap(),
            s.upsert_folder(b, "INBOX", "Inbox", "inbox", true).unwrap(),
        );
        s.insert_messages(a, fa, &[threaded(1, "a@x", "", ""), msg(2, "m1")]).unwrap();
        s.insert_messages(b, fb, &[threaded(1, "r@x", "a@x", "a@x")]).unwrap();
        assert_eq!(s.count_threads(&Scope::Role("inbox".into())).unwrap(), 3);
    }

    #[test]
    fn existing_databases_are_threaded_on_open() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts(id, email, name, provider, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, auth, username)
               VALUES (1, 'a@example.com', '', 'imap', 'h', 993, 'tls', 'h', 465, 'tls', 'password', 'a');
             INSERT INTO folders(id, account_id, path, name, role, selectable) VALUES (1, 1, 'INBOX', 'Inbox', 'inbox', 1);
             INSERT INTO messages(account_id, folder_id, uid, message_id, in_reply_to, refs, date) VALUES
               (1, 1, 1, 'a@x', '', '', 1), (1, 1, 2, 'b@x', 'a@x', 'a@x', 2);",
        )
        .unwrap();
        let s = Store::init(conn).unwrap();
        assert_eq!(s.count_threads(&Scope::Folder(1)).unwrap(), 1);
    }
}
