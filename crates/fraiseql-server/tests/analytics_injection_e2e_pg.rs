//! #794 / #795 regression: the analytics path must not let a client inject SQL.
//!
//! This is the gate for both CRITICALs. The unit tests in
//! `fraiseql-core/src/compiler/window_functions/tests.rs` pin the planner's validation
//! functions; only this test proves the property that actually matters — that a request
//! arriving over **HTTP `/graphql`**, against a **real database**, through the **real
//! handler**, cannot read a relation it was not granted.
//!
//! Why the response assertion is load-bearing as well as the error assertion:
//! `WindowProjector::project` copies *every* column the database returned into the
//! GraphQL response, regardless of what the plan asked for. So "the query errored" and
//! "the injected column is absent" are genuinely different claims, and a fix that only
//! made the query fail *later* would satisfy the first while still leaking. Both are
//! asserted for every payload.
//!
//! The payloads are taken verbatim from the issue bodies, where each was verified to
//! generate SQL that executes against PostgreSQL 16 and returns `pg_authid` contents.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_injprobe` fixture → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use axum::{Router, body::Body, routing::post};
use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_server::routes::graphql::{AppState, graphql_handler};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

/// The fact table the root field `injprobe_window` / `injprobe_aggregate` resolves to.
///
/// Deliberately **not** `tf_sales`: that name is a seeded fixture in
/// `tests/sql/postgres/init-analytics.sql` which `fraiseql-core`'s
/// `fact_table_integration` suite introspects. Dropping and recreating it here would
/// silently break those tests against the shared database.
const FACT_TABLE: &str = "tf_injprobe";

/// Metadata as the compiled schema would carry it: one measure, one declared dimension.
fn sales_metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               FACT_TABLE.to_string(),
        measures:                 vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:               DimensionColumn {
            name:  "dimensions".to_string(),
            paths: vec![DimensionPath {
                name:      "category".to_string(),
                json_path: "dimensions->>'category'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        partial_period:           None,
        native_measures:          std::collections::HashMap::new(),
        native_dimension_mapping: std::collections::HashMap::new(),
    }
}

/// Create the fact table the analytics path needs, so a *successful* injection would
/// really execute rather than fail on a missing relation.
async fn setup() -> Option<Router> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");

    for stmt in [
        "DROP TABLE IF EXISTS tf_injprobe",
        "CREATE TABLE tf_injprobe (revenue numeric NOT NULL, dimensions jsonb NOT NULL)",
        "INSERT INTO tf_injprobe VALUES (100, '{\"category\":\"Electronics\"}')",
    ] {
        let _: Vec<std::collections::HashMap<String, serde_json::Value>> =
            adapter.execute_raw_query(stmt).await.expect("fixture setup");
    }

    let mut schema = CompiledSchema::new();
    schema.add_fact_table(FACT_TABLE.to_string(), sales_metadata());

    let state = AppState::new(Arc::new(Executor::new(schema, Arc::new(adapter))));
    Some(
        Router::new()
            .route("/graphql", post(graphql_handler::<PostgresAdapter>))
            .with_state(state),
    )
}

async fn post_graphql(router: Router, body: &Value) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// Assert the request was refused **and** that nothing the attacker asked for came back.
///
/// Two different claims, checked against two different scopes:
///
/// * `injected_keys` — attacker-chosen alias names (`leak`, `whoami`). A validation error
///   legitimately quotes the input it rejected, so these are asserted absent from `data` only;
///   finding one there would mean the column was actually projected.
/// * `catalog_values` — data that only exists inside the database (`pg_database_owner`, a
///   `version()` string). These must not appear **anywhere** in the response, in any field, under
///   any circumstances: their presence is proof of exfiltration.
async fn assert_injection_refused(
    router: Router,
    body: &Value,
    injected_keys: &[&str],
    catalog_values: &[&str],
    label: &str,
) {
    let (status, response) = post_graphql(router, body).await;

    assert!(
        response.get("errors").is_some(),
        "{label}: expected a GraphQL error, got {status} {response}"
    );

    let data = response.get("data").map(ToString::to_string).unwrap_or_default();
    for needle in injected_keys {
        assert!(
            !data.contains(needle),
            "{label}: injected column '{needle}' was projected into data: {data}"
        );
    }

    let serialized = response.to_string();
    for needle in catalog_values {
        assert!(
            !serialized.contains(needle),
            "{label}: database contents '{needle}' reached the response: {serialized}"
        );
    }
}

