//! Performance optimization for encryption operations including batching,
//! parallelization, caching, and metrics collection.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use chrono::{DateTime, Utc};
use zeroize::Zeroizing;

/// Operation metrics for performance monitoring
#[derive(Debug, Clone)]
pub struct OperationMetrics {
    /// Operation type (encrypt, decrypt)
    pub operation:   String,
    /// Latency in microseconds
    pub latency_us:  u64,
    /// Success indicator
    pub success:     bool,
    /// Timestamp
    pub timestamp:   DateTime<Utc>,
    /// Field count (for batch operations)
    pub field_count: usize,
}

impl OperationMetrics {
    /// Create new operation metrics
    pub fn new(operation: impl Into<String>, latency_us: u64, field_count: usize) -> Self {
        Self {
            operation: operation.into(),
            latency_us,
            success: true,
            timestamp: Utc::now(),
            field_count,
        }
    }

    /// Mark as failed
    pub const fn with_failure(mut self) -> Self {
        self.success = false;
        self
    }

    /// Get latency in milliseconds
    pub fn latency_ms(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        // Reason: microsecond latency will never exceed f64 mantissa range
        let us = self.latency_us as f64;
        us / 1000.0
    }
}

/// Batch of encryption operations
#[derive(Debug, Clone)]
pub struct EncryptionBatch {
    /// Batch ID
    pub batch_id:   String,
    /// Fields to encrypt
    pub fields:     Vec<(String, String)>,
    /// Batch creation time
    pub created_at: DateTime<Utc>,
    /// Maximum batch size
    pub max_size:   usize,
}

impl EncryptionBatch {
    /// Create new batch
    pub fn new(batch_id: impl Into<String>, max_size: usize) -> Self {
        Self {
            batch_id: batch_id.into(),
            fields: Vec::new(),
            created_at: Utc::now(),
            max_size,
        }
    }

    /// Add field to batch
    pub fn add_field(
        &mut self,
        field_name: impl Into<String>,
        plaintext: impl Into<String>,
    ) -> bool {
        if self.fields.len() >= self.max_size {
            return false;
        }
        self.fields.push((field_name.into(), plaintext.into()));
        true
    }

    /// Check if batch is full
    pub const fn is_full(&self) -> bool {
        self.fields.len() >= self.max_size
    }

    /// Get batch size
    pub const fn size(&self) -> usize {
        self.fields.len()
    }

    /// Clear batch
    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

/// Key cache with LRU eviction.
///
/// Key bytes are stored in [`Zeroizing`] wrappers so that evicted entries are
/// overwritten in memory rather than lingering until the allocator reuses them.
pub struct KeyCache {
    /// Cached keys (zeroed on eviction/drop)
    cache:        HashMap<String, Zeroizing<Vec<u8>>>,
    /// Maximum cache size
    max_size:     usize,
    /// Access order for LRU
    access_order: Vec<String>,
    /// Cache hits
    hits:         Arc<AtomicU64>,
    /// Cache misses
    misses:       Arc<AtomicU64>,
}

impl KeyCache {
    /// Create new key cache
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            access_order: Vec::new(),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get key from cache
    pub fn get(&mut self, key_path: &str) -> Option<Vec<u8>> {
        if let Some(key) = self.cache.get(key_path) {
            // Update access order
            self.access_order.retain(|k| k != key_path);
            self.access_order.push(key_path.to_string());
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some((**key).clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert key into cache.
    ///
    /// The key bytes are stored in a [`Zeroizing`] wrapper so they are
    /// overwritten in memory when the entry is evicted or the cache is cleared.
    pub fn insert(&mut self, key_path: impl Into<String>, key: Vec<u8>) {
        let key_path = key_path.into();

        // If at capacity, evict LRU entry
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&key_path) {
            if let Some(lru_key) = self.access_order.first() {
                let lru = lru_key.clone();
                self.cache.remove(&lru);
                self.access_order.remove(0);
            }
        }

        // Insert or update (wrapped to zero on eviction)
        self.cache.insert(key_path.clone(), Zeroizing::new(key));

        // Update access order
        self.access_order.retain(|k| k != &key_path);
        self.access_order.push(key_path);
    }

    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64) {
        (self.hits.load(Ordering::Relaxed), self.misses.load(Ordering::Relaxed))
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        // Reason: cache hit/miss counters won't exceed f64 mantissa range
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        #[allow(clippy::cast_precision_loss)]
        // Reason: cache hit/miss counters won't exceed f64 mantissa range
        let misses = self.misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total > 0.0 { hits / total } else { 0.0 }
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }

    /// Get cached entry count
    pub fn entry_count(&self) -> usize {
        self.cache.len()
    }
}

impl Default for KeyCache {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Performance metrics collector
pub struct PerformanceMonitor {
    /// Collected metrics
    metrics:     Vec<OperationMetrics>,
    /// Maximum metrics to retain
    max_metrics: usize,
    /// Performance SLOs
    slos:        HashMap<String, u64>,
}

impl PerformanceMonitor {
    /// Create new performance monitor
    pub fn new(max_metrics: usize) -> Self {
        Self {
            metrics: Vec::new(),
            max_metrics,
            slos: HashMap::new(),
        }
    }

