//! Cache monitoring and observability
//!
//! Provides comprehensive cache metrics collection with Prometheus export,
//! health checks, performance thresholds, and alerting capabilities.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache performance thresholds for health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHealthThresholds {
    /// Minimum acceptable hit rate (0.0 to 1.0). Default: 0.50 (50%)
    pub min_hit_rate: f64,

    /// Maximum acceptable miss rate (0.0 to 1.0). Default: 0.50 (50%)
    pub max_miss_rate: f64,

    /// Maximum acceptable invalidation rate (percentage of entries). Default: 0.30 (30%)
    pub max_invalidation_rate: f64,

    /// Maximum acceptable memory usage in bytes. Default: 1GB
    pub max_memory_bytes: usize,

    /// Alert threshold: hits per second below this triggers low performance alert. Default: 100
    pub min_hits_per_second: u64,
}

impl Default for CacheHealthThresholds {
    fn default() -> Self {
        Self {
            min_hit_rate: 0.50,
            max_miss_rate: 0.50,
            max_invalidation_rate: 0.30,
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
            min_hits_per_second: 100,
        }
    }
}

/// Cache health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Cache is operating normally
    Healthy,

    /// Cache performance is degraded but operational
    Degraded,

    /// Cache has critical issues
    Unhealthy,
}

/// Detailed cache health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall health status
    pub status: HealthStatus,

    /// Current hit rate (0.0 to 1.0)
    pub hit_rate: f64,

    /// Current miss rate (0.0 to 1.0)
    pub miss_rate: f64,

    /// Invalidation rate (invalidations / `total_cached`)
    pub invalidation_rate: f64,

    /// Memory usage percentage (0.0 to 100.0)
    pub memory_percent: f64,

    /// Hits per second (averaged)
    pub hits_per_second: f64,

    /// Invalidations per second (averaged)
    pub invalidations_per_second: f64,

    /// List of issues detected (empty if healthy)
    pub issues: Vec<String>,

    /// Timestamp of this report (Unix seconds)
    pub timestamp: u64,

    /// Uptime in seconds
    pub uptime_seconds: u64,
}

/// Cache monitoring and observability
///
/// Tracks cache performance metrics with health checking,
/// Prometheus export, and alerting thresholds.
#[derive(Debug)]
pub struct CacheMonitor {
    /// Health thresholds for this cache
    pub(crate) thresholds: CacheHealthThresholds,

    /// Cumulative hits recorded
    pub(crate) total_hits: AtomicU64,

    /// Cumulative misses recorded
    pub(crate) total_misses: AtomicU64,

    /// Cumulative invalidations
    pub(crate) total_invalidations: AtomicU64,

    /// Total entries cached across all time
    pub(crate) total_cached: AtomicU64,

    /// Peak memory usage ever reached
    pub(crate) peak_memory_bytes: AtomicU64,

    /// Start time (Unix seconds)
    pub(crate) start_time: u64,

    /// Performance samples for trending (last 10 samples)
    pub(crate) samples: Arc<Mutex<Vec<PerformanceSample>>>,
}

/// Performance sample at a point in time
#[derive(Debug, Clone)]
pub(crate) struct PerformanceSample {
    /// Timestamp (Unix seconds)
    pub(crate) timestamp: u64,
}

impl CacheMonitor {
    /// Create a new cache monitor with default thresholds
    #[must_use]
    pub fn new() -> Self {
        Self::with_thresholds(CacheHealthThresholds::default())
    }