/// #794 sink 3 — a measure alias that appends a whole extra SELECT column.
#[tokio::test]
async fn window_measure_alias_cannot_exfiltrate_pg_authid() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table":  FACT_TABLE,
            "select": [{
                "type":  "measure",
                "name":  "revenue",
                "alias": "c, (SELECT string_agg(rolname, ',') FROM pg_authid) AS leak"
            }]
        }
    });

    assert_injection_refused(
        router,
        &body,
        &["leak"],
        &["pg_database_owner"],
        "#794 measure alias",
    )
    .await;
}

/// #794 sink 1 — a dimension path that breaks out of the single-quoted JSONB key.
#[tokio::test]
async fn window_dimension_path_cannot_break_out_of_the_jsonb_key() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table":  FACT_TABLE,
            "select": [{
                "type":  "dimension",
                "path":  "category'||(SELECT rolname FROM pg_authid LIMIT 1)||'",
                "alias": "d"
            }]
        }
    });

    assert_injection_refused(
        router,
        &body,
        &["d"],
        &["pg_database_owner", "Electronicspg"],
        "#794 dimension path",
    )
    .await;
}

/// #794 sink 3 — the window-function alias reaches `write!(sql, " AS {}", …)`.
#[tokio::test]
async fn window_function_alias_cannot_exfiltrate_the_current_user() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table":   FACT_TABLE,
            "select":  [],
            "windows": [{
                "function": {"type": "row_number"},
                "alias":    "rank, (SELECT current_user) AS whoami"
            }]
        }
    });

    assert_injection_refused(router, &body, &["whoami"], &[], "#794 window alias").await;
}

/// #794 sink 2 — the PARTITION BY dimension path repeats the same interpolation.
#[tokio::test]
async fn window_partition_by_path_cannot_break_out_of_the_jsonb_key() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table":   FACT_TABLE,
            "select":  [],
            "windows": [{
                "function":    {"type": "row_number"},
                "alias":       "rank",
                "partitionBy": [{"type": "dimension", "path": "x'||(SELECT version())||'"}]
            }]
        }
    });

    assert_injection_refused(router, &body, &[], &["PostgreSQL 1"], "#794 partitionBy path").await;
}

/// #795 — a subquery substituted for the FROM target on the window path.
#[tokio::test]
async fn window_table_cannot_substitute_the_relation() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table": "(SELECT jsonb_build_object('category', rolname) AS dimensions \
                      FROM pg_authid) AS x",
            // Select a *dimension*, not a measure: the substituted subquery exposes a
            // `dimensions` column, so pre-fix this SQL is valid and returns role names.
            // Asking for `revenue` instead would fail on a missing column, and the test
            // would pass for the wrong reason.
            "select": [{"type": "dimension", "path": "category", "alias": "r"}]
        }
    });

    assert_injection_refused(router, &body, &[], &["pg_database_owner"], "#795 window table").await;
}

/// #795 — the aggregate half, which is also the RLS-bypass half: the policy is looked
/// up by this same name, so an unpolicied relation composed no tenant filter at all.
#[tokio::test]
async fn aggregate_table_cannot_substitute_the_relation() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_aggregate { count } }",
        "variables": {
            "table": "(SELECT jsonb_build_object('category', rolname) AS dimensions \
                      FROM pg_authid) AS x",
            "groupBy":    {"category": true},
            "aggregates": [{"count": {}}]
        }
    });

    assert_injection_refused(router, &body, &[], &["pg_database_owner"], "#795 aggregate table")
        .await;
}

/// #795 — a plain identifier naming another relation is the RLS-bypass shape, and only
/// reconciliation against the resolved metadata catches it.
#[tokio::test]
async fn aggregate_table_cannot_name_another_relation() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_aggregate { count } }",
        "variables": {
            "table":      "pg_authid",
            "aggregates": [{"count": {}}]
        }
    });

    assert_injection_refused(
        router,
        &body,
        &[],
        &["pg_database_owner"],
        "#795 aggregate foreign table",
    )
    .await;
}

/// The guard must not break the query shape it protects — otherwise "everything errors"
/// would pass every assertion above.
#[tokio::test]
async fn a_legitimate_window_query_still_succeeds_over_http() {
    let Some(router) = setup().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let body = json!({
        "query": "{ injprobe_window { rank } }",
        "variables": {
            "table":   FACT_TABLE,
            "select":  [
                {"type": "measure",   "name": "revenue",  "alias": "revenue"},
                {"type": "dimension", "path": "category", "alias": "category"}
            ],
            "windows": [{
                "function":    {"type": "row_number"},
                "alias":       "rank",
                "partitionBy": [{"type": "dimension", "path": "category"}]
            }]
        }
    });

    let (status, response) = post_graphql(router, &body).await;

    assert_eq!(status, StatusCode::OK, "legitimate query must succeed: {response}");
    assert!(
        response.get("errors").is_none(),
        "legitimate window query must not error: {response}"
    );
}
