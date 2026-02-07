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
    pub fn with_failure(mut self) -> Self {
        self.success = false;
        self
    }

    /// Get latency in milliseconds
    pub fn latency_ms(&self) -> f64 {
        self.latency_us as f64 / 1000.0
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
    pub fn is_full(&self) -> bool {
        self.fields.len() >= self.max_size
    }

    /// Get batch size
    pub fn size(&self) -> usize {
        self.fields.len()
    }

    /// Clear batch
    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

/// Key cache with LRU eviction
pub struct KeyCache {
    /// Cached keys
    cache:        HashMap<String, Vec<u8>>,
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
            Some(key.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert key into cache
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

        // Insert or update
        self.cache.insert(key_path.clone(), key);

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
        let hits = self.hits.load(Ordering::Relaxed) as f64;
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
        let idx = (latencies.len() as f64 * 0.99) as usize;
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
        let successful = self.metrics.iter().filter(|m| m.success).count() as f64;
        successful / self.metrics.len() as f64
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
    pub fn operations_per_second(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        self.metrics.len() as f64
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
    pub fn metric_count(&self) -> usize {
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
        self.start.elapsed().as_micros() as u64
    }

    /// Get elapsed milliseconds
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed_us() as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_metrics_creation() {
        let metric = OperationMetrics::new("encrypt", 1000, 5);
        assert_eq!(metric.operation, "encrypt");
        assert_eq!(metric.latency_us, 1000);
        assert_eq!(metric.field_count, 5);
        assert!(metric.success);
    }

    #[test]
    fn test_operation_metrics_failure() {
        let metric = OperationMetrics::new("decrypt", 2000, 3).with_failure();
        assert!(!metric.success);
    }

    #[test]
    fn test_operation_metrics_latency_ms() {
        let metric = OperationMetrics::new("encrypt", 5000, 1);
        assert_eq!(metric.latency_ms(), 5.0);
    }

    #[test]
    fn test_encryption_batch_creation() {
        let batch = EncryptionBatch::new("batch1", 100);
        assert_eq!(batch.batch_id, "batch1");
        assert_eq!(batch.max_size, 100);
        assert_eq!(batch.size(), 0);
    }

    #[test]
    fn test_encryption_batch_add_field() {
        let mut batch = EncryptionBatch::new("batch1", 10);
        let result = batch.add_field("email", "user@example.com");
        assert!(result);
        assert_eq!(batch.size(), 1);
    }

    #[test]
    fn test_encryption_batch_full() {
        let mut batch = EncryptionBatch::new("batch1", 2);
        batch.add_field("email", "user@example.com");
        batch.add_field("phone", "555-1234");
        assert!(batch.is_full());
        let result = batch.add_field("ssn", "123-45-6789");
        assert!(!result); // Batch full
    }

    #[test]
    fn test_encryption_batch_clear() {
        let mut batch = EncryptionBatch::new("batch1", 10);
        batch.add_field("email", "user@example.com");
        assert_eq!(batch.size(), 1);
        batch.clear();
        assert_eq!(batch.size(), 0);
    }

    #[test]
    fn test_key_cache_creation() {
        let cache = KeyCache::new(100);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn test_key_cache_insert_and_get() {
        let mut cache = KeyCache::new(100);
        let key = vec![1, 2, 3, 4];
        cache.insert("key1", key.clone());
        let retrieved = cache.get("key1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), key);
    }

    #[test]
    fn test_key_cache_miss() {
        let mut cache = KeyCache::new(100);
        let result = cache.get("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_key_cache_lru_eviction() {
        let mut cache = KeyCache::new(2);
        cache.insert("key1", vec![1]);
        cache.insert("key2", vec![2]);
        cache.insert("key3", vec![3]); // Should evict key1

        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_some());
        assert!(cache.get("key3").is_some());
    }

    #[test]
    fn test_key_cache_hit_rate() {
        let mut cache = KeyCache::new(100);
        cache.insert("key1", vec![1]);
        cache.get("key1"); // hit
        cache.get("key1"); // hit
        cache.get("key2"); // miss

        let (hits, misses) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert_eq!(cache.hit_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_key_cache_clear() {
        let mut cache = KeyCache::new(100);
        cache.insert("key1", vec![1]);
        assert_eq!(cache.size(), 1);
        cache.clear();
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_key_cache_default() {
        let cache = KeyCache::default();
        assert_eq!(cache.max_size, 1000);
    }

    #[test]
    fn test_performance_monitor_record_metric() {
        let mut monitor = PerformanceMonitor::new(100);
        let metric = OperationMetrics::new("encrypt", 1000, 5);
        monitor.record_metric(metric);
        assert_eq!(monitor.metric_count(), 1);
    }

    #[test]
    fn test_performance_monitor_average_latency() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 2000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 3000, 5));
        assert_eq!(monitor.average_latency_us(), 2000);
    }

    #[test]
    fn test_performance_monitor_p99_latency() {
        let mut monitor = PerformanceMonitor::new(100);
        for i in 1..=100 {
            monitor.record_metric(OperationMetrics::new("encrypt", i * 100, 1));
        }
        let p99 = monitor.p99_latency_us();
        assert!(p99 >= 9900); // p99 should be high
    }

    #[test]
    fn test_performance_monitor_success_rate() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        let mut metric = OperationMetrics::new("encrypt", 2000, 5);
        metric = metric.with_failure();
        monitor.record_metric(metric);
        assert_eq!(monitor.success_rate(), 0.5);
    }

    #[test]
    fn test_performance_monitor_slo() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.set_slo("encrypt", 2000);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 1500, 5));
        assert!(monitor.check_slo("encrypt"));
    }

    #[test]
    fn test_performance_monitor_slo_violation() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.set_slo("encrypt", 1400);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 2000, 5));
        assert!(!monitor.check_slo("encrypt")); // Average 1500 > SLO 1400
    }

    #[test]
    fn test_performance_monitor_clear() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        assert_eq!(monitor.metric_count(), 1);
        monitor.clear();
        assert_eq!(monitor.metric_count(), 0);
    }

    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed_us = timer.elapsed_us();
        assert!(elapsed_us >= 10000); // At least 10ms
    }

    #[test]
    fn test_operation_timer_ms() {
        let timer = OperationTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed_ms = timer.elapsed_ms();
        assert!(elapsed_ms >= 10.0);
    }

    #[test]
    fn test_performance_monitor_metrics_for_operation() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("decrypt", 2000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 1500, 5));

        let encrypt_metrics = monitor.metrics_for_operation("encrypt");
        assert_eq!(encrypt_metrics.len(), 2);
    }

    #[test]
    fn test_performance_monitor_successful_metrics() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        let mut failed = OperationMetrics::new("encrypt", 2000, 5);
        failed = failed.with_failure();
        monitor.record_metric(failed);

        let successful = monitor.successful_metrics();
        assert_eq!(successful.len(), 1);
    }

    #[test]
    fn test_performance_monitor_failed_metrics() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        let mut failed = OperationMetrics::new("encrypt", 2000, 5);
        failed = failed.with_failure();
        monitor.record_metric(failed);

        let failed_metrics = monitor.failed_metrics();
        assert_eq!(failed_metrics.len(), 1);
    }

    #[test]
    fn test_performance_monitor_p50_latency() {
        let mut monitor = PerformanceMonitor::new(100);
        for i in 1..=10 {
            monitor.record_metric(OperationMetrics::new("encrypt", i * 100, 1));
        }
        let p50 = monitor.p50_latency_us();
        assert_eq!(p50, 600); // Median of 100-1000 (index 5 in 0-9 range)
    }

    #[test]
    fn test_performance_monitor_max_min_latency() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 5000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 3000, 5));

        assert_eq!(monitor.max_latency_us(), 5000);
        assert_eq!(monitor.min_latency_us(), 1000);
    }

    #[test]
    fn test_performance_monitor_error_rate() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        let mut failed = OperationMetrics::new("encrypt", 2000, 5);
        failed = failed.with_failure();
        monitor.record_metric(failed);

        assert_eq!(monitor.error_rate(), 0.5);
    }

    #[test]
    fn test_performance_monitor_total_fields_processed() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 2000, 10));
        monitor.record_metric(OperationMetrics::new("encrypt", 3000, 3));

        assert_eq!(monitor.total_fields_processed(), 18);
    }

    #[test]
    fn test_performance_monitor_average_latency_for_operation() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("decrypt", 4000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 2000, 5));

        let avg_encrypt = monitor.average_latency_for_operation_us("encrypt");
        assert_eq!(avg_encrypt, 1500);
    }

    #[test]
    fn test_performance_monitor_check_all_slos() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.set_slo("encrypt", 2000);
        monitor.set_slo("decrypt", 3000);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("decrypt", 4000, 5));

        let violations = monitor.check_all_slos();
        assert_eq!(violations.len(), 2);

        // Check that encrypt passes and decrypt fails (order-independent)
        let encrypt_pass = violations.iter().any(|(op, passed)| op == "encrypt" && *passed);
        let decrypt_fail = violations.iter().any(|(op, passed)| op == "decrypt" && !*passed);
        assert!(encrypt_pass);
        assert!(decrypt_fail);
    }

    #[test]
    fn test_performance_monitor_operation_count() {
        let mut monitor = PerformanceMonitor::new(100);
        monitor.record_metric(OperationMetrics::new("encrypt", 1000, 5));
        monitor.record_metric(OperationMetrics::new("encrypt", 2000, 5));
        monitor.record_metric(OperationMetrics::new("decrypt", 3000, 5));

        assert_eq!(monitor.operation_count("encrypt"), 2);
        assert_eq!(monitor.operation_count("decrypt"), 1);
    }
}
