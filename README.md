# Brick

A desktop mail client with a built-in to-do list and calendar, on one Rust engine with two native front ends.

```
core/    Rust engine: local SQLite store, IMAP sync, SMTP, OAuth, HTML sanitising
linux/   Qt 6 Widgets app (C++)
macos/   SwiftUI app
```

The front ends hold no mail logic. They call the engine through four C
functions ([core/include/brick.h](core/include/brick.h)): JSON requests in,
JSON events out. Every request is answered from the local database, so the UI
never waits on the network; anything that needs the server runs in the
background and reports back as an event.

## Status

| Part | State |
|---|---|
| Engine | Builds. Unit tests pass, and an end-to-end test passes against a real IMAP/SMTP server. |
| C interface | Builds and runs from a C program. |
| Linux Qt app | Builds and runs with Qt 6.8.3. Mail, To-do, Calendar (month, week, day), repeats, `.ics`, invitations, unified inbox, signatures, aliases and the Claude button, all driven and checked through the debug hook (headless, against stand-in servers). Not yet tried on a real desktop or a real mail account. |
| macOS app | Builds cleanly (Swift 6.4, no warnings). Mail runs. The To-do and Calendar tabs, the Claude button and everything added since (week and day views, repeats, `.ics`, invitations, unified inbox, aliases, signatures, conversations) compile but have **not yet been tried by hand**. The calendar's slow day click was fixed in source, not yet confirmed on a Mac. |
| Claude extraction | Tested against a stand-in server only; no request has been sent to the real API. |
| Keychain storage (macOS) | Compiles; not exercised (tests use the in-memory store). |
| Secrets file (Linux) | Unit-tested, and checked once through the running app. |
| `config.toml` | Engine unit- and API-tested; on Linux the conversations toggle checked through the running app (headless). The macOS side of `ui.threaded` compiles but has not been run. |
| Gmail and Outlook sign-in | The OAuth flow is tested against a stand-in server, not against Google or Microsoft. |

Expect bugs in anything marked as not yet tried or run.

## What works in the engine

- Fastmail (app password), Gmail and Outlook (OAuth 2 with PKCE, XOAUTH2), and any IMAP/SMTP server
- Folder discovery with special-use roles (inbox, sent, archive, trash, junk)
- Incremental sync: newest 300 messages per folder first, then history in the background
- Flag changes and deletions made on other devices are picked up
- Bodies fetched on demand and cached; the newest 30 inbox bodies are fetched ahead of time
- Full-text search over headers and downloaded bodies (SQLite FTS5)
- Mark read/unread, star, archive, move to Trash, delete from Trash; all applied locally first
- Send and reply (plain text) with correct threading headers; a copy is filed in Sent where the provider doesn't do that itself
- HTML mail is sanitised: scripts, frames and event handlers removed, remote images blocked until you allow them per message
- Credentials never go in the database: on macOS they live in the Keychain, on Linux in a file only you can read (see below)

## To-do and calendar

A to-do and a calendar entry are the same thing: an item. With no date it shows
only in the To-do tab; with a day it also shows on the Calendar tab; with times
it is a timed entry. Any item can be ticked off.

- Items live in Brick's own database on each machine. They are not synced to
  Fastmail, Google or Outlook calendars, or between machines.
- **Add to To-do** on an open email sends it to Claude, which proposes items.
  You review, edit and untick them before anything is saved.
- An item made from an email keeps a link back to it, and the email shows the
  items made from it.

To use the Claude button, add an Anthropic API key under Settings → Claude. It
is stored with your mail passwords. Each click sends that one email's sender,
recipients, subject, date and text to Anthropic's API; attachments are not
sent, and nothing is sent at any other time. The model defaults to
`claude-opus-5-5` and can be changed in the same place (`claude-haiku-5-5` is
much cheaper and likely good enough for this). Requests opt in to Anthropic's
server-side fallback, so if a safety classifier declines an email the API
retries it on another model.

## Not done yet

- Reminders for items
- Calendar sync (CalDAV, Google, Outlook), and syncing items between machines
- Repeats other than a plain daily, weekly, monthly or yearly one (every other week, on given weekdays)

- Push (IMAP IDLE): mail is checked every two minutes and on demand
- Grouping by subject when the headers don't link messages (threading uses Message-ID, In-Reply-To and References only)
- HTML composing, sending attachments, drafts saved to the server
- Fastmail over JMAP (it uses IMAP for now)
- Undo for archive, trash and move (needs an engine change: a moved message disappears locally until the next sync)
- Notifications for new mail
- Flag reconciliation re-reads the newest 2,000 UIDs per folder each pass rather than using CONDSTORE

