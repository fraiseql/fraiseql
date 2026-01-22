//! Prometheus metrics instrumentation for comprehensive production monitoring.
//!
//! This module provides a complete metrics infrastructure for observability,
//! enabling real-time monitoring dashboards, performance tracking, and alerting.
//!
//! # Metrics Overview
//!
//! **Event Processing Metrics**:
//! - `events_processed_total`: Counter of all processed events
//! - `event_processing_duration_ms`: Histogram of event processing duration
//! - `events_in_flight`: Gauge of currently processing events
//!
//! **Action Execution Metrics**:
//! - `actions_executed_total`: Counter of all action executions
//! - `action_success_total`: Counter of successful actions
//! - `action_failure_total`: Counter of failed actions
//! - `action_duration_ms`: Histogram of action execution duration
//! - `action_timeout_total`: Counter of timed-out actions
//!
//! **Queue Metrics**:
//! - `queue_depth`: Gauge of pending jobs
//! - `queue_jobs_enqueued_total`: Counter of enqueued jobs
//! - `queue_jobs_processed_total`: Counter of processed jobs
//! - `queue_retry_total`: Counter of retried jobs
//! - `queue_deadletter_total`: Counter of deadlettered jobs
//!
//! **Cache Metrics**:
//! - `cache_hits_total`: Counter of cache hits
//! - `cache_misses_total`: Counter of cache misses
//! - `cache_hit_rate`: Gauge of hit rate percentage
//!
//! **Deduplication Metrics**:
//! - `dedup_checks_total`: Counter of dedup checks
//! - `dedup_duplicates_found_total`: Counter of duplicates found
//! - `dedup_hit_rate`: Gauge of duplicate detection rate
//!
//! **Checkpoint Metrics**:
//! - `checkpoint_saves_total`: Counter of checkpoint saves
//! - `checkpoint_save_duration_ms`: Histogram of save duration
//! - `checkpoint_recovery_total`: Counter of checkpoint recoveries

#[cfg(feature = "metrics")]
pub mod http;

#[cfg(feature = "metrics")]
use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Registry, Result as PrometheusResult};

/// Central registry for all observer metrics.
///
/// This struct holds all metric definitions and is shared across the system
/// via `Arc<ObserverMetrics>`. All metric operations are thread-safe through
/// Prometheus's internal synchronization.
#[cfg(feature = "metrics")]
pub struct ObserverMetrics {
    /// Prometheus registry for metric collection
    pub registry: Registry,

    // Event processing metrics
    /// Total number of events processed
    pub events_processed_total: Counter,
    /// Distribution of event processing duration in milliseconds
    pub event_processing_duration_ms: Histogram,
    /// Number of events currently in flight
    pub events_in_flight: Gauge,

    // Action execution metrics
    /// Total number of actions executed
    pub actions_executed_total: Counter,
    /// Total number of successful actions
    pub action_success_total: Counter,
    /// Total number of failed actions
    pub action_failure_total: Counter,
    /// Distribution of action execution duration in milliseconds
    pub action_duration_ms: Histogram,
    /// Total number of action timeouts
    pub action_timeout_total: Counter,

    // Queue metrics
    /// Current queue depth (pending jobs)
    pub queue_depth: Gauge,
    /// Total jobs enqueued
    pub queue_jobs_enqueued_total: Counter,
    /// Total jobs processed
    pub queue_jobs_processed_total: Counter,
    /// Total jobs retried
    pub queue_retry_total: Counter,
    /// Total jobs moved to dead letter queue
    pub queue_deadletter_total: Counter,

    // Cache metrics
    /// Total cache hits
    pub cache_hits_total: Counter,
    /// Total cache misses
    pub cache_misses_total: Counter,
    /// Current cache hit rate (percentage)
    pub cache_hit_rate: Gauge,

    // Deduplication metrics
    /// Total deduplication checks
    pub dedup_checks_total: Counter,
    /// Total duplicates found
    pub dedup_duplicates_found_total: Counter,
    /// Current deduplication hit rate (percentage)
    pub dedup_hit_rate: Gauge,

    // Checkpoint metrics
    /// Total checkpoint saves
    pub checkpoint_saves_total: Counter,
    /// Distribution of checkpoint save duration in milliseconds
    pub checkpoint_save_duration_ms: Histogram,
    /// Total checkpoint recoveries
    pub checkpoint_recovery_total: Counter,
}

