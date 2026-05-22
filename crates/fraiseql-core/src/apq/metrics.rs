//! APQ metrics tracking
//!
//! Provides metrics for monitoring APQ performance including cache hit rates,
//! query storage, and error tracking.

use std::sync::atomic::{AtomicU64, Ordering};

/// APQ metrics tracker
///
/// Tracks performance metrics for APQ including:
/// - Cache hits (queries retrieved from cache)
/// - Cache misses (queries not found, client provides full query)
/// - Queries stored (new queries persisted)
/// - Errors (failed operations)
///
/// All operations are lock-free using atomic operations.
#[derive(Debug)]
pub struct ApqMetrics {
    /// Number of cache hits
    hits: AtomicU64,

    /// Number of cache misses
    misses: AtomicU64,

    /// Number of queries stored
    stored: AtomicU64,

    /// Number of errors
    errors: AtomicU64,
}

impl ApqMetrics {
    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a query stored
    pub fn record_store(&self) {
        self.stored.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total hits
    #[must_use]
    pub fn get_hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Get total misses
    #[must_use]
    pub fn get_misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Get total stored
    #[must_use]
    pub fn get_stored(&self) -> u64 {
        self.stored.load(Ordering::Relaxed)
    }

    /// Get total errors
    #[must_use]
    pub fn get_errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Get cache hit rate as percentage (0.0 to 1.0)
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);

        if hits + misses == 0 {
            0.0
        } else {
            // Small precision loss is acceptable for metrics percentages
            #[allow(clippy::cast_precision_loss)]
            // Reason: APQ hit/miss counters are display metrics; f64 precision loss is acceptable
            {
                hits as f64 / (hits + misses) as f64
            }
        }
    }

    /// Get metrics as JSON value
    #[must_use]
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hits": self.get_hits(),
            "misses": self.get_misses(),
            "stored": self.get_stored(),
            "errors": self.get_errors(),
            "hit_rate": self.hit_rate(),
        })
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.stored.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
    }
}

impl Default for ApqMetrics {
    fn default() -> Self {
        Self {
            hits:   AtomicU64::new(0),
            misses: AtomicU64::new(0),
            stored: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }
}
