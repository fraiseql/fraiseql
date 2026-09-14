#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
//! #1325 — an authored function reaches the server that has to run it.
//!
//! The `functions` section of a compiled schema had a reader and no writer.
//! `fraiseql-server` has parsed it since #896 (`ExtendedCompiledSchema.functions`),
//! and the CLI's invoke harness has read it back since the harness shipped — but
//! `IntermediateSchema` had no `functions` field, so nothing in the tree could
//! *produce* one. The only writer was a test that hand-built the JSON. Shipping a
//! function meant hand-editing `schema.compiled.json`, which the next
//! `fraiseql compile` erased.
//!
//! This suite drives the **real producer** — `fraiseql_cli::commands::compile::run`,
//! the function the CLI dispatches — and hands its output to the **real consumer**,
//! `CompiledSchemaLoader::load_extended`. Asserting the producer against a
//! hand-written fixture would prove only that the fixture agrees with itself;
//! asserting the consumer against a locally re-derived expectation would prove only
//! that the test can copy the loader. The two ends have to meet on one artifact.
//!
//! It also pins that the section is inside the integrity envelope: `_content_hash`
//! is computed *after* the section is spliced in, so tampering with a function
//! definition fails the same check tampering with a query does (#899). A section
//! written outside the hash would round-trip perfectly and be silently editable.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** unsafe to split. `compile::run` resolves `fraiseql.toml` against
//! the *process* working directory, so the test chdirs; a second `#[tokio::test]` in
//! this file would race it. Everything goes in the one test.

#![cfg(feature = "functions-runtime")]

use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::schema::loader::CompiledSchemaLoader;
use tempfile::TempDir;

/// An SDK-shaped `schema.json` declaring one function alongside ordinary types.
const SCHEMA_JSON: &str = r#"{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "sql_source": "v_order",
      "is_input": false,
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "status", "type": "String", "nullable": false}
      ]
    }
  ],
  "queries": [
    {
      "name": "orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_order",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [
    {
      "name": "updateOrder",
      "return_type": "Order",
      "sql_source": "fn_update_order",
      "operation": "update",
      "invalidates_views": ["v_order"],
      "arguments": []
    }
  ],
  "subscriptions": [],
  "functions": [
    {
      "name": "notify_approved",
      "trigger": "after:mutation:Order:update",
      "runtime": "Deno",
      "timeout_ms": 2000,
      "when": [{"field": "status", "changed_to": "approved"}]
    }
  ]
}"#;

#[tokio::test]
async fn an_authored_function_survives_the_compiler_and_loads_in_the_server() {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), SCHEMA_JSON).expect("write schema.json");
    // The module the declaration promises, where the server would look for it. Its
    // absence is a compile-time failure when `module_dir` exists, so writing it is
    // part of authoring a function, not test scaffolding.
    std::fs::create_dir(dir.path().join("functions")).expect("module dir");
    std::fs::write(
        dir.path().join("functions/notify_approved.ts"),
        "export default async function (event: unknown) { return { ok: true }; }\n",
    )
    .expect("write module");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into the scratch project");
    let compiled = fraiseql_cli::commands::compile::run(
        "schema.json",
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        "schema.compiled.json",
        false,
        None,
        None,
        false,
        false,
        false,
    )
    .await;
    std::env::set_current_dir(original).expect("restore cwd");
    compiled.expect("a schema declaring a function must compile");

    let path = dir.path().join("schema.compiled.json");
    let contents = std::fs::read_to_string(&path).expect("the compiler must have written output");

    // ── The consumer parses what the producer wrote ──────────────────────────
    let extended = CompiledSchemaLoader::new(&path)
        .load_extended()
        .await
        .expect("the server must load the artifact its own compiler produced");

    let functions = extended.functions.expect(
        "the compiled artifact carries no `functions` section — an authored function \
                 declaration reached nothing, which is the whole of #1325",
    );

    assert_eq!(
        functions.definitions.len(),
        1,
        "expected the one authored definition, got {:?}",
        functions.definitions.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    let def = &functions.definitions[0];
    assert_eq!(def.name, "notify_approved");
    assert_eq!(def.trigger, "after:mutation:Order:update");
    assert_eq!(
        def.timeout_ms,
        Some(2000),
        "the authored timeout must survive; falling back to the trigger default would be \
         invisible, because both are timeouts"
    );
    assert_eq!(
        def.when.len(),
        1,
        "the authored `when` predicate must survive — a dropped predicate fires the function \
         on every update instead of failing"
    );

    // The `module_dir` an author never wrote: the compiler supplies the convention
    // so the wire format can keep requiring it, and a hand-written compiled schema
    // that omits it still fails loudly.
    assert_eq!(
        functions.module_dir,
        std::path::PathBuf::from("functions"),
        "the compiler must emit an explicit module_dir"
    );

    // ── The dispatcher accepts what the compiler emitted ────────────────────
    //
    // `build_functions_subsystem` does two things: load the modules, and build the
    // trigger registry. Only the second can run here (the first needs the Deno runtime
    // compiled in and a `module_dir` resolved against the server's own working
    // directory), and it is the half that reads the compiled declarations. A
    // definition that survives serialization but no longer loads as a trigger is a
    // function the server would refuse at boot.
    fraiseql_functions::triggers::registry::TriggerRegistry::load_from_definitions(
        &functions.definitions,
    )
    .expect("the dispatcher must accept the trigger set the compiler emitted");

    // ── The section is inside the integrity envelope ─────────────────────────
    CompiledSchema::from_json(&contents, true)
        .expect("the compiler's own output must verify its own content hash");

    let mut tampered: serde_json::Value =
        serde_json::from_str(&contents).expect("the artifact is JSON");
    tampered["functions"]["definitions"][0]["name"] =
        serde_json::Value::String("notify_everything".to_string());
    let tampered = serde_json::to_string_pretty(&tampered).expect("re-serialize");

    let err = CompiledSchema::from_json(&tampered, true).expect_err(
        "editing a function definition must fail the integrity check — a section spliced in \
         after the hash was computed would be silently editable, which is the one thing the \
         hash exists to prevent",
    );
    assert!(
        err.to_string().contains("integrity"),
        "expected a schema-integrity failure, got: {err}"
    );
}
