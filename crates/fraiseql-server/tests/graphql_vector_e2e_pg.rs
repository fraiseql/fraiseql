//! #386: pgvector similarity search, executed end to end against PostgreSQL.
//!
//! The first vector test in this repository that executes SQL: every earlier
//! "vector test" asserted `sql.contains("<=>")`, which is how a mis-parenthesised
//! cast, a jsonb-bound operand and a non-boolean predicate all shipped invisibly.
//! These tests assert **row identity and row order** returned by a real pgvector
//! instance through the real `/graphql` handler.
//!
//! Requires the pgvector extension (the rigs run `pgvector/pgvector:pg16`;
//! `CREATE EXTENSION` here fails loudly on a non-pgvector server rather than
//! self-skipping — a silently skipped suite reads as passing).
//!
//! **Execution engine:** `PostgreSQL` + pgvector · **Infrastructure:**
//! `DATABASE_URL` · **Parallelism:** creates and drops its own `p27_vec` schema →
//! run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use axum::{Router, body::Body, routing::post};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition, VectorConfig,
    },
};
use fraiseql_server::routes::graphql::{AppState, graphql_handler};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const SCHEMA: &str = "p27_vec";

/// Four documents with hand-computed distances to the query vector `[1,0,0]`,
/// chosen so the three metrics produce three DIFFERENT orders — an
/// always-cosine (or unordered) implementation cannot pass all three tests.
///
/// | id | vector        | cosine dist | L2 dist | raw inner product |
/// |----|---------------|-------------|---------|-------------------|
/// | 1  | [1, 0, 0]     | 0           | 0       | 1                 |
/// | 2  | [5, 0.01, 0]  | ~2e-6       | ~4.0    | 5                 |
/// | 3  | [0.1, 0.02,0] | ~0.0194     | ~0.9    | 0.1               |
/// | 4  | [0, 1, 0]     | 1           | ~1.41   | 0                 |
///
/// cosine order: 1,2,3,4 · L2 order: 1,3,4,2 · inner-product order: 2,1,3,4.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        "CREATE EXTENSION IF NOT EXISTS vector".to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        // `thumbnail` is a SECOND vector field, of a different dimension and a
        // different metric, whose nearest-neighbour order is the REVERSE of
        // `embedding`'s (#959). A `field:` selector that quietly searched the first
        // vector field would return the other order, which is the only assertion
        // that can tell the two apart.
        format!(
            "CREATE TABLE {SCHEMA}.tb_doc (id bigint PRIMARY KEY, title text NOT NULL, \
             embedding vector(3) NOT NULL, thumbnail vector(2) NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_doc VALUES \
             (1, 'exact',      '[1,0,0]',      '[4,0]'), \
             (2, 'longer',     '[5,0.01,0]',   '[3,0]'), \
             (3, 'shorter',    '[0.1,0.02,0]', '[2,0]'), \
             (4, 'orthogonal', '[0,1,0]',      '[1,0]')"
        ),
        // The view exposes the vector BOTH ways deliberately: as a native
        // `embedding` column (the `nearest` storage contract — index-eligible)
        // and as its text form inside `data` (what the threshold WHERE
        // operators resolve via `data->>'embedding'`).
        format!(
            "CREATE VIEW {SCHEMA}.v_doc AS SELECT id, jsonb_build_object('id', id, 'title', \
             title, 'embedding', embedding::text) AS data, embedding, thumbnail \
             FROM {SCHEMA}.tb_doc"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup (needs pgvector)");
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut doc = TypeDefinition::new("Doc", format!("{SCHEMA}.v_doc"));
    doc.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("title", FieldType::String),
        FieldDefinition::new("embedding", FieldType::Vector)
            .with_vector_config(VectorConfig::new(3)),
    ];
    schema.types.push(doc);

    // A second type over the SAME view, declaring both vectors (#959). Kept
    // separate from `Doc` so the single-vector-field path — where `nearest.field`
    // is optional — keeps its own end-to-end coverage rather than being replaced
    // by the multi-field one.
    let mut multi = TypeDefinition::new("MultiDoc", format!("{SCHEMA}.v_doc"));
    multi.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("title", FieldType::String),
        FieldDefinition::new("embedding", FieldType::Vector)
            .with_vector_config(VectorConfig::new(3)),
        FieldDefinition::new("thumbnail", FieldType::Vector).with_vector_config(VectorConfig {
            dimensions: 2,
            distance_metric: fraiseql_core::schema::DistanceMetric::L2,
            ..VectorConfig::new(2)
        }),
    ];
    schema.types.push(multi);

    let mut multi_docs = QueryDefinition::new("multiDocs", "MultiDoc")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_doc"));
    multi_docs.auto_params.has_where = true;
    multi_docs.auto_params.has_limit = true;
    multi_docs.auto_params.has_offset = true;
    schema.queries.push(multi_docs);

    let mut docs = QueryDefinition::new("docs", "Doc")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_doc"));
    docs.auto_params.has_where = true;
    docs.auto_params.has_limit = true;
    docs.auto_params.has_offset = true;
    schema.queries.push(docs);
    schema.build_indexes();
    schema
}

