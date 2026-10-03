#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #1364 — a nullable nested object stored as JSON `null`, or absent, is `null` in
//! the response, not an object of nulls.
//!
//! The query SQL projector built every selected nested object with an unconditional
//! `jsonb_build_object(...)`: when the stored value was `null` or missing, each inner
//! read was NULL and the object was built anyway — `"customer": {"id": null, ...}`. A
//! client could not tell "no customer" from "a customer whose fields are null", and a
//! non-null sub-field (`id: ID!`) was answered with `null`.
//!
//! Driven through the real executor against PostgreSQL, so the assertion is on the
//! response a client receives, at depth 1 and depth 2.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const SCHEMA: &str = "issue_1364";

async fn provision(adapter: &PostgresAdapter) {
    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    // One row per shape the stored value can take.
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, data FROM (VALUES \
             ('00000000-0000-0000-0000-000000000001'::uuid, \
              '{{\"id\": \"o-null\", \"customer\": null}}'::jsonb), \
             ('00000000-0000-0000-0000-000000000002'::uuid, \
              '{{\"id\": \"o-missing\"}}'::jsonb), \
             ('00000000-0000-0000-0000-000000000003'::uuid, \
              '{{\"id\": \"o-present\", \"customer\": {{\"id\": \"c1\", \"name\": \"Ada\", \
                \"address\": null}}}}'::jsonb), \
             ('00000000-0000-0000-0000-000000000004'::uuid, \
              '{{\"id\": \"o-deep\", \"customer\": {{\"id\": \"c2\", \"name\": \"Bob\", \
                \"address\": {{\"city\": \"Lyon\"}}}}}}'::jsonb) \
             ) AS t(id, data)"
        ))
        .await
        .unwrap();
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
                    { "name": "customer", "field_type": { "Object": "Customer" }, "nullable": true }
                ]
            },
            {
                "name": "Customer",
                "sql_source": format!("{SCHEMA}.v_customer"),
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "name", "field_type": "String", "nullable": true },
                    { "name": "address", "field_type": { "Object": "Address" }, "nullable": true }
                ]
            },
            {
                "name": "Address",
                "sql_source": format!("{SCHEMA}.v_address"),
                "fields": [{ "name": "city", "field_type": "String", "nullable": true }]
            }
        ],
        "queries": [{
            "name": "orders",
            "return_type": "Order",
            "returns_list": true,
            "nullable": false,
            "sql_source": format!("{SCHEMA}.v_order")
        }]
    }))
    .expect("schema")
}

fn by_id(orders: &[Value], id: &str) -> Value {
    orders.iter().find(|o| o["id"] == json!(id)).cloned().unwrap_or_else(|| {
        panic!("order {id} missing from {orders:?}");
    })
}

#[tokio::test]
async fn a_null_or_missing_nested_object_is_null_at_every_depth() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));

    let response = executor
        .execute("{ orders { id customer { id name address { city } } } }", None)
        .await
        .unwrap();
    let orders = response["data"]["orders"].as_array().cloned().unwrap();

    assert_eq!(by_id(&orders, "o-null")["customer"], Value::Null, "stored null → null");
    assert_eq!(by_id(&orders, "o-missing")["customer"], Value::Null, "missing key → null");
    assert_eq!(
        by_id(&orders, "o-present")["customer"],
        json!({"id": "c1", "name": "Ada", "address": null}),
        "a present object is projected, and its own null sub-object is null (depth 2)"
    );
    assert_eq!(
        by_id(&orders, "o-deep")["customer"]["address"],
        json!({"city": "Lyon"}),
        "a present sub-object is still projected"
    );
}
