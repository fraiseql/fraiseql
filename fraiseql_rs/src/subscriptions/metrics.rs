//! Prometheus metrics for subscriptions
//!
//! Tracks and exposes metrics for WebSocket connections, subscriptions, and events.
//! Includes security metrics from Phase 4 event delivery validation.

use prometheus::{Counter, CounterVec, Gauge, Histogram, HistogramVec, Registry};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Subscription metrics collector
#[derive(Debug)]
pub struct SubscriptionMetrics {
    /// Total WebSocket connections ever created
    pub total_connections: Counter,

    /// Currently active WebSocket connections
    pub active_connections: Gauge,

    /// Total subscriptions ever created
    pub total_subscriptions: Counter,

    /// Currently active subscriptions
    pub active_subscriptions: Gauge,

    /// Total events published
    pub total_events_published: Counter,

    /// Total events delivered to subscribers
    pub total_events_delivered: Counter,

    /// Events by type (counter)
    pub events_by_type: CounterVec,

    /// Subscription latency in seconds (from creation to first event)
    pub subscription_latency_seconds: Histogram,

    /// Event delivery latency in seconds (from publish to delivery)
    pub event_delivery_latency_seconds: Histogram,

    /// WebSocket message size in bytes
    pub message_size_bytes: HistogramVec,

    /// Connection uptime in seconds
    pub connection_uptime_seconds: Histogram,

    /// Active subscriptions per connection
    pub subscriptions_per_connection: Gauge,

    /// Rate limit rejections by reason
    pub rate_limit_rejections: CounterVec,

    /// Errors by type
    pub errors_by_type: CounterVec,
}

impl SubscriptionMetrics {
    /// Create new metrics with default registry
    pub fn new() -> Result<Arc<Self>, prometheus::Error> {
        let registry = Registry::new();
        Self::with_registry(&registry)
    }

