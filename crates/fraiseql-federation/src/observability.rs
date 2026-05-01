//! Federation observability: per-subgraph latency tracking and entity resolution metrics.
//!
//! Provides structures for collecting and reporting federation-level metrics
//! that can be exported to `OpenTelemetry` or Prometheus.
//!
//! # Prometheus histogram design
//!
//! `SubgraphLatencyTracker` stores per-subgraph cumulative bucket counters
//! using `DashMap<String, Arc<SubgraphHistogram>>`. Each `SubgraphHistogram` holds
//! 11 atomic bucket counters (`[0..9]` for `LATENCY_BUCKETS_SECS`, `[10]` for `+Inf`),
//! an integer microsecond sum, and a total count. This design avoids `Mutex` contention
//! on the hot `record()` path and produces correct Prometheus histogram text exposition.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Histogram bucket boundaries in seconds (10 buckets + implicit +Inf).
pub const LATENCY_BUCKETS_SECS: [f64; 10] = [
    0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.0, 2.5, 5.0,
];

/// Per-subgraph histogram with atomic bucket counters.
#[derive(Debug)]
pub struct SubgraphHistogram {
    /// Cumulative bucket counts: `[0..9]` for `LATENCY_BUCKETS_SECS[i]`, `[10]` = +Inf.
    bucket_counts: [AtomicU64; 11],
    /// Sum of all recorded durations in microseconds (integer to avoid f64 atomics).
    sum_microseconds: AtomicU64,
    /// Total number of observations.
    count: AtomicU64,
    /// Number of successful observations.
    success_count: AtomicU64,
}

impl SubgraphHistogram {
    /// Create a new zero-initialized histogram.
    fn new() -> Self {
        Self {
            bucket_counts:    std::array::from_fn(|_| AtomicU64::new(0)),
            sum_microseconds: AtomicU64::new(0),
            count:            AtomicU64::new(0),
            success_count:    AtomicU64::new(0),
        }
    }

