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
db.execute(
    "INSERT INTO accounts (id, email, name, provider, auth, username, imap_host, imap_port, imap_security,"
    " smtp_host, smtp_port, smtp_security) VALUES (2, 'sam@work.example', 'Sam Rivers', 'custom', 'password',"
    " 'sam@work.example', '127.0.0.1', 1, 'tls', '127.0.0.1', 1, 'tls')"
)
folders = [(1, 1, "INBOX", "Inbox", "inbox"), (2, 1, "Sent", "Sent", "sent"), (3, 1, "Archive", "Archive", "archive"),
           (4, 1, "Trash", "Trash", "trash"), (5, 1, "Junk", "Junk", "junk"),
           (6, 2, "INBOX", "Inbox", "inbox"), (7, 2, "Sent", "Sent", "sent")]
for fid, acc, path, name, role in folders:
    db.execute("INSERT INTO folders (id, account_id, path, name, role, selectable, backfill_uid) VALUES (?,?,?,?,?,1,1)",
               (fid, acc, path, name, role))

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
import datetime as dt

# A calendar invitation, in the first account's inbox.
wed = dt.date.today() + dt.timedelta(days=5)
ics = ("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:inv1@fieldnotes\r\n"
       "SUMMARY:Venue walk-through\r\nLOCATION:The Old Granary\r\n"
       f"DTSTART:{wed:%Y%m%d}T140000\r\nDTEND:{wed:%Y%m%d}T150000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
raw = (
    "From: Ana Kowalski <ana@fieldnotes.example>\r\nTo: Sam Rivers <sam@example.com>\r\n"
    "Subject: Invitation: Venue walk-through\r\nMessage-ID: <demo-invite@example>\r\nMIME-Version: 1.0\r\n"
    "Content-Type: multipart/alternative; boundary=b1\r\n\r\n--b1\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
    "Ana has invited you to a walk-through of the venue.\r\n--b1\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\n"
    + ics + "--b1--\r\n").encode()
db.execute(
    "INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, from_name, from_addr, to_text,"
    " date, size, seen, flagged, snippet) VALUES (8,1,1,8,'<demo-invite@example>','Invitation: Venue walk-through',"
    "'Ana Kowalski','ana@fieldnotes.example','Sam Rivers <sam@example.com>',?,?,0,0,'Ana has invited you to a walk-through of the venue.')",
    (now - 2 * hour, len(raw)))
db.execute("INSERT INTO bodies (message_id, raw) VALUES (8,?)", (raw,))
db.execute("INSERT INTO messages_fts (rowid, subject, sender, recipients, body) VALUES (8,'Invitation: Venue walk-through','Ana Kowalski','sam@example.com','walk-through of the venue')")

# A second account, so there is a unified inbox.
work = [
    (6, "Dev Patel", "dev@work.example", "Quarterly numbers are in", "The numbers are in the shared folder.", 30 * 60, 0),
    (7, "IT Helpdesk", "it@work.example", "Password expiry notice", "Your password expires in 7 days.", 5 * hour, 1),
]
for mid, name, addr, subject, text, ago, seen in work:
    raw = (f"From: {name} <{addr}>\r\nTo: sam@work.example\r\nSubject: {subject}\r\nMessage-ID: <demo{mid}@work>\r\n"
           f"Content-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n").encode()
    db.execute(
        "INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, from_name, from_addr, to_text,"
        " date, size, seen, flagged, snippet) VALUES (?,2,6,?,?,?,?,?,'sam@work.example',?,?,?,0,?)",
        (mid, mid, f"<demo{mid}@work>", subject, name, addr, now - ago, len(raw), seen, text))
    db.execute("INSERT INTO bodies (message_id, raw) VALUES (?,?)", (mid, raw))
    db.execute("INSERT INTO messages_fts (rowid, subject, sender, recipients, body) VALUES (?,?,?,?,?)",
               (mid, subject, f"{name} {addr}", "sam@work.example", text))
# A conversation: two replies to the first work message, linked by headers only.
replies = [
    (20, "Sam Rivera", "sam@work.example", "Re: Quarterly numbers are in", "Thanks, looking now.", 20 * 60, 1, "<demo6@work>", "<demo6@work>"),
    (21, "Dev Patel", "dev@work.example", "Re: Quarterly numbers are in", "Great. Can you check the Q3 column?", 10 * 60, 0, "<demo20@work>", "<demo6@work> <demo20@work>"),
]
for mid, name, addr, subject, text, ago, seen, reply_to, refs in replies:
    raw = (f"From: {name} <{addr}>\r\nTo: sam@work.example\r\nSubject: {subject}\r\nMessage-ID: <demo{mid}@work>\r\n"
           f"In-Reply-To: {reply_to}\r\nReferences: {refs}\r\n"
           f"Content-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n").encode()
    db.execute(
        "INSERT INTO messages (id, account_id, folder_id, uid, message_id, in_reply_to, refs, subject, from_name,"
        " from_addr, to_text, date, size, seen, flagged, snippet) VALUES (?,2,6,?,?,?,?,?,?,?,'sam@work.example',?,?,?,0,?)",
        (mid, mid, f"demo{mid}@work", reply_to.strip("<>"), refs.replace("<", "").replace(">", ""),
         subject, name, addr, now - ago, len(raw), seen, text))
    db.execute("INSERT INTO bodies (message_id, raw) VALUES (?,?)", (mid, raw))
    db.execute("INSERT INTO messages_fts (rowid, subject, sender, recipients, body) VALUES (?,?,?,?,?)",
               (mid, subject, f"{name} {addr}", "sam@work.example", text))
# The engine stores ids without angle brackets, as the mail parser gives them.
db.execute("UPDATE messages SET message_id = replace(replace(message_id, '<', ''), '>', '')")
db.execute("UPDATE folders SET unread=2, total=4 WHERE id=6")

unread = sum(1 for m in mails if not m[5]) + 1
db.execute("UPDATE folders SET unread=?, total=? WHERE id=1", (unread, len(mails) + 1))
db.commit()
print(f"seeded {len(mails)} messages")
