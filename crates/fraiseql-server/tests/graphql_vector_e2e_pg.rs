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
///
/// The same rows carry an 8-bit `fingerprint` for the binary metrics (#959),
/// against the query bits `11110000`:
///
/// | id | fingerprint | hamming | jaccard |
/// |----|-------------|---------|---------|
/// | 1  | 11111111    | 4       | 0.5     |
/// | 2  | 11110000    | 0       | 0       |
/// | 3  | 00000011    | 6       | 1.0     |
/// | 4  | 10000000    | 3       | 0.75    |
///
/// hamming order: 2,4,1,3 · jaccard order: 2,1,4,3. Jaccard normalises by the
/// union of set bits, so id4 — three of the query's bits missing but none
/// spurious — is *closer* than id1 under hamming and *further* under jaccard.
/// Neither order is the insertion order, which is what a `::bit` (i.e.
/// `bit(1)`) cast would collapse the comparison to.
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
             embedding vector(3) NOT NULL, thumbnail vector(2) NOT NULL, \
             fingerprint bit(8) NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_doc VALUES \
             (1, 'exact',      '[1,0,0]',      '[4,0]', '11111111'), \
             (2, 'longer',     '[5,0.01,0]',   '[3,0]', '11110000'), \
             (3, 'shorter',    '[0.1,0.02,0]', '[2,0]', '00000011'), \
             (4, 'orthogonal', '[0,1,0]',      '[1,0]', '10000000')"
        ),
        // The view exposes the vector BOTH ways deliberately: as a native
        // `embedding` column (the `nearest` storage contract — index-eligible)
        // and as its text form inside `data` (what the threshold WHERE
        // operators resolve via `data->>'embedding'`). `fingerprint`, the
        // binary vector (#959), follows the same contract.
        format!(
            "CREATE VIEW {SCHEMA}.v_doc AS SELECT id, jsonb_build_object('id', id, 'title', \
             title, 'embedding', embedding::text, 'fingerprint', fingerprint::text) AS data, \
             embedding, thumbnail, fingerprint FROM {SCHEMA}.tb_doc"
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
        // Declared against `embedding`, deliberately: selecting it while
        // searching `thumbnail` is the mismatch the refusal exists for (#959).
        FieldDefinition::new("thumbnailDistance", FieldType::Float)
            .with_vector_distance("embedding"),
    ];
    schema.types.push(multi);

    let mut multi_docs = QueryDefinition::new("multiDocs", "MultiDoc")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_doc"));
    multi_docs.auto_params.has_where = true;
    multi_docs.auto_params.has_limit = true;
    multi_docs.auto_params.has_offset = true;
    schema.queries.push(multi_docs);

    // `similarity` carries the distance the search ordered by (#959). Declared
    // on `Doc`, whose single vector field keeps `nearest.field` optional, so the
    // test exercises the ordinary shape.
    doc_fields_with_similarity(&mut schema);

    let mut docs = QueryDefinition::new("docs", "Doc")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_doc"));
    docs.auto_params.has_where = true;
    docs.auto_params.has_limit = true;
    docs.auto_params.has_offset = true;
    schema.queries.push(docs);

    // Two types over the same `fingerprint` column, differing only in the
    // metric they DECLARE (#959). One type could not tell "the declared metric
    // is applied" from "hamming is hardcoded": the jaccard type is what makes
    // the default path falsifiable, and the `metric:` override on the hamming
    // one is the second, independent route to the same order.
    for (type_name, query_name, metric) in [
        ("BitDoc", "bitDocs", fraiseql_core::schema::DistanceMetric::Hamming),
        ("JacDoc", "jacDocs", fraiseql_core::schema::DistanceMetric::Jaccard),
    ] {
        let mut bit_doc = TypeDefinition::new(type_name, format!("{SCHEMA}.v_doc"));
        bit_doc.fields = vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("title", FieldType::String),
            FieldDefinition::bit_vector(
                "fingerprint",
                VectorConfig {
                    distance_metric: metric,
                    ..VectorConfig::binary(8)
                },
            ),
            FieldDefinition::new("bitDistance", FieldType::Float)
                .with_vector_distance("fingerprint"),
        ];
        schema.types.push(bit_doc);

        let mut query = QueryDefinition::new(query_name, type_name)
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_doc"));
        query.auto_params.has_where = true;
        query.auto_params.has_limit = true;
        query.auto_params.has_offset = true;
        schema.queries.push(query);
    }

    schema.build_indexes();
    schema
}

