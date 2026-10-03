#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #1396 — `descendantOfId` / `ancestorOfId` execute.
//!
//! The operators take a node *id* and filter on the subtree under (or the ancestors of)
//! that node's path, which lives in the hierarchy's table. The runtime never built the
//! context that names that table, so every such filter failed with "requires
//! `HierarchyContext`" whatever was configured; and a schema-qualified table was quoted
//! as one identifier. Driven through the real executor against PostgreSQL with the
//! issue's setup: a schema-qualified node table, items whose `data.path` is an ltree.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::json;

const SCHEMA: &str = "issue_1396";
const NODE_3_4: &str = "00000000-0000-0000-0000-000000000034";

async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE EXTENSION IF NOT EXISTS ltree").await.unwrap();
    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_node (id uuid PRIMARY KEY, path ltree NOT NULL)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {SCHEMA}.tb_node VALUES \
             ('00000000-0000-0000-0000-000000000003', '3'), \
             ('{NODE_3_4}', '3.4')"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, data FROM (VALUES \
             (gen_random_uuid(), '{{\"name\": \"root\", \"path\": \"3\"}}'::jsonb), \
             (gen_random_uuid(), '{{\"name\": \"node\", \"path\": \"3.4\"}}'::jsonb), \
             (gen_random_uuid(), '{{\"name\": \"child\", \"path\": \"3.4.7\"}}'::jsonb), \
             (gen_random_uuid(), '{{\"name\": \"other\", \"path\": \"5.1\"}}'::jsonb) \
             ) AS t(id, data)"
        ))
        .await
        .unwrap();
}

fn schema() -> CompiledSchema {
    CompiledSchema::from_json(
        &json!({
            "naming_convention": "camelCase",
            "hierarchies_config": {
                "node": { "table": format!("{SCHEMA}.tb_node"), "path_column": "path" }
            },
            "types": [{
                "name": "Item",
                "sql_source": format!("{SCHEMA}.v_item"),
                "fields": [
                    { "name": "name", "field_type": "String" },
                    { "name": "path", "field_type": { "Scalar": "LTree" }, "hierarchy": "node" }
                ]
            }],
            "queries": [{
                "name": "items",
                "return_type": "Item",
                "returns_list": true,
                "nullable": false,
                "sql_source": format!("{SCHEMA}.v_item"),
                "auto_params": { "has_where": true }
            }]
        })
        .to_string(),
        false,
    )
    .expect("schema")
}

async fn names(executor: &Executor, operator: &str) -> Vec<String> {
    let query =
        format!("{{ items(where: {{ path: {{ {operator}: \"{NODE_3_4}\" }} }}) {{ name }} }}");
    let response = executor.execute(&query, None).await.unwrap();
    let mut names: Vec<String> = response["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items in {response}"))
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn descendant_and_ancestor_of_id_resolve_through_the_fields_hierarchy() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));

    assert_eq!(
        names(&executor, "descendantOfId").await,
        ["child", "node"],
        "the subtree of 3.4"
    );
    assert_eq!(
        names(&executor, "ancestorOfId").await,
        ["node", "root"],
        "3.4 and its ancestors"
    );
}

/// A field linking a hierarchy that is not declared is refused when the schema loads,
/// rather than failing every request that uses the operator.
#[test]
fn a_field_linking_an_undeclared_hierarchy_is_refused_at_load() {
    let error = CompiledSchema::from_json(
        &json!({
            "types": [{
                "name": "Item",
                "sql_source": "v_item",
                "fields": [{ "name": "path", "field_type": { "Scalar": "LTree" }, "hierarchy": "nodes" }]
            }],
            "queries": []
        })
        .to_string(),
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Item.path") && error.contains("nodes"), "{error}");
}
