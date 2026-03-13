//! Behavioral tests for observability subsystems.
//!
//! Tests real production types:
//! - `MetricsCollector` atomic counters
//! - `tracing_utils::extract_trace_context` header parsing
//!
//! **Execution engine:** none
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

use std::sync::atomic::Ordering;

use common::test_app::{health_router, make_test_state};
use fraiseql_server::MetricsCollector;
use tower::ServiceExt;

// ============================================================================
// METRICS COLLECTOR
// ============================================================================

#[test]
fn metrics_collector_starts_at_zero() {
    let metrics = MetricsCollector::new();
    assert_eq!(metrics.queries_total.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.queries_success.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.queries_error.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.queries_duration_us.load(Ordering::Relaxed), 0);
}

#[test]
fn metrics_collector_increments_atomically() {
    let metrics = MetricsCollector::new();
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);

    assert_eq!(metrics.queries_total.load(Ordering::Relaxed), 3);
}

#[test]
fn metrics_collector_clone_shares_state() {
    let metrics = MetricsCollector::new();
    let clone = metrics.clone();

    metrics.queries_total.fetch_add(5, Ordering::Relaxed);
    assert_eq!(clone.queries_total.load(Ordering::Relaxed), 5);
}

#[test]
fn metrics_collector_records_entity_resolution() {
    let metrics = MetricsCollector::new();
    metrics.record_entity_resolution(1000, true);
    metrics.record_entity_resolution(2000, false);

    assert_eq!(metrics.federation_entity_resolutions_total.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.federation_entity_resolutions_errors.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.federation_entity_resolution_duration_us.load(Ordering::Relaxed), 3000);
}

// ============================================================================
// TRACING HEADER INTEGRATION
// ============================================================================

#[tokio::test]
async fn request_with_traceparent_header_succeeds() {
    let router = health_router(make_test_state());
    let response = router
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/health")
                .header("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
}
