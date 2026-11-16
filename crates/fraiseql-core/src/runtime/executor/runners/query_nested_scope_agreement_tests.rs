//! The load-time refusal of a scoped embedding (#1737) and the runtime row gate agree on
//! which fields hold a scoped type.
//!
//! Both read [`CompiledSchema::scoping_read`]. This test holds them to it from the outside:
//! in a served document, a field is row-gated when it is read exactly where the schema is
//! refused for it when it loads. A rule that drifted on one side would either refuse a
//! schema the runtime serves ungated, or load one whose nested level the runtime gates
//! with no declaration to show for it.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_pass_by_value)] // Reason: helpers take the JSON a call site builds inline

use serde_json::{Value, json};

use super::NestedRowGates;
use crate::schema::CompiledSchema;

fn field(name: &str, field_type: Value) -> Value {
    json!({ "name": name, "field_type": field_type })
}

fn list_query(name: &str, type_name: &str, scoped: bool) -> Value {
    let mut query = json!({
        "name": name,
        "return_type": type_name,
        "returns_list": true,
        "sql_source": format!("v_{}", type_name.to_lowercase()),
        "arguments": []
    });
    if scoped {
        query["inject_params"] = json!({ "tenant_id": { "source": "jwt", "claim": "org_id" } });
    }
    query
}

/// `Order` embeds the scoped `OrderLine` three ways (an object, a list, under the value
/// object `Section`), beside an unscoped `Note` and a value object `Money`. Nothing is
/// declared, and the schema is built without the load check, which would refuse it.
fn undeclared() -> CompiledSchema {
    let stored = |name: &str, fields: Vec<Value>| {
        let mut all = vec![field("id", json!("ID")), field("tenantId", json!("ID"))];
        all.extend(fields);
        json!({ "name": name, "sql_source": format!("v_{}", name.to_lowercase()), "fields": all })
    };
    let value_object = |name: &str, fields: Vec<Value>| json!({ "name": name, "sql_source": "", "embedded": true, "fields": fields });
    let line = || json!({ "Object": "OrderLine" });
    let document = json!({
        "types": [
            stored("Order", vec![
                field("line", line()),
                field("lines", json!({ "List": { "item": line() } })),
                field("sections", json!({ "List": { "item": { "Object": "Section" } } })),
                field("note", json!({ "Object": "Note" })),
                field("total", json!({ "Object": "Money" })),
            ]),
            stored("OrderLine", vec![]),
            stored("Note", vec![]),
            value_object("Section", vec![field("lines", json!({ "List": { "item": line() } }))]),
            value_object("Money", vec![field("amount", json!("Float"))]),
            value_object("CreateLineSuccess", vec![field("line", line())]),
        ],
        "queries": [
            list_query("orders", "Order", true),
            list_query("orderLines", "OrderLine", true),
            list_query("notes", "Note", false),
        ],
        "mutations": [{
            "name": "createLine",
            "return_type": "CreateLineSuccess",
            "sql_source": "fn_create_line",
            "arguments": []
        }],
        "subscriptions": []
    });
    let mut schema: CompiledSchema = serde_json::from_value(document).unwrap();
    schema.build_indexes();
    schema
}

#[test]
fn a_served_field_is_row_gated_exactly_where_the_schema_is_refused_for_it() {
    let schema = undeclared();
    let gates = NestedRowGates::build(&schema, None);
    let violations = schema.embedded_scope_violations();
    let refused = |owner: &str, field: &str| {
        violations.iter().any(|v| v.starts_with(&format!("`{owner}.{field}` embeds")))
    };

    let mut gated = Vec::new();
    for owner in schema.types.iter().filter(|t| t.name.as_str() != "CreateLineSuccess") {
        for field in &owner.fields {
            let is_gated = gates.get(owner.name.as_str(), field.name.as_str()).is_some();
            assert_eq!(
                is_gated,
                refused(owner.name.as_str(), field.name.as_str()),
                "`{}.{}`: gated at run time = {is_gated}; violations: {violations:#?}",
                owner.name,
                field.name
            );
            if is_gated {
                gated.push(format!("{}.{}", owner.name, field.name));
            }
        }
    }
    // Non-vacuous: the three embeddings are the ones found, on both sides.
    assert_eq!(gated, ["Order.line", "Order.lines", "Section.lines"]);
    assert_eq!(violations.len(), 3, "{violations:#?}");
}

/// A mutation's payload is outside the load-time rule and inside the runtime gate: the
/// engine filters the scoped level of what the function returns.
#[test]
fn a_payload_field_is_row_gated_and_not_refused() {
    let schema = undeclared();
    let gates = NestedRowGates::build(&schema, None);

    assert!(gates.get("CreateLineSuccess", "line").is_some());
    assert!(
        !schema
            .embedded_scope_violations()
            .iter()
            .any(|v| v.contains("CreateLineSuccess")),
        "a payload is not a served document"
    );
}
