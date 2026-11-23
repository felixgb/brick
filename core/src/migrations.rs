//! Versioned schema migrations for the SQLite store.
//!
//! The database's `PRAGMA user_version` is the number of migrations applied.
//! On open, every migration past that number runs in order, each in its own
//! transaction together with the version bump, so a failure leaves the
//! database exactly as it was at the last completed step.
//!
//! To change the schema, append a migration to [`MIGRATIONS`]; never edit one
//! that has shipped, and don't change the baseline `SCHEMA` strings in
//! `store.rs`, `items.rs` or `threads.rs` (they are migration 1). A new table
//! or column goes in a new step, written so it applies to a database at the
//! previous version.
//!
//! Foreign keys are off while migrations run, so a step can rebuild a table
//! the way SQLite recommends (create the new one, copy, drop, rename); they
//! are checked before each step commits.
//!
//! An existing database is copied to `<name>.v<old version>.bak` beside it
//! before it is upgraded: to-dos and calendar items exist only in this file.

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub struct Migration {
    pub description: &'static str,
    pub up: fn(&Connection) -> Result<()>,
}

/// Every schema change, oldest first. Index + 1 is the version it produces.
pub const MIGRATIONS: &[Migration] = &[Migration {
    description: "baseline: mail, items and threading as of the first versioned release",
    up: baseline,
}];

pub fn current_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// Brings the database at `conn` up to the newest version. `path` is the
/// database file, used for the backup; None for an in-memory database.
pub fn run(conn: &mut Connection, path: Option<&Path>) -> Result<()> {
    apply(conn, path, MIGRATIONS)
}

fn version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn has_tables(conn: &Connection) -> Result<bool> {
    Ok(conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table'")?.exists([])?)
}

fn apply(conn: &mut Connection, path: Option<&Path>, migrations: &[Migration]) -> Result<()> {
    let from = version(conn)?;
    let to = migrations.len() as i64;
    if from > to {
        bail!(
            "this mail store was written by a newer version of Brick (schema version {from}; this version knows up \
             to {to}). Update Brick, or move the database aside to start over."
        );
    }
    if from == to {
        return Ok(());
    }

    if let Some(path) = path.filter(|_| has_tables(conn).unwrap_or(false)) {
        backup(conn, path, from)?;
    }

    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = (|| -> Result<()> {
        for (i, m) in migrations.iter().enumerate().skip(from as usize) {
            let next = i as i64 + 1;
            let tx = conn.transaction()?;
            (m.up)(&tx).with_context(|| format!("upgrading the mail store to version {next} ({})", m.description))?;
            let broken = tx.prepare("PRAGMA foreign_key_check")?.exists([])?;
            if broken {
                bail!("upgrading the mail store to version {next} ({}) broke a foreign key", m.description);
            }
            tx.pragma_update(None, "user_version", next)?;
            tx.commit()?;
            log::info!("mail store upgraded to version {next}: {}", m.description);
        }
        Ok(())
    })();
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
}

/// Copies the database to `<file>.v<version>.bak`, replacing an older copy of
/// the same version.
fn backup(conn: &Connection, path: &Path, version: i64) -> Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".v{version}.bak"));
    let dest = path.with_file_name(name);
    let _ = std::fs::remove_file(&dest);
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])
        .with_context(|| format!("backing up the mail store to {} before upgrading it", dest.display()))?;
    Ok(())
}

fn has_column(db: &Connection, table: &str, column: &str) -> Result<bool> {
    Ok(db.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?.exists([table, column])?)
}

