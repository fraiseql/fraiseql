//! Federation observability: per-subgraph latency tracking and entity resolution metrics.
//!
//! Provides structures for collecting and reporting federation-level metrics
//! that can be exported to `OpenTelemetry` or Prometheus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-subgraph latency tracker for federation queries.
///
/// Records timing for each subgraph fetch and provides breakdown data
/// suitable for OTEL span attributes or Prometheus histograms.
#[derive(Debug)]
pub struct SubgraphLatencyTracker {
    entries: Mutex<Vec<SubgraphLatencyEntry>>,
}

/// A single latency measurement for a subgraph fetch.
#[derive(Debug, Clone)]
pub struct SubgraphLatencyEntry {
    /// Subgraph name or URL
    pub subgraph: String,

    /// Duration of the fetch
    pub duration: Duration,

    /// Number of entities resolved in this fetch
    pub entity_count: usize,

    /// Whether the fetch succeeded
    pub success: bool,
}

impl SubgraphLatencyTracker {
    /// Create a new empty tracker.
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Record a completed subgraph fetch.
    pub fn record(
        &self,
        subgraph: impl Into<String>,
        duration: Duration,
        entity_count: usize,
        success: bool,
    ) {
        let entry = SubgraphLatencyEntry {
            subgraph: subgraph.into(),
            duration,
            entity_count,
            success,
        };
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.push(entry);
    }

    /// Start timing a subgraph fetch. Returns a guard that records on drop.
    pub fn start(&self, subgraph: impl Into<String>) -> SubgraphTimer<'_> {
        SubgraphTimer {
            tracker:  self,
            subgraph: subgraph.into(),
            start:    Instant::now(),
        }
    }

    /// Get all recorded entries.
    pub fn entries(&self) -> Vec<SubgraphLatencyEntry> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clone()
    }

    /// Get per-subgraph latency summary as OTEL span attributes.
    ///
    /// Returns attributes like:
    /// - `federation.subgraph.users.latency_ms` = "12.5"
    /// - `federation.subgraph.users.entity_count` = "42"
    pub fn to_span_attributes(&self) -> HashMap<String, String> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut attrs = HashMap::new();

        for entry in entries.iter() {
            let prefix = format!("federation.subgraph.{}", entry.subgraph);
            attrs.insert(
                format!("{prefix}.latency_ms"),
                format!("{:.2}", entry.duration.as_secs_f64() * 1000.0),
            );
            attrs.insert(format!("{prefix}.entity_count"), entry.entity_count.to_string());
            attrs.insert(format!("{prefix}.success"), entry.success.to_string());
        }

        attrs
    }

    /// Total latency across all subgraph fetches.
    pub fn total_latency(&self) -> Duration {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.iter().map(|e| e.duration).sum()
    }
}

impl Default for SubgraphLatencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer guard that records latency on completion.
pub struct SubgraphTimer<'a> {
    tracker:  &'a SubgraphLatencyTracker,
    subgraph: String,
    start:    Instant,
}

impl SubgraphTimer<'_> {
    /// Complete the timer, recording success and entity count.
    pub fn finish(self, entity_count: usize, success: bool) {
        let duration = self.start.elapsed();
        self.tracker.record(&self.subgraph, duration, entity_count, success);
    }
}

/// Federation entity resolution metrics counters.
///
/// Thread-safe atomic counters for tracking entity resolution outcomes.
/// Suitable for Prometheus `counter` metric export.
#[derive(Debug)]
pub struct EntityResolutionMetrics {
    /// Total successful entity resolutions
    pub success_total: AtomicU64,

    /// Total failed entity resolutions
    pub failure_total: AtomicU64,

    /// Total entities resolved
    pub entities_resolved_total: AtomicU64,
}

impl EntityResolutionMetrics {
    /// Create new zero-initialized metrics.
    pub const fn new() -> Self {
        Self {
            success_total:          AtomicU64::new(0),
            failure_total:          AtomicU64::new(0),
            entities_resolved_total: AtomicU64::new(0),
        }
    }

    /// Record a successful resolution batch.
    pub fn record_success(&self, entity_count: u64) {
        self.success_total.fetch_add(1, Ordering::Relaxed);
        self.entities_resolved_total.fetch_add(entity_count, Ordering::Relaxed);
    }

    /// Record a failed resolution.
    pub fn record_failure(&self) {
        self.failure_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current success count.
    pub fn successes(&self) -> u64 {
        self.success_total.load(Ordering::Relaxed)
    }

    /// Get current failure count.
    pub fn failures(&self) -> u64 {
        self.failure_total.load(Ordering::Relaxed)
    }

    /// Get total entities resolved.
    pub fn entities_resolved(&self) -> u64 {
        self.entities_resolved_total.load(Ordering::Relaxed)
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.success_total.store(0, Ordering::Relaxed);
        self.failure_total.store(0, Ordering::Relaxed);
        self.entities_resolved_total.store(0, Ordering::Relaxed);
    }
}

impl Default for EntityResolutionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_latency_tracker_record_and_retrieve() {
        let tracker = SubgraphLatencyTracker::new();
        tracker.record("users", Duration::from_millis(15), 10, true);
        tracker.record("orders", Duration::from_millis(25), 5, true);

        let entries = tracker.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].subgraph, "users");
        assert_eq!(entries[0].entity_count, 10);
        assert!(entries[0].success);
        assert_eq!(entries[1].subgraph, "orders");
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
        assert_eq!(attrs["federation.subgraph.users.entity_count"], "10");
        assert_eq!(attrs["federation.subgraph.users.success"], "true");
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
        assert_eq!(entries[0].entity_count, 5);
        assert!(entries[0].duration >= Duration::from_millis(1));
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
}