    /// Record an observation.
    fn record(&self, duration: Duration, success: bool) {
        let secs = duration.as_secs_f64();

        // Update cumulative buckets — each bucket includes all smaller durations.
        for (i, &boundary) in LATENCY_BUCKETS_SECS.iter().enumerate() {
            if secs <= boundary {
                self.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        // +Inf bucket always incremented
        self.bucket_counts[10].fetch_add(1, Ordering::Relaxed);

        // duration.as_micros() returns u128; truncation to u64 is acceptable
        // for latencies up to ~584,942 years.
        #[allow(clippy::cast_possible_truncation)]
        let micros = duration.as_micros() as u64;
        self.sum_microseconds.fetch_add(micros, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        if success {
            self.success_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A single latency measurement for a subgraph fetch (for backward compatibility).
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

/// Per-subgraph latency tracker for federation queries.
///
/// Records timing for each subgraph fetch using lock-free atomic histogram buckets.
/// Produces standard Prometheus text exposition via [`to_prometheus_histogram`].
#[derive(Debug, Default)]
pub struct SubgraphLatencyTracker {
    histograms: DashMap<String, Arc<SubgraphHistogram>>,
}

impl SubgraphLatencyTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            histograms: DashMap::new(),
        }
    }

    /// Record a completed subgraph fetch.
    pub fn record(
        &self,
        subgraph: impl Into<String>,
        duration: Duration,
        _entity_count: usize,
        success: bool,
    ) {
        let name = subgraph.into();
        let histogram = self
            .histograms
            .entry(name)
            .or_insert_with(|| Arc::new(SubgraphHistogram::new()))
            .clone();
        histogram.record(duration, success);
    }

    /// Start timing a subgraph fetch. Returns a guard that records on drop.
    pub fn start(&self, subgraph: impl Into<String>) -> SubgraphTimer<'_> {
        SubgraphTimer {
            tracker:  self,
            subgraph: subgraph.into(),
            start:    Instant::now(),
        }
    }

    /// Get all recorded entries (backward-compatible aggregate view).
    ///
    /// Returns one entry per subgraph with aggregate `count`, total `duration`,
    /// and `success` reflecting whether all observations succeeded.
    #[allow(clippy::cast_possible_truncation)] // Reason: count fits in usize on any platform
    pub fn entries(&self) -> Vec<SubgraphLatencyEntry> {
        self.histograms
            .iter()
            .map(|entry| {
                let h = entry.value();
                let count = h.count.load(Ordering::Relaxed);
                let sum_us = h.sum_microseconds.load(Ordering::Relaxed);
                let success_count = h.success_count.load(Ordering::Relaxed);
                SubgraphLatencyEntry {
                    subgraph:     entry.key().clone(),
                    duration:     Duration::from_micros(sum_us),
                    entity_count: count as usize,
                    success:      success_count == count,
                }
            })
            .collect()
    }

    /// Get per-subgraph latency summary as OTEL span attributes.
    #[allow(clippy::cast_precision_loss)] // Reason: u64→f64 precision loss is negligible for aggregate metrics
    pub fn to_span_attributes(&self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();

        for entry in &self.histograms {
            let subgraph = entry.key();
            let h = entry.value();
            let count = h.count.load(Ordering::Relaxed);
            let sum_us = h.sum_microseconds.load(Ordering::Relaxed);
            let success_count = h.success_count.load(Ordering::Relaxed);

            let prefix = format!("federation.subgraph.{subgraph}");
            if count > 0 {
                let avg_ms = (sum_us as f64 / count as f64) / 1000.0;
                attrs.insert(format!("{prefix}.avg_latency_ms"), format!("{avg_ms:.2}"));
            }
            attrs.insert(format!("{prefix}.count"), count.to_string());
            if count > 0 {
                let rate = success_count as f64 / count as f64;
                attrs.insert(format!("{prefix}.success_rate"), format!("{rate:.4}"));
            }
        }

        attrs
    }

    /// Total latency across all subgraph fetches.
    pub fn total_latency(&self) -> Duration {
        let total_us: u64 = self
            .histograms
            .iter()
            .map(|entry| entry.value().sum_microseconds.load(Ordering::Relaxed))
            .sum();
        Duration::from_micros(total_us)
    }

    /// Emit standard Prometheus text exposition for the histogram.
    ///
    /// Produces `fraiseql_federation_subgraph_latency_seconds_bucket`,
    /// `_sum`, and `_count` lines for each subgraph.
    #[allow(clippy::cast_precision_loss)] // Reason: u64→f64 precision loss is negligible for Prometheus sum
    pub fn to_prometheus_histogram(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# HELP fraiseql_federation_subgraph_latency_seconds \
             Subgraph request latency\n\
             # TYPE fraiseql_federation_subgraph_latency_seconds histogram\n",
        );

        // Sort subgraph names for deterministic output
        let mut subgraphs: Vec<_> = self.histograms.iter().collect();
        subgraphs.sort_by(|a, b| a.key().cmp(b.key()));

        for entry in &subgraphs {
            let subgraph = entry.key();
            let h = entry.value();

            for (i, &boundary) in LATENCY_BUCKETS_SECS.iter().enumerate() {
                let count = h.bucket_counts[i].load(Ordering::Relaxed);
                let _ = writeln!(
                    out,
                    "fraiseql_federation_subgraph_latency_seconds_bucket\
                     {{subgraph=\"{subgraph}\",le=\"{boundary}\"}} {count}"
                );
            }
            // +Inf bucket
            let inf_count = h.bucket_counts[10].load(Ordering::Relaxed);
            let _ = writeln!(
                out,
                "fraiseql_federation_subgraph_latency_seconds_bucket\
                 {{subgraph=\"{subgraph}\",le=\"+Inf\"}} {inf_count}"
            );

            // _sum: convert microseconds to seconds
            let sum_us = h.sum_microseconds.load(Ordering::Relaxed);
            let sum_secs = sum_us as f64 / 1_000_000.0;
            let _ = writeln!(
                out,
                "fraiseql_federation_subgraph_latency_seconds_sum\
                 {{subgraph=\"{subgraph}\"}} {sum_secs:.6}"
            );

            // _count
            let count = h.count.load(Ordering::Relaxed);
            let _ = writeln!(
                out,
                "fraiseql_federation_subgraph_latency_seconds_count\
                 {{subgraph=\"{subgraph}\"}} {count}"
            );
        }

        out
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

    /// Emit Prometheus text exposition for entity resolution counters.
    pub fn to_prometheus_counters(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# HELP fraiseql_federation_entity_resolution_total \
             Total entity resolution operations\n\
             # TYPE fraiseql_federation_entity_resolution_total counter\n",
        );
        let _ = writeln!(
            out,
            "fraiseql_federation_entity_resolution_total{{outcome=\"success\"}} {}",
            self.successes()
        );
        let _ = writeln!(
            out,
            "fraiseql_federation_entity_resolution_total{{outcome=\"failure\"}} {}",
            self.failures()
        );
        out
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
        assert!(attrs.contains_key("federation.subgraph.users.avg_latency_ms"));
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

    // --- Cycle 4 Prometheus tests ---

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
        let inf_line = output
            .lines()
            .find(|l| l.contains("le=\"+Inf\""))
            .unwrap();
        let inf_count: u64 = inf_line.split_whitespace().last().unwrap().parse().unwrap();

        let count_line = output
            .lines()
            .find(|l| l.contains("_count"))
            .unwrap();
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
        assert!(
            (sum_val - 0.1).abs() < 0.001,
            "_sum should be ~0.1 seconds, got {sum_val}"
        );
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
        assert!(
            counter_output.contains("outcome=\"success\"} 1"),
            "counters: {counter_output}"
        );
    }
}