## Building

### Engine

```sh
cargo test -p brick-core
```

The end-to-end test needs a local mail server and is skipped otherwise. The
command is at the top of [core/tests/live_server.rs](core/tests/live_server.rs);
start a fresh container for each run.

### Linux

Needs Qt 6.2 or later with WebEngine. Ubuntu 22.04's own packages are Qt 6.2.4,
whose web engine is a 2021 Chromium, so the official Qt 6.8 binaries are the
better choice:

```sh
pip install --user aqtinstall
aqt install-qt linux desktop 6.8.3 linux_gcc_64 -m qtwebengine qtwebchannel qtpositioning -O ~/Qt
cmake -S linux -B linux/build -G Ninja -DCMAKE_PREFIX_PATH=$HOME/Qt/6.8.3/gcc_64
cmake --build linux/build
./linux/build/brick
```

#### Running and inspecting it without a display

`linux/tools/` lets a script (or Claude) run the app headless and look inside it:

```sh
linux/tools/run-headless.sh start --seed   # offscreen, own data dir, demo mail
linux/tools/seed-items.py                  # demo to-dos and calendar items
linux/tools/brickctl.py tree               # outline of every visible widget
linux/tools/brickctl.py set viewTabs index=2
linux/tools/brickctl.py shot /tmp/calendar.png
linux/tools/brickctl.py click monthGrid --pos 440,380 --double
```

The app opens a debug socket only when `BRICK_DEBUG_SOCKET` is set; see the
header of [linux/src/DebugServer.h](linux/src/DebugServer.h) for the commands.
`fake-claude.py` stands in for the Anthropic API (set
`BRICK_ANTHROPIC_BASE_URL`) so Add to To-do can be tried without a key.

Passwords, OAuth refresh tokens and the Anthropic key are kept in
`~/.local/share/brick/brick/secrets.json`, a file only your user can read or
write (mode 0600; Brick narrows it back if that changes). It is not encrypted:
anything running as you can read it, and backups copy it as it is, the same as
`~/.netrc` or an msmtp config. Brick does not use the Secret Service (GNOME
Keyring, KWallet), so it needs no particular desktop.
`BRICK_SECRETS=memory ./linux/build/brick` (or `store = "memory"` under
`[secrets]` in `config.toml`) keeps credentials in memory only;
you will be asked to sign in again on every start.

### Configuration

Every setting is in one TOML file, `config.toml`, in the data directory
(`~/.local/share/brick/brick/config.toml` on Linux,
`~/Library/Application Support/Brick/config.toml` on macOS), or wherever
`BRICK_CONFIG` points. The first run writes it with each setting commented out
at its default: the sync windows and poll interval, network timeouts, the
Claude model and limits, where secrets are kept, OAuth client IDs, and each
account's aliases and signature. Brick notices when the file changes, so most
edits apply without a restart (the secret store is chosen at startup, and the
front ends read `ui.threaded` when they start). The settings dialogs write to
the same file and keep your comments. A mistake in the file stops Brick from
starting with a message naming the line; a mistake made while it is running is
ignored, and the last good settings stay in use until the file is fixed.
`BRICK_SECRETS` and `BRICK_ANTHROPIC_BASE_URL` override the file.

### macOS

Needs Rust and Xcode 15 or later, on macOS 14 or later.

```sh
./macos/build-app.sh
open macos/Brick.app
```

## Setting up accounts

### Fastmail

Settings → Privacy & Security → Connected apps & API tokens → New app
password, with access to Mail (IMAP/SMTP). Use it as the password in Brick.

### Gmail

Google only allows OAuth, and you need your own client ID.

1. In the Google Cloud console, create a project and enable the Gmail API.
2. Configure the OAuth consent screen as External and add your own address as a user.
3. Set the publishing status to "In production". Left in "Testing", Google expires your sign-in every 7 days. You will see an "unverified app" warning when signing in; that is expected for a personal client.
4. Create credentials → OAuth client ID → application type "Desktop app".
5. Paste the client ID and client secret into Brick's Add account dialog.

### Outlook

1. In the Microsoft Entra admin centre, go to App registrations → New registration.
2. Supported account types: organisational directories and personal Microsoft accounts.
3. Redirect URI: platform "Public client/native", value `http://localhost`.
4. Under Authentication, allow public client flows.
5. Under API permissions, add the delegated permissions `IMAP.AccessAsUser.All`, `SMTP.Send` and `offline_access`.
6. Paste the Application (client) ID into Brick's Add account dialog.

A work or school account may need its administrator to approve the app.

The exact wording in both consoles changes from time to time.
