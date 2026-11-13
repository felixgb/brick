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
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- settings -------------------------------------------------------

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
}
