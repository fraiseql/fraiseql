//! Prometheus metrics registry for observer system
//!
//! This module defines and manages all Prometheus metrics for the observer system.
//!
//! # Scrape reachability (#612 item 14)
//!
//! These series register into the [`prometheus`] crate's **default global
//! registry** and are exposed by this crate's own `/metrics` handler
//! ([`crate::metrics::handler`], which calls `prometheus::gather()`). The
//! `fraiseql-server` `/metrics` endpoint uses a *different* ecosystem
//! (`metrics` / `metrics-exporter-prometheus`) and never calls
//! `prometheus::gather()`, so **it does not scrape these observer metrics** —
//! even when `observers-enterprise` compiles the series in. To collect observer
//! metrics today, mount this crate's `/metrics` handler. Bridging the two
//! registries so the server's `/metrics` also exposes them is tracked at
//! <https://github.com/fraiseql/fraiseql/issues/634>.

use std::sync::OnceLock;

use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts,
    Result as PrometheusResult,
};

/// Lazy-initialized global metrics registry.
///
/// Initialized once per process on first call to the registry accessor.
/// All callers in the same process share this instance.
///
/// **Test isolation**: In a test binary the registry is created by whichever test
/// runs first. Subsequent tests see the same `MetricsRegistry`. Do not rely on
/// the registry being freshly constructed within a single test function.
static GLOBAL_REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();

/// Prometheus metrics for observer system
///
/// Metrics are registered with Prometheus's default global registry
/// when they are created, making them collectible via `prometheus::gather()`.
#[derive(Clone)]
pub struct MetricsRegistry {
    // Event processing metrics
    pub(crate) events_processed_total:                IntCounter,
    pub(crate) events_failed_total:                   IntCounterVec,
    pub(crate) events_deserialization_failures_total: IntCounter,

    // Cache metrics
    pub(crate) cache_hits_total:      IntCounter,
    pub(crate) cache_misses_total:    IntCounter,
    pub(crate) cache_evictions_total: IntCounter,

    // Action execution metrics
    pub(crate) action_executed_total:   IntCounterVec,
    pub(crate) action_duration_seconds: HistogramVec,
    pub(crate) action_errors_total:     IntCounterVec,

    // Queue metrics
    pub(crate) backlog_size:       IntGauge,
    pub(crate) dlq_items:          IntGauge,
    pub(crate) dlq_overflow_total: IntCounter,
}

impl MetricsRegistry {
    /// Return the process-level singleton, initializing it on the first call.
    ///
    /// Registration with the Prometheus global registry happens exactly once per
    /// process, so calling this (or `new()` / `Default::default()`) any number of
    /// times is safe.
    ///
    /// # Errors
    ///
    /// Returns a Prometheus error if metric registration with the global
    /// registry fails (e.g., duplicate metric names from another library).
    ///
    /// # Panics
    ///
    /// Panics if the one-time global metrics initialization itself fails
    /// (e.g., Prometheus metric name collision with another library).
    pub fn global() -> PrometheusResult<Self> {
        Ok(GLOBAL_REGISTRY
            .get_or_init(|| {
                Self::register_metrics().expect("Failed to initialize global metrics registry")
            })
            .clone())
    }

    /// Return the process-level singleton.
    ///
    /// This always returns the same instance so that Prometheus metrics are
    /// registered exactly once.  Calling `new()` multiple times is safe.
    ///
    /// # Errors
    ///
    /// Returns a Prometheus error if metric registration with the global
    /// registry fails on first initialization.
    pub fn new() -> PrometheusResult<Self> {
        Self::global()
    }

