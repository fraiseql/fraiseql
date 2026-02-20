//! Behavioral tests for observability subsystems.
//!
//! Tests real production types:
//! - `MetricsCollector` atomic counters
//! - `TraceContext` W3C traceparent generation and child spans
//! - `tracing_utils::extract_trace_context` header parsing

mod common;

use std::sync::atomic::Ordering;

use common::test_app::{health_router, make_test_state};
use fraiseql_server::{MetricsCollector, TraceContext};
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
// TRACE CONTEXT
// ============================================================================

#[test]
fn trace_context_generates_valid_ids() {
    let ctx = TraceContext::new();
    assert!(!ctx.trace_id.is_empty());
    assert!(!ctx.span_id.is_empty());
    assert!(ctx.parent_span_id.is_none());
}

#[test]
fn trace_context_child_span_inherits_trace_id() {
    let parent = TraceContext::new();
    let child = parent.child_span();

    assert_eq!(child.trace_id, parent.trace_id);
    assert_ne!(child.span_id, parent.span_id);
    assert_eq!(child.parent_span_id, Some(parent.span_id.clone()));
}

#[test]
fn trace_context_w3c_traceparent_format() {
    let ctx = TraceContext::new();
    let traceparent = ctx.to_w3c_traceparent();

    // Format: 00-{trace_id}-{span_id}-{flags}
    let parts: Vec<&str> = traceparent.split('-').collect();
    assert_eq!(parts[0], "00", "version should be 00");
    assert_eq!(parts.len(), 4, "traceparent should have 4 parts");
}

#[test]
fn trace_context_baggage_propagation() {
    let ctx = TraceContext::new()
        .with_baggage("user_id".into(), "user-123".into())
        .with_baggage("env".into(), "test".into());

    assert_eq!(ctx.baggage_item("user_id"), Some("user-123"));
    assert_eq!(ctx.baggage_item("env"), Some("test"));
    assert_eq!(ctx.baggage_item("missing"), None);

    // Child inherits baggage
    let child = ctx.child_span();
    assert_eq!(child.baggage_item("user_id"), Some("user-123"));
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