    /// Record operation metric
    pub fn record_metric(&mut self, metric: OperationMetrics) {
        // Keep bounded history
        if self.metrics.len() >= self.max_metrics {
            self.metrics.remove(0);
        }
        self.metrics.push(metric);
    }

    /// Set SLO for operation
    pub fn set_slo(&mut self, operation: impl Into<String>, latency_us: u64) {
        self.slos.insert(operation.into(), latency_us);
    }

    /// Internal filter helper
    fn filter_metrics<F>(&self, predicate: F) -> Vec<&OperationMetrics>
    where
        F: Fn(&&OperationMetrics) -> bool,
    {
        self.metrics.iter().filter(predicate).collect()
    }

    /// Get metrics for operation
    pub fn metrics_for_operation(&self, operation: &str) -> Vec<&OperationMetrics> {
        self.filter_metrics(|m| m.operation == operation)
    }

    /// Get successful metrics
    pub fn successful_metrics(&self) -> Vec<&OperationMetrics> {
        self.filter_metrics(|m| m.success)
    }

    /// Get failed metrics
    pub fn failed_metrics(&self) -> Vec<&OperationMetrics> {
        self.filter_metrics(|m| !m.success)
    }

    /// Get average latency
    pub fn average_latency_us(&self) -> u64 {
        if self.metrics.is_empty() {
            return 0;
        }
        let sum: u64 = self.metrics.iter().map(|m| m.latency_us).sum();
        sum / self.metrics.len() as u64
    }

    /// Get average latency for operation
    pub fn average_latency_for_operation_us(&self, operation: &str) -> u64 {
        let metrics = self.metrics_for_operation(operation);
        if metrics.is_empty() {
            return 0;
        }
        let sum: u64 = metrics.iter().map(|m| m.latency_us).sum();
        sum / metrics.len() as u64
    }

    /// Get p50 latency (median)
    pub fn p50_latency_us(&self) -> u64 {
        if self.metrics.is_empty() {
            return 0;
        }
        let mut latencies: Vec<_> = self.metrics.iter().map(|m| m.latency_us).collect();
        latencies.sort_unstable();
        let idx = latencies.len() / 2;
        latencies[idx]
    }

    /// Get p99 latency
    pub fn p99_latency_us(&self) -> u64 {
        if self.metrics.is_empty() {
            return 0;
        }
        let mut latencies: Vec<_> = self.metrics.iter().map(|m| m.latency_us).collect();
        latencies.sort_unstable();
        let idx = latencies.len().saturating_mul(99) / 100;
        latencies[idx]
    }

    /// Get max latency
    pub fn max_latency_us(&self) -> u64 {
        self.metrics.iter().map(|m| m.latency_us).max().unwrap_or(0)
    }

    /// Get min latency
    pub fn min_latency_us(&self) -> u64 {
        self.metrics.iter().map(|m| m.latency_us).min().unwrap_or(0)
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        // Reason: metric counts won't exceed f64 mantissa range
        let successful = self.metrics.iter().filter(|m| m.success).count() as f64;
        #[allow(clippy::cast_precision_loss)]
        // Reason: metric counts won't exceed f64 mantissa range
        let total = self.metrics.len() as f64;
        successful / total
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }

    /// Get total fields processed
    pub fn total_fields_processed(&self) -> usize {
        self.metrics.iter().map(|m| m.field_count).sum()
    }

    /// Get operations per second
    pub const fn operations_per_second(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        // Reason: metric counts won't exceed f64 mantissa range
        let count = self.metrics.len() as f64;
        count
    }

    /// Check if SLO violated
    pub fn check_slo(&self, operation: &str) -> bool {
        if let Some(slo) = self.slos.get(operation) {
            let avg_latency = self.average_latency_for_operation_us(operation);
            avg_latency <= *slo
        } else {
            true // No SLO defined
        }
    }

    /// Get all SLO violations
    pub fn check_all_slos(&self) -> Vec<(String, bool)> {
        self.slos.keys().map(|op| (op.clone(), self.check_slo(op))).collect()
    }

    /// Get metric count
    pub const fn metric_count(&self) -> usize {
        self.metrics.len()
    }

    /// Get count by operation
    pub fn operation_count(&self, operation: &str) -> usize {
        self.metrics_for_operation(operation).len()
    }

    /// Clear metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new(10000)
    }
}

/// Operation timing utility
pub struct OperationTimer {
    start: Instant,
}

impl OperationTimer {
    /// Start operation timer
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get elapsed microseconds
    pub fn elapsed_us(&self) -> u64 {
        #[allow(clippy::cast_possible_truncation)]
        // Reason: elapsed micros won't exceed u64::MAX in practice
        let us = self.start.elapsed().as_micros() as u64;
        us
    }

    /// Get elapsed milliseconds
    pub fn elapsed_ms(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        // Reason: microsecond latency will never exceed f64 mantissa range
        let us = self.elapsed_us() as f64;
        us / 1000.0
    }
}

#[cfg(test)]
mod tests;
