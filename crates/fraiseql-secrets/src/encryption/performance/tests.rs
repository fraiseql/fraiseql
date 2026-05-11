#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
#[allow(clippy::float_cmp)] // Reason: 5000/1000.0 is exactly representable as f64
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
#[allow(clippy::float_cmp)] // Reason: comparing exact f64 division results in test
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
#[allow(clippy::float_cmp)] // Reason: 1.0/2.0 is exactly representable as f64
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
#[allow(clippy::float_cmp)] // Reason: 1.0 - 0.5 is exactly representable as f64
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