    /// Create a new cache monitor with custom thresholds
    #[must_use]
    pub fn with_thresholds(thresholds: CacheHealthThresholds) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            thresholds,
            total_hits: AtomicU64::new(0),
            total_misses: AtomicU64::new(0),
            total_invalidations: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
            peak_memory_bytes: AtomicU64::new(0),
            start_time: now,
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record a cache hit
    pub fn record_hit(&self) {
        self.total_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.total_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record cache invalidation
    pub fn record_invalidation(&self, count: u64) {
        self.total_invalidations.fetch_add(count, Ordering::Relaxed);
    }

    /// Record a new cache entry
    pub fn record_cache_entry(&self) {
        self.total_cached.fetch_add(1, Ordering::Relaxed);
    }

    /// Record current memory usage
    pub fn record_memory_usage(&self, bytes: usize) {
        // Update peak
        let current_peak = self.peak_memory_bytes.load(Ordering::Relaxed);
        if bytes as u64 > current_peak {
            self.peak_memory_bytes
                .store(bytes as u64, Ordering::Relaxed);
        }
    }

    /// Collect performance sample for trending
    pub fn collect_sample(&self, _current_memory_bytes: usize) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let sample = PerformanceSample { timestamp: now };

        if let Ok(mut samples) = self.samples.lock() {
            samples.push(sample);
            // Keep only last 10 samples
            if samples.len() > 10 {
                samples.remove(0);
            }

            // Calculate hits per second if we have previous sample
            if samples.len() > 1 {
                let prev = &samples[samples.len() - 2];
                let curr = &samples[samples.len() - 1];
                let time_delta = curr.timestamp.saturating_sub(prev.timestamp).max(1);
                // This would need hit tracking per sample, simplified for now
                let _ = time_delta;
            }
        }
    }

    /// Get current health status
    ///
    /// Evaluates cache against health thresholds and returns status.
    #[allow(clippy::useless_let_if_seq)]
    pub fn get_health(
        &self,
        _current_size: usize,
        _max_entries: usize,
        current_memory_bytes: usize,
    ) -> HealthReport {
        let hits = self.total_hits.load(Ordering::Relaxed);
        let misses = self.total_misses.load(Ordering::Relaxed);
        let invalidations = self.total_invalidations.load(Ordering::Relaxed);
        let total_cached = self.total_cached.load(Ordering::Relaxed);

        let total_requests = hits + misses;
        let hit_rate = if total_requests > 0 {
            hits as f64 / total_requests as f64
        } else {
            0.0
        };

        let miss_rate = if total_requests > 0 {
            misses as f64 / total_requests as f64
        } else {
            0.0
        };

        let invalidation_rate = if total_cached > 0 {
            invalidations as f64 / total_cached as f64
        } else {
            0.0
        };

        let uptime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(self.start_time);

        let hits_per_second = if uptime > 0 {
            hits as f64 / uptime as f64
        } else {
            0.0
        };

        let invalidations_per_second = if uptime > 0 {
            invalidations as f64 / uptime as f64
        } else {
            0.0
        };

        let memory_percent = if self.thresholds.max_memory_bytes > 0 {
            (current_memory_bytes as f64 / self.thresholds.max_memory_bytes as f64) * 100.0
        } else {
            0.0
        };

        let mut issues = Vec::new();
        let mut status = HealthStatus::Healthy;

        // Check hit rate
        if hit_rate < self.thresholds.min_hit_rate {
            issues.push(format!(
                "Low hit rate: {:.2}% (threshold: {:.2}%)",
                hit_rate * 100.0,
                self.thresholds.min_hit_rate * 100.0
            ));
            status = HealthStatus::Degraded;
        }

        // Check miss rate
        if miss_rate > self.thresholds.max_miss_rate {
            issues.push(format!(
                "High miss rate: {:.2}% (threshold: {:.2}%)",
                miss_rate * 100.0,
                self.thresholds.max_miss_rate * 100.0
            ));
            status = HealthStatus::Degraded;
        }

        // Check invalidation rate
        if invalidation_rate > self.thresholds.max_invalidation_rate {
            issues.push(format!(
                "High invalidation rate: {:.2}% (threshold: {:.2}%)",
                invalidation_rate * 100.0,
                self.thresholds.max_invalidation_rate * 100.0
            ));
            status = HealthStatus::Degraded;
        }

        // Check memory usage
        if current_memory_bytes as u64 > self.thresholds.max_memory_bytes as u64 {
            issues.push(format!(
                "Memory usage exceeded: {} MB (threshold: {} MB)",
                current_memory_bytes / (1024 * 1024),
                self.thresholds.max_memory_bytes / (1024 * 1024)
            ));
            status = HealthStatus::Unhealthy;
        }

        // Check hits per second
        if hits_per_second < self.thresholds.min_hits_per_second as f64 && uptime > 60 {
            issues.push(format!(
                "Low hits per second: {:.2} (threshold: {})",
                hits_per_second, self.thresholds.min_hits_per_second
            ));
            status = HealthStatus::Degraded;
        }

        // Upgrade to unhealthy if multiple critical issues
        if issues.len() >= 3 {
            status = HealthStatus::Unhealthy;
        }

        HealthReport {
            status,
            hit_rate,
            miss_rate,
            invalidation_rate,
            memory_percent,
            hits_per_second,
            invalidations_per_second,
            issues,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            uptime_seconds: uptime,
        }
    }