    /// Create metrics with custom registry
    pub fn with_registry(registry: &Registry) -> Result<Arc<Self>, prometheus::Error> {
        let total_connections = Counter::new(
            "fraiseql_subscriptions_total_connections",
            "Total WebSocket connections created",
        )?;
        registry.register(Box::new(total_connections.clone()))?;

        let active_connections = Gauge::new(
            "fraiseql_subscriptions_active_connections",
            "Currently active WebSocket connections",
        )?;
        registry.register(Box::new(active_connections.clone()))?;

        let total_subscriptions = Counter::new(
            "fraiseql_subscriptions_total_subscriptions",
            "Total subscriptions created",
        )?;
        registry.register(Box::new(total_subscriptions.clone()))?;

        let active_subscriptions = Gauge::new(
            "fraiseql_subscriptions_active_subscriptions",
            "Currently active subscriptions",
        )?;
        registry.register(Box::new(active_subscriptions.clone()))?;

        let total_events_published = Counter::new(
            "fraiseql_subscriptions_total_events_published",
            "Total events published",
        )?;
        registry.register(Box::new(total_events_published.clone()))?;

        let total_events_delivered = Counter::new(
            "fraiseql_subscriptions_total_events_delivered",
            "Total events delivered to subscribers",
        )?;
        registry.register(Box::new(total_events_delivered.clone()))?;

        let events_by_type = CounterVec::new(
            prometheus::Opts::new(
                "fraiseql_subscriptions_events_by_type",
                "Events published by type",
            ),
            &["event_type"],
        )?;
        registry.register(Box::new(events_by_type.clone()))?;

        let subscription_latency_seconds = Histogram::with_opts(prometheus::HistogramOpts::new(
            "fraiseql_subscriptions_latency_seconds",
            "Subscription latency from creation to first event",
        ))?;
        registry.register(Box::new(subscription_latency_seconds.clone()))?;

        let event_delivery_latency_seconds = Histogram::with_opts(prometheus::HistogramOpts::new(
            "fraiseql_subscriptions_event_delivery_latency_seconds",
            "Event delivery latency from publish to delivery",
        ))?;
        registry.register(Box::new(event_delivery_latency_seconds.clone()))?;

        let message_size_bytes = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "fraiseql_subscriptions_message_size_bytes",
                "WebSocket message size in bytes",
            ),
            &["message_type"],
        )?;
        registry.register(Box::new(message_size_bytes.clone()))?;

        let connection_uptime_seconds = Histogram::with_opts(prometheus::HistogramOpts::new(
            "fraiseql_subscriptions_connection_uptime_seconds",
            "WebSocket connection uptime in seconds",
        ))?;
        registry.register(Box::new(connection_uptime_seconds.clone()))?;

        let subscriptions_per_connection = Gauge::new(
            "fraiseql_subscriptions_per_connection_max",
            "Peak subscriptions per connection",
        )?;
        registry.register(Box::new(subscriptions_per_connection.clone()))?;

        let rate_limit_rejections = CounterVec::new(
            prometheus::Opts::new(
                "fraiseql_subscriptions_rate_limit_rejections",
                "Rate limit rejections by reason",
            ),
            &["reason"],
        )?;
        registry.register(Box::new(rate_limit_rejections.clone()))?;

        let errors_by_type = CounterVec::new(
            prometheus::Opts::new("fraiseql_subscriptions_errors", "Errors by type"),
            &["error_type"],
        )?;
        registry.register(Box::new(errors_by_type.clone()))?;

        Ok(Arc::new(Self {
            total_connections,
            active_connections,
            total_subscriptions,
            active_subscriptions,
            total_events_published,
            total_events_delivered,
            events_by_type,
            subscription_latency_seconds,
            event_delivery_latency_seconds,
            message_size_bytes,
            connection_uptime_seconds,
            subscriptions_per_connection,
            rate_limit_rejections,
            errors_by_type,
        }))
    }

    /// Record new connection
    pub fn record_connection_created(&self) {
        self.total_connections.inc();
        self.active_connections.inc();
    }

    /// Record connection closed
    pub fn record_connection_closed(&self) {
        self.active_connections.dec();
    }

    /// Record subscription created
    pub fn record_subscription_created(&self) {
        self.total_subscriptions.inc();
        self.active_subscriptions.inc();
    }

    /// Record subscription completed
    pub fn record_subscription_completed(&self) {
        self.active_subscriptions.dec();
    }

    /// Record event published
    pub fn record_event_published(&self, event_type: &str) {
        self.total_events_published.inc();
        self.events_by_type.with_label_values(&[event_type]).inc();
    }

    /// Record event delivered
    pub fn record_event_delivered(&self) {
        self.total_events_delivered.inc();
    }

    /// Record subscription latency
    pub fn record_subscription_latency(&self, latency_seconds: f64) {
        self.subscription_latency_seconds.observe(latency_seconds);
    }

    /// Record event delivery latency
    pub fn record_event_delivery_latency(&self, latency_seconds: f64) {
        self.event_delivery_latency_seconds.observe(latency_seconds);
    }

    /// Record WebSocket message size
    pub fn record_message_size(&self, message_type: &str, size_bytes: usize) {
        self.message_size_bytes
            .with_label_values(&[message_type])
            .observe(size_bytes as f64);
    }

    /// Record connection uptime when closing
    pub fn record_connection_uptime(&self, uptime_seconds: f64) {
        self.connection_uptime_seconds.observe(uptime_seconds);
    }

    /// Record max subscriptions per connection
    pub fn record_subscriptions_per_connection(&self, count: usize) {
        self.subscriptions_per_connection.set(count as f64);
    }

    /// Record rate limit rejection
    pub fn record_rate_limit_rejection(&self, reason: &str) {
        self.rate_limit_rejections
            .with_label_values(&[reason])
            .inc();
    }

    /// Record error
    pub fn record_error(&self, error_type: &str) {
        self.errors_by_type.with_label_values(&[error_type]).inc();
    }

    /// Get all metrics in Prometheus text format
    pub fn gather_metrics(&self) -> Result<String, prometheus::Error> {
        // Collect metrics from individual collectors
        let registry = Registry::new();

        registry.register(Box::new(self.total_connections.clone()))?;
        registry.register(Box::new(self.active_connections.clone()))?;
        registry.register(Box::new(self.total_subscriptions.clone()))?;
        registry.register(Box::new(self.active_subscriptions.clone()))?;
        registry.register(Box::new(self.total_events_published.clone()))?;
        registry.register(Box::new(self.total_events_delivered.clone()))?;
        registry.register(Box::new(self.events_by_type.clone()))?;
        registry.register(Box::new(self.subscription_latency_seconds.clone()))?;
        registry.register(Box::new(self.event_delivery_latency_seconds.clone()))?;
        registry.register(Box::new(self.message_size_bytes.clone()))?;
        registry.register(Box::new(self.connection_uptime_seconds.clone()))?;
        registry.register(Box::new(self.subscriptions_per_connection.clone()))?;
        registry.register(Box::new(self.rate_limit_rejections.clone()))?;
        registry.register(Box::new(self.errors_by_type.clone()))?;

        let encoder = prometheus::TextEncoder::new();
        encoder.encode_to_string(&registry.gather())
    }
}