#[cfg(feature = "metrics")]
impl ObserverMetrics {
    /// Create a new metrics registry and all metric instances.
    ///
    /// # Errors
    ///
    /// Returns error if any metric cannot be registered with the registry.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry = prometheus::Registry::new();
    /// let metrics = ObserverMetrics::new(&registry)?;
    /// ```
    pub fn new(registry: &Registry) -> PrometheusResult<Self> {
        // Event processing metrics
        let events_processed_total = Counter::new(
            "events_processed_total",
            "Total number of events processed",
        )?;
        registry.register(Box::new(events_processed_total.clone()))?;

        let event_processing_duration_ms = Histogram::with_opts(
            HistogramOpts::new(
                "event_processing_duration_ms",
                "Event processing duration in milliseconds",
            ),
        )?;
        registry.register(Box::new(event_processing_duration_ms.clone()))?;

        let events_in_flight =
            Gauge::new("events_in_flight", "Number of events currently in flight")?;
        registry.register(Box::new(events_in_flight.clone()))?;

        // Action execution metrics
        let actions_executed_total =
            Counter::new("actions_executed_total", "Total number of actions executed")?;
        registry.register(Box::new(actions_executed_total.clone()))?;

        let action_success_total =
            Counter::new("action_success_total", "Total number of successful actions")?;
        registry.register(Box::new(action_success_total.clone()))?;

        let action_failure_total =
            Counter::new("action_failure_total", "Total number of failed actions")?;
        registry.register(Box::new(action_failure_total.clone()))?;

        let action_duration_ms = Histogram::with_opts(
            HistogramOpts::new(
                "action_duration_ms",
                "Action execution duration in milliseconds",
            ),
        )?;
        registry.register(Box::new(action_duration_ms.clone()))?;

        let action_timeout_total =
            Counter::new("action_timeout_total", "Total number of action timeouts")?;
        registry.register(Box::new(action_timeout_total.clone()))?;

        // Queue metrics
        let queue_depth = Gauge::new("queue_depth", "Current queue depth (pending jobs)")?;
        registry.register(Box::new(queue_depth.clone()))?;

        let queue_jobs_enqueued_total =
            Counter::new("queue_jobs_enqueued_total", "Total jobs enqueued")?;
        registry.register(Box::new(queue_jobs_enqueued_total.clone()))?;

        let queue_jobs_processed_total =
            Counter::new("queue_jobs_processed_total", "Total jobs processed")?;
        registry.register(Box::new(queue_jobs_processed_total.clone()))?;

        let queue_retry_total = Counter::new("queue_retry_total", "Total jobs retried")?;
        registry.register(Box::new(queue_retry_total.clone()))?;

        let queue_deadletter_total =
            Counter::new("queue_deadletter_total", "Total jobs in dead letter queue")?;
        registry.register(Box::new(queue_deadletter_total.clone()))?;

        // Cache metrics
        let cache_hits_total = Counter::new("cache_hits_total", "Total cache hits")?;
        registry.register(Box::new(cache_hits_total.clone()))?;

        let cache_misses_total = Counter::new("cache_misses_total", "Total cache misses")?;
        registry.register(Box::new(cache_misses_total.clone()))?;

        let cache_hit_rate =
            Gauge::new("cache_hit_rate", "Current cache hit rate (percentage)")?;
        registry.register(Box::new(cache_hit_rate.clone()))?;

        // Deduplication metrics
        let dedup_checks_total =
            Counter::new("dedup_checks_total", "Total deduplication checks")?;
        registry.register(Box::new(dedup_checks_total.clone()))?;

        let dedup_duplicates_found_total =
            Counter::new("dedup_duplicates_found_total", "Total duplicates found")?;
        registry.register(Box::new(dedup_duplicates_found_total.clone()))?;

        let dedup_hit_rate =
            Gauge::new("dedup_hit_rate", "Current deduplication hit rate (percentage)")?;
        registry.register(Box::new(dedup_hit_rate.clone()))?;

        // Checkpoint metrics
        let checkpoint_saves_total =
            Counter::new("checkpoint_saves_total", "Total checkpoint saves")?;
        registry.register(Box::new(checkpoint_saves_total.clone()))?;

        let checkpoint_save_duration_ms = Histogram::with_opts(
            HistogramOpts::new(
                "checkpoint_save_duration_ms",
                "Checkpoint save duration in milliseconds",
            ),
        )?;
        registry.register(Box::new(checkpoint_save_duration_ms.clone()))?;

        let checkpoint_recovery_total =
            Counter::new("checkpoint_recovery_total", "Total checkpoint recoveries")?;
        registry.register(Box::new(checkpoint_recovery_total.clone()))?;

        Ok(Self {
            registry: registry.clone(),
            events_processed_total,
            event_processing_duration_ms,
            events_in_flight,
            actions_executed_total,
            action_success_total,
            action_failure_total,
            action_duration_ms,
            action_timeout_total,
            queue_depth,
            queue_jobs_enqueued_total,
            queue_jobs_processed_total,
            queue_retry_total,
            queue_deadletter_total,
            cache_hits_total,
            cache_misses_total,
            cache_hit_rate,
            dedup_checks_total,
            dedup_duplicates_found_total,
            dedup_hit_rate,
            checkpoint_saves_total,
            checkpoint_save_duration_ms,
            checkpoint_recovery_total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::core::Metric;

    #[test]
    #[cfg(feature = "metrics")]
    fn test_metrics_creation() {
        let registry = Registry::new();
        let _metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Verify registry has metrics
        let families = registry.gather();
        assert!(!families.is_empty());
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_metrics_registration() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Gather metrics to verify registry works
        let families = metrics.registry.gather();
        assert!(!families.is_empty());
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_counter_increment() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        metrics.events_processed_total.inc();
        let metric = metrics.events_processed_total.metric();
        assert_eq!(metric.get_counter().get_value() as i64, 1);

        metrics.events_processed_total.inc_by(5.0);
        let metric = metrics.events_processed_total.metric();
        assert_eq!(metric.get_counter().get_value() as i64, 6);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_gauge_operations() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        metrics.events_in_flight.set(5.0);
        let metric = metrics.events_in_flight.metric();
        assert_eq!(metric.get_gauge().get_value(), 5.0);

        metrics.events_in_flight.inc();
        let metric = metrics.events_in_flight.metric();
        assert_eq!(metric.get_gauge().get_value(), 6.0);

        metrics.events_in_flight.dec();
        let metric = metrics.events_in_flight.metric();
        assert_eq!(metric.get_gauge().get_value(), 5.0);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_histogram_observation() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        metrics.event_processing_duration_ms.observe(100.0);
        metrics.event_processing_duration_ms.observe(200.0);
        metrics.event_processing_duration_ms.observe(150.0);

        let metric = metrics.event_processing_duration_ms.metric();
        assert_eq!(metric.get_histogram().get_sample_count(), 3);
        assert!(metric.get_histogram().get_sample_sum() > 0.0);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_action_metrics() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Simulate action execution
        metrics.actions_executed_total.inc();
        metrics.action_success_total.inc();
        metrics.action_duration_ms.observe(50.0);

        // Verify metrics
        assert_eq!(
            metrics.actions_executed_total.metric().get_counter().get_value() as i64,
            1
        );
        assert_eq!(
            metrics.action_success_total.metric().get_counter().get_value() as i64,
            1
        );
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_queue_metrics() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Simulate queue operations
        metrics.queue_jobs_enqueued_total.inc();
        metrics.queue_depth.inc();
        metrics.queue_jobs_processed_total.inc();
        metrics.queue_depth.dec();

        assert_eq!(
            metrics.queue_jobs_enqueued_total.metric().get_counter().get_value() as i64,
            1
        );
        assert_eq!(metrics.queue_depth.metric().get_gauge().get_value(), 0.0);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_cache_metrics() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Simulate cache operations
        metrics.cache_hits_total.inc();
        metrics.cache_misses_total.inc_by(2.0);

        let total_hits = metrics.cache_hits_total.metric().get_counter().get_value();
        let total_misses = metrics.cache_misses_total.metric().get_counter().get_value();
        let hit_rate = (total_hits / (total_hits + total_misses)) * 100.0;

        metrics.cache_hit_rate.set(hit_rate);

        assert!(hit_rate > 0.0 && hit_rate <= 100.0);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_checkpoint_metrics() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Simulate checkpoint operations
        metrics.checkpoint_saves_total.inc();
        metrics.checkpoint_save_duration_ms.observe(75.0);
        metrics.checkpoint_recovery_total.inc();

        assert_eq!(
            metrics.checkpoint_saves_total.metric().get_counter().get_value() as i64,
            1
        );
        assert_eq!(
            metrics.checkpoint_recovery_total.metric().get_counter().get_value() as i64,
            1
        );
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_dedup_metrics() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Simulate dedup operations
        metrics.dedup_checks_total.inc_by(100.0);
        metrics.dedup_duplicates_found_total.inc_by(25.0);

        let checks = metrics.dedup_checks_total.metric().get_counter().get_value();
        let dups = metrics.dedup_duplicates_found_total.metric().get_counter().get_value();
        let rate = (dups / checks) * 100.0;

        metrics.dedup_hit_rate.set(rate);

        assert_eq!(checks as i64, 100);
        assert_eq!(dups as i64, 25);
        assert!(rate > 0.0 && rate <= 100.0);
    }
}
