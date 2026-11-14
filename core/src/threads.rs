//! Conversation threading.
//!
//! Every message carries a `thread_id`: the id of the oldest message known to
//! be in its conversation. Messages are linked through the Message-ID,
//! In-Reply-To and References headers, never by guessing from subjects, so two
//! unrelated "Hello" emails stay apart. A reply that arrives before its parent
//! is joined when the parent shows up, and two threads a new message bridges
//! are merged into one.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

pub const SCHEMA: &str = "
CREATE INDEX IF NOT EXISTS messages_by_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS messages_by_header_id ON messages(account_id, message_id);
CREATE INDEX IF NOT EXISTS messages_by_parent ON messages(account_id, in_reply_to);
";

/// Adds the column to databases made before threading, then threads their messages.
pub fn migrate(db: &Connection) -> Result<()> {
    let has = db
        .prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name = 'thread_id'")?
        .exists([])?;
    if !has {
        db.execute("ALTER TABLE messages ADD COLUMN thread_id INTEGER NOT NULL DEFAULT 0", [])?;
    }
    db.execute_batch(SCHEMA)?;
    backfill(db)
}

fn backfill(db: &Connection) -> Result<()> {
    let todo: Vec<(i64, i64, String, String, String)> = {
        let mut st = db.prepare(
            "SELECT id, account_id, message_id, in_reply_to, refs FROM messages
             WHERE thread_id = 0 ORDER BY date, id",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if todo.is_empty() {
        return Ok(());
    }
    db.execute_batch("BEGIN")?;
    for (id, account, message_id, reply_to, refs) in todo {
        assign(db, id, account, &message_id, &reply_to, &refs)?;
    }
    db.execute_batch("COMMIT")?;
    Ok(())
}

/// Puts message `id` into the conversation its headers point at (or a new one).
pub fn assign(db: &Connection, id: i64, account_id: i64, message_id: &str, in_reply_to: &str, refs: &str) -> Result<()> {
    let mut linked: Vec<i64> = Vec::new();

    // Messages this one answers or follows.
    let mut parent = db.prepare_cached(
        "SELECT thread_id FROM messages WHERE account_id = ?1 AND message_id = ?2 AND thread_id != 0 AND id != ?3",
    )?;
    for pid in refs.split_whitespace().chain(in_reply_to.split_whitespace()) {
        for t in parent.query_map(params![account_id, pid, id], |r| r.get::<_, i64>(0))? {
            linked.push(t?);
        }
    }

    // Replies that arrived before this message did.
    if !message_id.is_empty() {
        let mut kids = db.prepare_cached(
            "SELECT DISTINCT thread_id FROM messages
             WHERE account_id = ?1 AND in_reply_to = ?2 AND thread_id != 0 AND id != ?3",
        )?;
        for t in kids.query_map(params![account_id, message_id, id], |r| r.get::<_, i64>(0))? {
            linked.push(t?);
        }
    }

    let thread = linked.iter().copied().min().unwrap_or(id).min(id);
    db.execute("UPDATE messages SET thread_id = ?1 WHERE id = ?2", params![thread, id])?;
    for t in linked.into_iter().filter(|t| *t != thread) {
        db.execute("UPDATE messages SET thread_id = ?1 WHERE thread_id = ?2", params![thread, t])?;
    }
    Ok(())
}

/// The thread a message is in, if it exists.
pub fn of(db: &Connection, id: i64) -> Result<Option<i64>> {
    Ok(db.query_row("SELECT thread_id FROM messages WHERE id = ?1", [id], |r| r.get(0)).optional()?)
}