/// Add the distance field to `Doc` (#959).
///
/// A separate function rather than a fourth entry in `schema()`'s literal, so
/// the reason it is on `Doc` and not on `MultiDoc` stays next to it: `Doc`
/// declares one vector field, which is the case where `nearest.field` is
/// omitted and the distance still has exactly one meaning.
fn doc_fields_with_similarity(schema: &mut CompiledSchema) {
    let doc = schema.types.iter_mut().find(|t| t.name == "Doc").expect("Doc type");
    doc.fields.push(
        FieldDefinition::new("similarity", FieldType::Float).with_vector_distance("embedding"),
    );
    // A policy-gated field (#423) — selecting it makes the runner return the row
    // whole, because the per-row authorizer needs the complete parent to decide
    // on. That is the branch where a distance has to be added to the stored row
    // instead of replacing it; see the authenticated test below.
    doc.fields
        .push(FieldDefinition::new("gated", FieldType::String).with_authorize(true));
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

// ── binary (bit) vectors: hamming and jaccard, executed (#959) ───────────────

/// `nearest` on a `BitVector` field orders by the declared binary metric.
///
/// The expected order is neither the insertion order nor the float embedding's
/// order, so an implementation that ignored the bit column — or that cast it
/// with a length-less `::bit`, which is `bit(1)` and makes every row that
/// starts with a `1` equidistant — returns something else.
#[tokio::test]
async fn nearest_on_a_bit_vector_orders_by_hamming_distance() {
    if skip("nearest_hamming") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ bitDocs(nearest: {vector: "11110000", k: 4}) { id fingerprint } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids_under(&resp, "bitDocs"),
        vec![2, 4, 1, 3],
        "hamming counts differing bits: 0, 3, 4, 6 — {resp}"
    );
    assert_eq!(
        resp["data"]["bitDocs"][0]["fingerprint"].as_str(),
        Some("11110000"),
        "a BitVector field is delivered as its bit string: {resp}"
    );
}

/// Jaccard orders the same four rows differently, from two independent
/// directions: the `metric:` override on the hamming-declared type, and the
/// declared metric of the jaccard type with no override at all.
#[tokio::test]
async fn jaccard_orders_the_same_bits_differently_than_hamming() {
    if skip("nearest_jaccard") {
        return;
    }
    let router = setup().await.unwrap();
    let overridden =
        r#"{ bitDocs(nearest: {vector: "11110000", k: 4, metric: "jaccard"}) { id } }"#;
    let (status, resp) = post_graphql(router.clone(), &json!({"query": overridden})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids_under(&resp, "bitDocs"),
        vec![2, 1, 4, 3],
        "jaccard normalises by the union of set bits: 0, 0.5, 0.75, 1 — {resp}"
    );

    let declared = r#"{ jacDocs(nearest: {vector: "11110000", k: 4}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": declared})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(
        ids_under(&resp, "jacDocs"),
        vec![2, 1, 4, 3],
        "the field's declared metric applies when `metric:` is omitted: {resp}"
    );
}

