//! #923: a field typed with an author-declared enum, interface or union must keep that
//! kind through the compile.
//!
//! `SchemaConverter::parse_field_type` matched the twelve builtin scalar names and routed
//! everything else to `FieldType::Object`. It is a free function with no view of the
//! document it is parsing a name from, so it could not tell `status: OrderStatus` — where
//! the same file declares `enum OrderStatus` — from `author: User`. The consequence:
//! `FieldType::Enum`, `::Interface` and `::Union` had **no producer at all** for an
//! authored schema. `grep -rn "FieldType::Enum" crates/` found only the compiler's own
//! synthesized cascade types.
//!
//! This is not a cosmetic mislabel. Everything downstream branches on the variant:
//!
//! * introspection maps `Object(name)` to `TypeKind::Object`, so an introspecting client (Apollo,
//!   Relay, graphql-codegen) is told an enum-typed field is an OBJECT with no fields, and either
//!   rejects the schema or generates a nested selection for a scalar;
//! * the `TypeScript` client generator treats `Enum` as a leaf and `Object` as requiring a
//!   sub-selection, so it emits `status { … }` for a scalar enum value;
//! * `--emit-ddl` maps `Object` to `JSONB` instead of to the enum's own Postgres type.
//!
//! **Where the assertions are made.** Two levels, on purpose. The compiled artifact is
//! where the defect is visible (`{"Object": "OrderStatus"}` in the emitted file), and
//! introspection is where it is *felt* — so the enum case is carried all the way to
//! `IntrospectionBuilder`, which is the consumer that misreports. Asserting only on the
//! compiled JSON would leave the claim "and therefore introspection is right" untested.
//!
//! Everything starts from bytes on disk and the real binary.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{fs, process::Command};

use fraiseql_core::schema::{CompiledSchema, IntrospectionBuilder, IntrospectionType, TypeKind};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Write `schema` as `schema.json`, compile it with the real binary, and return the
/// emitted artifact as raw JSON.
fn compile(schema: &Value) -> Value {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), serde_json::to_string_pretty(schema).unwrap())
        .unwrap();

    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .expect("run fraiseql-cli");
    assert!(
        result.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap()
}

/// The compiled `field_type` of `Order.<field>`.
fn order_field_type(compiled: &Value, field: &str) -> Value {
    compiled["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Order")
        .expect("Order survives the compile")["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == field)
        .unwrap_or_else(|| panic!("Order.{field} survives the compile"))["field_type"]
        .clone()
}

/// One `Order` type carrying the supplied extra fields, plus whatever declarations the
/// caller adds at the top level.
fn schema_with(extra_fields: &Value, extra_decls: &Value) -> Value {
    let mut schema = json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [{
            "name": "orders", "return_type": "Order", "returns_list": true,
            "nullable": false, "sql_source": "v_order", "arguments": []
        }]
    });
    schema["types"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .extend(extra_fields.as_array().unwrap().iter().cloned());
    for (key, value) in extra_decls.as_object().unwrap() {
        schema[key] = value.clone();
    }
    schema
}

const ORDER_STATUS_ENUM: &str = "OrderStatus";

fn enum_decls() -> Value {
    json!({"enums": [{"name": ORDER_STATUS_ENUM, "values": [
        {"name": "PENDING"}, {"name": "SHIPPED"}, {"name": "CANCELLED"}
    ]}]})
}

// ── The compiled artifact ─────────────────────────────────────────────────────

/// The issue's own repro.
#[test]
fn an_enum_typed_field_compiles_to_the_enum_variant() {
    let compiled = compile(&schema_with(
        &json!([{"name": "status", "type": "OrderStatus", "nullable": false}]),
        &enum_decls(),
    ));

    assert_eq!(
        order_field_type(&compiled, "status"),
        json!({"Enum": "OrderStatus"}),
        "#923: a field typed with a declared enum must compile to the Enum variant, not to \
         an object reference to a type that is not an object"
    );
}

#[test]
fn an_interface_typed_field_compiles_to_the_interface_variant() {
    let compiled = compile(&schema_with(
        &json!([{"name": "actor", "type": "Node", "nullable": true}]),
        &json!({"interfaces": [{
            "name": "Node",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }]}),
    ));

    assert_eq!(order_field_type(&compiled, "actor"), json!({"Interface": "Node"}));
}

#[test]
fn a_union_typed_field_compiles_to_the_union_variant() {
    let compiled = compile(&schema_with(
        &json!([{"name": "outcome", "type": "OrderOutcome", "nullable": true}]),
        &json!({"unions": [{"name": "OrderOutcome", "member_types": ["Order"]}]}),
    ));

    assert_eq!(order_field_type(&compiled, "outcome"), json!({"Union": "OrderOutcome"}));
}

