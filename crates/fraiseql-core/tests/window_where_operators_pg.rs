#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! A fact table's window root runs every `where` operator it publishes, as its aggregate root
//! does.
//!
//! Both roots publish the one `<Name>AggregateWhere` input. The window root lowered its filter
//! through the generic `where` generator, which runs only `eq` and `neq` on a column, and no
//! node-id filter at all: every other published operator was refused before SQL. Each field
//! of the published input is read here from the schema, not listed by hand, and run through
//! both roots against PostgreSQL: the window root must answer it, and hold exactly the rows
//! the aggregate root counts.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** the suite creates and drops its own `tf_window_ops` and `tb_window_ops_node`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactRowScope, FactTableMetadata, FilterColumn,
        MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, HierarchyDefinition},
};
use serde_json::{Value, json};

const TABLE: &str = "tf_window_ops";
const NODES: &str = "tb_window_ops_node";
const NODE_AB: &str = "00000000-0000-0000-0000-0000000000ab";
const KEY: &str = "00000000-0000-0000-0000-000000000002";

fn filter(name: &str, sql_type: SqlType, hierarchy: Option<&str>) -> FilterColumn {
    FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: false,
        hierarchy: hierarchy.map(str::to_string),
    }
}

/// A fact table with a filter column of every type a filter publishes operators for, a JSONB
/// dimension, and a dimension mapped to a column.
fn metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![MeasureColumn {
            name:       "revenue".to_string(),
            sql_type:   SqlType::Decimal,
            nullable:   false,
            additivity: Additivity::Additive,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "category".to_string(),
                json_path: "data->>'category'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters:     vec![
            filter("id", SqlType::Int, None),
            filter("region", SqlType::Text, None),
            filter("n", SqlType::Int, None),
            filter("big", SqlType::BigInt, None),
            filter("price", SqlType::Decimal, None),
            filter("ratio", SqlType::Float, None),
            filter("flag", SqlType::Boolean, None),
            filter("key", SqlType::Uuid, None),
            filter("day", SqlType::Date, None),
            filter("at", SqlType::TimestampTz, None),
            filter("wall", SqlType::LocalTimestamp, None),
            filter("path", SqlType::Ltree, Some("node")),
        ],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::from([("shelf".to_string(), "n".to_string())]),
    }
}

