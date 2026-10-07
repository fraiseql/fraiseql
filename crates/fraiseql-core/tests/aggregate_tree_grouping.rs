#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::float_cmp)] // Reason: every total is a sum of small powers of two, exact in f64
#![allow(missing_docs)]

//! Issue #1498 — a fact table with a path on its rows rolls up by tree level and filters by
//! node id.
//!
//! #1460 gave an aggregate `where` the ltree path operators. Still missing for an org-chart,
//! location or taxonomy fact table: grouping by a tree level (`subpath`) or depth
//! (`nlevel`), and `descendant_of_id` / `ancestor_of_id`, which resolve a node id through a
//! declared hierarchy. Driven through the real executor against PostgreSQL.

mod common;

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, HierarchyDefinition},
};
use serde_json::{Value, json};

const TABLE: &str = "tf_issue_1498";
const NODES: &str = "tb_issue_1498_node";
const NODE_AB: &str = "00000000-0000-0000-0000-0000000014ab";
const NODE_ABC: &str = "00000000-0000-0000-0000-00000014abc0";

async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE EXTENSION IF NOT EXISTS ltree").await.unwrap();
    for table in [TABLE, NODES] {
        adapter
            .execute_raw_query(&format!("DROP TABLE IF EXISTS {table}"))
            .await
            .unwrap();
    }
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {TABLE} (revenue numeric NOT NULL, path ltree NOT NULL, \
             region text NOT NULL, data jsonb NOT NULL DEFAULT '{{}}')"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {TABLE} (revenue, path, region) VALUES \
             (1, 'a', 'eu'), (2, 'a.b', 'eu'), (4, 'a.b.c', 'us'), (8, 'a.x', 'us'), \
             (16, 'z', 'eu')"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {NODES} (id uuid PRIMARY KEY, path ltree NOT NULL)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {NODES} VALUES ('{NODE_AB}', 'a.b'), ('{NODE_ABC}', 'a.b.c')"
        ))
        .await
        .unwrap();
}

fn metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            FilterColumn {
                name:      "path".to_string(),
                sql_type:  SqlType::Ltree,
                indexed:   true,
                hierarchy: Some("node".to_string()),
            },
            FilterColumn {
                name:      "region".to_string(),
                sql_type:  SqlType::Text,
                indexed:   true,
                hierarchy: None,
            },
        ],
        calendar_dimensions:      vec![],
        partial_period:           None,
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

async fn executor() -> Executor {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let mut schema = CompiledSchema::new();
    schema.hierarchies_config = Some(HashMap::from([(
        "node".to_string(),
        HierarchyDefinition {
            table:       NODES.to_string(),
            path_column: "path".to_string(),
        },
    )]));
    Executor::new(schema, adapter)
}

async fn run(executor: &Executor, query: Value) -> fraiseql_core::error::Result<Vec<Value>> {
    let response = executor.execute_aggregate_query(&query, "sales_aggregate", &metadata()).await?;
    Ok(response["data"]["sales_aggregate"].as_array().cloned().unwrap_or_default())
}

fn total(row: &Value) -> f64 {
    row["revenue_sum"]
        .as_f64()
        .or_else(|| row["revenue_sum"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("no revenue_sum in {row}"))
}

/// `{group key → revenue}` for a grouped aggregate.
async fn grouped(executor: &Executor, group_by: Value, key: &str) -> HashMap<String, f64> {
    let rows = run(
        executor,
        json!({ "table": TABLE, "groupBy": group_by, "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await
    .unwrap_or_else(|e| panic!("group by failed: {e}"));
    rows.iter()
        .map(|row| {
            let k = row.get(key).unwrap_or_else(|| panic!("no `{key}` in {row}"));
            (k.as_str().map_or_else(|| k.to_string(), str::to_string), total(row))
        })
        .collect()
}

async fn revenue_where(executor: &Executor, filter: Value) -> f64 {
    let rows = run(
        executor,
        json!({ "table": TABLE, "where": filter, "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await
    .unwrap_or_else(|e| panic!("aggregate with where {filter} failed: {e}"));
    total(&rows[0])
}

#[tokio::test]
async fn a_path_rolls_up_to_a_tree_level() {
    let executor = executor().await;

    let level_1 = grouped(&executor, json!({ "path": { "level": 1 } }), "path").await;
    assert_eq!(level_1, HashMap::from([("a".into(), 15.0), ("z".into(), 16.0)]));

    // A path shallower than the level keeps its own value: `a` stays `a` at level 2.
    let level_2 = grouped(&executor, json!({ "path": { "level": 2 } }), "path").await;
    assert_eq!(
        level_2,
        HashMap::from([
            ("a".into(), 1.0),
            ("a.b".into(), 6.0),
            ("a.x".into(), 8.0),
            ("z".into(), 16.0)
        ])
    );
}

#[tokio::test]
async fn a_path_groups_by_its_depth() {
    let executor = executor().await;
    let depth = grouped(&executor, json!({ "path": "depth" }), "path_depth").await;
    assert_eq!(
        depth,
        HashMap::from([("1".into(), 17.0), ("2".into(), 10.0), ("3".into(), 4.0)])
    );
}

#[tokio::test]
async fn a_tree_level_combines_with_another_dimension() {
    let executor = executor().await;
    let rows = run(
        &executor,
        json!({ "table": TABLE, "groupBy": { "path": { "level": 1 }, "region": true },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await
    .unwrap();
    let got: HashMap<(String, String), f64> = rows
        .iter()
        .map(|r| {
            (
                (
                    r["path"].as_str().unwrap().to_string(),
                    r["region"].as_str().unwrap().to_string(),
                ),
                total(r),
            )
        })
        .collect();
    assert_eq!(
        got,
        HashMap::from([
            (("a".into(), "eu".into()), 3.0),
            (("a".into(), "us".into()), 12.0),
            (("z".into(), "eu".into()), 16.0)
        ])
    );
}

#[tokio::test]
async fn node_id_filters_resolve_through_the_declared_hierarchy() {
    let executor = executor().await;
    // Descendants of the node at a.b: a.b, a.b.c
    assert_eq!(revenue_where(&executor, json!({ "path_descendant_of_id": NODE_AB })).await, 6.0);
    assert_eq!(revenue_where(&executor, json!({ "path_descendantOfId": NODE_AB })).await, 6.0);
    // Ancestors of the node at a.b.c: a, a.b, a.b.c
    assert_eq!(revenue_where(&executor, json!({ "path_ancestor_of_id": NODE_ABC })).await, 7.0);
}

#[tokio::test]
async fn a_tree_level_needs_an_ltree_column() {
    let executor = executor().await;
    let err = run(
        &executor,
        json!({ "table": TABLE, "groupBy": { "region": { "level": 1 } },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("region") && err.contains("ltree"), "{err}");
}

#[tokio::test]
async fn a_node_id_filter_needs_a_declared_hierarchy() {
    let executor = executor().await;
    let err = run(
        &executor,
        json!({ "table": TABLE, "where": { "region_descendant_of_id": NODE_AB },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("region") && err.contains("hierarchy"), "{err}");
}
