# Brick TODO

The short list lives in [README.md](README.md) under "Not done yet". This is the
longer working list. Items are roughly ordered by priority within each section.

## Get to a working state

- [x] Compile the Linux Qt app for the first time and fix what breaks (one error: a helper named `sender` clashed with `QObject::sender`)
- [x] Compile the macOS To-do and Calendar tabs and the Claude button, and fix what breaks (compiled cleanly with Swift 6.4; nothing to fix)
- [ ] Confirm the calendar day click is snappy after the tap-gesture fix (`CalendarView.swift`); profile with Instruments if not
- [ ] Send one real request to the Anthropic API and check the extraction flow end to end
- [ ] Sign in to real Gmail and Outlook accounts and fix whatever the stand-in OAuth server hid
- [ ] Check secrets in `config.toml` through both running apps: add an account, rotate an OAuth token, set the Claude key
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
- [ ] Try each feature by hand: tick a repeating item, import an `.ics`, open an invitation, send as an alias
- [ ] Check the week view's layout and the double-click-to-add gesture against real clicks
- [ ] Check J and K work in the message list (`onKeyPress` needs the list focused)

## iPhone app

Draft in `ios/`, sharing `macos/Sources/Shared` (engine wrapper, models, `AppModel`, a few views) with the Mac.
Runs in the simulator against a seeded store; nothing below has been tried yet unless ticked.

- [x] Xcode project that builds the engine for the device or simulator and links it
- [x] Mail (mailboxes, list, reader, compose), To-do, month calendar, item editor; seen in simulator screenshots
- [ ] Try swipe actions, sending, adding a real password account, attachments (Quick Look), `.ics` import and export
- [x] Install on a phone (`make -C ios install`): installs and launches on an iPhone 12 mini
- [ ] Use it on the phone with a real account
- [ ] Gmail and Outlook sign-in: needs an engine change from the loopback redirect to a custom URL scheme
      (or `ASWebAuthenticationSession`) and iOS-type OAuth client IDs
- [ ] Check background refresh actually runs (`BGAppRefreshTask`; can't be triggered from `simctl`)
- [ ] Claude button and review sheet; Claude API key screen (for now: edit `config.toml` in the Files app)
- [ ] Week and day calendar views; conversation view in the reader; app icon
- [ ] Launch in the simulator takes ~15 s while dyld loads the debug build; check a Release build and the device

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
- [x] Match macOS features (compiles, not run; see above)

## Engine and interface

- [ ] Document panic and error behaviour at the C boundary in `brick.h`
- [ ] Version the JSON protocol, or add a `version` method, so front ends can detect mismatches
- [ ] Integration test that drives the engine through the C interface from a C program
- [ ] Schema migrations for the SQLite store

## Housekeeping

- [ ] CI for engine tests, and for both front ends once they compile
- [ ] Link this file from the README
- [ ] Licence
