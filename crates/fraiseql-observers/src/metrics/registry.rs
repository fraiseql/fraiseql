//! Prometheus metrics registry for observer system
//!
//! This module defines and manages all Prometheus metrics for the observer system.

use std::sync::OnceLock;

use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts,
    Result as PrometheusResult,
};

/// Lazy-initialized global metrics registry
static GLOBAL_REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();

/// Prometheus metrics for observer system
///
/// Metrics are registered with Prometheus's default global registry
/// when they are created, making them collectible via prometheus::gather().
#[derive(Clone)]
pub struct MetricsRegistry {
    // Event processing metrics
    events_processed_total: IntCounter,
    events_failed_total:    IntCounterVec,

    // Cache metrics
    cache_hits_total:      IntCounter,
    cache_misses_total:    IntCounter,
    cache_evictions_total: IntCounter,

    // Deduplication metrics
    dedup_detected_total:           IntCounter,
    dedup_processing_skipped_total: IntCounter,

    // Action execution metrics
    action_executed_total:   IntCounterVec,
    action_duration_seconds: HistogramVec,
    action_errors_total:     IntCounterVec,

    // Queue metrics
    backlog_size: IntGauge,
    dlq_items:    IntGauge,

    // Job queue metrics
    job_queued_total:     IntCounter,
    job_executed_total:   IntCounterVec,
    job_failed_total:     IntCounterVec,
    job_duration_seconds: HistogramVec,
    job_retry_attempts:   IntCounterVec,
    job_queue_depth:      IntGauge,
    job_dlq_items:        IntGauge,
}

impl MetricsRegistry {
    /// Create a new metrics registry and register with default Prometheus registry
    pub fn new() -> PrometheusResult<Self> {
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

        // Deduplication metrics
        let dedup_detected_total = IntCounter::new(
            "fraiseql_observer_dedup_detected_total",
            "Total duplicate events detected and skipped",
        )?;
        registry.register(Box::new(dedup_detected_total.clone()))?;

        let dedup_processing_skipped_total = IntCounter::new(
            "fraiseql_observer_dedup_processing_skipped_total",
            "Total processing cycles saved by deduplication",
        )?;
        registry.register(Box::new(dedup_processing_skipped_total.clone()))?;

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

        // Job queue metrics
        let job_queued_total = IntCounter::new(
            "fraiseql_observer_job_queued_total",
            "Total jobs queued for async execution",
        )?;
        registry.register(Box::new(job_queued_total.clone()))?;

        let job_executed_total = IntCounterVec::new(
            Opts::new("fraiseql_observer_job_executed_total", "Total jobs successfully executed"),
            &["action_type"],
        )?;
        registry.register(Box::new(job_executed_total.clone()))?;

        let job_failed_total = IntCounterVec::new(
            Opts::new("fraiseql_observer_job_failed_total", "Total jobs that failed"),
            &["action_type", "error_type"],
        )?;
        registry.register(Box::new(job_failed_total.clone()))?;

        let job_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "fraiseql_observer_job_duration_seconds",
                "Job execution duration in seconds",
            )
            .buckets(vec![0.001, 0.01, 0.1, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &["action_type"],
        )?;
        registry.register(Box::new(job_duration_seconds.clone()))?;

        let job_retry_attempts = IntCounterVec::new(
            Opts::new("fraiseql_observer_job_retry_attempts_total", "Total job retry attempts"),
            &["action_type"],
        )?;
        registry.register(Box::new(job_retry_attempts.clone()))?;

        let job_queue_depth = IntGauge::new(
            "fraiseql_observer_job_queue_depth",
            "Current number of jobs in queue waiting to execute",
        )?;
        registry.register(Box::new(job_queue_depth.clone()))?;

        let job_dlq_items = IntGauge::new(
            "fraiseql_observer_job_dlq_items",
            "Current number of jobs in job queue dead letter queue",
        )?;
        registry.register(Box::new(job_dlq_items.clone()))?;

        Ok(MetricsRegistry {
            events_processed_total,
            events_failed_total,
            cache_hits_total,
            cache_misses_total,
            cache_evictions_total,
            dedup_detected_total,
            dedup_processing_skipped_total,
            action_executed_total,
            action_duration_seconds,
            action_errors_total,
            backlog_size,
            dlq_items,
            job_queued_total,
            job_executed_total,
            job_failed_total,
            job_duration_seconds,
            job_retry_attempts,
            job_queue_depth,
            job_dlq_items,
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

    /// Record a detected duplicate event
    pub fn dedup_detected(&self) {
        self.dedup_detected_total.inc();
    }

    /// Record processing cycles saved by deduplication
    pub fn dedup_processing_skipped(&self) {
        self.dedup_processing_skipped_total.inc();
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
    pub fn set_backlog_size(&self, size: usize) {
        self.backlog_size.set(size as i64);
    }

    /// Update the current DLQ item count
    pub fn set_dlq_items(&self, count: usize) {
        self.dlq_items.set(count as i64);
    }

    // Job queue metrics methods

    /// Record a job was queued for async execution
    pub fn job_queued(&self) {
        self.job_queued_total.inc();
    }

    /// Record a job was successfully executed
    pub fn job_executed(&self, action_type: &str, duration_secs: f64) {
        self.job_executed_total.with_label_values(&[action_type]).inc();
        self.job_duration_seconds
            .with_label_values(&[action_type])
            .observe(duration_secs);
    }

    /// Record a job execution failure
    pub fn job_failed(&self, action_type: &str, error_type: &str) {
        self.job_failed_total.with_label_values(&[action_type, error_type]).inc();
    }

    /// Record a job retry attempt
    pub fn job_retry_attempt(&self, action_type: &str) {
        self.job_retry_attempts.with_label_values(&[action_type]).inc();
    }

    /// Update the current job queue depth
    pub fn set_job_queue_depth(&self, depth: usize) {
        self.job_queue_depth.set(depth as i64);
    }

    /// Update the current job DLQ item count
    pub fn set_job_dlq_items(&self, count: usize) {
        self.job_dlq_items.set(count as i64);
    }

    /// Get cache hit rate as percentage (0-100)
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

    /// Get deduplication save rate (percentage of processing saved)
    pub fn dedup_save_rate(&self) -> f64 {
        let total_processed = self.events_processed_total.get() as f64;
        let skipped = self.dedup_processing_skipped_total.get() as f64;
        let total_events_encountered = total_processed + skipped;

        if total_events_encountered == 0.0 {
            0.0
        } else {
            (skipped / total_events_encountered) * 100.0
        }
    }

    /// Get or create the global singleton metrics registry
    pub fn global() -> PrometheusResult<Self> {
        Ok(GLOBAL_REGISTRY
            .get_or_init(|| Self::new().expect("Failed to initialize global metrics registry"))
            .clone())
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to initialize metrics registry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_metrics_registry_initialization() {
        // Get the global metrics registry (initializes on first call)
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Verify it was initialized properly (metrics persist across tests due to global registry)
        let cache_hits = metrics.cache_hits_total.get();
        let cache_misses = metrics.cache_misses_total.get();

        // Just verify we can retrieve values (they may be non-zero from other tests)
        assert!(cache_hits >= 0);
        assert!(cache_misses >= 0);
    }

    #[test]
    fn test_event_metrics_recording() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record some events
        metrics.event_processed();
        metrics.event_processed();

        // Verify they were recorded (will be non-zero if this test ran)
        assert!(metrics.events_processed_total.get() >= 2);
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Call cache_hit and cache_miss to test the calculation logic
        let initial_hits = metrics.cache_hits_total.get();
        let initial_misses = metrics.cache_misses_total.get();

        metrics.cache_hit();
        metrics.cache_hit();
        metrics.cache_miss();

        // Verify the new values
        assert_eq!(metrics.cache_hits_total.get(), initial_hits + 2);
        assert_eq!(metrics.cache_misses_total.get(), initial_misses + 1);

        // Cache hit rate should be 2/3 = 66.67%
        let hit_rate = metrics.cache_hit_rate();
        assert!(hit_rate > 60.0 && hit_rate < 100.0);
    }

    #[test]
    fn test_action_execution_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record some action executions
        metrics.action_executed("webhook", 0.5);
        metrics.action_executed("slack", 0.1);

        // Verify they were recorded
        let webhook_count = metrics.action_executed_total.with_label_values(&["webhook"]).get();
        let slack_count = metrics.action_executed_total.with_label_values(&["slack"]).get();

        assert!(webhook_count >= 1);
        assert!(slack_count >= 1);
    }

    #[test]
    fn test_backlog_gauge_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set backlog size
        metrics.set_backlog_size(42);
        assert_eq!(metrics.backlog_size.get(), 42);

        // Update it
        metrics.set_backlog_size(100);
        assert_eq!(metrics.backlog_size.get(), 100);
    }

    #[test]
    fn test_job_queue_metrics_recording() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job queueing
        metrics.job_queued();
        metrics.job_queued();

        // Verify jobs were queued (will be non-zero if this test ran)
        assert!(metrics.job_queued_total.get() >= 2);
    }

    #[test]
    fn test_job_execution_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job executions
        metrics.job_executed("webhook", 0.5);
        metrics.job_executed("email", 1.2);

        // Verify they were recorded
        let webhook_count = metrics.job_executed_total.with_label_values(&["webhook"]).get();
        let email_count = metrics.job_executed_total.with_label_values(&["email"]).get();

        assert!(webhook_count >= 1);
        assert!(email_count >= 1);
    }

