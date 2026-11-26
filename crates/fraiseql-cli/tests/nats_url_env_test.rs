#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! The NATS URL of `[observers]`, through the real binary.
//!
//! `[observers]` names its URL either as written (`nats_url`) or by the environment variable
//! holding it (`nats_url_env`), which the server reads at startup: the compiled schema
//! carries the variable's name, never its value. A written URL with user info on any server
//! it lists is refused at compile, naming the key and pointing at `nats_url_env`, from
//! fraiseql.toml and from a schema document's `observers_config` alike.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// A value no output may show.
const PASSWORD: &str = "pw-NATS-SENTINEL";

fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    })
}

/// The outcome of one compile: whether it succeeded, its output, and the compiled schema.
struct Run {
    ok:     bool,
    log:    String,
    schema: Option<Value>,
}

/// `fraiseql compile fraiseql.toml --types types.json`, with `observers_toml` appended to
/// the TOML and `document_observers` as the document's `observers_config`.
fn compile(observers_toml: &str, document_observers: Option<Value>) -> Run {
    let dir = TempDir::new().unwrap();
    let mut doc = document();
    if let Some(observers) = document_observers {
        doc["observers_config"] = observers;
    }
    fs::write(dir.path().join("types.json"), doc.to_string()).unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n{observers_toml}\n"),
    )
    .unwrap();
    let out = dir.path().join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir.path())
        .args(["compile", "fraiseql.toml", "--types", "types.json", "-o"])
        .arg(&out)
        .output()
        .unwrap();
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = fs::read_to_string(&out).ok().map(|s| serde_json::from_str(&s).unwrap());
    Run {
        ok: output.status.success(),
        log,
        schema: compiled,
    }
}

fn observers(key_and_value: &str) -> String {
    format!("[observers]\nenabled = true\nbackend = \"nats\"\n{key_and_value}")
}

/// URLs with user info, on the only server or on one of several.
fn urls_with_user_info() -> [String; 3] {
    [
        format!("nats://app:{PASSWORD}@nats:4222"),
        format!("tls://{PASSWORD}@nats:4443"),
        format!("tls://nats-1:4443,tls://app:{PASSWORD}@nats-2:4443"),
    ]
}

#[test]
fn a_nats_url_holding_user_info_is_refused_naming_the_key_not_the_credential() {
    for url in urls_with_user_info() {
        let run = compile(&observers(&format!("nats_url = \"{url}\"")), None);
        assert!(!run.ok, "{url}: compiled\n{}", run.log);
        assert!(run.log.contains("[observers] nats_url"), "{url}: the key is named\n{}", run.log);
        assert!(run.log.contains("nats_url_env"), "{url}: points at nats_url_env\n{}", run.log);
        assert!(!run.log.contains(PASSWORD), "{url}: the credential is shown\n{}", run.log);
    }
}

#[test]
fn a_nats_url_with_user_info_in_a_schema_document_is_refused_too() {
    for url in urls_with_user_info() {
        let run = compile("", Some(json!({"enabled": true, "backend": "nats", "nats_url": url})));
        assert!(!run.ok, "{url}: compiled\n{}", run.log);
        assert!(run.log.contains("[observers] nats_url"), "{url}\n{}", run.log);
        assert!(!run.log.contains(PASSWORD), "{url}: the credential is shown\n{}", run.log);
    }
}

#[test]
fn nats_url_env_compiles_the_variable_name_only() {
    let run = compile(&observers("nats_url_env = \"OBSERVERS_NATS_URL\""), None);
    assert!(run.ok, "nats_url_env compiled\n{}", run.log);
    let observers = &run.schema.unwrap()["observers_config"];
    assert_eq!(observers["nats_url_env"], "OBSERVERS_NATS_URL", "{observers}");
    assert!(observers.get("nats_url").is_none_or(Value::is_null), "{observers}");
}

#[test]
fn a_plain_nats_url_still_compiles_as_written() {
    let run = compile(&observers("nats_url = \"tls://nats:4443\""), None);
    assert!(run.ok, "a URL with no user info compiled\n{}", run.log);
    let observers = &run.schema.unwrap()["observers_config"];
    assert_eq!(observers["nats_url"], "tls://nats:4443", "{observers}");
}

#[test]
fn both_keys_or_a_url_in_the_variable_name_are_refused() {
    for keys in [
        "nats_url = \"tls://nats:4443\"\nnats_url_env = \"OBSERVERS_NATS_URL\"",
        "nats_url_env = \"tls://nats:4443\"",
    ] {
        let run = compile(&observers(keys), None);
        assert!(!run.ok, "{keys}: compiled\n{}", run.log);
        assert!(run.log.contains("nats_url_env"), "{keys}\n{}", run.log);
    }
}
