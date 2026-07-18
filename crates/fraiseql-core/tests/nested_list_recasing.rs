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
//! exercise both Rust projectors that see raw list blobs:
//!
//! - [`project_entity`] — the canonical entity projector (mutation success/error payloads, and the
//!   query-path list pass delegates to it).
//! - [`project_nested_lists`] — the query-path pass that fixes list fields left raw by the SQL
//!   projection.

use fraiseql_core::{
    graphql::FieldSelection,
    runtime::{project_entity, project_nested_lists},
    schema::CompiledSchema,
};
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
                    { "name": "lineItems", "field_type": { "List": { "Object": "LineItem" } } }
                ]
            },
            {
                "name": "Customer",
                "sql_source": "v_customer",
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "displayName", "field_type": "String" },
                    { "name": "orders", "field_type": { "List": { "Object": "Order" } } }
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

#[test]
fn project_nested_lists_fixes_a_sql_projected_result() {
    // The query path already projected top-level fields at the SQL level (camelCase
    // `id`), but left the `lineItems` list as the raw stored sub-blob.
    let mut sql_projected = json!({
        "id": "o1",
        "lineItems": [
            { "id": "li1", "unit_price": 9.99, "sku_code": "ABC" }
        ]
    });
    let selections = vec![
        field("id"),
        nested("lineItems", vec![field("unitPrice"), field("skuCode")]),
    ];

    project_nested_lists(&mut sql_projected, "Order", &selections, &schema());

    assert_eq!(
        sql_projected,
        json!({
            "id": "o1",
            "lineItems": [ { "unitPrice": 9.99, "skuCode": "ABC" } ]
        })
    );
}

#[test]
fn project_nested_lists_reaches_lists_nested_inside_single_objects() {
    // `customer` is a single object the SQL side already projected (camelCase keys);
    // the list `orders` inside it is still raw. The pass must recurse into the object.
    let mut sql_projected = json!({
        "id": "o1",
        "customer": {
            "displayName": "Acme",
            "orders": [ { "id": "o9", "line_items": [] } ]
        }
    });
    let selections = vec![
        field("id"),
        nested("customer", vec![field("displayName"), nested("orders", vec![field("id")])]),
    ];

    project_nested_lists(&mut sql_projected, "Order", &selections, &schema());

    assert_eq!(
        sql_projected["customer"]["orders"],
        json!([ { "id": "o9" } ]),
        "a list nested inside an already-projected single object must still be projected"
    );
}
