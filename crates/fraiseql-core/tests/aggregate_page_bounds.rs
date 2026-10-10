#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1532: an aggregate's and a window query's `limit` / `offset` are read as a list's are.
//!
//! A value that is not a non-negative `Int` is refused, a `limit` above `[validation]
//! max_page_size` is refused, and both before any statement. They used to be read with
//! `as_u64()`, so `-1`, `1.5` and `"10"` meant *no limit* and `2^32` silently saturated to
//! `u32::MAX`; and `limit` had no page ceiling at all.
//!
//! The requests enter through [`Executor::execute`], as a client's do: the classifier sends
//! an `_aggregate` / `_window` root to the fact-table planners with the request's
//! **variables** as the query, so that is where `limit` and `offset` are sent. An inline
//! argument on the root is not read by anything, so it is refused rather than ignored.
//!
//! `tf_pgb_boom` raises on every row, so a refused request is answered by the refusal; had a
//! statement reached it, PostgreSQL's exception would be the answer. `tf_pgb_sale` serves
//! the requests that are within bounds.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_pgb_*` relations.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const SALE: &str = "tf_pgb_sale";
const BOOM: &str = "tf_pgb_boom";
/// The page ceiling the executor runs under; the fixture has four groups.
const MAX_PAGE_SIZE: u32 = 3;

fn fact_table(table: &str) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               table.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:       "qty".to_string(),
            sql_type:   SqlType::BigInt,
            nullable:   false,
            additivity: fraiseql_core::compiler::fact_table::Additivity::Additive,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "data->>'region'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP VIEW IF EXISTS {BOOM}"),
        format!("DROP TABLE IF EXISTS {SALE}"),
        "DROP FUNCTION IF EXISTS tf_pgb_boom_row(bigint)".to_string(),
        format!("CREATE TABLE {SALE} (id bigint, qty bigint, data jsonb)"),
        format!(
            "INSERT INTO {SALE} SELECT k, k, jsonb_build_object('region', 'r' || k) FROM \
             generate_series(1, 4) k"
        ),
        "CREATE FUNCTION tf_pgb_boom_row(bigint) RETURNS jsonb LANGUAGE plpgsql AS $$ BEGIN \
         RAISE EXCEPTION 'tf_pgb_boom was read'; END $$"
            .to_string(),
        format!("CREATE VIEW {BOOM} AS SELECT id, qty, tf_pgb_boom_row(id) AS data FROM {SALE}"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(SALE.to_string(), fact_table(SALE));
    schema.add_fact_table(BOOM.to_string(), fact_table(BOOM));
    let config = RuntimeConfig {
        max_page_size: Some(MAX_PAGE_SIZE),
        ..RuntimeConfig::default()
    };
    Some(Executor::with_config(schema, Arc::new(adapter), config))
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Aggregate,
    Window,
}

impl Kind {
    const BOTH: [Self; 2] = [Self::Aggregate, Self::Window];

    fn root(self, table: &str) -> String {
        let name = table.strip_prefix("tf_").unwrap();
        match self {
            Self::Aggregate => format!("{name}_aggregate"),
            Self::Window => format!("{name}_window"),
        }
    }

    /// The selection the projector reads: `region` on both shapes.
    fn document(self, table: &str, inline: &str) -> String {
        let root = self.root(table);
        match self {
            Self::Aggregate => format!("{{ {root}{inline} {{ region count }} }}"),
            Self::Window => format!("{{ {root}{inline} {{ region position }} }}"),
        }
    }

    /// The request, ordered by region, with `bounds` merged in.
    fn variables(self, table: &str, bounds: &Value) -> Value {
        let mut request = match self {
            Self::Aggregate => json!({
                "table": table,
                "groupBy": { "region": true },
                "aggregates": [{ "count": {} }],
                "orderBy": { "region": "ASC" },
            }),
            Self::Window => json!({
                "table": table,
                "select": [{ "type": "dimension", "path": "region", "alias": "region" }],
                "windows": [{ "function": { "type": "row_number" }, "alias": "position" }],
                "orderBy": [{ "field": "region", "direction": "ASC" }],
            }),
        };
        for (k, v) in bounds.as_object().unwrap() {
            request[k] = v.clone();
        }
        request
    }
}

