//! Performance monitoring and tracking for query execution.
//!
//! Tracks query performance metrics, builds performance profiles, and enables
//! analysis of query execution patterns for optimization.

#[allow(unused_imports)]
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Query performance data.
#[derive(Debug, Clone)]
pub struct QueryPerformance {
    /// Query execution time in microseconds
    pub duration_us: u64,

    /// Number of database queries executed
    pub db_queries: u32,

    /// Estimated query complexity (field count, depth, etc.)
    pub complexity: u32,

    /// Whether result was cached
    pub cached: bool,

    /// Database query time in microseconds
    pub db_duration_us: u64,

    /// Parse time in microseconds (for compilation-free operation)
    pub parse_duration_us: u64,

    /// Validation time in microseconds
    pub validation_duration_us: u64,
}

impl QueryPerformance {
    /// Create new query performance tracker.
    #[must_use]
    pub fn new(
        duration_us: u64,
        db_queries: u32,
        complexity: u32,
        cached: bool,
        db_duration_us: u64,
    ) -> Self {
        Self {
            duration_us,
            db_queries,
            complexity,
            cached,
            db_duration_us,
            parse_duration_us: 0,
            validation_duration_us: 0,
        }
    }

    /// Set parse duration.
    #[must_use]
    pub fn with_parse_duration(mut self, duration_us: u64) -> Self {
        self.parse_duration_us = duration_us;
        self
    }

    /// Set validation duration.
    #[must_use]
    pub fn with_validation_duration(mut self, duration_us: u64) -> Self {
        self.validation_duration_us = duration_us;
        self
    }

    /// Calculate total non-database time in microseconds.
    #[must_use]
    pub fn non_db_duration_us(&self) -> u64 {
        self.duration_us.saturating_sub(self.db_duration_us)
    }

    /// Calculate database time percentage.
    #[must_use]
    pub fn db_percentage(&self) -> f64 {
        if self.duration_us == 0 {
            0.0
        } else {
            (self.db_duration_us as f64 / self.duration_us as f64) * 100.0
        }
    }

    /// Check if query is slow (over threshold).
    #[must_use]
    pub fn is_slow(&self, threshold_ms: f64) -> bool {
        (self.duration_us as f64 / 1000.0) > threshold_ms
    }
}

/// Performance profile for a specific operation type.
#[derive(Debug, Clone)]
pub struct OperationProfile {
    /// Operation name (e.g., "`GetUser`", "`ListProducts`")
    pub operation: String,

    /// Total execution count
    pub count: u64,

    /// Total execution time (microseconds)
    pub total_duration_us: u64,

    /// Minimum execution time
    pub min_duration_us: u64,

    /// Maximum execution time
    pub max_duration_us: u64,

    /// Total database query count
    pub total_db_queries: u64,

    /// Average complexity
    pub avg_complexity: f64,

    /// Cache hit rate (0.0-1.0)
    pub cache_hit_rate: f64,
}

impl OperationProfile {
    /// Calculate average duration in milliseconds.
    #[must_use]
    pub fn avg_duration_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.total_duration_us as f64 / self.count as f64) / 1000.0
        }
    }

    /// Calculate average database queries per operation.
    #[must_use]
    pub fn avg_db_queries(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_db_queries as f64 / self.count as f64
        }
    }
}

/// Performance monitor for tracking query execution metrics.
#[derive(Debug, Clone)]
pub struct PerformanceMonitor {
    /// Total queries tracked
    queries_tracked: Arc<AtomicU64>,

    /// Total slow queries (above threshold)
    slow_queries: Arc<AtomicU64>,

    /// Total cached queries
    cached_queries: Arc<AtomicU64>,

    /// Total database queries
    db_queries_total: Arc<AtomicU64>,

    /// Total query execution time (microseconds)
    total_duration_us: Arc<AtomicU64>,

    /// Minimum query duration (microseconds)
    min_duration_us: Arc<AtomicU64>,

    /// Maximum query duration (microseconds)
    max_duration_us: Arc<AtomicU64>,

    /// Slow query threshold in milliseconds
    slow_query_threshold_ms: f64,
}

impl PerformanceMonitor {
    /// Create new performance monitor.
    #[must_use]
    pub fn new(slow_query_threshold_ms: f64) -> Self {
        Self {
            queries_tracked: Arc::new(AtomicU64::new(0)),
            slow_queries: Arc::new(AtomicU64::new(0)),
            cached_queries: Arc::new(AtomicU64::new(0)),
            db_queries_total: Arc::new(AtomicU64::new(0)),
            total_duration_us: Arc::new(AtomicU64::new(0)),
            min_duration_us: Arc::new(AtomicU64::new(u64::MAX)),
            max_duration_us: Arc::new(AtomicU64::new(0)),
            slow_query_threshold_ms,
        }
    }

