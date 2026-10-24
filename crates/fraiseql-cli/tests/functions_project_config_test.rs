#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! `[functions]` in the `fraiseql.toml` beside an SDK `schema.json` reaches the compiled
//! artifact, through the real binary: `module_dir` and `dlq_store` are deployment settings,
//! so they live in the project config, and a schema document carrying a different copy is
//! refused rather than one of them silently winning.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

fn project(toml: &str, functions_config: Option<Value>) -> TempDir {
    let dir = TempDir::new().unwrap();
    let mut schema = json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": [{"name": "updateOrder", "return_type": "Order",
                       "sql_source": "fn_update_order", "operation": "update",
                       "invalidates_views": ["v_order"], "arguments": []}],
        "functions": [{"name": "notify", "trigger": "after:mutation:Order:update",
                       "runtime": "Deno"}]
    });
    if let Some(config) = functions_config {
        schema["functions_config"] = config;
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

fn compiled_functions(dir: &TempDir) -> Value {
    let (ok, log) = run(dir);
    assert!(ok, "compile failed: {log}");
    let json = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    serde_json::from_str::<Value>(&json).unwrap()["functions"].clone()
}

const SETTINGS: &str = "[project]\nname = \"app\"\n\n[functions]\n\
                        module_dir = \"services/api/functions\"\ndlq_store = \"postgres\"\n";

#[test]
fn a_json_schema_takes_the_project_configs_functions_settings() {
    let functions = compiled_functions(&project(SETTINGS, None));
    assert_eq!(functions["module_dir"], "services/api/functions", "{functions}");
    assert_eq!(functions["dlq_store"], "postgres", "{functions}");
}

#[test]
fn a_schema_document_agreeing_with_the_project_config_compiles() {
    let copy = json!({"module_dir": "services/api/functions", "dlq_store": "postgres"});
    let functions = compiled_functions(&project(SETTINGS, Some(copy)));
    assert_eq!(functions["module_dir"], "services/api/functions", "{functions}");
}

#[test]
fn a_schema_document_disagreeing_with_the_project_config_is_refused() {
    let copy = json!({"module_dir": "functions", "dlq_store": "postgres"});
    let (ok, log) = run(&project(SETTINGS, Some(copy)));
    assert!(!ok, "a disagreeing copy compiled: {log}");
    for needle in [
        "fraiseql.toml [functions]",
        "schema.json functions_config",
        "disagree on `module_dir`",
    ] {
        assert!(log.contains(needle), "the refusal does not name `{needle}`: {log}");
    }
}

#[test]
fn a_schema_document_declaring_a_setting_the_project_config_omits_is_refused() {
    let copy = json!({"module_dir": "fns", "dlq_store": "memory"});
    let toml = "[project]\nname = \"app\"\n\n[functions]\nmodule_dir = \"fns\"\n";
    let (ok, log) = run(&project(toml, Some(copy)));
    assert!(!ok, "a disagreeing copy compiled: {log}");
    assert!(
        log.contains("disagree on `dlq_store`"),
        "the refusal does not name the key: {log}"
    );
}

#[test]
fn without_a_functions_table_the_schema_documents_copy_still_applies() {
    let copy = json!({"module_dir": "fns"});
    let functions = compiled_functions(&project("[project]\nname = \"app\"\n", Some(copy)));
    assert_eq!(functions["module_dir"], "fns", "{functions}");
}