/// The binary threshold WHERE operators, resolved through `data->>'fingerprint'`
/// rather than the native column — the JSONB path, where the cast is the whole
/// question.
#[tokio::test]
async fn binary_distance_threshold_filters_return_the_rows_within_them() {
    if skip("bit_thresholds") {
        return;
    }
    let router = setup().await.unwrap();
    // Hamming distances to 11110000: id1 = 4, id2 = 0, id3 = 6, id4 = 3.
    let hamming = r#"{ bitDocs(where: {fingerprint: {hamming_distance: {vector: "11110000", threshold: 3}}}) { id } }"#;
    let (status, resp) = post_graphql(router.clone(), &json!({"query": hamming})).await;
    assert_eq!(status, 200, "{resp}");
    let mut got = ids_under(&resp, "bitDocs");
    got.sort_unstable();
    assert_eq!(got, vec![2, 4], "within 3 differing bits, inclusive: {resp}");

    // Jaccard distances to 11110000: id1 = 0.5, id2 = 0, id3 = 1, id4 = 0.75.
    let jaccard = r#"{ bitDocs(where: {fingerprint: {jaccard_distance: {vector: "11110000", threshold: 0.5}}}) { id } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": jaccard})).await;
    assert_eq!(status, 200, "{resp}");
    let mut got = ids_under(&resp, "bitDocs");
    got.sort_unstable();
    assert_eq!(got, vec![1, 2], "the same threshold selects a different pair: {resp}");
}

/// A wrong-width query vector is refused before it reaches PostgreSQL.
///
/// This check is the only thing standing between the request and a wrong
/// answer: casting text to `bit(8)` pads a short value on the right and
/// truncates a long one, both silently, so an unvalidated 4-bit operand would
/// search `10100000` and report it as a match for `1010`.
#[tokio::test]
async fn nearest_bit_dimension_mismatch_is_refused() {
    if skip("nearest_bit_dims") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ bitDocs(nearest: {vector: "1010", k: 2}) { id } }"#;
    let (_status, resp) = post_graphql(router, &json!({"query": q})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("4 bits") && msg.contains("8 dimensions"),
        "the refusal names both sides of the mismatch: {resp}"
    );
}

/// A float metric on a binary field (and the reverse) is refused by name.
///
/// pgvector has no `<=>` over `bit` and no `<~>` over `vector`, so the
/// alternative is a raw SQL error from the driver naming neither the field nor
/// the way out.
#[tokio::test]
async fn a_metric_of_the_other_vector_kind_is_refused() {
    if skip("metric_kind_mismatch") {
        return;
    }
    let router = setup().await.unwrap();
    let float_on_bits =
        r#"{ bitDocs(nearest: {vector: "11110000", k: 2, metric: "cosine"}) { id } }"#;
    let (_status, resp) = post_graphql(router.clone(), &json!({"query": float_on_bits})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("float") && msg.contains("BitVector") && msg.contains("hamming"),
        "the refusal names the field's kind and its metrics: {resp}"
    );

    let bits_on_float = r#"{ docs(nearest: {vector: [1, 0, 0], k: 2, metric: "hamming"}) { id } }"#;
    let (_status, resp) = post_graphql(router, &json!({"query": bits_on_float})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("binary") && msg.contains("cosine"),
        "and the same in reverse on a float Vector field: {resp}"
    );
}