    /// Register all metrics and construct the struct.  Called at most once per
    /// process via `GLOBAL_REGISTRY`.
    fn register_metrics() -> PrometheusResult<Self> {
        // Get the default Prometheus registry
        let registry = prometheus::default_registry();

        // Event processing metrics
        let events_processed_total =
            IntCounter::new("fraiseql_observer_events_processed_total", "Total events processed")?;
        registry.register(Box::new(events_processed_total.clone()))?;

        let events_failed_total = IntCounterVec::new(
            Opts::new(
                "fraiseql_observer_events_failed_total",
                "Total events that failed processing",
            ),
            &["error_type"],
        )?;
        registry.register(Box::new(events_failed_total.clone()))?;

        let events_deserialization_failures_total = IntCounter::new(
            "fraiseql_observer_events_deserialization_failures_total",
            "Total events dropped because the raw payload could not be deserialized",
        )?;
        registry.register(Box::new(events_deserialization_failures_total.clone()))?;

        // Cache metrics
        let cache_hits_total =
            IntCounter::new("fraiseql_observer_cache_hits_total", "Total cache hits")?;
        registry.register(Box::new(cache_hits_total.clone()))?;

        let cache_misses_total =
            IntCounter::new("fraiseql_observer_cache_misses_total", "Total cache misses")?;
        registry.register(Box::new(cache_misses_total.clone()))?;

        let cache_evictions_total =
            IntCounter::new("fraiseql_observer_cache_evictions_total", "Total cache evictions")?;
        registry.register(Box::new(cache_evictions_total.clone()))?;

        // Action execution metrics
        let action_executed_total = IntCounterVec::new(
            Opts::new("fraiseql_observer_action_executed_total", "Total actions executed"),
            &["action_type"],
        )?;
        registry.register(Box::new(action_executed_total.clone()))?;

        let action_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "fraiseql_observer_action_duration_seconds",
                "Action execution duration in seconds",
            )
            .buckets(vec![0.001, 0.01, 0.1, 1.0, 5.0, 10.0, 30.0, 60.0]),
            &["action_type"],
        )?;
        registry.register(Box::new(action_duration_seconds.clone()))?;

        let action_errors_total = IntCounterVec::new(
            Opts::new("fraiseql_observer_action_errors_total", "Total action execution errors"),
            &["action_type", "error_type"],
        )?;
        registry.register(Box::new(action_errors_total.clone()))?;

        // Queue metrics
        let backlog_size = IntGauge::new(
            "fraiseql_observer_backlog_size",
            "Current number of events in processing queue",
        )?;
        registry.register(Box::new(backlog_size.clone()))?;

        let dlq_items = IntGauge::new(
            "fraiseql_observer_dlq_items",
            "Current number of items in dead letter queue",
        )?;
        registry.register(Box::new(dlq_items.clone()))?;

        let dlq_overflow_total = IntCounter::new(
            "fraiseql_observer_dlq_overflow_total",
            "Total entries dropped because the dead letter queue was at max_dlq_size",
        )?;
        registry.register(Box::new(dlq_overflow_total.clone()))?;

        Ok(MetricsRegistry {
            events_processed_total,
            events_failed_total,
            events_deserialization_failures_total,
            cache_hits_total,
            cache_misses_total,
            cache_evictions_total,
            action_executed_total,
            action_duration_seconds,
            action_errors_total,
            backlog_size,
            dlq_items,
            dlq_overflow_total,
        })
    }

    /// Record an event was processed
    pub fn event_processed(&self) {
        self.events_processed_total.inc();
    }

    /// Record an event processing failure
    pub fn event_failed(&self, error_type: &str) {
        self.events_failed_total.with_label_values(&[error_type]).inc();
    }

    /// Record an event dropped due to deserialization failure
    pub fn deserialization_failure(&self) {
        self.events_deserialization_failures_total.inc();
    }

    /// Record a cache hit
    pub fn cache_hit(&self) {
        self.cache_hits_total.inc();
    }

    /// Record a cache miss
    pub fn cache_miss(&self) {
        self.cache_misses_total.inc();
    }

    /// Record a cache eviction
    pub fn cache_eviction(&self) {
        self.cache_evictions_total.inc();
    }

    /// Record an action was executed
    pub fn action_executed(&self, action_type: &str, duration_secs: f64) {
        self.action_executed_total.with_label_values(&[action_type]).inc();
        self.action_duration_seconds
            .with_label_values(&[action_type])
            .observe(duration_secs);
    }

    /// Record an action execution error
    pub fn action_error(&self, action_type: &str, error_type: &str) {
        self.action_errors_total.with_label_values(&[action_type, error_type]).inc();
    }

    /// Update the current backlog size
    #[allow(clippy::cast_possible_wrap)] // Reason: backlog size won't exceed i64::MAX
    pub fn set_backlog_size(&self, size: usize) {
        self.backlog_size.set(size as i64);
    }

    /// Update the current DLQ item count
    #[allow(clippy::cast_possible_wrap)] // Reason: DLQ count won't exceed i64::MAX
    pub fn set_dlq_items(&self, count: usize) {
        self.dlq_items.set(count as i64);
    }

    /// Record an entry dropped because the DLQ was full (at `max_dlq_size`).
    pub fn dlq_overflow(&self) {
        self.dlq_overflow_total.inc();
    }

    // Job queue metrics methods

    /// Get cache hit rate as percentage (0-100)
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Reason: f64 precision is acceptable for metrics counters
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits_total.get() as f64;
        let misses = self.cache_misses_total.get() as f64;
        let total = hits + misses;

        if total == 0.0 {
            0.0
        } else {
            (hits / total) * 100.0
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::global().expect("Failed to initialize metrics registry")
    }
}
