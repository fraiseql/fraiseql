#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! `fraiseql compile` refuses a default on an input field an update receives.
//!
//! An update leaves an omitted field unchanged; a default is filled in for an omitted field,
//! so on an update it would overwrite the stored value every time. The compiler refuses the
//! schema, naming the field, its input type and the update, and accepts the same default on
//! an input only a create receives.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** one test — `compile_to_schema` resolves its input against the process
//! working directory, so cases in separate `#[tokio::test]`s would race on `chdir`.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};
use tempfile::TempDir;

/// `OrderInput { id: ID!, currency: String = <currency>, note: String = null }` written by
/// `createOrder`, and by `updateOrder` when `update_takes_order_input`; otherwise
/// `updateOrder` takes `PatchInput { id: ID!, currency: String }`.
fn schema(currency: &Value, update_takes_order_input: bool) -> String {
    let update_input = if update_takes_order_input {
        "OrderInput"
    } else {
        "PatchInput"
    };
    json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "input_types": [
            {
                "name": "OrderInput",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "currency", "type": "String", "nullable": true, "default": currency},
                    {"name": "note", "type": "String", "nullable": true, "default": null}
                ]
            },
            {
                "name": "PatchInput",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "currency", "type": "String", "nullable": true, "default": null}
                ]
            }
        ],
        "queries": [{
            "name": "orders",
            "return_type": "Order",
            "returns_list": true,
            "sql_source": "v_order"
        }],
        "mutations": [
            {
                "name": "createOrder",
                "return_type": "Order",
                "sql_source": "fn_create_order",
                "operation": "CREATE",
                "arguments": [{"name": "input", "type": "OrderInput", "nullable": false}]
            },
            {
                "name": "updateOrder",
                "return_type": "Order",
                "sql_source": "fn_update_order",
                "operation": "UPDATE",
                "arguments": [{"name": "input", "type": update_input, "nullable": false}]
            }
        ]
    })
    .to_string()
}

/// Compile `schema_json` from a fresh temp dir, restoring the working directory.
async fn compile(schema_json: &str) -> anyhow::Result<CompiledSchema> {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), schema_json).expect("write schema.json");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");
    result.map(|(artifact, _)| artifact.schema)
}

#[tokio::test]
async fn compile_refuses_a_default_an_update_would_write() {
    let err = compile(&schema(&json!("EUR"), true))
        .await
        .expect_err("a default on an update's input must not compile");
    let msg = format!("{err:#}");
    for part in [
        "`OrderInput.currency` (default \"EUR\")",
        "update mutation `updateOrder`",
    ] {
        assert!(msg.contains(part), "names {part}: {msg}");
    }
    assert!(msg.contains("overwrite"), "says why: {msg}");
    assert!(!msg.contains("createOrder"), "a create is not the problem: {msg}");

    // The same default on an input only a create receives compiles, and reaches the artifact.
    let compiled = compile(&schema(&json!("EUR"), false))
        .await
        .map_err(|e| format!("a create's default compiles: {e:#}"))
        .unwrap();
    let field = |input: &str, name: &str| {
        compiled
            .find_input_type(input)
            .unwrap()
            .find_field(name)
            .unwrap()
            .default_value
            .clone()
    };
    assert_eq!(field("OrderInput", "currency").as_deref(), Some("\"EUR\""));

    // A `null` default (an SDK's `= None`) declares no default, on a create or an update.
    assert_eq!(field("OrderInput", "note"), None);
    assert_eq!(field("PatchInput", "currency"), None);
    let compiled = compile(&schema(&Value::Null, true))
        .await
        .map_err(|e| format!("an update input with only null defaults compiles: {e:#}"))
        .unwrap();
    assert!(compiled.find_mutation("updateOrder").is_some());
}
