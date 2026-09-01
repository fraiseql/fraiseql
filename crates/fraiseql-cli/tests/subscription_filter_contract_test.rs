//! #1262: a subscription filter condition resolves against that subscription's own
//! declared arguments, at compile time.
//!
//! A `filter.conditions[].argument` naming an argument the subscription does not
//! declare used to compile clean — no error, not even a warning — and the runtime
//! then **failed open**: the client sends its variable under the *declared* GraphQL
//! name, `argument_paths` is keyed by the dangling name, the lookup misses, the
//! condition contributes nothing, and a subscriber that filtered receives every
//! event on the topic. A filter that matches nothing and a filter that matches
//! everything are indistinguishable there.
//!
//! Same class as `vector_distance`, where a dangling field reference is already a
//! compile error: the two facts sit side by side in the document, so the reference
//! has to be resolved where they do.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{fs, process::Command};

use fraiseql_core::schema::CompiledSchema;
use tempfile::TempDir;

fn run_compile(types_json: &str) -> (bool, String, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");
    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, "").unwrap();

    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run fraiseql-cli");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined, temp_dir)
}

/// One `Order` type, one query, and one subscription whose filter conditions are
/// supplied by the caller.
fn subscription_types_json(arguments: &str, conditions: &str) -> String {
    format!(
        r#"{{
  "types": [
    {{
      "name": "Order",
      "sql_source": "v_order",
      "fields": [
        {{"name": "id", "type": "ID", "nullable": false}},
        {{"name": "total", "type": "Float", "nullable": false}}
      ]
    }}
  ],
  "queries": [
    {{
      "name": "orders",
      "return_type": "Order",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_order"
    }}
  ],
  "subscriptions": [
    {{
      "name": "orderUpdated",
      "return_type": "Order",
      "topic": "order_events",
      "arguments": [{arguments}],
      "filter": {{"conditions": [{conditions}]}},
      "fields": ["id", "total"]
    }}
  ]
}}"#
    )
}

/// The repro from #1262: the subscription declares `orderId`, its filter names
/// `no_such_argument`. Nothing in the document can satisfy that reference.
#[test]
fn a_filter_naming_no_such_argument_is_a_compile_error() {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true}"#,
        r#"{"argument": "no_such_argument", "path": "$.id"}"#,
    );
    let (ok, log, _dir) = run_compile(&types);
    assert!(!ok, "a dangling subscription filter reference must refuse to compile: {log}");
    assert!(
        log.contains("no_such_argument"),
        "the error must name the dangling reference: {log}"
    );
    assert!(
        log.contains("orderId"),
        "the error must list the arguments the author can actually use: {log}"
    );
}

/// The near-miss the Python SDK produced (#1255): the author spells the filter
/// reference the way they spelled the parameter, `order_id`, while the declared
/// argument was translated to `orderId`. A casing miss is still a miss.
#[test]
fn a_filter_naming_the_untranslated_argument_spelling_is_a_compile_error() {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true}"#,
        r#"{"argument": "order_id", "path": "$.id"}"#,
    );
    let (ok, log, _dir) = run_compile(&types);
    assert!(!ok, "a case-mismatched filter reference must refuse to compile: {log}");
    assert!(log.contains("order_id"), "the error must name the dangling reference: {log}");
}

/// The positive control. Without it the refusal above could be produced by a check
/// that refuses every subscription filter, and the suite would not notice.
#[test]
fn a_filter_naming_a_declared_argument_compiles_and_reaches_the_artifact() {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true}"#,
        r#"{"argument": "orderId", "path": "$.id"}"#,
    );
    let (ok, log, dir) = run_compile(&types);
    assert!(ok, "a resolvable filter reference must compile: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let sub = schema
        .subscriptions
        .iter()
        .find(|s| s.name == "orderUpdated")
        .expect("orderUpdated subscription");
    let filter = sub.filter.as_ref().expect("filter must reach the compiled artifact");
    assert_eq!(filter.argument_paths.get("orderId"), Some(&"$.id".to_string()));
}

/// A subscription with several arguments resolves each condition independently —
/// one good reference does not vouch for a bad one beside it.
#[test]
fn one_resolvable_condition_does_not_excuse_a_dangling_sibling() {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true},
           {"name": "status", "type": "String", "nullable": true}"#,
        r#"{"argument": "orderId", "path": "$.id"},
           {"argument": "customerId", "path": "$.customer_id"}"#,
    );
    let (ok, log, _dir) = run_compile(&types);
    assert!(!ok, "a dangling sibling condition must refuse to compile: {log}");
    assert!(log.contains("customerId"), "the error must name the dangling sibling: {log}");
}
