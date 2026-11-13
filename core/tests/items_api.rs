//! The to-do/calendar item API as front ends use it. No network needed.

use brick_core::secrets::MemoryStore;
use brick_core::Engine;
use serde_json::{json, Value};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

fn raw(e: &Engine, method: &str, params: Value) -> Value {
    serde_json::from_str(&e.call(&json!({ "method": method, "params": params }).to_string())).unwrap()
}

fn call(e: &Engine, method: &str, params: Value) -> Value {
    let out = raw(e, method, params);
    assert!(out.get("error").is_none(), "{method} failed: {out}");
    out["ok"].clone()
}

#[test]
fn items_round_trip_through_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::channel::<Value>();
    let tx = Mutex::new(tx);
    let emit = Arc::new(move |json: String| {
        let _ = tx.lock().unwrap().send(serde_json::from_str(&json).unwrap());
    });
    let e = Engine::open_with(dir.path(), Arc::new(MemoryStore::default()), emit).unwrap();

    assert_eq!(call(&e, "items.list", json!({})), json!([]));

    // An undated to-do and a timed calendar entry are the same kind of thing.
    let todo = call(&e, "items.save", json!({ "item": { "title": "Buy stamps" } }));
    let event = call(
        &e,
        "items.save",
        json!({ "item": { "title": "Dentist", "date": "2026-12-01", "start_time": "09:30", "end_time": "10:00",
                          "location": "High Street" } }),
    );
    assert_eq!(todo["date"], Value::Null);
    assert_eq!(event["start_time"], "09:30");
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap()["type"], "items_changed");

    let listed = call(&e, "items.list", json!({}));
    assert_eq!(listed[0]["title"], "Dentist", "dated items come first");
    assert_eq!(listed[1]["title"], "Buy stamps");

    // Editing: clearing the date turns the calendar entry back into a plain to-do.
    let mut edited = event.clone();
    edited["date"] = Value::Null;
    let edited = call(&e, "items.save", json!({ "item": edited }));
    assert_eq!(edited["id"], event["id"]);
    assert_eq!(edited["start_time"], Value::Null, "times go when the day goes");

    call(&e, "items.set_done", json!({ "id": todo["id"], "done": true }));
    let listed = call(&e, "items.list", json!({}));
    assert!(listed.as_array().unwrap().iter().any(|i| i["id"] == todo["id"] && i["done"] == true));

    assert!(raw(&e, "items.save", json!({ "item": { "title": "" } })).get("error").is_some());

    // An item not made from an email has no source to open.
    assert_eq!(call(&e, "items.source", json!({ "id": todo["id"] })), Value::Null);

    call(&e, "items.delete", json!({ "id": todo["id"] }));
    assert_eq!(call(&e, "items.list", json!({})).as_array().unwrap().len(), 1);

    // The API key lives in the secret store, never in settings.
    let status = call(&e, "claude.status", json!({}));
    assert_eq!(status["configured"], false);
    assert_eq!(status["model"], "claude-opus-5-5");
    call(&e, "claude.set_key", json!({ "key": "  sk-ant-test  " }));
    assert_eq!(call(&e, "claude.status", json!({}))["configured"], true);
    assert_eq!(call(&e, "settings.get", json!({ "key": "anthropic-api-key" })), Value::Null);
    call(&e, "claude.set_key", json!({ "key": "" }));
    assert_eq!(call(&e, "claude.status", json!({}))["configured"], false);

    call(&e, "settings.set", json!({ "key": "claude_model", "value": "claude-haiku-5-5" }));
    assert_eq!(call(&e, "claude.status", json!({}))["model"], "claude-haiku-5-5");

    // Extracting from a message that doesn't exist fails straight away.
    assert!(raw(&e, "items.extract", json!({ "id": 999, "token": "t" })).get("error").is_some());
}