impl Default for SubscriptionMetrics {
    fn default() -> Self {
        // This won't be called in practice, but needed for trait completeness
        panic!("Use SubscriptionMetrics::new() or SubscriptionMetrics::with_registry() instead")
    }
}

/// Security-aware metrics for event delivery validation
///
/// Phase 4 metrics for tracking security-related events:
/// - Event delivery validations
/// - Security violations
/// - Filtering decisions
/// - RBAC rejections
///
/// Uses atomic operations for thread-safe, lock-free metrics collection.
#[derive(Debug)]
pub struct SecurityMetrics {
    /// Total events validated (passed through filter)
    pub validations_total: Arc<AtomicU64>,
    /// Events passed security checks
    pub validations_passed: Arc<AtomicU64>,
    /// Events rejected by security filter
    pub validations_rejected: Arc<AtomicU64>,
    /// Violations due to row-level filtering
    pub violations_row_filter: Arc<AtomicU64>,
    /// Violations due to tenant isolation
    pub violations_tenant_isolation: Arc<AtomicU64>,
    /// Violations due to RBAC field access
    pub violations_rbac: Arc<AtomicU64>,
    /// Violations due to federation boundaries
    pub violations_federation: Arc<AtomicU64>,
}

impl SecurityMetrics {
    /// Create new security metrics
    #[must_use] 
    pub fn new() -> Self {
        Self {
            validations_total: Arc::new(AtomicU64::new(0)),
            validations_passed: Arc::new(AtomicU64::new(0)),
            validations_rejected: Arc::new(AtomicU64::new(0)),
            violations_row_filter: Arc::new(AtomicU64::new(0)),
            violations_tenant_isolation: Arc::new(AtomicU64::new(0)),
            violations_rbac: Arc::new(AtomicU64::new(0)),
            violations_federation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record successful validation
    pub fn record_validation_passed(&self) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        self.validations_passed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed validation due to row-level filtering
    pub fn record_violation_row_filter(&self) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        self.validations_rejected.fetch_add(1, Ordering::Relaxed);
        self.violations_row_filter.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed validation due to tenant isolation
    pub fn record_violation_tenant_isolation(&self) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        self.validations_rejected.fetch_add(1, Ordering::Relaxed);
        self.violations_tenant_isolation
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed validation due to RBAC field access
    pub fn record_violation_rbac(&self) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        self.validations_rejected.fetch_add(1, Ordering::Relaxed);
        self.violations_rbac.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed validation due to federation boundaries
    pub fn record_violation_federation(&self) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        self.validations_rejected.fetch_add(1, Ordering::Relaxed);
        self.violations_federation.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total validations performed
    #[must_use] 
    pub fn total_validations(&self) -> u64 {
        self.validations_total.load(Ordering::Relaxed)
    }

    /// Get total passed validations
    #[must_use] 
    pub fn total_passed(&self) -> u64 {
        self.validations_passed.load(Ordering::Relaxed)
    }

    /// Get total rejected validations
    #[must_use] 
    pub fn total_rejected(&self) -> u64 {
        self.validations_rejected.load(Ordering::Relaxed)
    }

    /// Get rejection rate as percentage (0-100)
    #[must_use] 
    pub fn rejection_rate(&self) -> f64 {
        let total = self.validations_total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let rejected = self.validations_rejected.load(Ordering::Relaxed);
        (rejected as f64 / total as f64) * 100.0
    }

    /// Get total violations by type
    #[must_use] 
    pub fn violation_summary(&self) -> ViolationSummary {
        ViolationSummary {
            row_filter: self.violations_row_filter.load(Ordering::Relaxed),
            tenant_isolation: self.violations_tenant_isolation.load(Ordering::Relaxed),
            rbac: self.violations_rbac.load(Ordering::Relaxed),
            federation: self.violations_federation.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics to zero
    pub fn reset(&self) {
        self.validations_total.store(0, Ordering::Relaxed);
        self.validations_passed.store(0, Ordering::Relaxed);
        self.validations_rejected.store(0, Ordering::Relaxed);
        self.violations_row_filter.store(0, Ordering::Relaxed);
        self.violations_tenant_isolation.store(0, Ordering::Relaxed);
        self.violations_rbac.store(0, Ordering::Relaxed);
        self.violations_federation.store(0, Ordering::Relaxed);
    }
}

impl Clone for SecurityMetrics {
    fn clone(&self) -> Self {
        Self {
            validations_total: Arc::clone(&self.validations_total),
            validations_passed: Arc::clone(&self.validations_passed),
            validations_rejected: Arc::clone(&self.validations_rejected),
            violations_row_filter: Arc::clone(&self.violations_row_filter),
            violations_tenant_isolation: Arc::clone(&self.violations_tenant_isolation),
            violations_rbac: Arc::clone(&self.violations_rbac),
            violations_federation: Arc::clone(&self.violations_federation),
        }
    }
}

impl Default for SecurityMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of security violations by type
#[derive(Debug, Clone)]
pub struct ViolationSummary {
    /// Violations due to row-level filtering (user/tenant ID mismatch)
    pub row_filter: u64,
    /// Violations due to tenant isolation boundaries
    pub tenant_isolation: u64,
    /// Violations due to RBAC field access restrictions
    pub rbac: u64,
    /// Violations due to federation context boundaries
    pub federation: u64,
}

impl ViolationSummary {
    /// Get total violations across all types
    #[must_use] 
    pub const fn total(&self) -> u64 {
        self.row_filter + self.tenant_isolation + self.rbac + self.federation
    }

    /// Get breakdown as percentage of total (if total > 0)
    #[must_use] 
    pub fn percentages(&self) -> Option<ViolationPercentages> {
        let total = self.total();
        if total == 0 {
            return None;
        }

        Some(ViolationPercentages {
            row_filter: (self.row_filter as f64 / total as f64) * 100.0,
            tenant_isolation: (self.tenant_isolation as f64 / total as f64) * 100.0,
            rbac: (self.rbac as f64 / total as f64) * 100.0,
            federation: (self.federation as f64 / total as f64) * 100.0,
        })
    }
}

/// Violation percentages by type
#[derive(Debug, Clone)]
pub struct ViolationPercentages {
    /// Percentage of violations from row filtering
    pub row_filter: f64,
    /// Percentage of violations from tenant isolation
    pub tenant_isolation: f64,
    /// Percentage of violations from RBAC
    pub rbac: f64,
    /// Percentage of violations from federation
    pub federation: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        assert!(Arc::strong_count(&metrics) >= 1);
    }

    #[test]
    fn test_record_connection_created() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_connection_created();
        assert_eq!(metrics.total_connections.get() as u64, 1);
        assert_eq!(metrics.active_connections.get() as u64, 1);
    }

    #[test]
    fn test_record_connection_closed() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_connection_created();
        metrics.record_connection_closed();
        assert_eq!(metrics.total_connections.get() as u64, 1);
        assert_eq!(metrics.active_connections.get() as u64, 0);
    }

    #[test]
    fn test_record_subscription_created() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_subscription_created();
        assert_eq!(metrics.total_subscriptions.get() as u64, 1);
        assert_eq!(metrics.active_subscriptions.get() as u64, 1);
    }