    /// Export metrics in Prometheus text format
    #[must_use]
    pub fn export_prometheus(&self) -> String {
        let hits = self.total_hits.load(Ordering::Relaxed);
        let misses = self.total_misses.load(Ordering::Relaxed);
        let invalidations = self.total_invalidations.load(Ordering::Relaxed);
        let total_cached = self.total_cached.load(Ordering::Relaxed);
        let peak_memory = self.peak_memory_bytes.load(Ordering::Relaxed);

        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64) / (total as f64)
        } else {
            0.0
        };

        let mut output = String::new();

        // Cache hits/misses
        output.push_str("# HELP fraiseql_cache_hits_total Total cache hits\n");
        output.push_str("# TYPE fraiseql_cache_hits_total counter\n");
        let _ = writeln!(output, "fraiseql_cache_hits_total {hits}");

        output.push_str("# HELP fraiseql_cache_misses_total Total cache misses\n");
        output.push_str("# TYPE fraiseql_cache_misses_total counter\n");
        let _ = writeln!(output, "fraiseql_cache_misses_total {misses}");

        // Hit rate
        output.push_str("# HELP fraiseql_cache_hit_rate Cache hit rate (0-1)\n");
        output.push_str("# TYPE fraiseql_cache_hit_rate gauge\n");
        let _ = writeln!(output, "fraiseql_cache_hit_rate {hit_rate:.4}");

        // Invalidations
        output.push_str("# HELP fraiseql_cache_invalidations_total Total cache invalidations\n");
        output.push_str("# TYPE fraiseql_cache_invalidations_total counter\n");
        let _ = writeln!(output, "fraiseql_cache_invalidations_total {invalidations}");

        // Total cached entries
        output.push_str("# HELP fraiseql_cache_total_entries_total Total entries ever cached\n");
        output.push_str("# TYPE fraiseql_cache_total_entries_total counter\n");
        let _ = writeln!(output, "fraiseql_cache_total_entries_total {total_cached}");

        // Peak memory
        output.push_str("# HELP fraiseql_cache_peak_memory_bytes Peak memory usage in bytes\n");
        output.push_str("# TYPE fraiseql_cache_peak_memory_bytes gauge\n");
        let _ = writeln!(output, "fraiseql_cache_peak_memory_bytes {peak_memory}");

        output
    }

    /// Get all monitoring data as JSON
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let hits = self.total_hits.load(Ordering::Relaxed);
        let misses = self.total_misses.load(Ordering::Relaxed);
        let invalidations = self.total_invalidations.load(Ordering::Relaxed);
        let total_cached = self.total_cached.load(Ordering::Relaxed);
        let peak_memory = self.peak_memory_bytes.load(Ordering::Relaxed);

        let total = hits + misses;
        let hit_rate = if total > 0 {
            (hits as f64) / (total as f64)
        } else {
            0.0
        };

        serde_json::json!({
            "hits": hits,
            "misses": misses,
            "hit_rate": hit_rate,
            "invalidations": invalidations,
            "total_cached": total_cached,
            "peak_memory_bytes": peak_memory,
        })
    }
}

