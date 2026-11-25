#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — fail-loud.
//! `fraiseql functions invoke` loads modules from where the server would.
//!
//! The server resolves `module_dir` and `shared_dir` through the environment
//! (`FRAISEQL_FUNCTIONS_MODULE_DIR`, `FRAISEQL_FUNCTIONS_SHARED_DIR`) before loading a
//! function. The harness applies the same overrides, through the same loader, so what
//! it runs and the hash it prints are what the server would run and log.
//!
//! **Execution engine:** V8 (in the spawned CLI child)
//! **Infrastructure:** none
//! **Parallelism:** safe (each test forks its own binary + temp dir)

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use fraiseql_functions::modules::{hash::function_hash, resolve::resolve_specifier};
use tempfile::TempDir;

const FUNCTION: &str = "import { tag } from \"@shared/tag.ts\";\nexport default () => tag;\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `<dir>/functions/marker.ts` importing `@shared/tag.ts`, which exports `tag`.
fn modules(dir: &Path, tag: &str) -> PathBuf {
    let module_dir = dir.join("functions");
    write(&module_dir.join("marker.ts"), FUNCTION);
    write(&module_dir.join("shared/tag.ts"), &tag_module(tag));
    module_dir
}

fn tag_module(tag: &str) -> String {
    format!("export const tag = {tag:?};\n")
}

/// Compile a project declaring `marker` against `module_dir`; return the artifact.
fn compile(dir: &TempDir, module_dir: &Path) -> PathBuf {
    let types = serde_json::json!({
        "version": "2.0.0",
        "types": [{
            "name": "Order", "sql_source": "v_order", "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "mutations": [{
            "name": "updateOrder", "return_type": "Order", "sql_source": "fn_update_order",
            "operation": "update", "invalidates_views": ["v_order"], "arguments": []
        }],
        "functions": [{
            "name": "marker", "trigger": "after:mutation:Order:update", "runtime": "Deno"
        }]
    });
    let types_path = dir.path().join("types.json");
    std::fs::write(&types_path, types.to_string()).unwrap();
    let toml_path = dir.path().join("fraiseql.toml");
    std::fs::write(
        &toml_path,
        format!(
            "[project]\nname = \"dirs\"\nversion = \"1.0.0\"\ndatabase_target = \"postgresql\"\n\n\
             [functions]\nmodule_dir = {:?}\nshared_dir = {:?}\n",
            module_dir.display().to_string(),
            module_dir.join("shared").display().to_string(),
        ),
    )
    .unwrap();
    let schema = dir.path().join("schema.compiled.json");
    let out = cli()
        .arg("compile")
        .arg(&toml_path)
        .arg("--types")
        .arg(&types_path)
        .arg("--output")
        .arg(&schema)
        .output()
        .unwrap();
    assert!(out.status.success(), "compile:\n{}", String::from_utf8_lossy(&out.stderr));
    schema
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Run `invoke marker --json` with `env` set; return the parsed report.
fn invoke(dir: &TempDir, schema: &Path, env: &[(&str, &Path)]) -> serde_json::Value {
    let payload = dir.path().join("payload.json");
    std::fs::write(&payload, r#"{"event_kind":"update","old":{"id":"1"},"new":{"id":"1"}}"#)
        .unwrap();
    let mut command = cli();
    command
        .args(["functions", "invoke", "marker", "--json", "--payload"])
        .arg(&payload)
        .arg("--schema")
        .arg(schema)
        .env_remove("FRAISEQL_FUNCTIONS_MODULE_DIR")
        .env_remove("FRAISEQL_FUNCTIONS_SHARED_DIR");
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(stdout.lines().last().unwrap()).unwrap()
}

fn expected_hash(tag: &str) -> String {
    let modules = BTreeMap::from([(resolve_specifier("@shared/tag.ts").unwrap(), tag_module(tag))]);
    function_hash(FUNCTION.as_bytes(), &modules)
}

#[test]
fn invoke_runs_the_compiled_dirs_and_prints_the_hash() {
    let dir = TempDir::new().unwrap();
    let schema = compile(&dir, &modules(dir.path(), "compiled"));
    let report = invoke(&dir, &schema, &[]);
    assert_eq!(report["result"], "compiled");
    assert_eq!(report["hash"], expected_hash("compiled"));
}

#[test]
fn invoke_loads_from_the_module_dir_the_environment_names() {
    let dir = TempDir::new().unwrap();
    let schema = compile(&dir, &modules(dir.path(), "compiled"));
    let deployed = modules(&dir.path().join("deployed"), "deployed");
    let report = invoke(
        &dir,
        &schema,
        &[
            ("FRAISEQL_FUNCTIONS_MODULE_DIR", &deployed),
            ("FRAISEQL_FUNCTIONS_SHARED_DIR", &deployed.join("shared")),
        ],
    );
    assert_eq!(report["result"], "deployed");
    assert_eq!(report["hash"], expected_hash("deployed"));
}

#[test]
fn invoke_loads_from_the_shared_dir_the_environment_names() {
    let dir = TempDir::new().unwrap();
    let schema = compile(&dir, &modules(dir.path(), "compiled"));
    let shared = dir.path().join("elsewhere");
    write(&shared.join("tag.ts"), &tag_module("elsewhere"));
    let report = invoke(&dir, &schema, &[("FRAISEQL_FUNCTIONS_SHARED_DIR", &shared)]);
    assert_eq!(report["result"], "elsewhere");
    assert_eq!(report["hash"], expected_hash("elsewhere"));
}

#[test]
fn invoke_prints_the_hash_in_its_human_report() {
    let dir = TempDir::new().unwrap();
    let schema = compile(&dir, &modules(dir.path(), "compiled"));
    let payload = dir.path().join("payload.json");
    std::fs::write(&payload, r#"{"event_kind":"update","old":{"id":"1"},"new":{"id":"1"}}"#)
        .unwrap();
    let out = cli()
        .args(["functions", "invoke", "marker", "--payload"])
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .env_remove("FRAISEQL_FUNCTIONS_MODULE_DIR")
        .env_remove("FRAISEQL_FUNCTIONS_SHARED_DIR")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&format!("hash: {}", expected_hash("compiled"))), "{stdout}");
}

/// The override alone, with no shared module involved: the server would run the
/// deployed `marker`, so the harness must too.
#[test]
fn invoke_honours_the_module_dir_override_for_a_function_without_imports() {
    let dir = TempDir::new().unwrap();
    let compiled = modules(dir.path(), "compiled");
    write(&compiled.join("marker.ts"), "export default () => \"compiled\";\n");
    let schema = compile(&dir, &compiled);
    let deployed = dir.path().join("deployed");
    write(&deployed.join("marker.ts"), "export default () => \"deployed\";\n");
    let report = invoke(&dir, &schema, &[("FRAISEQL_FUNCTIONS_MODULE_DIR", &deployed)]);
    assert_eq!(report["result"], "deployed");
}