    #[test]
    fn test_record_subscription_completed() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_subscription_created();
        metrics.record_subscription_completed();
        assert_eq!(metrics.total_subscriptions.get() as u64, 1);
        assert_eq!(metrics.active_subscriptions.get() as u64, 0);
    }

    #[test]
    fn test_record_event_published() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_event_published("data_change");
        assert_eq!(metrics.total_events_published.get() as u64, 1);
    }

    #[test]
    fn test_record_event_delivered() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_event_delivered();
        assert_eq!(metrics.total_events_delivered.get() as u64, 1);
    }

    #[test]
    fn test_record_message_size() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_message_size("subscribe", 1024);
        // Just verify it doesn't panic
    }

    #[test]
    fn test_record_rate_limit_rejection() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_rate_limit_rejection("too_many_subscriptions");
        // Just verify it doesn't panic
    }

    #[test]
    fn test_record_error() {
        let metrics = SubscriptionMetrics::new().expect("Failed to create metrics");
        metrics.record_error("connection_timeout");
        // Just verify it doesn't panic
    }

    // ============================================================================
    // PHASE 4.3: Security Metrics - Unit Tests
    // ============================================================================

    #[test]
    fn test_security_metrics_creation() {
        let metrics = SecurityMetrics::new();
        assert_eq!(metrics.total_validations(), 0);
        assert_eq!(metrics.total_passed(), 0);
        assert_eq!(metrics.total_rejected(), 0);
        assert_eq!(metrics.rejection_rate(), 0.0);

        println!("✅ test_security_metrics_creation passed");
    }

    #[test]
    fn test_security_metrics_record_validation_passed() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_validation_passed();
        metrics.record_validation_passed();

        assert_eq!(metrics.total_validations(), 3);
        assert_eq!(metrics.total_passed(), 3);
        assert_eq!(metrics.total_rejected(), 0);
        assert_eq!(metrics.rejection_rate(), 0.0);

        println!("✅ test_security_metrics_record_validation_passed passed");
    }

    #[test]
    fn test_security_metrics_record_violation_row_filter() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_validation_passed();
        metrics.record_violation_row_filter();

        assert_eq!(metrics.total_validations(), 3);
        assert_eq!(metrics.total_passed(), 2);
        assert_eq!(metrics.total_rejected(), 1);
        assert!((metrics.rejection_rate() - 33.33).abs() < 0.1); // ~33.33%

        let summary = metrics.violation_summary();
        assert_eq!(summary.row_filter, 1);
        assert_eq!(summary.tenant_isolation, 0);
        assert_eq!(summary.rbac, 0);
        assert_eq!(summary.federation, 0);

        println!("✅ test_security_metrics_record_violation_row_filter passed");
    }

    #[test]
    fn test_security_metrics_record_violation_tenant_isolation() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_violation_tenant_isolation();
        metrics.record_violation_tenant_isolation();

        assert_eq!(metrics.total_validations(), 3);
        assert_eq!(metrics.total_passed(), 1);
        assert_eq!(metrics.total_rejected(), 2);

        let summary = metrics.violation_summary();
        assert_eq!(summary.tenant_isolation, 2);

        println!("✅ test_security_metrics_record_violation_tenant_isolation passed");
    }

    #[test]
    fn test_security_metrics_record_violation_rbac() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_validation_passed();
        metrics.record_violation_rbac();

        let summary = metrics.violation_summary();
        assert_eq!(summary.rbac, 1);

        println!("✅ test_security_metrics_record_violation_rbac passed");
    }

    #[test]
    fn test_security_metrics_record_violation_federation() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_violation_federation();

        let summary = metrics.violation_summary();
        assert_eq!(summary.federation, 1);

        println!("✅ test_security_metrics_record_violation_federation passed");
    }

    #[test]
    fn test_security_metrics_violation_summary_total() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_violation_row_filter();
        metrics.record_violation_tenant_isolation();
        metrics.record_violation_rbac();
        metrics.record_violation_federation();

        let summary = metrics.violation_summary();
        assert_eq!(summary.total(), 4);
        assert_eq!(summary.row_filter, 1);
        assert_eq!(summary.tenant_isolation, 1);
        assert_eq!(summary.rbac, 1);
        assert_eq!(summary.federation, 1);

        println!("✅ test_security_metrics_violation_summary_total passed");
    }

    #[test]
    fn test_security_metrics_violation_percentages() {
        let metrics = SecurityMetrics::new();

        // 100 validations: 50 passed, 50 rejected
        for _ in 0..50 {
            metrics.record_validation_passed();
        }

        // 25 row filter violations
        for _ in 0..25 {
            metrics.record_violation_row_filter();
        }

        // 15 tenant isolation violations
        for _ in 0..15 {
            metrics.record_violation_tenant_isolation();
        }

        // 10 RBAC violations
        for _ in 0..10 {
            metrics.record_violation_rbac();
        }

        let summary = metrics.violation_summary();
        assert_eq!(summary.total(), 50);

        let percentages = summary.percentages().unwrap();
        assert!((percentages.row_filter - 50.0).abs() < 0.1); // 25/50 = 50%
        assert!((percentages.tenant_isolation - 30.0).abs() < 0.1); // 15/50 = 30%
        assert!((percentages.rbac - 20.0).abs() < 0.1); // 10/50 = 20%
        assert_eq!(percentages.federation, 0.0); // 0/50 = 0%

        println!("✅ test_security_metrics_violation_percentages passed");
    }

    #[test]
    fn test_security_metrics_reset() {
        let metrics = SecurityMetrics::new();

        metrics.record_validation_passed();
        metrics.record_validation_passed();
        metrics.record_violation_row_filter();

        assert_eq!(metrics.total_validations(), 3);
        assert_eq!(metrics.total_passed(), 2);

        metrics.reset();

        assert_eq!(metrics.total_validations(), 0);
        assert_eq!(metrics.total_passed(), 0);
        assert_eq!(metrics.total_rejected(), 0);

        let summary = metrics.violation_summary();
        assert_eq!(summary.total(), 0);

        println!("✅ test_security_metrics_reset passed");
    }

    #[test]
    fn test_security_metrics_clone_shared_state() {
        let metrics1 = SecurityMetrics::new();
        let metrics2 = metrics1.clone();

        metrics1.record_validation_passed();
        metrics1.record_violation_row_filter();

        // Both should see same metrics (shared Arc)
        assert_eq!(metrics2.total_validations(), 2);
        assert_eq!(metrics2.total_passed(), 1);
        assert_eq!(metrics2.total_rejected(), 1);

        metrics2.record_validation_passed();

        // Both should see the new count
        assert_eq!(metrics1.total_validations(), 3);
        assert_eq!(metrics1.total_passed(), 2);

        println!("✅ test_security_metrics_clone_shared_state passed");
    }
}
