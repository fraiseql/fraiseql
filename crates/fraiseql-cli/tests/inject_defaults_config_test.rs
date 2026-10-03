#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! #1384 — `[inject_defaults]` in `fraiseql.toml` reaches the compiled schema, through
//! the real binary, in both workflows: a JSON schema with a project config, and a TOML
//! schema carrying the section itself. And a schema document whose own block disagrees
//! with the config is refused rather than one of them silently winning.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use fraiseql_core::schema::CompiledSchema;
use tempfile::TempDir;

const DEFAULTS: &str = "[inject_defaults]\ntenant_id = \"jwt:tenant_id\"\n";

const TYPES_JSON: &str = r#"{
  "types": [{"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}]
}"#;

fn run(dir: &TempDir, args: &[&str]) -> (bool, String) {
    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .arg("compile")
        .args(args)
        .args(["--output", out.to_str().unwrap()])
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

fn compiled(dir: &TempDir, args: &[&str]) -> CompiledSchema {
    let (ok, log) = run(dir, args);
    assert!(ok, "compile failed: {log}");
    let json = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    CompiledSchema::from_json(&json, false).unwrap()
}

fn json_project(schema_extra: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("schema.json"),
        format!(
            r#"{{
  "types": [{{"name": "Order", "sql_source": "v_order",
             "fields": [{{"name": "id", "type": "ID", "nullable": false}}]}}],
  "queries": [{{"name": "orders", "return_type": "Order", "returns_list": true,
               "sql_source": "v_order"}}]{schema_extra}
}}"#
        ),
    )
    .unwrap();
    fs::write(dir.path().join("fraiseql.toml"), DEFAULTS).unwrap();
    dir
}

#[test]
fn a_json_schema_takes_the_project_configs_defaults() {
    let dir = json_project("");
    let schema = compiled(&dir, &["schema.json"]);
    let orders = schema.queries.iter().find(|q| q.name == "orders").unwrap();
    assert!(orders.inject_params.contains_key("tenant_id"), "{:?}", orders.inject_params);
}

#[test]
fn a_toml_schema_takes_its_own_section() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), TYPES_JSON).unwrap();
    fs::write(
        dir.path().join("schema.toml"),
        format!(
            "[schema]\nname = \"t\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n\n\
             [queries.orders]\nreturn_type = \"Order\"\nreturn_array = true\n\
             sql_source = \"v_order\"\n\n{DEFAULTS}"
        ),
    )
    .unwrap();
    let schema = compiled(&dir, &["schema.toml", "--types", "types.json"]);
    let orders = schema.queries.iter().find(|q| q.name == "orders").unwrap();
    assert!(orders.inject_params.contains_key("tenant_id"), "{:?}", orders.inject_params);
}

/// The SDK emits the document's block from the same config; a difference means one is
/// stale, and which one the author meant cannot be guessed.
#[test]
fn a_schema_document_that_disagrees_with_the_config_is_refused() {
    let dir = json_project(r#", "inject_defaults": {"base": {"tenant_id": "jwt:org_id"}}"#);
    let (ok, log) = run(&dir, &["schema.json"]);
    assert!(!ok, "a disagreeing copy must be refused: {log}");
    assert!(log.contains("differs"), "{log}");
}
