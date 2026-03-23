//! Metrics feature integration tests.
//!
//! Validates that the metrics endpoint returns valid Prometheus-format output
//! and the JSON metrics endpoint returns correct structure, using the full
//! axum handler pipeline (not just unit-testing the formatter).
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test metrics_integration_test --features auth
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

mod common;

use std::sync::{Arc, atomic::Ordering};

use axum::{Router, routing::get};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::{
    metrics_server::MetricsCollector,
    routes::{
        graphql::AppState,
        metrics::{metrics_handler, metrics_json_handler},
    },
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::StatusCode;

use crate::common::test_app::{get_json, get_text};

fn make_metrics_state() -> AppState<FailingAdapter> {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    AppState::new(Arc::new(Executor::new(schema, adapter)))
}

fn metrics_router(state: AppState<FailingAdapter>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler::<FailingAdapter>))
        .route("/metrics/json", get(metrics_json_handler::<FailingAdapter>))
        .with_state(state)
}

// --- Prometheus text format endpoint ---

#[tokio::test]
async fn metrics_endpoint_returns_200() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (status, _body) = get_text(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn metrics_endpoint_contains_standard_counters() {
    let state = make_metrics_state();
    // Record some data
    state.metrics.queries_total.fetch_add(100, Ordering::Relaxed);
    state.metrics.queries_success.fetch_add(95, Ordering::Relaxed);
    state.metrics.queries_error.fetch_add(5, Ordering::Relaxed);
    state.metrics.cache_hits.fetch_add(80, Ordering::Relaxed);
    state.metrics.cache_misses.fetch_add(20, Ordering::Relaxed);

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    // Core GraphQL counters
    assert!(body.contains("fraiseql_graphql_queries_total 100"), "missing queries_total");
    assert!(body.contains("fraiseql_graphql_queries_success 95"), "missing queries_success");
    assert!(body.contains("fraiseql_graphql_queries_error 5"), "missing queries_error");

    // Cache metrics
    assert!(body.contains("fraiseql_cache_hits 80"), "missing cache_hits");
    assert!(body.contains("fraiseql_cache_misses 20"), "missing cache_misses");

    // HELP and TYPE annotations (Prometheus format compliance)
    assert!(body.contains("# HELP fraiseql_graphql_queries_total"));
    assert!(body.contains("# TYPE fraiseql_graphql_queries_total counter"));
}

#[tokio::test]
async fn metrics_endpoint_contains_db_pool_metrics() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_db_pool_connections_total"));
    assert!(body.contains("fraiseql_db_pool_connections_idle"));
    assert!(body.contains("fraiseql_db_pool_connections_active"));
    assert!(body.contains("fraiseql_db_pool_requests_waiting"));
}

#[tokio::test]
async fn metrics_endpoint_contains_apq_counters() {
    let state = make_metrics_state();
    state.apq_metrics.record_hit();
    state.apq_metrics.record_hit();
    state.apq_metrics.record_miss();
    state.apq_metrics.record_store();

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_apq_hits_total 2"), "APQ hits should be 2");
    assert!(body.contains("fraiseql_apq_misses_total 1"), "APQ misses should be 1");
    assert!(body.contains("fraiseql_apq_stored_total 1"), "APQ stored should be 1");
}

#[tokio::test]
async fn metrics_endpoint_contains_http_response_counters() {
    let state = make_metrics_state();
    state.metrics.http_requests_total.fetch_add(200, Ordering::Relaxed);
    state.metrics.http_responses_2xx.fetch_add(180, Ordering::Relaxed);
    state.metrics.http_responses_4xx.fetch_add(15, Ordering::Relaxed);
    state.metrics.http_responses_5xx.fetch_add(5, Ordering::Relaxed);

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_http_requests_total 200"));
    assert!(body.contains("fraiseql_http_responses_2xx 180"));
    assert!(body.contains("fraiseql_http_responses_4xx 15"));
    assert!(body.contains("fraiseql_http_responses_5xx 5"));
}

#[tokio::test]
async fn metrics_endpoint_contains_error_breakdown() {
    let state = make_metrics_state();
    state.metrics.validation_errors_total.fetch_add(3, Ordering::Relaxed);
    state.metrics.parse_errors_total.fetch_add(2, Ordering::Relaxed);
    state.metrics.execution_errors_total.fetch_add(1, Ordering::Relaxed);

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_validation_errors_total 3"));
    assert!(body.contains("fraiseql_parse_errors_total 2"));
    assert!(body.contains("fraiseql_execution_errors_total 1"));
}

#[tokio::test]
async fn metrics_endpoint_cache_hit_ratio_calculation() {
    let state = make_metrics_state();
    state.metrics.cache_hits.store(75, Ordering::Relaxed);
    state.metrics.cache_misses.store(25, Ordering::Relaxed);

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_cache_hit_ratio 0.750"), "hit ratio should be 0.750");
}

