//! iCalendar (.ics) import and export for items.
//!
//! Dated items are written as VEVENTs and undated ones as VTODOs. On the way
//! in, VEVENTs and VTODOs are both read. What is not understood is dropped
//! rather than guessed at: time zones are ignored (a time with a TZID is taken
//! as local wall-clock time, a UTC time is converted to local time), and
//! recurrence rules other than a plain daily/weekly/monthly/yearly repeat
//! (BYDAY, INTERVAL above 1, COUNT) are not imported as repeats.

use crate::items::Item;
use chrono::{Days, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

// ---- export ------------------------------------------------------------------

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Folds a content line to 75 octets, as RFC 5545 asks.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for ch in line.chars() {
        let n = ch.len_utf8();
        if width + n > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += n;
    }
    out
}

fn compact_date(d: &str) -> String {
    d.replace('-', "")
}

fn compact_time(d: &str, t: &str) -> String {
    format!("{}T{}00", compact_date(d), t.replace(':', ""))
}

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn export(items: &[Item]) -> String {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut lines: Vec<String> = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Brick//Brick Mail//EN".into(),
        "CALSCALE:GREGORIAN".into(),
    ];
    for item in items {
        let dated = item.date.is_some();
        lines.push(if dated { "BEGIN:VEVENT" } else { "BEGIN:VTODO" }.into());
        lines.push(format!("UID:brick-{}@brick.local", item.id));
        lines.push(format!("DTSTAMP:{stamp}"));
        lines.push(format!("SUMMARY:{}", escape(&item.title)));
        if !item.notes.is_empty() {
            lines.push(format!("DESCRIPTION:{}", escape(&item.notes)));
        }
        if !item.location.is_empty() {
            lines.push(format!("LOCATION:{}", escape(&item.location)));
        }
        if let Some(date) = &item.date {
            let last = item.end_date.as_deref().unwrap_or(date);
            match &item.start_time {
                Some(start) => {
                    lines.push(format!("DTSTART:{}", compact_time(date, start)));
                    if let Some(end) = &item.end_time {
                        lines.push(format!("DTEND:{}", compact_time(last, end)));
                    }
                }
                None => {
                    lines.push(format!("DTSTART;VALUE=DATE:{}", compact_date(date)));
                    // An all-day end is exclusive.
                    let after = parse_day(last).and_then(|d| d.checked_add_days(Days::new(1)));
                    if let Some(after) = after {
                        lines.push(format!("DTEND;VALUE=DATE:{}", after.format("%Y%m%d")));
                    }
                }
            }
            if !item.repeat_rule.is_empty() {
                let mut rule = format!("RRULE:FREQ={}", item.repeat_rule.to_ascii_uppercase());
                if let Some(until) = &item.repeat_until {
                    rule.push_str(&format!(";UNTIL={}", compact_date(until)));
                }
                lines.push(rule);
            }
        }
        if item.done {
            if dated {
                lines.push("X-BRICK-DONE:TRUE".into());
            } else {
                lines.push("STATUS:COMPLETED".into());
            }
        }
        lines.push(if dated { "END:VEVENT" } else { "END:VTODO" }.into());
    }
    lines.push("END:VCALENDAR".into());
    let mut out = lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("\r\n");
    out.push_str("\r\n");
    out
}

// ---- import ------------------------------------------------------------------

struct Line {
    name: String,
    params: String,
    value: String,
}

fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines() {
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            out.push(raw.to_string());
        }
    }
    out
}

fn parse_line(l: &str) -> Option<Line> {
    // The value starts at the first colon outside a quoted parameter.
    let mut quoted = false;
    let mut split = None;
    for (i, c) in l.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let i = split?;
    let (head, value) = (&l[..i], &l[i + 1..]);
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    Some(Line { name: name.to_ascii_uppercase(), params: params.to_ascii_uppercase(), value: value.to_string() })
}

/// A parsed DTSTART/DTEND/DUE: the day, and the time if it has one.
struct When {
    date: NaiveDate,
    time: Option<NaiveTime>,
}

fn parse_when(line: &Line) -> Option<When> {
    let v = line.value.trim();
    let date_only = line.params.contains("VALUE=DATE") && !line.params.contains("DATE-TIME") || v.len() == 8;
    if date_only {
        return Some(When { date: NaiveDate::parse_from_str(v.get(..8)?, "%Y%m%d").ok()?, time: None });
    }
    let utc = v.ends_with('Z');
    let naive = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let naive = if utc { Utc.from_utc_datetime(&naive).with_timezone(&Local).naive_local() } else { naive };
    Some(When { date: naive.date(), time: Some(naive.time()) })
}

fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn hhmm(t: NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

#[derive(Default)]
struct Builder {
    todo: bool,
    summary: String,
    notes: String,
    location: String,
    start: Option<When>,
    end: Option<When>,
    due: Option<When>,
    completed: bool,
    rrule: String,
}

impl Builder {
    /// Returns the item, and whether a repeat rule had to be left out.
    fn finish(self) -> Option<(Item, bool)> {
        let mut item = Item {
            title: if self.summary.trim().is_empty() { "(untitled)".into() } else { self.summary },
            notes: self.notes,
            location: self.location,
            done: self.completed,
            ..Default::default()
        };

        let primary = if self.todo { self.due.or(self.start) } else { self.start };
        let Some(start) = primary else { return Some((item, false)) };
        item.date = Some(day(start.date));
        item.start_time = start.time.map(hhmm);

        if !self.todo {
            if let Some(end) = self.end {
                match (start.time, end.time) {
                    // All-day: DTEND is the day after the last one.
                    (None, None) => {
                        let last = end.date.checked_sub_days(Days::new(1))?;
                        if last > start.date {
                            item.end_date = Some(day(last));
                        }
                    }
                    (Some(_), Some(t)) => {
                        if end.date > start.date {
                            item.end_date = Some(day(end.date));
                        }
                        item.end_time = Some(hhmm(t));
                    }
                    _ => {}
                }
            }
        }

        let mut dropped = false;
        if !self.rrule.is_empty() {
            let mut freq = String::new();
            let mut until = None;
            let mut plain = true;
            for part in self.rrule.split(';') {
                match part.split_once('=') {
                    Some(("FREQ", v)) => freq = v.to_ascii_lowercase(),
                    Some(("UNTIL", v)) => until = v.get(..8).and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok()),
                    Some(("INTERVAL", "1")) => {}
                    Some(("WKST", _)) => {}
                    _ => plain = false,
                }
            }
            if plain && crate::items::REPEATS.contains(&freq.as_str()) {
                item.repeat_rule = freq;
                item.repeat_until = until.map(day);
            } else {
                dropped = true;
            }
        }
        Some((item, dropped))
    }
}

/// Reads every VEVENT and VTODO in the text. Returns the items, and how many
/// repeat rules were too complex to keep.
pub fn import(text: &str) -> (Vec<Item>, usize) {
    let mut items = Vec::new();
    let mut dropped = 0;
    let mut current: Option<Builder> = None;
    let mut alarm_depth = 0;

    for raw in unfold(text) {
        let Some(line) = parse_line(raw.trim_end()) else { continue };
        match (line.name.as_str(), line.value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => current = Some(Builder::default()),
            ("BEGIN", "VTODO") => current = Some(Builder { todo: true, ..Default::default() }),
            ("BEGIN", "VALARM") => alarm_depth += 1,
            ("END", "VALARM") => alarm_depth = (alarm_depth - 1).max(0),
            ("END", "VEVENT" | "VTODO") => {
                if let Some((item, rule_dropped)) = current.take().and_then(Builder::finish) {
                    items.push(item);
                    dropped += usize::from(rule_dropped);
                }
            }
            _ if alarm_depth > 0 => {}
            _ => {
                let Some(b) = current.as_mut() else { continue };
                match line.name.as_str() {
                    "SUMMARY" => b.summary = unescape(&line.value),
                    "DESCRIPTION" => b.notes = unescape(&line.value),
                    "LOCATION" => b.location = unescape(&line.value),
                    "DTSTART" => b.start = parse_when(&line),
                    "DTEND" => b.end = parse_when(&line),
                    "DUE" => b.due = parse_when(&line),
                    "COMPLETED" => b.completed = true,
                    "STATUS" if line.value.eq_ignore_ascii_case("COMPLETED") => b.completed = true,
                    "PERCENT-COMPLETE" if line.value.trim() == "100" => b.completed = true,
                    "X-BRICK-DONE" if line.value.eq_ignore_ascii_case("TRUE") => b.completed = true,
                    "RRULE" => b.rrule = line.value.to_ascii_uppercase(),
                    _ => {}
                }
            }
        }
    }
    (items, dropped)
}

