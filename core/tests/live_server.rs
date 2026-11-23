//! End-to-end test against a real IMAP/SMTP server on localhost.
//!
//! Start one with:
//!   docker run --rm -d --name brick-greenmail -p 127.0.0.1:3143:3143 -p 127.0.0.1:3025:3025 \
//!     -e GREENMAIL_OPTS='-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=test:secret@example.com' \
//!     greenmail/standalone:2.1.2
//! then run:
//!   BRICK_TEST_SERVER=1 cargo test -p brick-core --test live_server -- --nocapture

use brick_core::imap::{self, Credential};
use brick_core::model::{Account, AuthKind, Security};
use brick_core::secrets::MemoryStore;
use brick_core::Engine;
use serde_json::{json, Value};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn call(e: &Engine, method: &str, params: Value) -> Value {
    let out: Value = serde_json::from_str(&e.call(&json!({ "method": method, "params": params }).to_string())).unwrap();
    assert!(out.get("error").is_none(), "{method} failed: {out}");
    out["ok"].clone()
}

fn wait_event(rx: &mpsc::Receiver<Value>, kind: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let ev = rx.recv_timeout(left).unwrap_or_else(|_| panic!("timed out waiting for {kind}"));
        let t = ev["type"].as_str().unwrap_or("");
        assert!(!t.ends_with("error"), "unexpected error event: {ev}");
        if t == kind {
            return ev;
        }
    }
}

fn wait_until<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn folder(e: &Engine, account_id: i64, role: &str) -> Option<Value> {
    call(e, "folders.list", json!({ "account_id": account_id }))
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["role"] == role)
        .cloned()
}

