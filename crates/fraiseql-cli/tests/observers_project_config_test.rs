#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! `[observers]` in the `fraiseql.toml` beside an SDK `schema.json` reaches the compiled
//! artifact, through the real binary: the backend and its connection URLs are deployment
//! settings, so they live in the project config, and a schema document carrying a different
//! copy is refused rather than one of them silently winning.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

fn project(toml: &str, observers_config: Option<Value>) -> TempDir {
    let dir = TempDir::new().unwrap();
    let mut schema = json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}]
    });
    if let Some(config) = observers_config {
        schema["observers_config"] = config;
    }
    fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    dir
}

fn run(dir: &TempDir) -> (bool, String) {
    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    (result.status.success(), log)
}

fn compiled_observers(dir: &TempDir) -> Value {
    let (ok, log) = run(dir);
    assert!(ok, "compile failed: {log}");
    let json = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    serde_json::from_str::<Value>(&json).unwrap()["observers_config"].clone()
}

const SETTINGS: &str = "[project]\nname = \"app\"\n\n[observers]\nenabled = true\n\
                        backend = \"nats\"\nnats_url = \"nats://bus:4222\"\n";

#[test]
fn a_json_schema_takes_the_project_configs_observers_settings() {
    let observers = compiled_observers(&project(SETTINGS, None));
    assert_eq!(
        observers,
        json!({"enabled": true, "backend": "nats", "nats_url": "nats://bus:4222"}),
        "{observers}"
    );
}

#[test]
fn a_schema_document_agreeing_with_the_project_config_compiles() {
    let copy = json!({"enabled": true, "backend": "nats", "nats_url": "nats://bus:4222"});
    let observers = compiled_observers(&project(SETTINGS, Some(copy)));
    assert_eq!(observers["nats_url"], "nats://bus:4222", "{observers}");
}

#[test]
fn a_schema_document_disagreeing_with_the_project_config_is_refused() {
    let copy = json!({"enabled": true, "backend": "postgres", "nats_url": "nats://bus:4222"});
    let (ok, log) = run(&project(SETTINGS, Some(copy)));
    assert!(!ok, "a disagreeing copy compiled: {log}");
    for needle in [
        "fraiseql.toml [observers]",
        "schema.json observers_config",
        "disagree on `backend`",
    ] {
        assert!(log.contains(needle), "the refusal does not name `{needle}`: {log}");
    }
}

#[test]
fn a_schema_document_declaring_a_setting_the_project_config_omits_is_refused() {
    let toml = "[project]\nname = \"app\"\n\n[observers]\nenabled = true\nbackend = \"nats\"\n";
    let copy = json!({"enabled": true, "backend": "nats", "nats_url": "nats://bus:4222"});
    let (ok, log) = run(&project(toml, Some(copy)));
    assert!(!ok, "a disagreeing copy compiled: {log}");
    assert!(
        log.contains("disagree on `nats_url`"),
        "the refusal does not name the key: {log}"
    );
}

#[test]
fn without_an_observers_table_the_schema_documents_copy_still_applies() {
    let copy = json!({"enabled": true, "backend": "nats", "nats_url": "nats://cache:4222"});
    let observers = compiled_observers(&project("[project]\nname = \"app\"\n", Some(copy)));
    assert_eq!(observers["nats_url"], "nats://cache:4222", "{observers}");
}

/// A bare table compiles the runtime's default transport, not one it does not have.
#[test]
fn a_bare_observers_table_compiles_the_postgres_backend() {
    let toml = "[project]\nname = \"app\"\n\n[observers]\nenabled = false\n";
    let observers = compiled_observers(&project(toml, None));
    assert_eq!(observers["backend"], "postgres", "{observers}");
}

/// The runtime has no Redis transport: `redis` and `redis_url` are refused at compile,
/// with what to write instead, in the project config and in a schema document alike.
#[test]
fn redis_is_refused_with_a_pointer_wherever_it_is_declared() {
    let table = "[project]\nname = \"app\"\n\n[observers]\nenabled = true\n";
    let cases = [
        ("backend in fraiseql.toml", format!("{table}backend = \"redis\"\n"), None),
        (
            "redis_url in fraiseql.toml",
            format!("{table}redis_url = \"redis://c:6379\"\n"),
            None,
        ),
        (
            "backend in schema.json",
            "[project]\nname = \"app\"\n".to_string(),
            Some(json!({"enabled": true, "backend": "redis"})),
        ),
        (
            "redis_url in schema.json",
            "[project]\nname = \"app\"\n".to_string(),
            Some(json!({"enabled": true, "redis_url": "redis://c:6379"})),
        ),
    ];
    for (case, toml, copy) in cases {
        let (ok, log) = run(&project(&toml, copy));
        assert!(!ok, "{case}: compiled: {log}");
        assert!(log.contains("no Redis transport"), "{case}: no pointer: {log}");
    }
}

#[test]
fn the_servers_runtime_sub_table_beside_a_json_schema_compiles_and_is_not_embedded() {
    let toml = format!("{SETTINGS}\n[observers.runtime]\nchannel_capacity = 512\n");
    let observers = compiled_observers(&project(&toml, None));
    assert!(observers.get("runtime").is_none(), "{observers}");
    assert_eq!(observers["backend"], "nats", "{observers}");
}

#[test]
fn a_handler_table_beside_a_json_schema_is_refused_by_name() {
    let toml = format!(
        "{SETTINGS}\n[[observers.handlers]]\nname = \"h\"\nevent = \"Order.created\"\n\
         action = \"webhook\"\n"
    );
    let (ok, log) = run(&project(&toml, None));
    assert!(!ok, "a compiled handler was accepted: {log}");
    assert!(
        log.contains("[[observers.handlers]] is not supported"),
        "the refusal does not name the block: {log}"
    );
}

#[test]
fn a_table_declaring_only_disabled_observers_still_owns_the_settings() {
    let copy = json!({"enabled": true, "backend": "postgres"});
    let toml = "[project]\nname = \"app\"\n\n[observers]\nenabled = false\n";
    let (ok, log) = run(&project(toml, Some(copy)));
    assert!(!ok, "the document's copy overrode the declared table: {log}");
    assert!(
        log.contains("disagree on `enabled`"),
        "the refusal does not name the key: {log}"
    );
}

#[test]
fn a_schema_document_with_another_nats_url_is_refused() {
    let copy = json!({"enabled": true, "backend": "nats", "nats_url": "nats://other:4222"});
    let (ok, log) = run(&project(SETTINGS, Some(copy)));
    assert!(!ok, "a disagreeing copy compiled: {log}");
    assert!(
        log.contains("disagree on `nats_url`"),
        "the refusal does not name the key: {log}"
    );
}

#[test]
fn an_agreeing_schema_document_declaring_handlers_is_still_refused() {
    let copy = json!({"enabled": true, "backend": "nats", "nats_url": "nats://bus:4222",
                      "handlers": [{"name": "h", "event": "Order.created", "action": "webhook"}]});
    let (ok, log) = run(&project(SETTINGS, Some(copy)));
    assert!(!ok, "a document's compiled handler was accepted: {log}");
    assert!(
        log.contains("observers_config declares 1 handler(s)"),
        "the refusal does not name the handlers: {log}"
    );
}
