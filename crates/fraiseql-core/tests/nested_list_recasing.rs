#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #489 — nested LIST-of-object output fields must be recased to the
//! camelCase surface and projected to the selection set, exactly like top-level
//! fields and nested single objects (the third recasing path after #456 mutation
//! input and #486 query arguments).
//!
//! The stored JSONB `data` builds nested arrays with `snake_case` keys and every
//! column (not just the selected ones); the SQL projector leaves list fields as the
//! raw sub-blob, so the recasing/projection must happen in Rust. These pure tests
//! exercise [`project_entity`], the canonical entity projector (mutation success/error
//! payloads; the query path's walk projects each list element through it).

use fraiseql_core::{graphql::FieldSelection, runtime::project_entity, schema::CompiledSchema};
use serde_json::json;

/// A leaf field selection (no sub-selection, no alias).
fn field(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

/// A field selection with a sub-selection.
fn nested(name: &str, sub: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: sub,
        directives:    vec![],
    }
}

/// Schema: an `Order` with a scalar `id`, a nested single `customer` (Object), and a
/// nested list `lineItems` (`[LineItem]`) whose elements carry `unitPrice`/`skuCode`
/// plus an unselected `id`.
fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [
            {
                "name": "Order",
                "sql_source": "v_order",
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "customer", "field_type": { "Object": "Customer" } },
                    { "name": "lineItems", "field_type": { "List": {"item": { "Object": "LineItem" }} } }
                ]
            },
            {
                "name": "Customer",
                "sql_source": "v_customer",
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "displayName", "field_type": "String" },
                    { "name": "orders", "field_type": { "List": {"item": { "Object": "Order" }} } }
                ]
            },
            {
                "name": "LineItem",
                "sql_source": "v_line_item",
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "unitPrice", "field_type": "Float" },
                    { "name": "skuCode", "field_type": "String" }
                ]
            }
        ]
    }))
    .expect("schema")
}

/// The raw stored entity: `snake_case` keys, the nested list built with `snake_case`
/// element keys plus an unselected `id`.
fn raw_order() -> serde_json::Value {
    json!({
        "id": "o1",
        "line_items": [
            { "id": "li1", "unit_price": 9.99, "sku_code": "ABC" },
            { "id": "li2", "unit_price": 4.50, "sku_code": "XYZ" }
        ]
    })
}

#[test]
fn project_entity_recases_and_projects_nested_list_objects() {
    // `{ id lineItems { unitPrice skuCode } }`
    let selections = vec![
        field("id"),
        nested("lineItems", vec![field("unitPrice"), field("skuCode")]),
    ];

    let out = project_entity(&raw_order(), "Order", &selections, &schema());

    assert_eq!(
        out,
        json!({
            "id": "o1",
            "lineItems": [
                { "unitPrice": 9.99, "skuCode": "ABC" },
                { "unitPrice": 4.50, "skuCode": "XYZ" }
            ]
        }),
        "nested list objects must be recased to camelCase and projected to the selection \
         set (no snake_case keys, no unselected `id`)"
    );
}