/// The fact table and its node table, and an executor whose schema publishes its roots and
/// declares the hierarchy; `None` without a database.
async fn executor() -> Option<Executor> {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        "CREATE EXTENSION IF NOT EXISTS ltree".to_string(),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("DROP TABLE IF EXISTS {NODES}"),
        format!(
            "CREATE TABLE {TABLE} (id int, region text, n int, big bigint, price numeric, \
             ratio float8, flag bool, key uuid, day date, at timestamptz, wall timestamp, \
             path ltree, revenue numeric, data jsonb)"
        ),
        format!(
            "INSERT INTO {TABLE} VALUES \
             (1, 'eu', 1, 1, 1.5, 1.5, true, '00000000-0000-0000-0000-000000000001', \
              '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00', 'a', 1, \
              '{{\"category\": \"apple\"}}'), \
             (2, 'us', 2, 2, 2.5, 2.5, false, '{KEY}', \
              '2026-01-02', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00', 'a.b', 2, \
              '{{\"category\": \"banana\"}}'), \
             (3, 'asia', 3, 3, 3.5, 3.5, true, '00000000-0000-0000-0000-000000000003', \
              '2026-01-03', '2026-01-03T00:00:00Z', '2026-01-03T00:00:00', 'a.b.c', 4, \
              '{{\"category\": \"cherry\"}}'), \
             (4, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 'z', 8, '{{}}')"
        ),
        format!("CREATE TABLE {NODES} (id uuid PRIMARY KEY, path ltree NOT NULL)"),
        format!("INSERT INTO {NODES} VALUES ('{NODE_AB}', 'a.b')"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.hierarchies_config = Some(HashMap::from([(
        "node".to_string(),
        HierarchyDefinition {
            table:       NODES.to_string(),
            path_column: "path".to_string(),
        },
    )]));
    schema.add_fact_table(TABLE.to_string(), metadata());
    schema.build_indexes();
    Some(Executor::new(schema, Arc::new(adapter)))
}

/// An operand of `field_type` for the published field `name`, one some seeded row matches.
fn operand(name: &str, field_type: &str) -> Value {
    if let Some(item) = field_type.strip_prefix('[').and_then(|t| t.strip_suffix("!]")) {
        return json!([operand(name, item), operand(name, item)]);
    }
    if name.starts_with("path_") && field_type == "String" {
        return json!(if name.ends_with("_ltxtquery") {
            "b"
        } else {
            "a.b"
        });
    }
    match field_type {
        "String" if name.starts_with("category_") => json!("banana"),
        "String" if name.starts_with("region_") => json!("us"),
        "Int" | "BigInt" if name.starts_with("big_") => json!(2),
        "Int" => json!(2),
        "BigInt" | "Decimal" if name.starts_with("price_") => json!("2.5"),
        "BigInt" => json!("2"),
        "Float" => json!(2.5),
        "Boolean" => json!(true),
        "UUID" if name.ends_with("_of_id") => json!(NODE_AB),
        "UUID" => json!(KEY),
        "Date" => json!("2026-01-02"),
        "DateTime" => json!("2026-01-02T00:00:00Z"),
        "LocalDateTime" => json!("2026-01-02T00:00:00"),
        other => panic!("`{name}`: no operand for `{other}`"),
    }
}

/// Every published `where` field runs on the window root and holds the rows the aggregate
/// root counts for it.
#[tokio::test]
async fn the_window_root_runs_every_where_operator_it_publishes() {
    let Some(executor) = executor().await else {
        return;
    };
    let schema = executor.schema();
    let window_where = schema
        .fact_root("window_ops_window")
        .and_then(|root| root.arguments.iter().find(|a| a.name == "where"))
        .map(|a| a.arg_type.to_graphql_string())
        .expect("the window root publishes a `where`");
    let fields: Vec<(String, String)> = schema
        .find_input_type(&window_where)
        .unwrap_or_else(|| panic!("`{window_where}` is published"))
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.field_type.clone()))
        .collect();

    let mut failures = Vec::new();
    for (name, field_type) in &fields {
        let filter = json!({ name: operand(name, field_type) });
        let counted = executor
            .execute_aggregate_query(
                &json!({ "aggregates": [{ "count": true }], "where": filter }),
                "window_ops_aggregate",
                &metadata(),
            )
            .await
            .unwrap_or_else(|e| panic!("the aggregate root runs `{name}`: {e}"));
        let expected = &counted["data"]["window_ops_aggregate"][0]["count"];
        let windowed = executor
            .execute_window_query(
                &json!({
                    "select": [{ "type": "filter", "name": "id", "alias": "id" }],
                    "windows": [{ "function": { "type": "row_number" }, "alias": "n",
                                  "orderBy": [{ "field": "id", "direction": "ASC" }] }],
                    "where": filter
                }),
                "window_ops_window",
                &metadata(),
            )
            .await;
        match windowed {
            Ok(response) => {
                let rows = response["data"]["window_ops_window"].as_array().map(Vec::len);
                if rows.map(|n| json!(n)).as_ref() != Some(expected) {
                    failures
                        .push(format!("{filter}: {rows:?} rows, the aggregate counts {expected}"));
                }
            },
            Err(error) => failures.push(format!("{filter}: refused: {error}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} operators:\n{}",
        failures.len(),
        fields.len(),
        failures.join("\n")
    );

    // The walk read the whole published input: each kind of operator is in it.
    for published in [
        "region_icontains",
        "category_startswith",
        "n_gt",
        "shelf_lte",
        "big_in",
        "price_nin",
        "ratio_gte",
        "day_lt",
        "at_gt",
        "wall_lte",
        "key_in",
        "flag_isnull",
        "path_descendant_of",
        "path_matches_any_lquery",
        "path_depth_gte",
        "path_descendant_of_id",
        "path_ancestor_of_id",
    ] {
        assert!(fields.iter().any(|(name, _)| name == published), "`{published}` is published");
    }
}
