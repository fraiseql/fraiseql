//! Prometheus metrics for subscriptions
//!
//! Tracks and exposes metrics for WebSocket connections, subscriptions, and events.

use prometheus::{Counter, CounterVec, Gauge, Histogram, HistogramVec, Registry};
use std::sync::Arc;

/// Subscription metrics collector
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
        Ok(encoder.encode_to_string(&registry.gather())?)
    }
}

impl Default for SubscriptionMetrics {
    fn default() -> Self {
        // This won't be called in practice, but needed for trait completeness
        panic!("Use SubscriptionMetrics::new() or SubscriptionMetrics::with_registry() instead")
    }
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
}
