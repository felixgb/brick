# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Brick is a desktop mail client with a built-in to-do list and calendar: one Rust engine (`core/`) and two native front ends, Qt 6 Widgets in C++ (`linux/`) and SwiftUI (`macos/`). README.md has the user-facing feature list and a status table; TODO.md is the longer working list.

## Commands

The root `Makefile` wraps the common commands (`make help` lists them all):

```sh
make                      # build the Linux app (configures CMake on first run; also builds the engine)
make run                  # build and launch the Linux app
make test                 # all engine tests
make test-live            # fresh greenmail container → live IMAP/SMTP test → container removed
make fmt / make clippy    # Rust formatting / lints
make headless             # build, start headless with demo mail + seeded to-dos/calendar items
make headless-restart     # rebuild and restart the headless app
make headless-stop / headless-log
make shot SHOT=/tmp/x.png # screenshot the headless app (default /tmp/brick.png)
make macos                # build macos/Brick.app (Mac only)
make clean / distclean
```

Overridable variables: `QT_PREFIX` (default `~/Qt/6.8.3/gcc_64`), `BUILD_DIR`, `GENERATOR`, `PREFIX`, `CARGO`.

The underlying commands:

```sh
# Engine
cargo test -p brick-core                          # all engine tests
cargo test -p brick-core --test items_api         # one integration test file
cargo test -p brick-core ics::                    # unit tests in one module (filter by path/name)

# End-to-end IMAP/SMTP test: skipped unless BRICK_TEST_SERVER=1. Start a fresh
# greenmail container for each run (exact docker command at the top of core/tests/live_server.rs).
BRICK_TEST_SERVER=1 cargo test -p brick-core --test live_server -- --nocapture

# Linux app (Qt 6.2+ with WebEngine; official Qt 6.8.3 via aqtinstall in ~/Qt is the tested setup)
cmake -S linux -B linux/build -G Ninja -DCMAKE_PREFIX_PATH=$HOME/Qt/6.8.3/gcc_64
cmake --build linux/build                         # also runs `cargo build --release -p brick-core`
./linux/build/brick

# macOS app (needs a Mac: Rust + Xcode 15, macOS 14)
./macos/build-app.sh && open macos/Brick.app
```

There is no Swift toolchain on this Linux machine, so macOS code cannot be compiled or run here. Anything written in `macos/` is unverified until built on a Mac; say so rather than claiming it works.

## Running the Linux app headless (how to check UI changes)

```sh
linux/tools/run-headless.sh start --seed   # offscreen Qt, data in /tmp/brick-debug, demo mail, in-memory secrets
linux/tools/seed-items.py                  # demo to-dos and calendar items
linux/tools/brickctl.py tree               # widget tree with objectNames
linux/tools/brickctl.py set viewTabs index=2
linux/tools/brickctl.py click monthGrid --pos 440,380 --double
linux/tools/brickctl.py shot /tmp/calendar.png   # then Read the PNG to look at it
linux/tools/run-headless.sh stop | restart | log
```

`run-headless.sh` only builds if `linux/build/brick` is missing; rebuild with `cmake --build linux/build` and `restart` after changes. The debug socket protocol (tree, click, key, type, set, action, engine `call`, screenshot…) is documented at the top of `linux/src/DebugServer.h`. Widgets are addressed by `objectName`, so give new widgets one if they need to be driven.

Environment variables:
- `BRICK_CONFIG` — path of the config file instead of `<data dir>/config.toml`.
- `BRICK_DEBUG_SOCKET` — enables the debug server (and suppresses the first-run account dialog).
- `BRICK_SECRETS=memory` — in-memory credential store instead of the Keychain (macOS) or `secrets.json` (Linux). Overrides `secrets.store` in config.toml.
- `BRICK_ANTHROPIC_BASE_URL` — point Claude extraction at `linux/tools/fake-claude.py` instead of the real API. Overrides `claude.base_url`.
- `BRICK_TEST_SERVER=1` — enable the live IMAP/SMTP test.

## Architecture

**The engine owns everything; front ends hold no mail logic.** The boundary is four C functions in `core/include/brick.h` (`brick_open`, `brick_call`, `brick_string_free`, `brick_close`): JSON request in, JSON response out (`{"ok": ...}` or `{"error": "..."}`), plus JSON events delivered on a background thread via the callback passed to `brick_open`.

