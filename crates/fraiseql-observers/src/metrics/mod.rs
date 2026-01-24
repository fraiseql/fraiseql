//! Prometheus metrics for the observer system
//!
//! This module provides comprehensive metrics for production monitoring including:
//! - Event processing statistics
//! - Cache performance
//! - Deduplication effectiveness
//! - Action execution times
//! - Queue/backlog monitoring

#[cfg(feature = "metrics")]
pub mod registry;
#[cfg(feature = "metrics")]
pub mod handler;

#[cfg(feature = "metrics")]
pub use registry::MetricsRegistry;

// No-op metrics when feature is disabled
#[cfg(not(feature = "metrics"))]
pub struct MetricsRegistry;

#[cfg(not(feature = "metrics"))]
impl MetricsRegistry {
    /// Create a no-op metrics registry when metrics feature is disabled
    pub fn new() -> Self {
        MetricsRegistry
    }

    /// Increment a counter (no-op)
    pub fn event_processed(&self) {}

    /// Record cache hit (no-op)
    pub fn cache_hit(&self) {}

    /// Record cache miss (no-op)
    pub fn cache_miss(&self) {}

    /// Record dedup detection (no-op)
    pub fn dedup_detected(&self) {}

    /// Record action duration (no-op)
    pub fn action_executed(&self, _action_type: &str, _duration_secs: f64) {}

    /// Record action error (no-op)
    pub fn action_error(&self, _action_type: &str, _error_type: &str) {}

    /// Update backlog size (no-op)
    pub fn set_backlog_size(&self, _size: usize) {}

    /// Get current metric registry (no-op)
    pub fn registry(&self) -> Option<&'static prometheus::Registry> {
        None
    }
}

#[cfg(not(feature = "metrics"))]
impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}