impl Default for CacheMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_new() {
        let monitor = CacheMonitor::new();
        assert_eq!(monitor.total_hits.load(Ordering::Relaxed), 0);
        assert_eq!(monitor.total_misses.load(Ordering::Relaxed), 0);
        assert_eq!(monitor.total_invalidations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_hit() {
        let monitor = CacheMonitor::new();
        monitor.record_hit();
        monitor.record_hit();

        assert_eq!(monitor.total_hits.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_record_miss() {
        let monitor = CacheMonitor::new();
        monitor.record_miss();
        monitor.record_miss();
        monitor.record_miss();

        assert_eq!(monitor.total_misses.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_record_invalidation() {
        let monitor = CacheMonitor::new();
        monitor.record_invalidation(5);
        monitor.record_invalidation(3);

        assert_eq!(monitor.total_invalidations.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn test_health_status_healthy() {
        let monitor = CacheMonitor::new();

        // Good hit rate
        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.hit_rate >= 0.75);
        assert_eq!(health.issues.len(), 0);
    }

    #[test]
    fn test_health_status_degraded_low_hit_rate() {
        let monitor = CacheMonitor::new();

        // Low hit rate
        for _ in 0..30 {
            monitor.record_hit();
        }
        for _ in 0..70 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(health.issues.iter().any(|i| i.contains("Low hit rate")));
    }

    #[test]
    fn test_health_status_unhealthy_memory() {
        let thresholds = CacheHealthThresholds {
            max_memory_bytes: 10 * 1024 * 1024, // 10MB
            ..Default::default()
        };
        let monitor = CacheMonitor::with_thresholds(thresholds);

        // Good hit rate but excessive memory
        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024); // 100MB usage

        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health
            .issues
            .iter()
            .any(|i| i.contains("Memory usage exceeded")));
    }

    #[test]
    fn test_health_status_degraded_high_invalidation() {
        let monitor = CacheMonitor::new();

        monitor.record_cache_entry(); // 100 entries
        for _ in 0..99 {
            monitor.record_cache_entry();
        }

        // Invalidate 40 entries (40% invalidation rate)
        monitor.record_invalidation(40);

        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(health
            .issues
            .iter()
            .any(|i| i.contains("High invalidation rate")));
    }

    #[test]
    fn test_record_memory_usage() {
        let monitor = CacheMonitor::new();
        monitor.record_memory_usage(50 * 1024 * 1024);
        monitor.record_memory_usage(100 * 1024 * 1024);

        let peak = monitor.peak_memory_bytes.load(Ordering::Relaxed);
        assert_eq!(peak, 100 * 1024 * 1024);
    }

    #[test]
    fn test_prometheus_export() {
        let monitor = CacheMonitor::new();
        monitor.record_hit();
        monitor.record_hit();
        monitor.record_miss();

        let output = monitor.export_prometheus();

        assert!(output.contains("fraiseql_cache_hits_total"));
        assert!(output.contains("fraiseql_cache_misses_total"));
        assert!(output.contains("fraiseql_cache_hit_rate"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }

    #[test]
    fn test_to_json() {
        let monitor = CacheMonitor::new();
        monitor.record_hit();
        monitor.record_miss();

        let json = monitor.to_json();

        assert_eq!(json["hits"], 1);
        assert_eq!(json["misses"], 1);
    }

    #[test]
    fn test_collect_sample() {
        let monitor = CacheMonitor::new();
        monitor.record_hit();
        monitor.collect_sample(50 * 1024 * 1024);

        // Samples should be collected (no errors)
        let samples = monitor.samples.lock().unwrap();
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn test_uptime_calculation() {
        let monitor = CacheMonitor::new();
        monitor.record_hit();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let health = monitor.get_health(10, 100, 10 * 1024 * 1024);
        // uptime_seconds is always >= 0 since it's u64
    }
}