- `brick_call` only touches the local SQLite store and must stay fast enough to call on the UI thread. Anything needing the network (sync, body fetch, send, OAuth, Claude extraction) is spawned on the engine's tokio runtime and reports back as an event (`body_ready`, `send_done {token}`, `items_extracted {token}`, …). Async requests carry a caller-chosen `token` to match the later event.
- Local-first mutations: flags, moves, item edits are applied to the store immediately, then pushed to the server in the background.
- `brick.h`'s header comment is **the protocol spec**: every method, its params and results, every event, and the settings keys. `macos/Sources/CBrick/include/brick.h` is a symlink to it.

Adding or changing an engine method touches, in order:
1. `core/src/engine.rs` — `Engine::dispatch` is the single `match method { ... }` routing every request.
2. The logic in the relevant module (`store.rs` for SQL, `sync.rs` for network work, `items.rs`, `ics.rs`, `claude.rs`, `threads.rs`, …).
3. `core/include/brick.h` (documentation).
4. Each front end: `linux/src/Core.{h,cpp}` and `macos/Sources/Brick/Core.swift` are thin wrappers over the C API; the UI code calls methods by name string and listens for events (Linux: `Core::event` signal; macOS: `AppModel.swift`).

Engine modules worth knowing:
- `config.rs` — **all settings live in `config.toml`** (data dir, or `BRICK_CONFIG`): typed `Config` struct with serde defaults and `deny_unknown_fields`, a commented `TEMPLATE` written on first open (a test checks its commented defaults equal `Config::default()`), re-read when the file's mtime/size changes, edited in place with `toml_edit` so comments survive. `settings.get/set` go through `ConfigFile::setting/set_setting`; old flat keys (`claude_model`, `aliases_<id>`, `google_client_id`, …) are mapped in `resolve`. New tunables: add a field + default, a commented line in `TEMPLATE`, and read it via `inner.config.get()` at the point of use (not cached), so hand edits apply live. The SQLite `settings` table is legacy, read only to migrate old values on open. Secrets never go in this file.
- `store.rs` — SQLite (bundled rusqlite, FTS5 for search). Schema is a `CREATE TABLE IF NOT EXISTS` string with **no migrations**: a new column will not appear in existing databases (the schema-migration item in TODO.md is still open).
- `sync.rs` — all network activity: initial window of newest 300 per folder, background history backfill, flag/deletion reconciliation over the newest 2,000 UIDs, body prefetch for the newest 30 inbox messages, 2-minute poll (no IDLE).
- `imap.rs`, `smtp.rs`, `oauth.rs` (PKCE + XOAUTH2 for Gmail/Outlook), `providers.rs` (server presets), `secrets.rs` (`KeyringStore` = macOS Keychain only; `FileStore` = Linux `secrets.json`, mode 0600; `MemoryStore`). Linux deliberately does not use the Secret Service or any keyring daemon, so the app works without GNOME or KDE; don't reintroduce it.
- `mime.rs` — parsing and HTML sanitising (ammonia); remote images blocked unless `allow_remote`.
- `threads.rs` — conversations linked only by Message-ID / In-Reply-To / References. Subject-based grouping is deliberately not done.
- `items.rs` — to-dos and calendar entries are one type ("item"): no date = to-do only, a day = also on the calendar, times = timed entry. Repeating items keep their date as the next occurrence; `set_done` advances them.
- `claude.rs` — sends one email to the Anthropic Messages API and returns proposed items; nothing is saved until the front end calls `items.save`. Default model `claude-opus-5-5`, overridable via the `claude_model` setting.

The Linux CMake build invokes cargo itself and statically links `target/release/libbrick_core.a`; the macOS SwiftPM package links the same static library through the `CBrick` shim module.

## Conventions

- Feature parity: features usually land in the engine + Linux first (where they can be run and checked headlessly), then the Swift side is written. TODO.md and README's status table track exactly what is engine-only, Linux-only, or written-but-not-compiled on macOS. Update both when that changes, and keep the wording honest about what was actually tested (e.g. "against a stand-in server", "not yet compiled").
- Tests that need credentials use `Engine::open_with(dir, Arc::new(MemoryStore::default()), emit)`; see `core/tests/items_api.rs` for the pattern of driving the engine through JSON calls.
