# Brick TODO

The short list lives in [README.md](README.md) under "Not done yet". This is the
longer working list. Items are roughly ordered by priority within each section.

## Get to a working state

- [x] Compile the Linux Qt app for the first time and fix what breaks (one error: a helper named `sender` clashed with `QObject::sender`)
- [ ] Compile the macOS To-do and Calendar tabs and the Claude button, and fix what breaks
- [ ] Confirm the calendar day click is snappy after the tap-gesture fix (`CalendarView.swift`); profile with Instruments if not
- [ ] Send one real request to the Anthropic API and check the extraction flow end to end
- [ ] Sign in to real Gmail and Outlook accounts and fix whatever the stand-in OAuth server hid
- [ ] Exercise Keychain / Secret Service storage outside the in-memory test store
- [ ] Run the end-to-end IMAP/SMTP test in CI

## Speed (the app's main promise)

- [ ] Measure the time from click to repaint for: opening a message, switching folder, selecting a calendar day, search
- [ ] Add a debug timing log around `brick_call` so slow methods are visible from either front end
- [ ] Check `items.list` is not reloaded in full after every small change (`reloadItems` after save/delete)
- [ ] Virtualise the message list so large folders stay smooth
- [ ] Check start-up time with a large local store

## Mail

- [ ] IMAP IDLE for push, replacing the two-minute poll
- [ ] Conversation threading in the message list
- [ ] HTML composing
- [ ] Sending attachments
- [ ] Drafts saved to the server
- [ ] Fastmail over JMAP
- [ ] Decode IMAP modified UTF-7 folder names
- [ ] Use CONDSTORE for flag reconciliation instead of re-reading the newest 2,000 UIDs
- [ ] Multiple identities / aliases per account
- [ ] Signatures
- [ ] Undo for archive, trash and move
- [ ] Keyboard shortcuts for triage (archive, trash, next/previous, mark read)
- [ ] Notifications for new mail
- [ ] Unified inbox across accounts

## To-do and calendar

- [ ] Week and day views
- [ ] Edit the end date of a multi-day item
- [ ] Recurring items
- [ ] Reminders
- [ ] Calendar sync (CalDAV, Google, Outlook)
- [ ] Sync items between machines
- [ ] Import and export `.ics`
- [ ] Open invitations in email as items
- [x] To-do and Calendar tabs in the Linux app

## Claude extraction

- [ ] Show the cost or token count of each extraction before sending
- [ ] Handle very long emails (`truncated` is reported; decide what the user sees)
- [ ] Rate-limit and error handling for API failures
- [ ] Consider extracting from several selected emails at once

## Linux app

- [ ] Try it on a real desktop and a real mail account (so far only run headless against stand-in servers)
- [ ] Week and day calendar views (done for neither app)
- [ ] Calendar: switch month when clicking a faded day from the next or previous month
- [ ] Extraction review dialog is tall with three proposals; make the cards more compact
- [ ] Keyboard-only use of the calendar day panel (Tab from the grid)

- [ ] Packaging (AppImage or Flatpak) and install of `brick.desktop`
- [x] Decide how to run and inspect it for debugging: offscreen platform plus a debug socket (`linux/tools/`)
- [ ] Match macOS features as they land

## Engine and interface

- [ ] Document panic and error behaviour at the C boundary in `brick.h`
- [ ] Version the JSON protocol, or add a `version` method, so front ends can detect mismatches
- [ ] Integration test that drives the engine through the C interface from a C program
- [ ] Schema migrations for the SQLite store

## Housekeeping

- [ ] CI for engine tests, and for both front ends once they compile
- [ ] Link this file from the README
- [ ] Licence
