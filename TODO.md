# Brick TODO

The short list lives in [README.md](README.md) under "Not done yet". This is the
longer working list. Items are roughly ordered by priority within each section.

## Get to a working state

- [x] Compile the Linux Qt app for the first time and fix what breaks (one error: a helper named `sender` clashed with `QObject::sender`)
- [x] Compile the macOS To-do and Calendar tabs and the Claude button, and fix what breaks (compiled cleanly with Swift 6.4; nothing to fix)
- [ ] Confirm the calendar day click is snappy after the tap-gesture fix (`CalendarView.swift`); profile with Instruments if not
- [ ] Send one real request to the Anthropic API and check the extraction flow end to end
- [ ] Add a real Gmail account with an app password
- [ ] Sign in to a real Outlook account and fix whatever the stand-in OAuth server hid
- [ ] Check secrets in `config.toml` through the running apps: add an account, rotate an OAuth token, set the Claude key
      (the iPhone has read a password and the Claude key from a `config.toml` copied from the Mac)
- [ ] Run the end-to-end IMAP/SMTP test in CI

## Speed (the app's main promise)

- [ ] Measure the time from click to repaint for: opening a message, switching folder, selecting a calendar day, search
- [ ] Add a debug timing log around `brick_call` so slow methods are visible from either front end
- [ ] Check `items.list` is not reloaded in full after every small change (`reloadItems` after save/delete)
- [ ] Virtualise the message list so large folders stay smooth
- [ ] Check start-up time with a large local store

## Mail

- [ ] IMAP IDLE for push, replacing the two-minute poll
- [x] Conversation threading in the message list (engine, Linux and macOS; macOS compiles, not tried by hand)
- [ ] Thread by subject when the headers are missing or broken (not done on purpose: it merges unrelated mail)
- [ ] Undo and moving a conversation need a conversation-aware design
- [ ] HTML composing
- [ ] Sending attachments
- [ ] Drafts saved to the server
- [ ] Fastmail over JMAP
- [x] Decode IMAP modified UTF-7 folder names (engine; tested)
- [ ] Use CONDSTORE for flag reconciliation instead of re-reading the newest 2,000 UIDs
- [x] Multiple identities / aliases per account (engine + Linux; macOS compiles, not tried)
- [x] Signatures (Linux; macOS compiles, not tried)
- [ ] Undo for archive, trash and move. Needs an engine change first: `messages.move` deletes the local row at once and the message only reappears after the next sync, so there is nothing to move back
- [x] Keyboard shortcuts for triage (Linux: A, Delete, M, S, J, K; macOS compiles, not tried)
- [ ] Notifications for new mail
- [x] Unified inbox across accounts (engine + Linux; macOS compiles, not tried)

## To-do and calendar

- [x] Week and day views (Linux; macOS compiles, not tried)
- [x] Edit the end date of a multi-day item (Linux; macOS compiles, not tried)
- [x] Recurring items: daily/weekly/monthly/yearly with an optional end (engine + Linux; macOS compiles, not tried)
- [ ] Reminders
- [ ] Calendar sync (CalDAV, Google, Outlook)
- [ ] Sync items between machines
- [x] Import and export `.ics` (engine + Linux; macOS compiles, not tried)
- [x] Open invitations in email as items (engine + Linux; macOS compiles, not tried)
- [x] To-do and Calendar tabs in the Linux app

## Configuration (`config.toml`)

- [x] Build the macOS side on a Mac: `ui.threaded` moved from UserDefaults to `config.toml` (compiles; not run)
- [ ] Front ends read `ui.threaded` once at startup; have the engine emit an event when `config.toml` changes so they can follow hand edits
- [ ] Surface `config.status` errors (file edited into an invalid state) in the UI instead of only the log
- [ ] Window geometry and splitter positions are still in QSettings on Linux; decide whether layout state belongs in the file

## Claude extraction

- [ ] Show the cost or token count of each extraction before sending
- [ ] Handle very long emails (`truncated` is reported; decide what the user sees)
- [ ] Rate-limit and error handling for API failures
- [ ] Consider extracting from several selected emails at once

## macOS follow-ups for what the engine and Linux app now do

All of this is now written in Swift (repeat, end-day and until controls, week and day views, `.ics` import and
export under File, the invitation bar, All Inboxes, aliases and signatures in Settings > Accounts, J and K,
conversations). It now compiles cleanly with Swift 6.4, but **none of it has been run**:

- [x] Build on a Mac and fix whatever the compiler finds (`./macos/build-app.sh`)
- [ ] Run the Mac app after `Core`, `Models`, `AppModel` and a few views moved to `macos/Sources/Shared` for the
      iPhone (compiles; not launched since). Check attachment saving and `.ics` import/export, whose panels moved
      to `AppModel+Mac.swift`