/// A bit-vector operand is a string; an array is refused, naming the shape.
#[tokio::test]
async fn nearest_on_a_bit_vector_refuses_a_float_array_operand() {
    if skip("bit_operand_shape") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r"{ bitDocs(nearest: {vector: [1, 0, 1, 0, 0, 0, 0, 0], k: 2}) { id } }";
    let (_status, resp) = post_graphql(router, &json!({"query": q})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("BitVector") && msg.contains("string"),
        "the refusal names the operand shape a BitVector field takes: {resp}"
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

// ── the distance in the response (#959) ─────────────────────────────────────

/// A selected `vector_distance` field carries the distance the rows were
/// ordered by — the numbers, not just the order.
///
/// The values are hand-computed against the fixture, so a projection that
/// invented a plausible number (a different metric, a different vector, the
/// squared distance) fails here even though the row order is right.
#[tokio::test]
async fn a_vector_distance_field_carries_the_distance_the_rows_were_ordered_by() {
    if skip("distance_in_response") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r"{ docs(nearest: {vector: [1, 0, 0], k: 4}) { id similarity } }";
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");

    let rows = resp["data"]["docs"].as_array().expect("rows");
    let got: Vec<(i64, f64)> = rows
        .iter()
        .map(|r| (r["id"].as_i64().expect("id"), r["similarity"].as_f64().expect("similarity")))
        .collect();
    assert_eq!(got.len(), 4, "{resp}");

    // Cosine distances to [1,0,0]: id1 = 0, id2 ≈ 2e-6, id3 ≈ 0.0194, id4 = 1.
    let expected = [(1_i64, 0.0_f64), (2, 2e-6), (3, 0.0194), (4, 1.0)];
    for ((id, distance), (want_id, want)) in got.iter().zip(expected) {
        assert_eq!(*id, want_id, "row order is the distance order: {resp}");
        assert!(
            (distance - want).abs() < 1e-3,
            "row {id}: distance {distance} is not the cosine distance {want}: {resp}"
        );
    }
}

/// The reported distance follows the metric the search used, because it is the
/// same expression the ORDER BY was built from.
#[tokio::test]
async fn the_reported_distance_follows_the_metric_the_search_used() {
    if skip("distance_follows_metric") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ docs(nearest: {vector: [1, 0, 0], k: 1, metric: "l2"}) { id similarity } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    // L2 puts id1 first at distance 0; under inner product the same row reports
    // -1 (pgvector negates), and under cosine 0 — so only an L2 reading passes
    // together with the row identity below.
    assert_eq!(resp["data"]["docs"][0]["id"].as_i64(), Some(1), "{resp}");
    let d = resp["data"]["docs"][0]["similarity"].as_f64().expect("similarity");
    assert!(d.abs() < 1e-6, "L2 distance from a vector to itself is 0, got {d}: {resp}");

    let router = setup().await.unwrap();
    let q = r#"{ docs(nearest: {vector: [1, 0, 0], k: 1, metric: "inner_product"}) { id similarity } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    assert_eq!(resp["data"]["docs"][0]["id"].as_i64(), Some(2), "{resp}");
    let d = resp["data"]["docs"][0]["similarity"].as_f64().expect("similarity");
    assert!(
        (d + 5.0).abs() < 1e-6,
        "pgvector's <#> returns the negated inner product, -5 for [5,0.01,0], got {d}: {resp}"
    );
}

/// Selecting the distance without a search is refused, not answered with null.
///
/// A null would be indistinguishable from a row whose distance is genuinely
/// unknown, on a response that otherwise looks like it worked.
#[tokio::test]
async fn a_distance_field_without_a_nearest_search_is_refused() {
    if skip("distance_without_nearest") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r"{ docs(limit: 2) { id similarity } }";
    let (_status, resp) = post_graphql(router, &json!({"query": q})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("similarity") && msg.contains("nearest"),
        "the refusal names the field and the way out: {resp}"
    );
}

/// The distance of a binary search is the bit metric's own number: a count of
/// differing bits for hamming, a ratio for jaccard.
#[tokio::test]
async fn a_binary_search_reports_its_own_metrics_distance() {
    if skip("bit_distance_in_response") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ bitDocs(nearest: {vector: "11110000", k: 4}) { id bitDistance } }"#;
    let (status, resp) = post_graphql(router.clone(), &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    let got: Vec<(i64, f64)> = resp["data"]["bitDocs"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| (r["id"].as_i64().expect("id"), r["bitDistance"].as_f64().expect("distance")))
        .collect();
    assert_eq!(got, vec![(2, 0.0), (4, 3.0), (1, 4.0), (3, 6.0)], "hamming counts bits: {resp}");

    let q = r#"{ jacDocs(nearest: {vector: "11110000", k: 2}) { id bitDistance } }"#;
    let (status, resp) = post_graphql(router, &json!({"query": q})).await;
    assert_eq!(status, 200, "{resp}");
    let second = resp["data"]["jacDocs"][1]["bitDistance"].as_f64().expect("distance");
    assert!(
        (second - 0.5).abs() < 1e-9,
        "jaccard is a ratio, not a bit count: id1 shares 4 of 8 set bits — {resp}"
    );
}

/// A distance field measures the vector field it was declared against, and a
/// search of a *different* one is refused rather than reported as that field's
/// distance.
#[tokio::test]
async fn a_distance_field_for_another_vector_field_is_refused() {
    if skip("distance_field_mismatch") {
        return;
    }
    let router = setup().await.unwrap();
    let q = r#"{ multiDocs(nearest: {vector: [0, 0], k: 2, field: "thumbnail"}) { id } }"#;
    let (status, resp) = post_graphql(router.clone(), &json!({"query": q})).await;
    assert_eq!(status, 200, "the control query works: {resp}");

    // `Doc.similarity` measures `embedding`; asking for it while searching
    // `thumbnail` would report one embedding space's distance as another's.
    let q = r#"{ multiDocs(nearest: {vector: [0, 0], k: 2, field: "thumbnail"}) { id thumbnailDistance } }"#;
    let (_status, resp) = post_graphql(router, &json!({"query": q})).await;
    let msg = resp["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("embedding") && msg.contains("thumbnail"),
        "the refusal names both the declared field and the searched one: {resp}"
    );
}

/// A distance selected alongside a policy-gated field, which is the case where
/// the row must come back **whole**.
///
/// The gated-field path skips the field projection on purpose (#423: the per-row
/// authorizer decides on the complete parent), so the distance cannot replace
/// the projection — it is concatenated onto the stored row. This test is what
/// stops that branch from being an unexecuted claim: it goes through
/// `execute_with_security`, the same entry the HTTP handler calls once a request
/// carries a principal, against the real database.
#[tokio::test]
async fn a_distance_survives_the_full_row_path_a_gated_field_forces() {
    // An allow-all authorizer: this test is about the projection shape a gated
    // field forces, not about the decision it makes. Without one configured the
    // read fails closed, which is the correct behaviour and not what is under
    // test here.
    struct AllowAll;
    impl fraiseql_core::security::FieldAuthorizer for AllowAll {
        fn authorize_field(
            &self,
            _req: &fraiseql_core::security::FieldAuthzRequest<'_>,
        ) -> fraiseql_core::error::Result<fraiseql_core::security::FieldAuthzDecision> {
            Ok(fraiseql_core::security::FieldAuthzDecision::Allow)
        }
    }
    if skip("distance_with_gated_field") {
        return;
    }
    let url = try_database_url().unwrap();
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let config =
        fraiseql_core::runtime::RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll));
    let executor = Executor::with_config(schema(), Arc::new(adapter), config);

    let security = fraiseql_core::security::SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec!["admin".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-vector".to_string(),
        ip_address:       None,
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        authenticated_at: chrono::Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    };

    let q = r"{ docs(nearest: {vector: [1, 0, 0], k: 2}) { id title gated similarity } }";
    let resp = executor
        .execute_with_security(q, None, &security)
        .await
        .expect("the gated-field read must succeed");

    let rows = resp["data"]["docs"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{resp}");
    assert_eq!(rows[0]["id"].as_i64(), Some(1), "{resp}");
    assert_eq!(
        rows[0]["title"].as_str(),
        Some("exact"),
        "the stored row survives the concatenation: {resp}"
    );
    let d = rows[0]["similarity"]
        .as_f64()
        .unwrap_or_else(|| panic!("similarity must be projected: {resp}"));
    assert!(d.abs() < 1e-9, "and the distance is added to it, not instead of it: {resp}");
    let d1 = rows[1]["similarity"].as_f64().expect("similarity");
    assert!(d1 > 0.0 && d1 < 1e-5, "row 2's cosine distance is ~2e-6, got {d1}: {resp}");
}