#[tokio::test]
async fn metrics_endpoint_average_duration_calculation() {
    let state = make_metrics_state();
    state.metrics.queries_total.store(10, Ordering::Relaxed);
    state.metrics.queries_duration_us.store(100_000, Ordering::Relaxed); // 100ms total

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    // Average = 100ms / 10 = 10ms
    assert!(body.contains("fraiseql_graphql_query_duration_ms 10"), "avg should be 10ms");
}

#[tokio::test]
async fn metrics_endpoint_per_operation_histogram() {
    let state = make_metrics_state();
    state.metrics.operation_metrics.record("GetUsers", 10_000, false); // 10ms
    state.metrics.operation_metrics.record("GetUsers", 20_000, false); // 20ms
    state.metrics.operation_metrics.record("CreatePost", 5_000, true); // 5ms error

    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_query_duration_seconds_count{operation=\"GetUsers\"} 2"));
    assert!(body.contains("fraiseql_query_duration_seconds_count{operation=\"CreatePost\"} 1"));
    assert!(body.contains("fraiseql_query_errors_total{operation=\"CreatePost\"} 1"));
    assert!(body.contains("fraiseql_query_errors_total{operation=\"GetUsers\"} 0"));
}

#[tokio::test]
async fn metrics_endpoint_subscription_counters() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    // Subscription counters should always be present (even at zero)
    assert!(body.contains("fraiseql_ws_connections_total"));
    assert!(body.contains("fraiseql_ws_subscriptions_total"));
}

#[tokio::test]
async fn metrics_endpoint_multi_root_counter() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (_, body) = get_text(&router, "/metrics").await;

    assert!(body.contains("fraiseql_multi_root_queries_total"));
}

// --- JSON metrics endpoint ---

#[tokio::test]
async fn json_metrics_endpoint_returns_200() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (status, _) = get_json(&router, "/metrics/json").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn json_metrics_endpoint_returns_expected_fields() {
    let state = make_metrics_state();
    state.metrics.queries_total.store(50, Ordering::Relaxed);
    state.metrics.queries_success.store(48, Ordering::Relaxed);
    state.metrics.queries_error.store(2, Ordering::Relaxed);

    let router = metrics_router(state);
    let (_, json) = get_json(&router, "/metrics/json").await;

    assert_eq!(json["queries_total"], 50);
    assert_eq!(json["queries_success"], 48);
    assert_eq!(json["queries_error"], 2);
    assert!(json.get("avg_query_duration_ms").is_some());
    assert!(json.get("cache_hit_ratio").is_some());
    assert!(json.get("pool_connections_total").is_some());
    assert!(json.get("pool_connections_idle").is_some());
    assert!(json.get("pool_connections_active").is_some());
    assert!(json.get("pool_requests_waiting").is_some());
}

#[tokio::test]
async fn json_metrics_empty_state_has_zero_values() {
    let state = make_metrics_state();
    let router = metrics_router(state);
    let (_, json) = get_json(&router, "/metrics/json").await;

    assert_eq!(json["queries_total"], 0);
    assert_eq!(json["queries_success"], 0);
    assert_eq!(json["queries_error"], 0);
    assert_eq!(json["avg_query_duration_ms"], 0.0);
    assert_eq!(json["cache_hit_ratio"], 0.0);
}

// --- Concurrent metrics safety ---

#[tokio::test]
async fn concurrent_metric_updates_are_safe() {
    let collector = Arc::new(MetricsCollector::new());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = collector.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..1000 {
                c.queries_total.fetch_add(1, Ordering::Relaxed);
                c.queries_success.fetch_add(1, Ordering::Relaxed);
                c.cache_hits.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(collector.queries_total.load(Ordering::Relaxed), 10_000);
    assert_eq!(collector.queries_success.load(Ordering::Relaxed), 10_000);
    assert_eq!(collector.cache_hits.load(Ordering::Relaxed), 10_000);
}

// --- Federation metrics ---

#[tokio::test]
async fn federation_metrics_recording() {
    let collector = MetricsCollector::new();

    collector.record_entity_resolution(1000, true);
    collector.record_entity_resolution(2000, false);
    collector.record_subgraph_request(500, true);
    collector.record_mutation(3000, false);
    collector.record_entity_cache_hit();
    collector.record_entity_cache_miss();

    assert_eq!(collector.federation_entity_resolutions_total.load(Ordering::Relaxed), 2);
    assert_eq!(collector.federation_entity_resolutions_errors.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_entity_resolution_duration_us.load(Ordering::Relaxed), 3000);
    assert_eq!(collector.federation_subgraph_requests_total.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_mutations_total.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_mutations_errors.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_entity_cache_hits.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_entity_cache_misses.load(Ordering::Relaxed), 1);
    assert_eq!(collector.federation_errors_total.load(Ordering::Relaxed), 2);
}
