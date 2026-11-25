#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
//! `[functions] shared_dir`: carried into the compiled section, and checked at compile time.
//!
//! The checks run when `module_dir` exists, as module presence does: the compiler is then
//! looking at the real layout. Each refusal test is built from the exact layout it must
//! reject and asserts the reason, so an unrelated refusal cannot pass it.
//!
//! **Execution engine:** in-memory · **Infrastructure:** none · **Parallelism:** safe

use std::path::Path;

use fraiseql_cli::schema::{
    CompiledArtifact, ConvertOptions, SchemaConverter, intermediate::IntermediateSchema,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A project tree: `functions/notify.ts` and `functions/shared/`.
fn project() -> TempDir {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("functions/shared")).unwrap();
    std::fs::write(root.path().join("functions/notify.ts"), "export default () => 1;\n").unwrap();
    root
}

/// One mutation and one Deno function on it, with the given `functions_config`.
fn corpus(functions_config: Value) -> Value {
    let mut corpus = json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "mutations": [{
            "name": "updateOrder",
            "return_type": "Order",
            "sql_source": "fn_update_order",
            "operation": "update",
            "invalidates_views": ["v_order"],
            "arguments": []
        }],
        "functions": [{
            "name": "notify",
            "trigger": "after:mutation:Order:update",
            "runtime": "Deno"
        }],
    });
    // Assigned rather than interpolated: `json!` reads an interpolated value by
    // reference, and a by-value parameter it never consumes is a clippy error.
    corpus["functions_config"] = functions_config;
    corpus
}

fn config(root: &Path, shared_dir: Option<&str>) -> Value {
    let mut config = json!({"module_dir": root.join("functions").to_str().unwrap()});
    if let Some(shared_dir) = shared_dir {
        config["shared_dir"] = json!(root.join(shared_dir).to_str().unwrap());
    }
    config
}

fn compile(corpus: Value) -> anyhow::Result<CompiledArtifact> {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
}

fn refusal(corpus: Value) -> String {
    match compile(corpus) {
        Ok(_) => panic!("this layout must not compile"),
        Err(error) => format!("{error:#}"),
    }
}

fn write_deno_json(root: &Path, text: &str) {
    std::fs::write(root.join("functions/deno.json"), text).unwrap();
}

// ── Carried ──────────────────────────────────────────────────────────────────

#[test]
fn a_declared_shared_dir_reaches_the_compiled_section() {
    let root = project();
    let artifact = compile(corpus(config(root.path(), Some("functions/shared")))).unwrap();
    assert_eq!(
        artifact.functions.unwrap().shared_dir,
        Some(root.path().join("functions/shared"))
    );
}

#[test]
fn an_undeclared_shared_dir_is_absent_from_the_compiled_section() {
    let root = project();
    let artifact = compile(corpus(config(root.path(), None))).unwrap();
    let functions = artifact.functions.unwrap();
    assert_eq!(functions.shared_dir, None);
    assert!(serde_json::to_value(&functions).unwrap().get("shared_dir").is_none());
}

/// A `[functions]` table that only sets `shared_dir` still has nothing to apply to
/// without a function or a source.
#[test]
fn a_shared_dir_with_no_function_is_refused() {
    let mut corpus = corpus(json!({"shared_dir": "functions/shared"}));
    corpus["functions"] = json!([]);
    assert!(refusal(corpus).contains("declares no function"));
}

// ── Layout ───────────────────────────────────────────────────────────────────

#[test]
fn a_shared_dir_that_does_not_exist_is_refused() {
    let root = project();
    let message = refusal(corpus(config(root.path(), Some("functions/lib"))));
    assert!(
        message.contains("shared_dir") && message.contains("not a directory"),
        "{message}"
    );
}

#[test]
fn module_dir_as_shared_dir_is_refused() {
    let root = project();
    let message = refusal(corpus(config(root.path(), Some("functions"))));
    assert!(message.contains("is or contains `module_dir`"), "{message}");
}

/// The compiler is not looking at the deployment layout, so it cannot check it; the
/// server's loader does at boot.
#[test]
fn an_absent_module_dir_leaves_shared_dir_unchecked() {
    let corpus = corpus(json!({
        "module_dir": "no/such/functions",
        "shared_dir": "no/such/functions/shared",
    }));
    compile(corpus).unwrap();
}

// ── deno.json, for editors ───────────────────────────────────────────────────