async fn setup() -> Option<Router> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter))));
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

fn ids(response: &Value) -> Vec<i64> {
    ids_under(response, "docs")
}

/// The row ids under a named response key, in order.
fn ids_under(response: &Value, key: &str) -> Vec<i64> {
    response["data"][key]
        .as_array()
        .unwrap_or_else(|| panic!("expected data.{key} list, got: {response}"))
        .iter()
        .map(|d| d["id"].as_i64().expect("id"))
        .collect()
}

fn skip(test: &str) -> bool {
    if try_database_url().is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
        return true;
    }
    false
}

// ── nearest: row ORDER per metric ────────────────────────────────────────────

#[tokio::test]
async fn nearest_orders_by_the_declared_cosine_metric() {
    if skip("nearest_cosine") {
        return;
    }
    let router = setup().await.unwrap();
    let query = r"{ docs(nearest: {vector: [1, 0, 0], k: 4}) { id title } }";
    let (status, resp) = post_graphql(router, &json!({"query": query})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids(&resp),
        vec![1, 2, 3, 4],
        "cosine similarity order, proven by executed rows — not sql.contains: {resp}"
    );
}

#[tokio::test]
async fn nearest_metric_override_changes_the_row_order() {
    if skip("nearest_l2_and_ip") {
        return;
    }
    let router = setup().await.unwrap();

    let l2 = r#"{ docs(nearest: {vector: [1, 0, 0], k: 4, metric: "l2"}) { id } }"#;
    let (status, resp) = post_graphql(router.clone(), &json!({"query": l2})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(ids(&resp), vec![1, 3, 4, 2], "L2 puts the long vector last: {resp}");

    let ip = r#"{ docs(nearest: {vector: [1, 0, 0], k: 4, metric: "inner_product"}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": ip})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(ids(&resp), vec![2, 1, 3, 4], "inner product rewards magnitude: {resp}");
}

#[tokio::test]
async fn nearest_k_limits_the_rows() {
    if skip("nearest_k") {
        return;
    }
    let router = setup().await.unwrap();
    let query = r"{ docs(nearest: {vector: [1, 0, 0], k: 2}) { id } }";
    let (status, resp) = post_graphql(router, &json!({"query": query})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(ids(&resp), vec![1, 2]);
}

#[tokio::test]
async fn nearest_composes_with_a_where_filter() {
    if skip("nearest_where_compose") {
        return;
    }
    let router = setup().await.unwrap();
    // Filter excludes the exact match; the remaining rows still come back in
    // cosine order.
    let query =
        r#"{ docs(where: {title: {neq: "exact"}}, nearest: {vector: [1, 0, 0], k: 4}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": query})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(ids(&resp), vec![2, 3, 4], "filtered AND similarity-ordered: {resp}");
}

#[tokio::test]
async fn nearest_dimension_mismatch_is_refused() {
    if skip("nearest_dims") {
        return;
    }
    let router = setup().await.unwrap();
    let query = r"{ docs(nearest: {vector: [1, 0], k: 2}) { id } }";
    let (_status, resp) = post_graphql(router, &json!({"query": query})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("2 components") && msg.contains("3 dimensions"),
        "the refusal names both sides of the mismatch: {resp}"
    );
}

// ── threshold WHERE operators: executed rows, not templates ──────────────────

#[tokio::test]
async fn cosine_distance_threshold_filter_returns_the_rows_within_it() {
    if skip("threshold_filter") {
        return;
    }
    let router = setup().await.unwrap();
    // Distances to [1,0,0]: id1 = 0, id2 ≈ 2e-6, id3 ≈ 0.0194, id4 = 1.
    let query = r"{ docs(where: {embedding: {cosine_distance: {vector: [1, 0, 0], threshold: 0.01}}}) { id } }";
    let (status, resp) = post_graphql(router, &json!({"query": query})).await;
    assert_eq!(status, 200, "{resp}");
    let mut got = ids(&resp);
    got.sort_unstable();
    assert_eq!(
        got,
        vec![1, 2],
        "rows within the distance threshold, executed against real pgvector: {resp}"
    );
}

#[tokio::test]
async fn inner_product_threshold_keeps_rows_at_least_that_similar() {
    if skip("ip_threshold") {
        return;
    }
    let router = setup().await.unwrap();
    // Raw inner products with [1,0,0]: id1 = 1, id2 = 5, id3 = 0.1, id4 = 0.
    let query = r"{ docs(where: {embedding: {inner_product: {vector: [1, 0, 0], threshold: 0.5}}}) { id } }";
    let (status, resp) = post_graphql(router, &json!({"query": query})).await;
    assert_eq!(status, 200, "{resp}");
    let mut got = ids(&resp);
    got.sort_unstable();
    assert_eq!(got, vec![1, 2], "minimum-inner-product semantics: {resp}");
}

#[tokio::test]
async fn hamming_distance_is_loudly_unsupported() {
    if skip("hamming_refusal") {
        return;
    }
    let router = setup().await.unwrap();
    let query = r"{ docs(where: {embedding: {hamming_distance: {vector: [1, 0, 0], threshold: 1}}}) { id } }";
    let (_status, resp) = post_graphql(router, &json!({"query": query})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("binary"),
        "the refusal explains the bit-vector gap instead of emitting broken SQL: {resp}"
    );
}

// ── nearest: choosing among several vector fields (#959) ─────────────────────

/// `nearest.field` searches the named embedding, not the first one declared.
///
/// The fixture's two vectors order the same four rows in opposite directions, so a
/// selector that were ignored — or applied to the wrong column — returns the reverse
/// of what this asserts. A test on a type with one vector field cannot tell the
/// difference, which is why the second column exists.
#[tokio::test]
async fn nearest_field_searches_the_named_vector_field() {
    if skip("nearest_field_selector") {
        return;
    }

    let router = setup().await.unwrap();
    let q = r#"{ multiDocs(nearest: {vector: [0, 0], k: 4, field: "thumbnail"}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids_under(&resp, "multiDocs"),
        vec![4, 3, 2, 1],
        "`thumbnail` orders by distance from the origin: [1,0] is nearest — {resp}"
    );

    let router = setup().await.unwrap();
    let q = r#"{ multiDocs(nearest: {vector: [1, 0, 0], k: 4, field: "embedding"}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids_under(&resp, "multiDocs"),
        vec![1, 2, 3, 4],
        "`embedding` keeps its own cosine order — {resp}"
    );
}

