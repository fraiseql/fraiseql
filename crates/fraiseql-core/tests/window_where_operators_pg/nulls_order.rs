//! A window's `orderBy` item puts its key's NULLs first or last, as its direction names
//! (`DESC_NULLS_LAST`, …), inside `OVER (…)` and in the window rows' own `orderBy`.
//!
//! Inside `OVER`, the placement changes the answers, not only their order: a NULL score ranks
//! first or last, and `lag` reads across the NULLs in the order they were put. Each answer is
//! compared with the one PostgreSQL gives the same `ORDER BY … NULLS …`.
//!
//! **Parallelism:** creates and drops its own `tf_window_nulls`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn,
        SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_window_nulls";
const ROOT: &str = "window_nulls_window";

/// `(id, score)`: tied scores, and NULLs among them.
const ROWS: [(i64, Option<i64>); 7] = [
    (1, Some(30)),
    (2, None),
    (3, Some(10)),
    (4, Some(30)),
    (5, None),
    (6, Some(20)),
    (7, None),
];

fn metadata() -> FactTableMetadata {
    let filter = |name: &str| FilterColumn {
        name:      name.to_string(),
        sql_type:  SqlType::Int,
        indexed:   false,
        hierarchy: None,
    };
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
            column:     None,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![filter("id"), filter("score")],
        calendar_dimensions:      vec![],
        grain:                    None,
        native_dimension_mapping: HashMap::new(),
    }
}

async fn setup() -> Option<(PostgresAdapter, Executor)> {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let values: Vec<String> = ROWS
        .iter()
        .map(|(id, score)| {
            let score = score.map_or_else(|| "NULL".to_string(), |s| s.to_string());
            format!("({id}, {score}, 1, '{{}}')")
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id int, score int, revenue numeric, data jsonb)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(TABLE.to_string(), metadata());
    schema.build_indexes();
    let executor = Executor::new(schema, Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

/// A column of a row as an integer, or `None` for NULL, whether it came back a number or text.
fn int(row: &Value, column: &str) -> Option<i64> {
    let value = &row[column];
    value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

async fn window_rows(
    executor: &Executor,
    query: &Value,
    columns: &[&str],
) -> Vec<Vec<Option<i64>>> {
    let response = executor
        .execute_window_query(query, ROOT, &metadata())
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    response["data"][ROOT]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|row| columns.iter().map(|c| int(row, c)).collect())
        .collect()
}

async fn oracle(adapter: &PostgresAdapter, sql: &str, columns: &[&str]) -> Vec<Vec<Option<i64>>> {
    adapter
        .execute_raw_query(sql)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            let row = serde_json::to_value(row).unwrap();
            columns.iter().map(|c| int(&row, c)).collect()
        })
        .collect()
}

fn key(field: &str, direction: &str, nulls: &str) -> Value {
    json!({ "field": field, "direction": format!("{direction}_NULLS_{nulls}") })
}

/// Inside `OVER`: a rank and a `lag` under each non-default placement, as PostgreSQL computes
/// them for the same `NULLS` clause.
#[tokio::test]
async fn a_window_ranks_and_reads_across_nulls_where_its_order_puts_them() {
    let Some((adapter, executor)) = setup().await else {
        return;
    };
    let query = json!({
        "select": [{ "type": "filter", "name": "id", "alias": "id" }],
        "windows": [
            { "function": { "type": "rank" }, "alias": "high",
              "orderBy": [key("score", "DESC", "LAST")] },
            { "function": { "type": "rank" }, "alias": "low",
              "orderBy": [key("score", "ASC", "FIRST")] },
            { "function": { "type": "lag", "field": "id", "offset": 1 }, "alias": "before",
              "orderBy": [key("score", "DESC", "LAST"), { "field": "id", "direction": "ASC" }] }
        ],
        "orderBy": [{ "field": "id", "direction": "ASC" }]
    });
    let columns = ["id", "high", "low", "before"];
    assert_eq!(
        window_rows(&executor, &query, &columns).await,
        oracle(
            &adapter,
            &format!(
                "SELECT id, rank() OVER (ORDER BY score DESC NULLS LAST) AS high, \
                 rank() OVER (ORDER BY score ASC NULLS FIRST) AS low, \
                 lag(id, 1) OVER (ORDER BY score DESC NULLS LAST, id ASC) AS before \
                 FROM {TABLE} ORDER BY id"
            ),
            &columns
        )
        .await
    );
}

/// The window rows' own `orderBy`, on a filter column and on a `lag` alias (NULL on the first
/// row it reads), each under the placement that is not its direction's default.
#[tokio::test]
async fn the_window_rows_sort_their_nulls_where_the_item_asks() {
    let Some((adapter, executor)) = setup().await else {
        return;
    };
    let previous = json!({ "function": { "type": "lag", "field": "id", "offset": 1 },
                           "alias": "previous",
                           "orderBy": [{ "field": "id", "direction": "ASC" }] });
    for (order, sql_order) in [
        (
            vec![key("score", "DESC", "LAST"), json!({ "field": "id" })],
            "score DESC NULLS LAST, id",
        ),
        (
            vec![key("score", "ASC", "FIRST"), json!({ "field": "id" })],
            "score ASC NULLS FIRST, id",
        ),
        (vec![key("previous", "DESC", "LAST")], "previous DESC NULLS LAST"),
    ] {
        let query = json!({
            "select": [{ "type": "filter", "name": "id", "alias": "id" }],
            "windows": [previous],
            "orderBy": order
        });
        assert_eq!(
            window_rows(&executor, &query, &["id"]).await,
            oracle(
                &adapter,
                &format!(
                    "SELECT id FROM (SELECT id, score, lag(id, 1) OVER (ORDER BY id ASC) AS \
                     previous FROM {TABLE}) w ORDER BY {sql_order}"
                ),
                &["id"]
            )
            .await,
            "{sql_order}"
        );
    }
}