#[test]
fn a_deno_json_naming_shared_dir_compiles() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "./shared/"}}"#);
    compile(corpus(config(root.path(), Some("functions/shared")))).unwrap();
}

#[test]
fn no_deno_json_compiles() {
    let root = project();
    compile(corpus(config(root.path(), Some("functions/shared")))).unwrap();
}

#[test]
fn a_deno_json_naming_another_directory_is_refused_naming_both() {
    let root = project();
    std::fs::create_dir(root.path().join("functions/lib")).unwrap();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "./lib/"}}"#);
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(
        message.contains("deno.json")
            && message.contains("./lib/")
            && message.contains(root.path().join("functions/shared").to_str().unwrap()),
        "the refusal must name the deno.json entry and shared_dir; got: {message}"
    );
}

#[test]
fn a_deno_json_naming_a_missing_directory_is_refused() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "./gone/"}}"#);
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(message.contains("not a directory relative to module_dir"), "{message}");
}

#[test]
fn a_deno_json_mapping_shared_to_a_url_is_refused() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "https://example.com/shared/"}}"#);
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(message.contains("https://example.com/shared/"), "{message}");
}

#[test]
fn a_deno_json_without_the_shared_entry_is_refused() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"zod": "npm:zod"}}"#);
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(message.contains("no `imports` entry for `@shared/`"), "{message}");
}

#[test]
fn a_deno_json_mapping_shared_without_a_shared_dir_is_refused() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "./shared/"}}"#);
    let message = refusal(corpus(config(root.path(), None)));
    assert!(message.contains("`[functions] shared_dir` is not set"), "{message}");
}

#[test]
fn a_deno_json_with_no_shared_entry_and_no_shared_dir_compiles() {
    let root = project();
    write_deno_json(root.path(), r#"{"compilerOptions": {"strict": true}}"#);
    compile(corpus(config(root.path(), None))).unwrap();
}

#[test]
fn a_deno_json_that_is_not_json_is_refused() {
    let root = project();
    write_deno_json(root.path(), "{ // a comment\n}");
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(message.contains("not valid JSON"), "{message}");
}

// ── From fraiseql.toml, through the real binary ──────────────────────────────

/// Compile `schema.json` beside `fraiseql.toml` with the binary; the artifact's
/// `functions` section on success, the log on refusal.
fn compile_project(toml: &str, functions_config: Option<Value>) -> Result<Value, String> {
    let dir = TempDir::new().unwrap();
    let mut schema = corpus(Value::Null);
    match functions_config {
        Some(config) => schema["functions_config"] = config,
        None => {
            schema.as_object_mut().unwrap().remove("functions_config");
        },
    }
    std::fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    std::fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    let out = dir.path().join("schema.compiled.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    if !result.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let compiled: Value = serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    Ok(compiled["functions"].clone())
}

const TOML: &str = "[project]\nname = \"app\"\n\n[functions]\nmodule_dir = \"functions\"\n\
                    shared_dir = \"functions/shared\"\n";

#[test]
fn the_project_configs_shared_dir_reaches_the_artifact() {
    let functions = compile_project(TOML, None).unwrap();
    assert_eq!(functions["shared_dir"], "functions/shared", "{functions}");
}

#[test]
fn a_schema_document_disagreeing_on_shared_dir_is_refused_naming_the_key() {
    let copy = json!({"module_dir": "functions", "shared_dir": "lib"});
    let log = compile_project(TOML, Some(copy)).unwrap_err();
    assert!(log.contains("disagree on `shared_dir`"), "{log}");
}

/// `shared_dir` alone configures the table: it is not dropped as an unconfigured one.
#[test]
fn a_table_setting_only_shared_dir_reaches_the_artifact() {
    let toml = "[project]\nname = \"app\"\n\n[functions]\nshared_dir = \"functions/shared\"\n";
    let functions = compile_project(toml, None).unwrap();
    assert_eq!(functions["shared_dir"], "functions/shared", "{functions}");
}

/// Deno refuses a prefix entry whose value lacks the trailing `/`, so editors would
/// resolve nothing through it, even though the directory is the right one.
#[test]
fn a_deno_json_entry_without_the_trailing_slash_is_refused() {
    let root = project();
    write_deno_json(root.path(), r#"{"imports": {"@shared/": "./shared"}}"#);
    let message = refusal(corpus(config(root.path(), Some("functions/shared"))));
    assert!(message.contains("not a directory relative to module_dir"), "{message}");
}
