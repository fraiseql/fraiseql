#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! #1397: a cascade mutation declares typed success fields, and the compiler adds them to
//! its `<Mutation>Payload` next to `entity`, `cascade` and `updatedFields`.
//!
//! The function returns them in its row's `result jsonb` column. A declaration
//! the payload could not carry fails the compile: a name the payload already has, a type
//! that is not a leaf (a scalar or an enum), or a mutation with no payload (not `cascade`).
//!
//! **Execution engine:** in-memory (no database: the refusals are the compiler's).

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::{CompiledSchema, FieldType};
use serde_json::{Value, json};

/// `createOrder`, a cascade mutation returning `Order`, with `success_fields`.
fn schema_with(success_fields: &Value, cascade: bool) -> Value {
    json!({
        "version": "2.0.0",
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [
                { "name": "id", "type": "ID", "nullable": false },
                { "name": "total", "type": "Float", "nullable": false }
            ]
        }],
        "enums": [{ "name": "Recovery", "values": [{ "name": "FULL" }, { "name": "PARTIAL" }] }],
        "queries": [],
        "mutations": [{
            "name": "createOrder",
            "return_type": "Order",
            "cascade": cascade,
            "sql_source": "fn_create_order",
            "operation": "CREATE",
            "success_fields": success_fields
        }]
    })
}

fn compile(schema: &Value) -> Result<CompiledSchema, String> {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema.to_string()).unwrap();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(compile_to_schema(CompileOptions {
            skip_hash: true,
            ..CompileOptions::new(path.to_str().unwrap())
        }))
        .map(|(compiled, _)| compiled.schema)
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn success_fields_are_typed_fields_of_the_payload() {
    let schema = compile(&schema_with(
        &json!([
            { "name": "recoveredItems", "type": "Int", "nullable": false },
            { "name": "recovery", "type": "Recovery", "nullable": true }
        ]),
        true,
    ))
    .unwrap();
    let payload = schema.find_type("CreateOrderPayload").expect("the cascade payload");
    let field = |name: &str| {
        payload
            .fields
            .iter()
            .find(|f| f.name.as_str() == name)
            .unwrap_or_else(|| panic!("payload has `{name}`: {payload:?}"))
    };
    assert_eq!(field("recoveredItems").field_type, FieldType::Int);
    assert!(!field("recoveredItems").nullable);
    assert_eq!(field("recovery").field_type, FieldType::Enum("Recovery".to_string()));
    assert!(field("recovery").nullable);
    // The payload keeps its own fields.
    for own in ["entity", "cascade", "updatedFields"] {
        field(own);
    }
    // The runtime reads them from the mutation.
    let mutation = schema.mutations.iter().find(|m| m.name == "createOrder").unwrap();
    let declared: Vec<Value> = serde_json::to_value(mutation).unwrap()["success_fields"]
        .as_array()
        .unwrap_or_else(|| panic!("the mutation carries its success fields: {mutation:?}"))
        .iter()
        .map(|f| f["name"].clone())
        .collect();
    assert_eq!(declared, [json!("recoveredItems"), json!("recovery")]);
}

/// Each declaration the payload could not carry fails the compile, naming the field.
#[test]
fn a_success_field_the_payload_cannot_carry_fails_the_compile() {
    for (fields, cascade, names, why) in [
        (
            json!([{ "name": "entity", "type": "Int", "nullable": true }]),
            true,
            "entity",
            "the payload's own field",
        ),
        (
            json!([{ "name": "cascade", "type": "Int", "nullable": true }]),
            true,
            "cascade",
            "the payload's own field",
        ),
        (
            json!([{ "name": "updatedFields", "type": "Int", "nullable": true }]),
            true,
            "updatedFields",
            "the payload's own field",
        ),
        (
            json!([{ "name": "__typename", "type": "String", "nullable": true }]),
            true,
            "__typename",
            "reserved",
        ),
        (
            json!([{ "name": "n", "type": "Int", "nullable": true }, { "name": "n", "type": "Int", "nullable": true }]),
            true,
            "n",
            "declared twice",
        ),
        (
            json!([{ "name": "order", "type": "Order", "nullable": true }]),
            true,
            "order",
            "an object type",
        ),
        (
            json!([{ "name": "counts", "type": "[Int]", "nullable": true }]),
            true,
            "counts",
            "a list",
        ),
        (
            json!([{ "name": "n", "type": "Int", "nullable": true }]),
            false,
            "createOrder",
            "no payload: not cascade",
        ),
    ] {
        let Err(err) = compile(&schema_with(&fields, cascade)) else {
            panic!("{why}: {fields} compiled")
        };
        assert!(
            err.contains("success_fields") && err.contains(&format!("`{names}`")),
            "{why}: {err}"
        );
    }
}

/// A type that already owns the payload's name leaves the mutation returning its entity, so
/// its success fields would be declared and never served: refused.
#[test]
fn success_fields_on_a_mutation_whose_payload_name_is_taken_fail_the_compile() {
    let mut schema = schema_with(&json!([{ "name": "n", "type": "Int", "nullable": true }]), true);
    schema["types"].as_array_mut().unwrap().push(json!({
        "name": "CreateOrderPayload",
        "sql_source": "v_create_order_payload",
        "fields": [{ "name": "id", "type": "ID", "nullable": false }]
    }));
    let Err(err) = compile(&schema) else {
        panic!("compiled")
    };
    assert!(err.contains("success_fields") && err.contains("`CreateOrderPayload`"), "{err}");
}