    /// Record a query execution.
    pub fn record_query(&self, performance: QueryPerformance) {
        // Track query count
        self.queries_tracked.fetch_add(1, Ordering::Relaxed);

        // Track duration
        self.total_duration_us
            .fetch_add(performance.duration_us, Ordering::Relaxed);

        // Track min/max
        let mut min = self.min_duration_us.load(Ordering::Relaxed);
        while performance.duration_us < min && self.min_duration_us.compare_exchange(min, performance.duration_us, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            min = self.min_duration_us.load(Ordering::Relaxed);
        }

        let mut max = self.max_duration_us.load(Ordering::Relaxed);
        while performance.duration_us > max && self.max_duration_us.compare_exchange(max, performance.duration_us, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            max = self.max_duration_us.load(Ordering::Relaxed);
        }

        // Track slow queries
        if performance.is_slow(self.slow_query_threshold_ms) {
            self.slow_queries.fetch_add(1, Ordering::Relaxed);
        }

        // Track cached queries
        if performance.cached {
            self.cached_queries.fetch_add(1, Ordering::Relaxed);
        }

        // Track database queries
        self.db_queries_total
            .fetch_add(u64::from(performance.db_queries), Ordering::Relaxed);
    }

    /// Get performance statistics.
    #[must_use]
    pub fn stats(&self) -> PerformanceStats {
        let queries_tracked = self.queries_tracked.load(Ordering::Relaxed);
        let slow_queries = self.slow_queries.load(Ordering::Relaxed);
        let cached_queries = self.cached_queries.load(Ordering::Relaxed);
        let db_queries_total = self.db_queries_total.load(Ordering::Relaxed);
        let total_duration_us = self.total_duration_us.load(Ordering::Relaxed);
        let min_duration_us = self.min_duration_us.load(Ordering::Relaxed);
        let max_duration_us = self.max_duration_us.load(Ordering::Relaxed);

        PerformanceStats {
            queries_tracked,
            slow_queries,
            cached_queries,
            db_queries_total,
            total_duration_us,
            min_duration_us,
            max_duration_us,
        }
    }

    /// Get average query duration in milliseconds.
    #[must_use]
    pub fn avg_duration_ms(&self) -> f64 {
        let stats = self.stats();
        if stats.queries_tracked == 0 {
            0.0
        } else {
            (stats.total_duration_us as f64 / stats.queries_tracked as f64) / 1000.0
        }
    }

    /// Get slow query percentage.
    #[must_use]
    pub fn slow_query_percentage(&self) -> f64 {
        let stats = self.stats();
        if stats.queries_tracked == 0 {
            0.0
        } else {
            (stats.slow_queries as f64 / stats.queries_tracked as f64) * 100.0
        }
    }

    /// Get cache hit rate (0.0-1.0).
    #[must_use]
    pub fn cache_hit_rate(&self) -> f64 {
        let stats = self.stats();
        if stats.queries_tracked == 0 {
            0.0
        } else {
            stats.cached_queries as f64 / stats.queries_tracked as f64
        }
    }

    /// Create timing guard for duration tracking.
    #[must_use]
    pub fn create_timer(&self) -> PerformanceTimer {
        PerformanceTimer::new()
    }
}

/// Performance statistics snapshot.
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Total queries tracked
    pub queries_tracked: u64,

    /// Total slow queries
    pub slow_queries: u64,

    /// Total cached queries
    pub cached_queries: u64,

    /// Total database queries
    pub db_queries_total: u64,

    /// Total execution time (microseconds)
    pub total_duration_us: u64,

    /// Minimum duration (microseconds)
    pub min_duration_us: u64,

    /// Maximum duration (microseconds)
    pub max_duration_us: u64,
}

impl PerformanceStats {
    /// Average query duration in milliseconds.
    #[must_use]
    pub fn avg_duration_ms(&self) -> f64 {
        if self.queries_tracked == 0 {
            0.0
        } else {
            (self.total_duration_us as f64 / self.queries_tracked as f64) / 1000.0
        }
    }

    /// Average database queries per operation.
    #[must_use]
    pub fn avg_db_queries(&self) -> f64 {
        if self.queries_tracked == 0 {
            0.0
        } else {
            self.db_queries_total as f64 / self.queries_tracked as f64
        }
    }

    /// Slow query percentage.
    #[must_use]
    pub fn slow_query_percentage(&self) -> f64 {
        if self.queries_tracked == 0 {
            0.0
        } else {
            (self.slow_queries as f64 / self.queries_tracked as f64) * 100.0
        }
    }
}

/// Timer for measuring operation duration.
#[derive(Debug)]
pub struct PerformanceTimer {
    start: Instant,
}

