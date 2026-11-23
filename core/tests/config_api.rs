//! config.toml as front ends see it through settings.get / settings.set.

use brick_core::secrets::MemoryStore;
use brick_core::store::Store;
use brick_core::Engine;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

fn open(dir: &Path) -> Engine {
    Engine::open_with(dir, Arc::new(MemoryStore::default()), Arc::new(|_| {})).unwrap()
}

fn raw(e: &Engine, method: &str, params: Value) -> Value {
    serde_json::from_str(&e.call(&json!({ "method": method, "params": params }).to_string())).unwrap()
}

fn call(e: &Engine, method: &str, params: Value) -> Value {
    let out = raw(e, method, params);
    assert!(out.get("error").is_none(), "{method} failed: {out}");
    out["ok"].clone()
}

#[test]
fn settings_are_kept_in_config_toml() {
    let dir = tempfile::tempdir().unwrap();
    let e = open(dir.path());
    let file = dir.path().join("config.toml");
    assert!(file.exists(), "template written on first open");

    let status = call(&e, "config.status", json!({}));
    assert_eq!(status["path"], file.display().to_string());
    assert_eq!(status["error"], Value::Null);

    // The flat names front ends have always used still work.
    call(&e, "settings.set", json!({ "key": "google_client_id", "value": "abc.apps.example" }));
    call(&e, "settings.set", json!({ "key": "aliases_1", "value": "a@x.example, b@x.example" }));
    call(&e, "settings.set", json!({ "key": "sync.poll_interval_secs", "value": "600" }));
    call(&e, "settings.set", json!({ "key": "ui.threaded", "value": "false" }));
    assert_eq!(call(&e, "settings.get", json!({ "key": "google_client_id" })), "abc.apps.example");
    assert_eq!(call(&e, "settings.get", json!({ "key": "aliases_1" })), "a@x.example\nb@x.example");
    assert_eq!(call(&e, "settings.get", json!({ "key": "ui.threaded" })), "false");
    assert_eq!(call(&e, "settings.get", json!({ "key": "claude_model" })), Value::Null);

    let text = std::fs::read_to_string(&file).unwrap();
    for want in ["[oauth.google]", "client_id = \"abc.apps.example\"", "[accounts.1]", "poll_interval_secs = 600", "threaded = false"] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }

    // Typos and bad values are refused.
    assert!(raw(&e, "settings.set", json!({ "key": "sync.pol_interval_secs", "value": "5" })).get("error").is_some());
    assert!(raw(&e, "settings.set", json!({ "key": "sync.poll_interval_secs", "value": "often" })).get("error").is_some());
    assert!(raw(&e, "settings.get", json!({ "key": "nonsense" })).get("error").is_some());

    // Survives a restart.
    drop(e);
    let e = open(dir.path());
    assert_eq!(call(&e, "settings.get", json!({ "key": "sync.poll_interval_secs" })), "600");
}

#[test]
fn settings_from_the_old_database_table_move_to_the_file() {
    let dir = tempfile::tempdir().unwrap();
    {
        let s = Store::open(&dir.path().join("brick.sqlite")).unwrap();
        s.set_setting("claude_model", "claude-sonnet-5-5").unwrap();
        s.set_setting("signature_2", "Cheers,\nA. Person").unwrap();
    }
    let e = open(dir.path());
    assert_eq!(call(&e, "claude.status", json!({}))["model"], "claude-sonnet-5-5");
    assert_eq!(call(&e, "settings.get", json!({ "key": "signature_2" })), "Cheers,\nA. Person");
    drop(e);
    let s = Store::open(&dir.path().join("brick.sqlite")).unwrap();
    assert!(s.legacy_settings().unwrap().is_empty(), "moved rows are removed from the database");
}