- [ ] Try each feature by hand: tick a repeating item, import an `.ics`, open an invitation, send as an alias
- [ ] Check the week view's layout and the double-click-to-add gesture against real clicks
- [ ] Check J and K work in the message list (`onKeyPress` needs the list focused)

## iPhone app

Draft in `ios/`, sharing `macos/Sources/Shared` (engine wrapper, models, `AppModel`, a few views) with the Mac.
Checked in the simulator against a seeded store, and installed on an iPhone 12 mini where a real Fastmail
account syncs (the account record and `config.toml` were copied from the Mac, not added in the app).

Done:

- [x] Xcode project that builds the engine for the device or simulator and links it
- [x] Mail (mailboxes, list, reader, compose), To-do, month calendar, item editor; seen in simulator screenshots
- [x] Install on a phone (`make -C ios install`): installs, launches, and syncs a Fastmail account

Try what is written but not yet used:

- [ ] Add a password account through Add Account (the phone's account was copied in by hand)
- [ ] Swipe actions in the message list: archive, delete, read/unread, flag
- [ ] Send, reply, reply all and forward; From picker with aliases; signatures
- [ ] Reader: flag, archive and delete buttons; mark unread; the conversation strip; "Show" remote images;
      adding an invitation; opening an attachment in Quick Look
- [ ] Search, pull to refresh, the conversations toggle, All Inboxes with two accounts
- [ ] To-do: quick add, tick, swipe to delete; editor with repeats; "Open Email" from an item made from mail
- [ ] Calendar: swipe and arrows between months, add on a day, `.ics` import (Files) and export (share sheet)
- [ ] Settings: aliases and signature, remove an account
- [ ] Check background refresh actually runs (`BGAppRefreshTask`; can't be triggered from `simctl`), and that the
      store and `config.toml` can be opened while the phone is locked (file protection class)
- [ ] Editing `config.toml` in the Files app is picked up by the running app

Missing:

- [ ] Outlook sign-in (Gmail uses an app password and works): needs an engine change from the loopback redirect to a custom URL scheme
      (or `ASWebAuthenticationSession`) and iOS-type OAuth client IDs; until then Add Account says to use an app
      password with Other (IMAP)
- [ ] Claude button and review sheet, and a Claude key and model screen (for now: edit `config.toml` in Files)
- [ ] Week and day calendar views
- [ ] App icon from `icon/brick.svg`
- [ ] Select several messages at once (edit mode) for bulk archive, delete and flag
- [ ] Notifications for new mail (needs background refresh first; there is no push without a server)
- [ ] Import and export of `.ics` attachments straight from the reader (open in Brick from other apps)
- [ ] iPad layout (the target is iPhone only)
- [ ] A way to move an account from the Mac to the phone (done by hand: the `accounts` row with `sqlite3` and
      `config.toml` with `devicectl device copy`); or just make Add Account the answer
- [ ] To-dos and calendar items don't reach the phone from the Mac (see "Sync items between machines")

Polish and housekeeping:

- [ ] Plain-text mail shows in a small monospaced font; use a readable proportional one
- [ ] Attachments opened in Quick Look are left in the temporary folder; clear them
- [ ] Launch in the simulator takes ~15 s while dyld loads the debug build; check a Release build on the device
- [ ] The Debug app is 17 MB; check the Release size with the engine stripped
- [ ] A permanent debug hook for driving screens (like the Linux debug socket), since `simctl` can't tap and the
      screenshot harness has to be written and removed each time
- [ ] Free Personal Team signing: device installs expire after 7 days (`make -C ios install` again)

## Linux app

- [ ] Try it on a real desktop and a real mail account (so far only run headless against stand-in servers)
- [x] Week and day calendar views
- [ ] Calendar: switch month when clicking a faded day from the next or previous month
- [ ] Extraction review dialog is tall with three proposals; make the cards more compact
- [ ] Keyboard-only use of the calendar day panel (Tab from the grid)
- [ ] Week view: a narrow overlapping block can lose its title; show the full text in a tooltip
- [ ] Invitations are also listed as an attachment named "attachment" in the reader

- [ ] Packaging (AppImage or Flatpak) and install of `brick.desktop`
- [x] Decide how to run and inspect it for debugging: offscreen platform plus a debug socket (`linux/tools/`)
- [x] Match macOS features (the Mac side compiles but is not run; see above)

## Engine and interface

- [ ] Document panic and error behaviour at the C boundary in `brick.h`
- [ ] Version the JSON protocol, or add a `version` method, so front ends can detect mismatches
- [ ] Integration test that drives the engine through the C interface from a C program
- [ ] Schema migrations for the SQLite store

## Housekeeping

- [ ] CI for engine tests, and builds of the three front ends (Linux, macOS, iPhone)
- [ ] Link this file from the README (README's "Not done yet" is the short list but doesn't point here)
- [ ] Licence
