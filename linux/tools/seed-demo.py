#!/usr/bin/env python3
"""Fill a Brick database with a demo account and a few messages, so screenshots
have something to show. The app must not be running. The account points at a
port nothing listens on, so no network traffic happens.

    seed-demo.py /tmp/brick-debug/data/brick/brick/brick.sqlite
"""
import sqlite3
import sys
import time

db = sqlite3.connect(sys.argv[1])
now = int(time.time())
hour = 3600

db.execute(
    "INSERT INTO accounts (id, email, name, provider, auth, username, imap_host, imap_port, imap_security,"
    " smtp_host, smtp_port, smtp_security) VALUES (1, 'sam@example.com', 'Sam Rivers', 'custom', 'password',"
    " 'sam@example.com', '127.0.0.1', 1, 'tls', '127.0.0.1', 1, 'tls')"
)
folders = [(1, "INBOX", "Inbox", "inbox"), (2, "Sent", "Sent", "sent"), (3, "Archive", "Archive", "archive"),
           (4, "Trash", "Trash", "trash"), (5, "Junk", "Junk", "junk")]
for fid, path, name, role in folders:
    db.execute("INSERT INTO folders (id, account_id, path, name, role, selectable, backfill_uid) VALUES (?,1,?,?,?,1,1)",
               (fid, path, name, role))

mails = [
    ("Priya Nair", "priya@harbourworks.example", "Dentist moved to Thursday 15:30",
     "Hi Sam, we've had to move your check-up to Thursday at 15:30 at the Quay Street surgery. "
     "Please reply if that doesn't work. Priya", 1 * hour, 0, 1),
    ("Tomás Ibarra", "tomas@fieldnotes.example", "Notes from the planning call",
     "Thanks all. Actions: Sam to send the revised budget by Friday; Ana to book the venue for the 21st. "
     "Next call is Tuesday at 10:00.", 3 * hour, 0, 0),
    ("Library Services", "no-reply@library.example", "Your books are due back on Monday",
     "Two items on your account are due back this coming Monday. Renew online or return them to any branch.",
     7 * hour, 1, 0),
    ("Ana Kowalski", "ana@fieldnotes.example", "Re: Venue for the 21st",
     "The Old Granary can do the 21st from 18:00 to 22:00. Deposit is due by the end of next week.",
     26 * hour, 1, 0),
    ("Jo from Fastbike", "jo@fastbike.example", "Your service is booked",
     "Your bike service is booked for Saturday 09:00. Bring the locking key.", 50 * hour, 1, 0),
]
for i, (name, addr, subject, text, ago, seen, flagged) in enumerate(mails, start=1):
    raw = (
        f"From: {name} <{addr}>\r\nTo: Sam Rivers <sam@example.com>\r\nSubject: {subject}\r\n"
        f"Message-ID: <demo{i}@example>\r\nDate: Thu, 01 Jan 2026 09:00:00 +0000\r\n"
        f"Content-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n"
    ).encode()
    db.execute(
        "INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, from_name, from_addr, to_text,"
        " date, size, seen, flagged, snippet) VALUES (?,1,1,?,?,?,?,?,'Sam Rivers <sam@example.com>',?,?,?,?,?)",
        (i, i, f"<demo{i}@example>", subject, name, addr, now - ago, len(raw), seen, flagged, text[:120]),
    )
    db.execute("INSERT INTO bodies (message_id, raw) VALUES (?,?)", (i, raw))
    db.execute("INSERT INTO messages_fts (rowid, subject, sender, recipients, body) VALUES (?,?,?,?,?)",
               (i, subject, f"{name} {addr}", "sam@example.com", text))
unread = sum(1 for m in mails if not m[5])
db.execute("UPDATE folders SET unread=?, total=? WHERE id=1", (unread, len(mails)))
db.commit()
print(f"seeded {len(mails)} messages")