    #[test]
    fn test_job_failure_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job failures
        metrics.job_failed("webhook", "timeout");
        metrics.job_failed("webhook", "connection_error");
        metrics.job_failed("email", "authentication_failed");

        // Verify they were recorded
        let webhook_timeout =
            metrics.job_failed_total.with_label_values(&["webhook", "timeout"]).get();
        let webhook_connection = metrics
            .job_failed_total
            .with_label_values(&["webhook", "connection_error"])
            .get();
        let email_auth = metrics
            .job_failed_total
            .with_label_values(&["email", "authentication_failed"])
            .get();

        assert!(webhook_timeout >= 1);
        assert!(webhook_connection >= 1);
        assert!(email_auth >= 1);
    }

    #[test]
    fn test_job_retry_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record retry attempts
        metrics.job_retry_attempt("webhook");
        metrics.job_retry_attempt("webhook");
        metrics.job_retry_attempt("slack");

        // Verify they were recorded
        let webhook_retries = metrics.job_retry_attempts.with_label_values(&["webhook"]).get();
        let slack_retries = metrics.job_retry_attempts.with_label_values(&["slack"]).get();

        assert!(webhook_retries >= 2);
        assert!(slack_retries >= 1);
    }

    #[test]
    fn test_job_queue_depth_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set job queue depth
        metrics.set_job_queue_depth(42);
        assert_eq!(metrics.job_queue_depth.get(), 42);

        // Update it
        metrics.set_job_queue_depth(100);
        assert_eq!(metrics.job_queue_depth.get(), 100);
    }

    #[test]
    fn test_job_dlq_items_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set job DLQ items
        metrics.set_job_dlq_items(5);
        assert_eq!(metrics.job_dlq_items.get(), 5);

        // Update it
        metrics.set_job_dlq_items(15);
        assert_eq!(metrics.job_dlq_items.get(), 15);
    }
}