/// The list path recurses through `parse_field_type`, so it needs its own case —
/// resolving only the unwrapped scalar position would leave `[OrderStatus!]` broken.
#[test]
fn a_list_of_enums_resolves_the_element_kind() {
    let compiled = compile(&schema_with(
        &json!([{"name": "history", "type": "[OrderStatus!]", "nullable": true}]),
        &enum_decls(),
    ));

    assert_eq!(
        order_field_type(&compiled, "history"),
        json!({"List": {"Enum": "OrderStatus"}}),
        "#923: the element of a list must be resolved too"
    );
}

/// Query arguments go through the other `parse_field_type` caller (`convert_argument`),
/// which is a separate seam — guarding one caller is this compiler's recurring defect.
#[test]
fn an_enum_typed_query_argument_resolves_to_the_enum_variant() {
    let mut schema = schema_with(&json!([]), &enum_decls());
    schema["queries"][0]["arguments"] = json!([{
        "name": "status", "type": "OrderStatus", "nullable": true
    }]);
    let compiled = compile(&schema);

    let arg = &compiled["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["name"] == "orders")
        .expect("the query survives")["arguments"]
        .as_array()
        .unwrap()[0];

    assert_eq!(
        arg["arg_type"],
        json!({"Enum": "OrderStatus"}),
        "#923: arguments are converted by `convert_argument`, a different caller of the \
         same parser"
    );
}

/// Counterweight: an ordinary object reference must stay an object. Without this, a
/// resolver that returned `Enum` for everything would pass every case above.
#[test]
fn an_object_typed_field_still_compiles_to_the_object_variant() {
    let mut schema = schema_with(
        &json!([{"name": "customer", "type": "Customer", "nullable": true}]),
        &json!({}),
    );
    schema["types"].as_array_mut().unwrap().push(json!({
        "name": "Customer",
        "sql_source": "v_customer",
        "fields": [{"name": "id", "type": "ID", "nullable": false}]
    }));
    let compiled = compile(&schema);

    assert_eq!(order_field_type(&compiled, "customer"), json!({"Object": "Customer"}));
}

/// A name declared nowhere stays an object reference, deliberately. `SchemaValidator`
/// already reports it by name, and #724 chose a warning there rather than an error
/// because a `--schema-dir` author can declare a custom scalar in a file this pass cannot
/// see. Pinned so that a future strictening is a deliberate change to this assertion.
#[test]
fn an_undeclared_type_name_still_compiles_to_an_object_reference() {
    let compiled = compile(&schema_with(
        &json!([{"name": "note", "type": "Untyped", "nullable": true}]),
        &json!({}),
    ));

    assert_eq!(order_field_type(&compiled, "note"), json!({"Object": "Untyped"}));
}

// ── The consumer that misreported ─────────────────────────────────────────────

/// A field declared non-null is wrapped in `NON_NULL`; the declared type is inside.
fn unwrap_non_null(t: &IntrospectionType) -> &IntrospectionType {
    if t.kind == TypeKind::NonNull {
        t.of_type.as_ref().expect("NON_NULL wraps a type")
    } else {
        t
    }
}

/// The reason the variant matters. An introspecting client asks this question, and the
/// answer was `OBJECT` for a scalar enum.
#[test]
fn an_enum_typed_field_introspects_as_enum_not_object() {
    let compiled = compile(&schema_with(
        &json!([{"name": "status", "type": "OrderStatus", "nullable": false}]),
        &enum_decls(),
    ));

    let schema: CompiledSchema =
        CompiledSchema::from_json(&serde_json::to_string(&compiled).unwrap(), false).unwrap();
    let introspection = IntrospectionBuilder::build(&schema);

    let order = introspection
        .types
        .iter()
        .find(|t| t.name.as_deref() == Some("Order"))
        .expect("Order is introspectable");
    let status = order
        .fields
        .as_ref()
        .expect("Order has fields")
        .iter()
        .find(|f| f.name == "status")
        .expect("Order.status is introspectable");

    let named = unwrap_non_null(&status.field_type);

    assert_eq!(
        named.kind,
        TypeKind::Enum,
        "#923: introspection must report Order.status as ENUM. Reporting OBJECT tells an \
         introspection-driven client that a scalar value is an object with no fields, so \
         it either rejects the schema or generates a nested selection for it"
    );
    assert_eq!(named.name.as_deref(), Some("OrderStatus"));
}
