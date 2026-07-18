#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #489 — a nested LIST-of-object query output must be recased to camelCase
//! and projected to the selection set, not returned as the raw stored JSONB blob.
//!
//! End-to-end on the QUERY path: the view's `data` builds a nested `line_items`
//! array with `snake_case` element keys (`unit_price`, `sku_code`) and an unselected
//! `id`. The SQL projector leaves list fields as the raw sub-blob, so the recasing +
//! projection happens in Rust (`project_nested_lists`, wired into the query runner).
//! We run a real GraphQL query through parse → SQL → project → serialize and assert
//! the array comes back camelCased and projected.
//!
//! Query-only by design (no mutation), so it needs no `core.*` install and runs
//! against a bare PostgreSQL.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::json;

const SCHEMA: &str = "issue_489";

async fn provision(adapter: &PostgresAdapter) {
    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_order (\
             id UUID PRIMARY KEY DEFAULT gen_random_uuid(), code TEXT)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_line (\
             id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
             fk_order UUID REFERENCES {SCHEMA}.tb_order(id), \
             unit_price NUMERIC, sku_code TEXT, position INT)"
        ))
        .await
        .unwrap();

    // `data.line_items` is a jsonb_agg with snake_case element keys and an `id` the
    // query below does not select — exactly the #489 shape.
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object(\
               'id', id, 'code', code, \
               'line_items', COALESCE((SELECT jsonb_agg(jsonb_build_object(\
                   'id', l.id::text, 'unit_price', l.unit_price, 'sku_code', l.sku_code) \
                   ORDER BY l.position) FROM {SCHEMA}.tb_line l WHERE l.fk_order = {SCHEMA}.tb_order.id), \
                 '[]'::jsonb)\
             ) AS data FROM {SCHEMA}.tb_order"
        ))
        .await
        .unwrap();
}

async fn seed(adapter: &PostgresAdapter) -> String {
    let rows = adapter
        .execute_raw_query(&format!(
            "INSERT INTO {SCHEMA}.tb_order (code) VALUES ('ORD-1') RETURNING id::text AS id"
        ))
        .await
        .unwrap();
    let id = rows
        .into_iter()
        .next()
        .and_then(|r| r.get("id").and_then(|v| v.as_str().map(ToString::to_string)))
        .expect("order id");
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {SCHEMA}.tb_line (fk_order, unit_price, sku_code, position) VALUES \
             ('{id}'::uuid, 9.99, 'ABC', 1), ('{id}'::uuid, 4.50, 'XYZ', 2)"
        ))
        .await
        .unwrap();
    id
}

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [
            {
                "name": "Order",
                "sql_source": format!("{SCHEMA}.v_order"),
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "code", "field_type": "String" },
                    { "name": "lineItems", "field_type": { "List": { "Object": "LineItem" } } }
                ]
            },
            {
                "name": "LineItem",
                "sql_source": format!("{SCHEMA}.v_line"),
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "unitPrice", "field_type": "Float" },
                    { "name": "skuCode", "field_type": "String" }
                ]
            }
        ],
        "queries": [
            {
                "name": "order",
                "return_type": "Order",
                "returns_list": false,
                "nullable": true,
                "sql_source": format!("{SCHEMA}.v_order"),
                "arguments": [ { "name": "id", "arg_type": "ID", "nullable": false } ]
            }
        ]
    }))
    .expect("schema")
}

#[tokio::test]
async fn nested_list_query_output_is_recased_and_projected() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed(&adapter).await;

    let executor = Executor::new(schema(), Arc::clone(&adapter));

    // Select the nested list with camelCase fields; do NOT select the element `id`.
    let doc = format!("{{ order(id: \"{id}\") {{ id lineItems {{ unitPrice skuCode }} }} }}");
    let res = executor.execute(&doc, None).await.unwrap();
    let order = res.get("data").and_then(|d| d.get("order")).cloned().unwrap();

    assert!(
        order["lineItems"].is_array(),
        "lineItems must be an array; got:\n{}",
        serde_json::to_string_pretty(&order).unwrap()
    );
    let items = order["lineItems"].as_array().unwrap();
    assert_eq!(items.len(), 2, "both line items present");

    // #489 contract: elements are recased to camelCase and projected to the selection.
    assert_eq!(items[0]["skuCode"], json!("ABC"), "element recased to camelCase (skuCode)");
    assert_eq!(items[1]["skuCode"], json!("XYZ"));
    assert!(
        items[0].get("unitPrice").is_some(),
        "element field `unitPrice` recased + present"
    );

    for it in items {
        assert!(it.get("sku_code").is_none(), "snake_case `sku_code` must not leak");
        assert!(it.get("unit_price").is_none(), "snake_case `unit_price` must not leak");
        assert!(it.get("id").is_none(), "unselected element key `id` must not leak");
    }
}