#[test]
fn add_account_send_sync_read_flag_move() {
    if std::env::var("BRICK_TEST_SERVER").is_err() {
        eprintln!("skipped: set BRICK_TEST_SERVER=1 with a local server running");
        return;
    }

    // The server starts with only an inbox; make the folders a real account has.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let acc = Account {
            id: 0,
            email: "test@example.com".into(),
            name: "Test".into(),
            provider: "custom".into(),
            auth: AuthKind::Password,
            username: "test".into(),
            imap_host: "localhost".into(),
            imap_port: 3143,
            imap_security: Security::Plain,
            smtp_host: "localhost".into(),
            smtp_port: 3025,
            smtp_security: Security::Plain,
        };
        let mut s = imap::connect(&acc, &Credential::Password("secret".into()), std::time::Duration::from_secs(20)).await.unwrap();
        for f in ["Trash", "Sent", "Archive"] {
            let _ = s.create(f).await;
        }
        s.logout().await.unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::channel::<Value>();
    let tx = std::sync::Mutex::new(tx);
    let emit = Arc::new(move |json: String| {
        let _ = tx.lock().unwrap().send(serde_json::from_str(&json).unwrap());
    });
    let e = Engine::open_with(dir.path(), Arc::new(MemoryStore::default()), emit).unwrap();

    // A wrong password is rejected and nothing is saved.
    let bad = json!({
        "provider": "custom", "email": "test@example.com", "name": "Test", "username": "test",
        "password": "wrong", "imap_host": "localhost", "imap_port": 3143, "imap_security": "plain",
        "smtp_host": "localhost", "smtp_port": 3025, "smtp_security": "plain",
    });
    call(&e, "accounts.add", bad.clone());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ev = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("account_error");
        if ev["type"] == "account_error" {
            break;
        }
    }
    assert_eq!(call(&e, "accounts.list", json!({})).as_array().unwrap().len(), 0);

    let mut good = bad;
    good["password"] = json!("secret");
    call(&e, "accounts.add", good);
    let account_id = wait_event(&rx, "account_added")["account_id"].as_i64().unwrap();

    // Send ourselves two messages.
    for (i, subject) in ["Hello from Brick", "Second message"].iter().enumerate() {
        call(
            &e,
            "send",
            json!({ "account_id": account_id, "to": "Test <test@example.com>", "subject": subject,
                    "text": format!("Body number {i} with a zebra in it."), "token": format!("t{i}") }),
        );
        assert_eq!(wait_event(&rx, "send_done")["token"], format!("t{i}"));
    }

    // They arrive in the inbox after a sync.
    let inbox = wait_until("inbox with two messages", || {
        call(&e, "sync.now", json!({}));
        folder(&e, account_id, "inbox").filter(|f| f["total"] == 2)
    });
    let inbox_id = inbox["id"].as_i64().unwrap();
    assert_eq!(inbox["unread"], 2);

    let list = call(&e, "messages.list", json!({ "folder_id": inbox_id }));
    let rows = list["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let first = rows.iter().find(|r| r["subject"] == "Hello from Brick").expect("subject parsed").clone();
    assert_eq!(first["from_addr"], "test@example.com");
    assert_eq!(first["from_name"], "Test");
    assert_eq!(first["seen"], false);
    let id = first["id"].as_i64().unwrap();

    // A copy was filed in Sent, because this provider does not do it itself.
    wait_until("sent copy", || {
        call(&e, "sync.now", json!({}));
        folder(&e, account_id, "sent").filter(|f| f["total"] == 2)
    });

    // The body is fetched (or was prefetched) and rendered.
    let body = wait_until("message body", || {
        let got = call(&e, "messages.get", json!({ "id": id }));
        Some(got["body"].clone()).filter(|b| !b.is_null())
    });
    assert!(body["text"].as_str().unwrap().contains("zebra"));
    assert!(body["html"].as_str().unwrap().contains("Content-Security-Policy"));

    // Full-text search covers bodies once downloaded.
    wait_until("search hit", || {
        Some(call(&e, "messages.search", json!({ "query": "zebr" }))).filter(|r| !r["rows"].as_array().unwrap().is_empty())
    });

    // Mark read: instant locally, and it survives a round trip through the server.
    call(&e, "messages.set_flags", json!({ "ids": [id], "seen": true, "flagged": true }));
    assert_eq!(folder(&e, account_id, "inbox").unwrap()["unread"], 1);
    std::thread::sleep(Duration::from_millis(500));
    call(&e, "sync.now", json!({}));
    wait_event(&rx, "sync_finished");
    let row = &call(&e, "messages.get", json!({ "id": id }))["message"];
    assert_eq!((row["seen"].clone(), row["flagged"].clone()), (json!(true), json!(true)));

    // Reply threads onto the original.
    call(&e, "send", json!({ "account_id": account_id, "to": "test@example.com", "subject": "Re: Hello from Brick",
                             "text": "reply", "reply_to_id": id, "token": "r" }));
    wait_event(&rx, "send_done");
    wait_until("answered flag", || {
        Some(call(&e, "messages.get", json!({ "id": id }))["message"]["answered"].clone()).filter(|v| *v == json!(true))
    });

    // Move to Trash: gone from the inbox immediately, present in Trash after sync.
    call(&e, "messages.move", json!({ "ids": [id], "to": "trash" }));
    let now = call(&e, "messages.list", json!({ "folder_id": inbox_id }));
    assert!(now["rows"].as_array().unwrap().iter().all(|r| r["id"] != id));
    let trash = wait_until("message in trash", || {
        call(&e, "sync.now", json!({}));
        folder(&e, account_id, "trash").filter(|f| f["total"] == 1)
    });
    let trashed = call(&e, "messages.list", json!({ "folder_id": trash["id"] }));
    assert_eq!(trashed["rows"][0]["subject"], "Hello from Brick");
    assert_eq!(trashed["rows"][0]["seen"], true);

    // Deleting from Trash removes it for good.
    let trashed_id = trashed["rows"][0]["id"].as_i64().unwrap();
    call(&e, "messages.move", json!({ "ids": [trashed_id], "to": "trash" }));
    std::thread::sleep(Duration::from_millis(500));
    wait_until("empty trash", || {
        call(&e, "sync.now", json!({}));
        folder(&e, account_id, "trash").filter(|f| f["total"] == 0)
    });

    // The reply arrived and the inbox reflects the server.
    wait_until("reply in inbox", || {
        call(&e, "sync.now", json!({}));
        folder(&e, account_id, "inbox").filter(|f| f["total"] == 2)
    });

    call(&e, "accounts.remove", json!({ "id": account_id }));
    assert!(call(&e, "accounts.list", json!({})).as_array().unwrap().is_empty());
}