/// Whether the stored items already contain this one, so importing the same
/// file twice doesn't double everything.
pub fn same(a: &Item, b: &Item) -> bool {
    a.title == b.title && a.date == b.date && a.start_time == b.start_time && a.end_date == b.end_date
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> Item {
        Item { title: title.into(), ..Default::default() }
    }

    #[test]
    fn export_then_import_round_trips() {
        let originals = vec![
            Item {
                id: 1,
                notes: "Bring the, er; things\nand a coat".into(),
                location: "Room 4, Quay St".into(),
                date: Some("2026-11-20".into()),
                start_time: Some("14:00".into()),
                end_time: Some("15:30".into()),
                repeat_rule: "weekly".into(),
                repeat_until: Some("2027-01-29".into()),
                ..item("Planning meeting")
            },
            Item {
                id: 2,
                date: Some("2026-11-26".into()),
                end_date: Some("2026-11-28".into()),
                done: true,
                ..item("Conference")
            },
            Item { id: 3, done: true, ..item("Renew passport") },
            Item { id: 4, date: Some("2026-12-01".into()), ..item("Venue deposit due") },
        ];
        let text = export(&originals);
        assert!(text.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(text.contains("DTEND;VALUE=DATE:20261129"), "all-day end is exclusive");
        assert!(text.contains("BEGIN:VTODO"));
        assert!(text.lines().all(|l| l.len() <= 75), "lines are folded");

        let (back, dropped) = import(&text);
        assert_eq!(dropped, 0);
        assert_eq!(back.len(), 4);
        for (a, b) in originals.iter().zip(&back) {
            assert_eq!((&a.title, &a.notes, &a.location), (&b.title, &b.notes, &b.location));
            assert_eq!((&a.date, &a.end_date, &a.start_time, &a.end_time), (&b.date, &b.end_date, &b.start_time, &b.end_time));
            assert_eq!((&a.repeat_rule, &a.repeat_until, a.done), (&b.repeat_rule, &b.repeat_until, b.done));
        }
    }

    #[test]
    fn imports_what_other_calendars_write() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:x\r\n\
SUMMARY:Dentist\\, Quay Street\r\nDESCRIPTION:Long line that has been\r\n  folded by another program\r\n\
DTSTART;TZID=Europe/London:20261119T153000\r\nDTEND;TZID=Europe/London:20261119T161500\r\n\
BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261224\r\nDTEND;VALUE=DATE:20261227\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:Every other week\r\nDTSTART;VALUE=DATE:20261101\r\nRRULE:FREQ=WEEKLY;INTERVAL=2\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:Tuesdays\r\nDTSTART;VALUE=DATE:20261103\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:Birthday\r\nDTSTART;VALUE=DATE:20260805\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
BEGIN:VTODO\r\nSUMMARY:File taxes\r\nDUE;VALUE=DATE:20270131\r\nSTATUS:NEEDS-ACTION\r\nEND:VTODO\r\n\
BEGIN:VTODO\r\nSUMMARY:Buy stamps\r\nSTATUS:COMPLETED\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let (items, dropped) = import(text);
        assert_eq!(items.len(), 7);
        assert_eq!(dropped, 2, "INTERVAL=2 and BYDAY are not representable");

        let d = &items[0];
        assert_eq!(d.title, "Dentist, Quay Street");
        assert_eq!(d.notes, "Long line that has been folded by another program");
        assert_eq!((d.date.as_deref(), d.start_time.as_deref(), d.end_time.as_deref()), (Some("2026-11-19"), Some("15:30"), Some("16:15")));

        let h = &items[1];
        assert_eq!((h.date.as_deref(), h.end_date.as_deref(), h.start_time.as_deref()), (Some("2026-12-24"), Some("2026-12-26"), None));

        assert_eq!(items[2].repeat_rule, "", "a repeat we can't keep is dropped, the item stays");
        assert_eq!(items[4].repeat_rule, "yearly");
        let t = &items[5];
        assert_eq!((t.title.as_str(), t.date.as_deref(), t.done), ("File taxes", Some("2027-01-31"), false));
        assert_eq!((items[6].date.as_deref(), items[6].done), (None, true));
    }

    #[test]
    fn utc_times_become_local() {
        let (items, _) = import("BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261119T120000Z\r\nEND:VEVENT\r\n");
        let expected = Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap().with_timezone(&Local);
        assert_eq!(items[0].start_time.as_deref(), Some(expected.format("%H:%M").to_string().as_str()));
        assert_eq!(items[0].date.as_deref(), Some(expected.format("%Y-%m-%d").to_string().as_str()));
    }

    #[test]
    fn garbage_imports_nothing() {
        assert!(import("not a calendar").0.is_empty());
        assert!(import("").0.is_empty());
    }
}