async fn run(
    executor: &Executor,
    kind: Kind,
    table: &str,
    bounds: &Value,
) -> Result<Value, String> {
    executor
        .execute(&kind.document(table, ""), Some(&kind.variables(table, bounds)))
        .await
        .map_err(|e| e.to_string())
}

fn regions(response: &Value, kind: Kind, table: &str) -> Vec<Value> {
    response["data"][kind.root(table)]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| g["region"].clone())
        .collect()
}

fn assert_refused_before_any_statement(result: Result<Value, String>, names: &str, why: &str) {
    let err = result.expect_err(why);
    assert!(!err.contains("was read"), "{why}: refused before any statement: {err}");
    assert!(err.contains(&format!("`{names}`")), "{why}: names the argument: {err}");
}

/// The bounds within range are applied, so the refusals below are refusals of the value
/// and not of the shape.
#[tokio::test]
async fn bounds_within_range_page_the_groups() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1532: DATABASE_URL not set");
        return;
    };
    for kind in Kind::BOTH {
        let page = run(&executor, kind, SALE, &json!({ "limit": 2, "offset": 1 })).await.unwrap();
        assert_eq!(regions(&page, kind, SALE), vec![json!("r2"), json!("r3")], "{kind:?}");
    }
}

#[tokio::test]
async fn a_limit_that_is_not_a_non_negative_int_is_refused() {
    let Some(executor) = executor().await else {
        return;
    };
    for kind in Kind::BOTH {
        for limit in [
            json!(-1),
            json!(1.5),
            json!("10"),
            json!(4_294_967_296_u64),
            json!(true),
        ] {
            let result = run(&executor, kind, BOOM, &json!({ "limit": limit })).await;
            assert_refused_before_any_statement(
                result,
                "limit",
                &format!("{kind:?} limit {limit}"),
            );
        }
    }
}

#[tokio::test]
async fn an_offset_that_is_not_a_non_negative_int_is_refused() {
    let Some(executor) = executor().await else {
        return;
    };
    for kind in Kind::BOTH {
        for offset in [json!(-1), json!(2.5), json!("1"), json!(4_294_967_296_u64)] {
            let result = run(&executor, kind, BOOM, &json!({ "offset": offset })).await;
            assert_refused_before_any_statement(
                result,
                "offset",
                &format!("{kind:?} offset {offset}"),
            );
        }
    }
}

#[tokio::test]
async fn a_limit_above_the_page_ceiling_is_refused() {
    let Some(executor) = executor().await else {
        return;
    };
    for kind in Kind::BOTH {
        let at = run(&executor, kind, SALE, &json!({ "limit": MAX_PAGE_SIZE })).await.unwrap();
        assert_eq!(regions(&at, kind, SALE).len(), 3, "{kind:?}: the ceiling itself is served");

        let result = run(&executor, kind, BOOM, &json!({ "limit": MAX_PAGE_SIZE + 1 })).await;
        let why = format!("{kind:?} limit above max_page_size");
        assert_refused_before_any_statement(result.clone(), "limit", &why);
        assert!(result.unwrap_err().contains("maximum page size"), "{why}");
    }
}

/// An inline argument is not where the planners read `limit`: it used to be dropped, and the
/// whole table served under a `200`.
#[tokio::test]
async fn an_inline_argument_on_the_root_is_refused_not_ignored() {
    let Some(executor) = executor().await else {
        return;
    };
    for kind in Kind::BOTH {
        let result = executor
            .execute(&kind.document(SALE, "(limit: 1)"), Some(&kind.variables(SALE, &json!({}))))
            .await;
        let err = match result {
            Ok(served) => {
                panic!("{kind:?}: the inline limit was ignored and this was served: {served}")
            },
            Err(e) => e.to_string(),
        };
        assert!(err.contains("variables"), "{kind:?}: names where the request goes: {err}");
    }
}
