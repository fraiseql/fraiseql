//! Behavioral tests for query intelligence API endpoints.
//!
//! Exercises the real `explain_handler`, `validate_handler`, and `stats_handler`
//! through axum's `tower::ServiceExt::oneshot`.
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables use _ prefix
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures
#![allow(clippy::match_same_arms)] // Reason: test data clarity
#![allow(clippy::branches_sharing_code)] // Reason: test assertion clarity
#![allow(clippy::undocumented_unsafe_blocks)] // Reason: test exercises unsafe paths

mod common;

use common::test_app::{api_router, get_json, make_test_state, make_test_state_with, post_json};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestQueryBuilder, TestSchemaBuilder},
};
use http::StatusCode;

// ============================================================================
// EXPLAIN ENDPOINT
// ============================================================================

#[tokio::test]
async fn explain_returns_complexity_for_valid_query() {
    let router = api_router(make_test_state());
    let (status, json) = post_json(
        &router,
        "/api/v1/query/explain",
        serde_json::json!({ "query": "query { users { id name } }" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");
    assert!(json["data"]["complexity"]["depth"].is_number());
    assert!(json["data"]["complexity"]["complexity"].is_number());
    assert!(json["data"]["complexity"]["alias_count"].is_number());
    assert!(json["data"]["estimated_cost"].as_u64().unwrap() > 0);
}

// Migration 9: explain_returns_sql_when_query_matches_schema
#[tokio::test]
async fn explain_returns_sql_when_query_matches_schema() {
    // Build a schema with a "users" query backed by "v_user" view
    let schema = TestSchemaBuilder::new()
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .with_sql_source("v_user")
                .build(),
        )
        .build();
    let state = make_test_state_with(FailingAdapter::new(), schema);
    let router = api_router(state);
    let (_, json) = post_json(
        &router,
        "/api/v1/query/explain",
        serde_json::json!({ "query": "query { users { id } }" }),
    )
    .await;

    assert!(
        json["data"]["sql"].is_string(),
        "expected SQL string, got: {}",
        json["data"]["sql"]
    );
    assert!(json["data"]["views_accessed"].as_array().is_some());
}

#[tokio::test]
async fn explain_returns_null_sql_for_unknown_query() {
    // Empty schema — planner can't match "users"
    let router = api_router(make_test_state());
    let (status, json) = post_json(
        &router,
        "/api/v1/query/explain",
        serde_json::json!({ "query": "query { users { id } }" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["data"]["sql"].is_null());
    assert_eq!(json["data"]["query_type"], "unknown");
}

#[tokio::test]
async fn explain_rejects_empty_query() {
    let router = api_router(make_test_state());
    let (status, json) =
        post_json(&router, "/api/v1/query/explain", serde_json::json!({ "query": "" })).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn explain_deeply_nested_query_generates_warnings() {
    // Build a query with depth > 10 to trigger the depth warning
    let deep_query =
        "query { a { b { c { d { e { f { g { h { i { j { k { l } } } } } } } } } } } }";
    let router = api_router(make_test_state());
    let (status, json) =
        post_json(&router, "/api/v1/query/explain", serde_json::json!({ "query": deep_query }))
            .await;

    assert_eq!(status, StatusCode::OK);
    let warnings = json["data"]["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w.as_str().unwrap().contains("depth")),
        "Expected depth warning, got: {warnings:?}"
    );
}

// ============================================================================
// VALIDATE ENDPOINT
// ============================================================================

#[tokio::test]
async fn validate_accepts_well_formed_query() {
    let router = api_router(make_test_state());
    let (status, json) = post_json(
        &router,
        "/api/v1/query/validate",
        serde_json::json!({ "query": "query { users { id } }" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["valid"], true);
    assert!(json["data"]["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn validate_rejects_mismatched_braces() {
    let router = api_router(make_test_state());
    let (status, json) = post_json(
        &router,
        "/api/v1/query/validate",
        serde_json::json!({ "query": "query { users { id }" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["valid"], false);
    let errors = json["data"]["errors"].as_array().unwrap();
    assert!(!errors.is_empty());
    assert!(errors[0].as_str().unwrap().contains("Expected }"));
}

#[tokio::test]
async fn validate_reports_empty_query_as_invalid() {
    let router = api_router(make_test_state());
    let (status, json) =
        post_json(&router, "/api/v1/query/validate", serde_json::json!({ "query": "" })).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["valid"], false);
}

// ============================================================================
// STATS ENDPOINT
// ============================================================================

#[tokio::test]
async fn stats_returns_zero_counters_on_fresh_state() {
    let router = api_router(make_test_state());
    let (status, json) = get_json(&router, "/api/v1/query/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");
    assert_eq!(json["data"]["total_queries"], 0);
    assert_eq!(json["data"]["successful_queries"], 0);
    assert_eq!(json["data"]["failed_queries"], 0);
    assert_eq!(json["data"]["average_latency_ms"], 0.0);
}

#[tokio::test]
async fn stats_reflects_metrics_atomics() {
    use std::sync::atomic::Ordering;

    let state = make_test_state();
    // Manually bump the metrics atomics that stats_handler reads
    state.metrics.queries_total.fetch_add(5, Ordering::Relaxed);
    state.metrics.queries_success.fetch_add(4, Ordering::Relaxed);
    state.metrics.queries_error.fetch_add(1, Ordering::Relaxed);
    state.metrics.queries_duration_us.fetch_add(10_000, Ordering::Relaxed); // 10ms total

    let router = api_router(state);
    let (_, json) = get_json(&router, "/api/v1/query/stats").await;

    assert_eq!(json["data"]["total_queries"], 5);
    assert_eq!(json["data"]["successful_queries"], 4);
    assert_eq!(json["data"]["failed_queries"], 1);
    // avg = 10_000us / 5 queries / 1000 = 2.0 ms
    let avg = json["data"]["average_latency_ms"].as_f64().unwrap();
    assert!((avg - 2.0).abs() < 0.01, "expected ~2.0ms, got {avg}");
}