/// The selected field's declared dimensions are the ones validated. Checking the
/// wrong field's would accept a vector of the right length for a different embedding
/// space — a search that runs and answers about the wrong thing.
#[tokio::test]
async fn nearest_field_validates_the_selected_fields_dimensions() {
    if skip("nearest_field_dimensions") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ multiDocs(nearest: {vector: [1, 0, 0], k: 2, field: "thumbnail"}) { id } }"#;
    let (_, resp) = post_graphql(router, &json!({"query": q})).await;

    let message = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("3 components") && message.contains("2 dimensions"),
        "the refusal must name the selected field's dimensions: {resp}"
    );
}

/// Omitting `field:` on a type that declares several is refused, with the candidates
/// named. Answering it by field order would silently search one embedding space and
/// report it as another.
#[tokio::test]
async fn nearest_without_a_field_on_a_multi_vector_type_is_refused() {
    if skip("nearest_field_ambiguous") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r"{ multiDocs(nearest: {vector: [1, 0, 0], k: 2}) { id } }";
    let (_, resp) = post_graphql(router, &json!({"query": q})).await;

    let message = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(message.contains("nearest.field"), "the refusal must name the way out: {resp}");
    assert!(
        message.contains("embedding") && message.contains("thumbnail"),
        "the refusal must name the candidates: {resp}"
    );
}
