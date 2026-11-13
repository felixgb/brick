//! To-do and calendar items. They are one thing: an item with no date shows
//! only in the to-do list, an item with a day also shows on the calendar, and
//! an item with times shows as a timed block. All dates and times are local
//! wall-clock values; nothing here is synced anywhere.

use anyhow::{bail, Result};
use chrono::{NaiveDate, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS items (
    id                INTEGER PRIMARY KEY,
    title             TEXT NOT NULL,
    notes             TEXT NOT NULL DEFAULT '',
    location          TEXT NOT NULL DEFAULT '',
    date              TEXT,            -- YYYY-MM-DD
    end_date          TEXT,            -- YYYY-MM-DD, for items spanning days
    start_time        TEXT,            -- HH:MM
    end_time          TEXT,            -- HH:MM
    done              INTEGER NOT NULL DEFAULT 0,
    done_at           INTEGER,
    -- Where the item came from, if it was made from an email. The Message-ID
    -- header is kept because local row ids change when a message is moved.
    source_account_id INTEGER,
    source_message_id TEXT NOT NULL DEFAULT '',
    source_subject    TEXT NOT NULL DEFAULT '',
    source_from       TEXT NOT NULL DEFAULT '',
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS items_by_date ON items(date);
CREATE INDEX IF NOT EXISTS items_by_source ON items(source_message_id);
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    #[serde(default)]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub source_account_id: Option<i64>,
    #[serde(default)]
    pub source_message_id: String,
    #[serde(default)]
    pub source_subject: String,
    #[serde(default)]
    pub source_from: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

fn clean_date(v: Option<String>) -> Option<String> {
    let v = v?;
    NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok().map(|d| d.format("%Y-%m-%d").to_string())
}

fn clean_time(v: Option<String>) -> Option<String> {
    let v = v?;
    let v = v.trim();
    NaiveTime::parse_from_str(v, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(v, "%H:%M:%S"))
        .ok()
        .map(|t| t.format("%H:%M").to_string())
}

impl Item {
    /// Drops malformed or contradictory date fields so stored items are always
    /// consistent: times need a day, an end needs a start, and ends come last.
    pub fn normalise(mut self) -> Self {
        self.title = self.title.trim().to_string();
        self.date = clean_date(self.date);
        self.end_date = clean_date(self.end_date);
        self.start_time = clean_time(self.start_time);
        self.end_time = clean_time(self.end_time);

        if self.date.is_none() {
            self.end_date = None;
            self.start_time = None;
            self.end_time = None;
        }
        if self.end_date.as_ref().is_some_and(|e| Some(e) <= self.date.as_ref()) {
            self.end_date = None;
        }
        if self.start_time.is_none() {
            self.end_time = None;
        }
        // On a single day the end must follow the start; across days anything goes.
        if self.end_date.is_none() && self.end_time.as_ref().is_some_and(|e| Some(e) <= self.start_time.as_ref()) {
            self.end_time = None;
        }
        self
    }
}

const COLS: &str = "id, title, notes, location, date, end_date, start_time, end_time, done, \
                    source_account_id, source_message_id, source_subject, source_from, created_at, updated_at";

fn from_row(r: &Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        location: r.get(3)?,
        date: r.get(4)?,
        end_date: r.get(5)?,
        start_time: r.get(6)?,
        end_time: r.get(7)?,
        done: r.get(8)?,
        source_account_id: r.get(9)?,
        source_message_id: r.get(10)?,
        source_subject: r.get(11)?,
        source_from: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// All items: dated ones first in date order, then undated, newest first.
pub fn list(db: &Connection) -> Result<Vec<Item>> {
    let mut st = db.prepare(&format!(
        "SELECT {COLS} FROM items
         ORDER BY date IS NULL, date, start_time IS NULL, start_time, created_at DESC, id DESC"
    ))?;
    let rows = st.query_map([], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get(db: &Connection, id: i64) -> Result<Option<Item>> {
    Ok(db.query_row(&format!("SELECT {COLS} FROM items WHERE id = ?1"), [id], from_row).optional()?)
}

/// Items made from the email with this Message-ID header.
pub fn for_source(db: &Connection, message_id: &str) -> Result<Vec<Item>> {
    if message_id.is_empty() {
        return Ok(vec![]);
    }
    let mut st = db.prepare(&format!("SELECT {COLS} FROM items WHERE source_message_id = ?1 ORDER BY id"))?;
    let rows = st.query_map([message_id], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Inserts (id 0) or updates an item and returns it as stored.
pub fn save(db: &Connection, item: Item) -> Result<Item> {
    let item = item.normalise();
    if item.title.is_empty() {
        bail!("give the item a title");
    }
    let ts = now();
    let id = if item.id == 0 {
        db.execute(
            "INSERT INTO items(title, notes, location, date, end_date, start_time, end_time, done, done_at,
                               source_account_id, source_message_id, source_subject, source_from,
                               created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
            params![
                item.title,
                item.notes,
                item.location,
                item.date,
                item.end_date,
                item.start_time,
                item.end_time,
                item.done,
                item.done.then_some(ts),
                item.source_account_id,
                item.source_message_id,
                item.source_subject,
                item.source_from,
                ts
            ],
        )?;
        db.last_insert_rowid()
    } else {
        // The link back to the source email is set once, at creation.
        let n = db.execute(
            "UPDATE items SET title = ?2, notes = ?3, location = ?4, date = ?5, end_date = ?6,
                              start_time = ?7, end_time = ?8,
                              done_at = CASE WHEN ?9 AND NOT done THEN ?10 WHEN ?9 THEN done_at ELSE NULL END,
                              done = ?9, updated_at = ?10
             WHERE id = ?1",
            params![
                item.id,
                item.title,
                item.notes,
                item.location,
                item.date,
                item.end_date,
                item.start_time,
                item.end_time,
                item.done,
                ts
            ],
        )?;
        if n == 0 {
            bail!("that item no longer exists");
        }
        item.id
    };
    get(db, id)?.ok_or_else(|| anyhow::anyhow!("item could not be read back"))
}

pub fn set_done(db: &Connection, id: i64, done: bool) -> Result<()> {
    let ts = now();
    db.execute(
        "UPDATE items SET done = ?2, done_at = CASE WHEN ?2 THEN ?3 ELSE NULL END, updated_at = ?3 WHERE id = ?1",
        params![id, done, ts],
    )?;
    Ok(())
}

pub fn delete(db: &Connection, id: i64) -> Result<()> {
    db.execute("DELETE FROM items WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        c
    }

    fn item(title: &str) -> Item {
        Item { title: title.into(), ..Default::default() }
    }

    #[test]
    fn normalise_enforces_consistency() {
        let i = Item { start_time: Some("09:00".into()), end_time: Some("10:00".into()), ..item("x") }.normalise();
        assert_eq!((i.start_time, i.end_time), (None, None), "times need a day");

        let i = Item {
            date: Some("2026-11-20".into()),
            start_time: Some("9:5".into()),
            end_time: Some("08:00".into()),
            end_date: Some("2026-11-20".into()),
            ..item("x")
        }
        .normalise();
        assert_eq!(i.start_time.as_deref(), Some("09:05"));
        assert_eq!(i.end_time, None, "end before start is dropped");
        assert_eq!(i.end_date, None, "end date equal to start is dropped");

        let i = Item { date: Some("next tuesday".into()), ..item("x") }.normalise();
        assert_eq!(i.date, None);

        let i = Item {
            date: Some("2026-11-20".into()),
            end_date: Some("2026-11-21".into()),
            start_time: Some("22:00".into()),
            end_time: Some("02:00".into()),
            ..item("x")
        }
        .normalise();
        assert_eq!(i.end_time.as_deref(), Some("02:00"), "overnight items keep their end");
    }

    #[test]
    fn save_list_done_delete() {
        let c = db();
        assert!(save(&c, item("  ")).is_err());

        let undated = save(&c, item("Buy stamps")).unwrap();
        let later = save(&c, Item { date: Some("2026-12-01".into()), ..item("Dentist") }).unwrap();
        let sooner = save(
            &c,
            Item {
                date: Some("2026-11-20".into()),
                start_time: Some("14:00".into()),
                source_message_id: "abc@example.com".into(),
                source_subject: "Meeting".into(),
                ..item("Planning meeting")
            },
        )
        .unwrap();

        let titles: Vec<_> = list(&c).unwrap().into_iter().map(|i| i.title).collect();
        assert_eq!(titles, ["Planning meeting", "Dentist", "Buy stamps"]);
        assert_eq!(for_source(&c, "abc@example.com").unwrap().len(), 1);
        assert!(for_source(&c, "").unwrap().is_empty());

        // Editing keeps the source link even though the edit doesn't carry it.
        let edited = save(&c, Item { id: sooner.id, title: "Planning".into(), date: sooner.date.clone(), ..Default::default() }).unwrap();
        assert_eq!(edited.source_subject, "Meeting");
        assert_eq!(edited.start_time, None);

        set_done(&c, undated.id, true).unwrap();
        assert!(get(&c, undated.id).unwrap().unwrap().done);
        set_done(&c, undated.id, false).unwrap();
        assert!(!get(&c, undated.id).unwrap().unwrap().done);

        delete(&c, later.id).unwrap();
        assert_eq!(list(&c).unwrap().len(), 2);
        assert!(save(&c, Item { id: later.id, ..item("gone") }).is_err());
    }
}