impl PerformanceTimer {
    /// Create new performance timer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Record duration and consume timer.
    #[must_use]
    pub fn record(self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Record duration and get reference to elapsed time.
    #[must_use]
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

impl Default for PerformanceTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_performance_creation() {
        let perf = QueryPerformance::new(1000, 2, 5, false, 500);
        assert_eq!(perf.duration_us, 1000);
        assert_eq!(perf.db_queries, 2);
        assert_eq!(perf.complexity, 5);
        assert!(!perf.cached);
    }

    #[test]
    fn test_query_performance_with_durations() {
        let perf = QueryPerformance::new(1000, 2, 5, false, 500)
            .with_parse_duration(100)
            .with_validation_duration(150);

        assert_eq!(perf.parse_duration_us, 100);
        assert_eq!(perf.validation_duration_us, 150);
    }

    #[test]
    fn test_query_performance_calculations() {
        let perf = QueryPerformance::new(1000, 2, 5, false, 600);

        assert_eq!(perf.non_db_duration_us(), 400);
        assert!(perf.db_percentage() > 59.0 && perf.db_percentage() < 61.0); // ~60%
    }

    #[test]
    fn test_query_performance_is_slow() {
        let fast = QueryPerformance::new(5000, 1, 3, false, 1000); // 5ms
        let slow = QueryPerformance::new(50000, 3, 8, false, 40000); // 50ms

        assert!(!fast.is_slow(10.0));
        assert!(slow.is_slow(10.0));
    }

    #[test]
    fn test_performance_monitor_creation() {
        let monitor = PerformanceMonitor::new(100.0);
        let stats = monitor.stats();

        assert_eq!(stats.queries_tracked, 0);
        assert_eq!(stats.slow_queries, 0);
    }

    #[test]
    fn test_performance_monitor_record_query() {
        let monitor = PerformanceMonitor::new(10.0);
        let perf = QueryPerformance::new(5000, 2, 5, false, 2500);

        monitor.record_query(perf);

        let stats = monitor.stats();
        assert_eq!(stats.queries_tracked, 1);
        assert_eq!(stats.total_duration_us, 5000);
        assert_eq!(stats.db_queries_total, 2);
    }

    #[test]
    fn test_performance_monitor_slow_queries() {
        let monitor = PerformanceMonitor::new(10.0);

        let slow_perf = QueryPerformance::new(20000, 2, 5, false, 10000); // 20ms
        monitor.record_query(slow_perf);

        let stats = monitor.stats();
        assert_eq!(stats.slow_queries, 1);
        assert_eq!(stats.queries_tracked, 1);
    }

    #[test]
    fn test_performance_monitor_cached_queries() {
        let monitor = PerformanceMonitor::new(10.0);

        let cached_perf = QueryPerformance::new(1000, 0, 5, true, 0);
        monitor.record_query(cached_perf);

        let stats = monitor.stats();
        assert_eq!(stats.cached_queries, 1);
    }

    #[test]
    fn test_performance_monitor_avg_calculations() {
        let monitor = PerformanceMonitor::new(10.0);

        monitor.record_query(QueryPerformance::new(2000, 1, 5, false, 1000));
        monitor.record_query(QueryPerformance::new(4000, 3, 5, false, 2000));

        assert!((monitor.avg_duration_ms() - 3.0).abs() < f64::EPSILON); // (2000 + 4000) / 2 / 1000
    }

    #[test]
    fn test_performance_monitor_cache_hit_rate() {
        let monitor = PerformanceMonitor::new(10.0);

        monitor.record_query(QueryPerformance::new(1000, 1, 5, false, 800));
        monitor.record_query(QueryPerformance::new(1000, 0, 5, true, 0));
        monitor.record_query(QueryPerformance::new(1000, 1, 5, false, 800));

        let rate = monitor.cache_hit_rate();
        assert!(rate > 0.32 && rate < 0.34); // ~33%
    }

    #[test]
    fn test_performance_timer_creation() {
        let timer = PerformanceTimer::new();
        let duration = timer.elapsed_us();
        assert!(duration < 1000); // Should be very fast
    }

    #[test]
    fn test_performance_timer_record() {
        let timer = PerformanceTimer::new();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let duration = timer.record();

        assert!(duration >= 10000); // At least 10ms in microseconds
    }

    #[test]
    fn test_performance_stats_calculations() {
        let stats = PerformanceStats {
            queries_tracked: 100,
            slow_queries: 10,
            cached_queries: 30,
            db_queries_total: 200,
            total_duration_us: 500_000,
            min_duration_us: 1000,
            max_duration_us: 50_000,
        };

        assert!((stats.avg_duration_ms() - 5.0).abs() < f64::EPSILON);
        assert!((stats.avg_db_queries() - 2.0).abs() < f64::EPSILON);
        assert!((stats.slow_query_percentage() - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_operation_profile_creation() {
        let profile = OperationProfile {
            operation: "GetUser".to_string(),
            count: 100,
            total_duration_us: 500_000,
            min_duration_us: 1000,
            max_duration_us: 50_000,
            total_db_queries: 200,
            avg_complexity: 5.5,
            cache_hit_rate: 0.75,
        };

        assert!((profile.avg_duration_ms() - 5.0).abs() < f64::EPSILON);
        assert!((profile.avg_db_queries() - 2.0).abs() < f64::EPSILON);
    }
}
