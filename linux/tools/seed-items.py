#!/usr/bin/env python3
"""Add demo to-dos and calendar items to a running Brick, dated around today.
Pairs with seed-demo.py (two of the items link back to its messages)."""
import datetime as dt
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from brickctl import DEFAULT_SOCKET, send  # noqa: E402

today = dt.date.today()


def day(n):
    return (today + dt.timedelta(days=n)).isoformat()


def item(title, date=None, start=None, end=None, location="", notes="", done=False, end_date=None, source=None):
    o = {"id": 0, "title": title, "notes": notes, "location": location, "date": date, "end_date": end_date,
         "start_time": start, "end_time": end, "done": done, "source_account_id": None,
         "source_message_id": "", "source_subject": "", "source_from": ""}
    if source:
        o.update(source_account_id=1, source_message_id=source[0], source_subject=source[1], source_from=source[2])
    return o


# Thursday-ish: next Thursday relative to today
thu = (3 - today.weekday()) % 7 or 7
items = [
    item("Send the revised budget", day(-2), notes="Version 3, with the venue deposit.",
         source=("<demo2@example>", "Notes from the planning call", "tomas@fieldnotes.example")),
    item("Return library books", day(3 if today.weekday() != 0 else 7), location="Central branch",
         source=("<demo3@example>", "Your books are due back on Monday", "no-reply@library.example")),
    item("Dentist check-up", day(thu), "15:30", "16:15", "Quay Street surgery",
         source=("<demo1@example>", "Dentist moved to Thursday 15:30", "priya@harbourworks.example")),
    item("Team planning call", day(1), "10:00", "11:00", "Video call"),
    item("Lunch with Ana", day(1), "12:30", location="The Granary café"),
    item("Book the venue", day(5), source=("<demo4@example>", "Re: Venue for the 21st", "ana@fieldnotes.example")),
    item("Venue deposit due", day(9)),
    item("Bike service", day(4 if today.weekday() < 5 else 6), "09:00", location="Fastbike"),
    item("Conference", day(12), end_date=day(14), location="Bristol"),
    item("Standup", day(0), "09:30", "09:45"),
    item("Review pull requests", day(0)),
    item("Water the plants", day(0), done=True),
    item("Renew passport"),
    item("Read the ASAN paper"),
    item("Order printer ink"),
    item("Tidy the shed", done=True),
]
for it in items:
    r = send(DEFAULT_SOCKET, {"cmd": "call", "method": "items.save", "params": {"item": it}})
    if not r.get("ok"):
        sys.exit("items.save failed: %s" % r.get("error"))
print("added %d items" % len(items))
