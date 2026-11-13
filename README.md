# Brick

A desktop mail client with one Rust engine and two native front ends.

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
| Linux Qt app | Written, **never compiled**: Qt 6 is not installed on the machine it was written on. |
| macOS app | Written, **never compiled**: it was written on Linux. |
| Keychain storage | Compiles; not exercised (tests use the in-memory store). |
| Gmail and Outlook sign-in | The OAuth flow is tested against a stand-in server, not against Google or Microsoft. |

Expect compile errors in both front ends on first build.

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
- Credentials live in the macOS Keychain or the Linux Secret Service, never in the database

## Not done yet

- Push (IMAP IDLE): mail is checked every two minutes and on demand
- Conversation threading in the list
- HTML composing, sending attachments, drafts saved to the server
- Fastmail over JMAP (it uses IMAP for now)
- Folder names with non-ASCII characters are shown in their raw IMAP encoding
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

Passwords and tokens are stored through the Secret Service, so a keyring daemon
(gnome-keyring, KWallet, KeePassXC) must be running. To try the app without
one, `BRICK_SECRETS=memory ./linux/build/brick` keeps credentials in memory
only; you will be asked to sign in again on every start.

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
