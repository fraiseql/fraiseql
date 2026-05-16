#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_latency_tracker_record_and_retrieve() {
    let tracker = SubgraphLatencyTracker::new();
    tracker.record("users", Duration::from_millis(15), 10, true);
    tracker.record("orders", Duration::from_millis(25), 5, true);

    let entries = tracker.entries();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_latency_tracker_total_latency() {
    let tracker = SubgraphLatencyTracker::new();
    tracker.record("users", Duration::from_millis(15), 10, true);
    tracker.record("orders", Duration::from_millis(25), 5, true);

    let total = tracker.total_latency();
    assert_eq!(total, Duration::from_millis(40));
}

#[test]
fn test_latency_tracker_span_attributes() {
    let tracker = SubgraphLatencyTracker::new();
    tracker.record("users", Duration::from_millis(15), 10, true);

    let attrs = tracker.to_span_attributes();
    assert!(attrs.contains_key("federation.subgraph.users.latency_ms"));
    assert_eq!(attrs["federation.subgraph.users.count"], "1");
    assert_eq!(attrs["federation.subgraph.users.success_rate"], "1.0000");
}

#[test]
fn test_latency_tracker_timer() {
    let tracker = SubgraphLatencyTracker::new();
    let timer = tracker.start("users");
    // Simulate some work
    std::thread::sleep(Duration::from_millis(1));
    timer.finish(5, true);

    let entries = tracker.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].subgraph, "users");
}

#[test]
fn test_entity_resolution_metrics_success() {
    let metrics = EntityResolutionMetrics::new();
    metrics.record_success(10);
    metrics.record_success(5);

    assert_eq!(metrics.successes(), 2);
    assert_eq!(metrics.entities_resolved(), 15);
    assert_eq!(metrics.failures(), 0);
}

#[test]
fn test_entity_resolution_metrics_failure() {
    let metrics = EntityResolutionMetrics::new();
    metrics.record_failure();
    metrics.record_failure();

    assert_eq!(metrics.failures(), 2);
    assert_eq!(metrics.successes(), 0);
}

#[test]
fn test_entity_resolution_metrics_reset() {
    let metrics = EntityResolutionMetrics::new();
    metrics.record_success(10);
    metrics.record_failure();

    metrics.reset();

    assert_eq!(metrics.successes(), 0);
    assert_eq!(metrics.failures(), 0);
    assert_eq!(metrics.entities_resolved(), 0);
}

#[test]
fn test_entity_resolution_metrics_mixed() {
    let metrics = EntityResolutionMetrics::new();
    metrics.record_success(5);
    metrics.record_failure();
    metrics.record_success(3);

    assert_eq!(metrics.successes(), 2);
    assert_eq!(metrics.failures(), 1);
    assert_eq!(metrics.entities_resolved(), 8);
}

#[test]
fn test_latency_tracker_empty() {
    let tracker = SubgraphLatencyTracker::new();
    assert!(tracker.entries().is_empty());
    assert_eq!(tracker.total_latency(), Duration::ZERO);
    assert!(tracker.to_span_attributes().is_empty());
}

// --- Prometheus tests ---

#[test]
fn test_prometheus_histogram_valid_format() {
    let tracker = SubgraphLatencyTracker::new();
    tracker.record("users", Duration::from_millis(5), 3, true);
    tracker.record("users", Duration::from_millis(50), 2, true);
    tracker.record("users", Duration::from_millis(200), 1, false);

    let output = tracker.to_prometheus_histogram();

    // Must contain TYPE and HELP
    assert!(output.contains("# TYPE fraiseql_federation_subgraph_latency_seconds histogram"));
    assert!(output.contains("# HELP fraiseql_federation_subgraph_latency_seconds"));

    // Must contain bucket lines
    assert!(output.contains("le=\"0.005\""));
    assert!(output.contains("le=\"+Inf\""));

    // +Inf count must equal _count
    let inf_line = output.lines().find(|l| l.contains("le=\"+Inf\"")).unwrap();
    let inf_count: u64 = inf_line.split_whitespace().last().unwrap().parse().unwrap();

    let count_line = output.lines().find(|l| l.contains("_count")).unwrap();
    let total_count: u64 = count_line.split_whitespace().last().unwrap().parse().unwrap();

    assert_eq!(inf_count, total_count, "+Inf must equal _count");
    assert_eq!(total_count, 3, "_count must equal total record() calls");
}

#[test]
fn test_prometheus_histogram_sum_is_seconds() {
    let tracker = SubgraphLatencyTracker::new();
    tracker.record("svc", Duration::from_millis(100), 1, true);

    let output = tracker.to_prometheus_histogram();
    let sum_line = output.lines().find(|l| l.contains("_sum")).unwrap();
    let sum_val: f64 = sum_line.split_whitespace().last().unwrap().parse().unwrap();

    // 100ms = 0.1s
    assert!((sum_val - 0.1).abs() < 0.001, "_sum should be ~0.1 seconds, got {sum_val}");
}

#[test]
fn test_prometheus_entity_resolution_counters() {
    let metrics = EntityResolutionMetrics::new();
    metrics.record_success(5);
    metrics.record_success(3);
    metrics.record_failure();

    let output = metrics.to_prometheus_counters();

    assert!(output.contains("# TYPE fraiseql_federation_entity_resolution_total counter"));
    assert!(output.contains("outcome=\"success\"} 2"));
    assert!(output.contains("outcome=\"failure\"} 1"));
}

#[test]
fn test_prometheus_histogram_after_entity_resolution() {
    let tracker = SubgraphLatencyTracker::new();
    let metrics = EntityResolutionMetrics::new();

    // Simulate entity resolution
    tracker.record("products", Duration::from_millis(10), 5, true);
    metrics.record_success(5);

    let hist_output = tracker.to_prometheus_histogram();
    let counter_output = metrics.to_prometheus_counters();

    // Both should have non-zero values
    assert!(
        hist_output.contains("_count{subgraph=\"products\"} 1"),
        "histogram: {hist_output}"
    );
    assert!(counter_output.contains("outcome=\"success\"} 1"), "counters: {counter_output}");
}