/// Version 1. Databases from before versioning are at 0 whatever they hold, so
/// this creates what is missing and adds the columns that were added by hand
/// before (items' repeat columns, messages.thread_id).
fn baseline(db: &Connection) -> Result<()> {
    db.execute_batch(crate::store::SCHEMA)?;
    db.execute_batch(crate::items::SCHEMA)?;
    for (column, ddl) in [
        ("repeat_rule", "ALTER TABLE items ADD COLUMN repeat_rule TEXT NOT NULL DEFAULT ''"),
        ("repeat_until", "ALTER TABLE items ADD COLUMN repeat_until TEXT"),
    ] {
        if !has_column(db, "items", column)? {
            db.execute(ddl, [])?;
        }
    }
    if !has_column(db, "messages", "thread_id")? {
        db.execute("ALTER TABLE messages ADD COLUMN thread_id INTEGER NOT NULL DEFAULT 0", [])?;
    }
    db.execute_batch(crate::threads::SCHEMA)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table's columns, as `table.column type notnull default`.
    fn shape(c: &Connection) -> Vec<String> {
        let mut st = c
            .prepare(
                "SELECT m.name, p.name, p.type, p.\"notnull\", ifnull(p.dflt_value, '') FROM sqlite_master m
                 JOIN pragma_table_info(m.name) p WHERE m.type = 'table' AND m.name NOT LIKE 'messages_fts%'
                 ORDER BY m.name, p.name",
            )
            .unwrap();
        let rows = st
            .query_map([], |r| {
                Ok(format!(
                    "{}.{} {} {} {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let mut indexes: Vec<String> = c
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('index', 'trigger') AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let mut out = rows;
        out.append(&mut indexes);
        out
    }

    fn fresh() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        run(&mut c, None).unwrap();
        c
    }

    /// A database as the last unversioned release left it, minus the columns
    /// earlier releases added on open.
    fn legacy() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::store::SCHEMA).unwrap();
        c.execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, title TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
             location TEXT NOT NULL DEFAULT '', date TEXT, end_date TEXT, start_time TEXT, end_time TEXT,
             done INTEGER NOT NULL DEFAULT 0, done_at INTEGER, source_account_id INTEGER,
             source_message_id TEXT NOT NULL DEFAULT '', source_subject TEXT NOT NULL DEFAULT '',
             source_from TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO items(title, created_at, updated_at) VALUES('old', 1, 1);",
        )
        .unwrap();
        c
    }

    #[test]
    fn new_database_is_at_the_current_version() {
        let c = fresh();
        assert_eq!(version(&c).unwrap(), current_version());
        assert!(has_column(&c, "messages", "thread_id").unwrap());
    }

    #[test]
    fn unversioned_database_ends_up_like_a_new_one() {
        let mut c = legacy();
        run(&mut c, None).unwrap();
        assert_eq!(version(&c).unwrap(), current_version());
        assert_eq!(shape(&c), shape(&fresh()));
        let title: String = c.query_row("SELECT title FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "old", "data survives");
        run(&mut c, None).unwrap(); // nothing left to do
    }

    #[test]
    fn newer_database_is_refused() {
        let mut c = fresh();
        c.pragma_update(None, "user_version", current_version() + 1).unwrap();
        let err = run(&mut c, None).unwrap_err().to_string();
        assert!(err.contains("newer version of Brick"), "{err}");
    }

    #[test]
    fn failed_step_rolls_back_and_keeps_the_version() {
        let mut c = fresh();
        let steps = [
            Migration { description: "baseline", up: baseline },
            Migration {
                description: "add a column",
                up: |db| Ok(db.execute_batch("ALTER TABLE accounts ADD COLUMN colour TEXT NOT NULL DEFAULT ''")?),
            },
            Migration { description: "fails halfway", up: |db| {
                db.execute_batch("ALTER TABLE accounts ADD COLUMN half TEXT")?;
                bail!("boom")
            } },
        ];
        let err = apply(&mut c, None, &steps).unwrap_err();
        assert!(format!("{err:#}").contains("version 3 (fails halfway): boom"), "{err:#}");
        assert_eq!(version(&c).unwrap(), 2, "steps before the failure stay applied");
        assert!(has_column(&c, "accounts", "colour").unwrap());
        assert!(!has_column(&c, "accounts", "half").unwrap(), "the failed step was rolled back");
        let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1, "foreign keys are back on");
    }

    #[test]
    fn table_rebuild_keeps_rows_and_foreign_keys() {
        // The pattern a future step uses to change a column's type or constraint.
        let mut c = fresh();
        c.execute_batch(
            "INSERT INTO accounts(id, email, name, provider, auth, username, imap_host, imap_port, imap_security,
                                  smtp_host, smtp_port, smtp_security)
             VALUES (1, 'a@x', '', 'custom', 'password', 'a', 'h', 993, 'tls', 'h', 465, 'tls');
             INSERT INTO folders(id, account_id, path, name) VALUES (1, 1, 'INBOX', 'Inbox');",
        )
        .unwrap();
        let steps = [
            Migration { description: "baseline", up: baseline },
            Migration {
                description: "rebuild accounts",
                up: |db| {
                    db.execute_batch(
                        "CREATE TABLE accounts_new (
                             id INTEGER PRIMARY KEY, email TEXT NOT NULL, name TEXT NOT NULL,
                             provider TEXT NOT NULL, auth TEXT NOT NULL, username TEXT NOT NULL,
                             imap_host TEXT NOT NULL, imap_port INTEGER NOT NULL, imap_security TEXT NOT NULL,
                             smtp_host TEXT NOT NULL, smtp_port INTEGER NOT NULL, smtp_security TEXT NOT NULL);
                         INSERT INTO accounts_new SELECT * FROM accounts;
                         DROP TABLE accounts;
                         ALTER TABLE accounts_new RENAME TO accounts;",
                    )?;
                    Ok(())
                },
            },
        ];
        apply(&mut c, None, &steps).unwrap();
        let folders: i64 = c.query_row("SELECT count(*) FROM folders", [], |r| r.get(0)).unwrap();
        assert_eq!(folders, 1, "dropping the parent table with foreign keys off didn't cascade");
    }

    #[test]
    fn existing_file_is_backed_up_before_upgrading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brick.sqlite");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(crate::store::SCHEMA).unwrap();
            c.execute_batch("INSERT INTO settings VALUES ('k', 'v')").unwrap();
        }
        let mut c = Connection::open(&path).unwrap();
        run(&mut c, Some(&path)).unwrap();
        let bak = dir.path().join("brick.sqlite.v0.bak");
        let old = Connection::open(&bak).unwrap();
        assert_eq!(version(&old).unwrap(), 0);
        let v: String = old.query_row("SELECT value FROM settings", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "v");

        // A brand-new file has nothing worth keeping.
        let new = dir.path().join("new.sqlite");
        run(&mut Connection::open(&new).unwrap(), Some(&new)).unwrap();
        assert!(!dir.path().join("new.sqlite.v0.bak").exists());
    }
}
