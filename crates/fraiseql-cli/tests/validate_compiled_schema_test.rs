#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
//! `fraiseql validate` checks a compiled schema, through the real binary: its help says
//! so, the `schema.compiled.json` that `fraiseql compile` writes validates, and an
//! authoring `schema.json` is refused with a pointer to `fraiseql compile --check`, not a
//! serde error about a key its author never wrote.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::json;
use tempfile::TempDir;

fn authoring_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let schema = json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}]
    });
    fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    dir
}

fn cli(dir: &TempDir, args: &[&str]) -> (bool, String) {
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(args)
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

fn assert_refused_as_authoring(ok: bool, log: &str) {
    assert!(!ok, "validate accepted an authoring schema: {log}");
    for needle in [
        "`schema.json` is an authoring schema",
        "fraiseql validate checks a compiled schema",
        "fraiseql compile schema.json --check",
    ] {
        assert!(log.contains(needle), "the refusal does not say `{needle}`: {log}");
    }
}

#[test]
fn an_authoring_schema_the_compiler_accepts_is_refused_with_a_pointer_to_compile_check() {
    let dir = authoring_project();
    let (compiled, log) = cli(&dir, &["compile", "schema.json", "--check"]);
    assert!(compiled, "the fixture does not compile: {log}");

    let (ok, log) = cli(&dir, &["validate", "schema.json"]);
    assert_refused_as_authoring(ok, &log);
}

#[test]
fn an_authoring_schema_is_refused_before_the_database_is_reached() {
    let dir = authoring_project();
    let (ok, log) = cli(
        &dir,
        &[
            "validate",
            "schema.json",
            "--against-db",
            "postgresql://nobody@127.0.0.1:1/none",
        ],
    );
    assert_refused_as_authoring(ok, &log);
}

#[test]
fn the_schema_compile_writes_validates() {
    let dir = authoring_project();
    let (compiled, log) = cli(&dir, &["compile", "schema.json", "-o", "schema.compiled.json"]);
    assert!(compiled, "the fixture does not compile: {log}");

    let (ok, log) = cli(&dir, &["validate", "schema.compiled.json"]);
    assert!(ok, "the compiled schema did not validate: {log}");
}

#[test]
fn a_file_that_is_neither_schema_is_refused_as_not_a_compiled_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("other.json"), r#"{"types": 7}"#).unwrap();

    let (ok, log) = cli(&dir, &["validate", "other.json"]);
    assert!(!ok, "validate accepted a malformed file: {log}");
    assert!(log.contains("`other.json` is not a valid compiled schema"), "{log}");
    assert!(
        !log.contains("authoring schema"),
        "a malformed file was called authoring: {log}"
    );
}

#[test]
fn the_help_says_it_takes_a_compiled_schema() {
    let dir = TempDir::new().unwrap();
    let (ok, help) = cli(&dir, &["validate", "--help"]);
    assert!(ok, "{help}");
    assert!(help.contains("fraiseql validate schema.compiled.json"), "{help}");
    assert!(
        !help.contains("fraiseql validate schema.json"),
        "the help still names schema.json: {help}"
    );
}
