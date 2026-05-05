//! Canonical metrics implementation for `fraiseql-server`.
//!
//! Use [`MetricsCollector`] to instrument request handling.
//! The previous `observability/metrics.rs` ghost layer has been removed.
//!
//! Tracks:
//! - GraphQL query execution time
//! - Query success/error rates
//! - Database query performance
//! - Connection pool statistics
//! - HTTP request/response metrics

use std::{
    fmt::Write as _,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use dashmap::DashMap;

/// Metrics collector for the server.
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// Total GraphQL queries executed
    pub queries_total: Arc<AtomicU64>,

    /// Total successful queries
    pub queries_success: Arc<AtomicU64>,

    /// Total failed queries
    pub queries_error: Arc<AtomicU64>,

    /// Total query execution time (microseconds)
    pub queries_duration_us: Arc<AtomicU64>,

    /// Total database queries executed
    pub db_queries_total: Arc<AtomicU64>,

    /// Total database query time (microseconds)
    pub db_queries_duration_us: Arc<AtomicU64>,

    /// Total validation errors
    pub validation_errors_total: Arc<AtomicU64>,

    /// Total parse errors
    pub parse_errors_total: Arc<AtomicU64>,

    /// Total execution errors
    pub execution_errors_total: Arc<AtomicU64>,

    /// Total HTTP requests
    pub http_requests_total: Arc<AtomicU64>,

    /// Total HTTP 2xx responses
    pub http_responses_2xx: Arc<AtomicU64>,

    /// Total HTTP 4xx responses
    pub http_responses_4xx: Arc<AtomicU64>,

    /// Total HTTP 5xx responses
    pub http_responses_5xx: Arc<AtomicU64>,

    /// Cache hits
    pub cache_hits: Arc<AtomicU64>,

    /// Cache misses
    pub cache_misses: Arc<AtomicU64>,

    // Federation Metrics
    /// Federation entity resolutions (total)
    pub federation_entity_resolutions_total: Arc<AtomicU64>,

    /// Federation entity resolutions (errors)
    pub federation_entity_resolutions_errors: Arc<AtomicU64>,

    /// Federation entity resolution duration (microseconds)
    pub federation_entity_resolution_duration_us: Arc<AtomicU64>,

    /// Federation subgraph requests (total)
    pub federation_subgraph_requests_total: Arc<AtomicU64>,

    /// Federation subgraph requests (errors)
    pub federation_subgraph_requests_errors: Arc<AtomicU64>,

    /// Federation subgraph request duration (microseconds)
    pub federation_subgraph_request_duration_us: Arc<AtomicU64>,

    /// Federation mutations (total)
    pub federation_mutations_total: Arc<AtomicU64>,

    /// Federation mutations (errors)
    pub federation_mutations_errors: Arc<AtomicU64>,

    /// Federation mutation duration (microseconds)
    pub federation_mutation_duration_us: Arc<AtomicU64>,

    /// Federation entity cache hits
    pub federation_entity_cache_hits: Arc<AtomicU64>,

    /// Federation entity cache misses
    pub federation_entity_cache_misses: Arc<AtomicU64>,

    /// Federation errors
    pub federation_errors_total: Arc<AtomicU64>,

    /// Per-operation metrics (histogram + error counter)
    pub operation_metrics: Arc<OperationMetricsRegistry>,

    /// HTTP request duration histogram
    pub http_request_duration: Arc<Histogram>,

    /// Database query duration histogram
    pub db_query_duration: Arc<Histogram>,

    /// Total successful schema reloads
    pub schema_reloads_total: Arc<AtomicU64>,

    /// Total failed schema reload attempts
    pub schema_reload_errors_total: Arc<AtomicU64>,
}

impl MetricsCollector {
    /// Create new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queries_total: Arc::new(AtomicU64::new(0)),
            queries_success: Arc::new(AtomicU64::new(0)),
            queries_error: Arc::new(AtomicU64::new(0)),
            queries_duration_us: Arc::new(AtomicU64::new(0)),
            db_queries_total: Arc::new(AtomicU64::new(0)),
            db_queries_duration_us: Arc::new(AtomicU64::new(0)),
            validation_errors_total: Arc::new(AtomicU64::new(0)),
            parse_errors_total: Arc::new(AtomicU64::new(0)),
            execution_errors_total: Arc::new(AtomicU64::new(0)),
            http_requests_total: Arc::new(AtomicU64::new(0)),
            http_responses_2xx: Arc::new(AtomicU64::new(0)),
            http_responses_4xx: Arc::new(AtomicU64::new(0)),
            http_responses_5xx: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            federation_entity_resolutions_total: Arc::new(AtomicU64::new(0)),
            federation_entity_resolutions_errors: Arc::new(AtomicU64::new(0)),
            federation_entity_resolution_duration_us: Arc::new(AtomicU64::new(0)),
            federation_subgraph_requests_total: Arc::new(AtomicU64::new(0)),
            federation_subgraph_requests_errors: Arc::new(AtomicU64::new(0)),
            federation_subgraph_request_duration_us: Arc::new(AtomicU64::new(0)),
            federation_mutations_total: Arc::new(AtomicU64::new(0)),
            federation_mutations_errors: Arc::new(AtomicU64::new(0)),
            federation_mutation_duration_us: Arc::new(AtomicU64::new(0)),
            federation_entity_cache_hits: Arc::new(AtomicU64::new(0)),
            federation_entity_cache_misses: Arc::new(AtomicU64::new(0)),
            federation_errors_total: Arc::new(AtomicU64::new(0)),
            operation_metrics: Arc::new(OperationMetricsRegistry::default()),
            http_request_duration: Arc::new(Histogram::new()),
            db_query_duration: Arc::new(Histogram::new()),
            schema_reloads_total: Arc::new(AtomicU64::new(0)),
            schema_reload_errors_total: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl MetricsCollector {
    /// Record entity resolution completion (all strategies).
    ///
    /// # Arguments
    ///
    /// * `duration_us` - Resolution duration in microseconds
    /// * `success` - Whether resolution succeeded
    pub fn record_entity_resolution(&self, duration_us: u64, success: bool) {
        self.federation_entity_resolutions_total.fetch_add(1, Ordering::Relaxed);
        self.federation_entity_resolution_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
        if !success {
            self.federation_entity_resolutions_errors.fetch_add(1, Ordering::Relaxed);
            self.federation_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record subgraph request completion.
    ///
    /// # Arguments
    ///
    /// * `duration_us` - Request duration in microseconds
    /// * `success` - Whether request succeeded (HTTP 2xx)
    pub fn record_subgraph_request(&self, duration_us: u64, success: bool) {
        self.federation_subgraph_requests_total.fetch_add(1, Ordering::Relaxed);
        self.federation_subgraph_request_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
        if !success {
            self.federation_subgraph_requests_errors.fetch_add(1, Ordering::Relaxed);
            self.federation_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record federation mutation execution.
    ///
    /// # Arguments
    ///
    /// * `duration_us` - Mutation duration in microseconds
    /// * `success` - Whether mutation succeeded
    pub fn record_mutation(&self, duration_us: u64, success: bool) {
        self.federation_mutations_total.fetch_add(1, Ordering::Relaxed);
        self.federation_mutation_duration_us.fetch_add(duration_us, Ordering::Relaxed);
        if !success {
            self.federation_mutations_errors.fetch_add(1, Ordering::Relaxed);
            self.federation_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record entity cache hit.
    pub fn record_entity_cache_hit(&self) {
        self.federation_entity_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record entity cache miss.
    pub fn record_entity_cache_miss(&self) {
        self.federation_entity_cache_misses.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Histogram bucket upper bounds in microseconds.
/// Corresponds to: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s
const HISTOGRAM_BUCKET_BOUNDS_US: [u64; 11] = [
    1_000, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000, 1_000_000, 2_500_000,
    5_000_000,
];

/// Prometheus `le` labels matching [`HISTOGRAM_BUCKET_BOUNDS_US`], in seconds.
const HISTOGRAM_LE_LABELS: [&str; 11] = [
    "0.001", "0.005", "0.01", "0.025", "0.05", "0.1", "0.25", "0.5", "1", "2.5", "5",
];

/// Per-operation metrics: count, total duration, error count, and histogram buckets.
#[derive(Debug)]
pub struct OperationMetrics {
    count:         AtomicU64,
    duration_us:   AtomicU64,
    error_count:   AtomicU64,
    bucket_counts: [AtomicU64; 11],
}

impl OperationMetrics {
    fn new() -> Self {
        Self {
            count:         AtomicU64::new(0),
            duration_us:   AtomicU64::new(0),
            error_count:   AtomicU64::new(0),
            bucket_counts: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    fn record(&self, duration_us: u64, is_error: bool) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.duration_us.fetch_add(duration_us, Ordering::Relaxed);
        if is_error {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        // Increment only the first (smallest) bucket whose bound >= duration.
        // The cumulative sum is computed at render time.
        for (i, &bound) in HISTOGRAM_BUCKET_BOUNDS_US.iter().enumerate() {
            if duration_us <= bound {
                self.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // Duration exceeds all finite buckets — it only appears in +Inf
    }
}

/// Registry of per-operation metrics with cardinality guard.
///
/// Operations beyond `max_operations` are folded into an `__overflow__` bucket
/// to prevent unbounded label cardinality.
#[derive(Debug)]
pub struct OperationMetricsRegistry {
    operations:     DashMap<String, OperationMetrics>,
    max_operations: usize,
    overflow:       OperationMetrics,
}

impl OperationMetricsRegistry {
    /// Create a new registry with the given cardinality limit.
    #[must_use]
    pub fn new(max_operations: usize) -> Self {
        Self {
            operations: DashMap::new(),
            max_operations,
            overflow: OperationMetrics::new(),
        }
    }

    /// Record a query execution for the given operation name.
    pub fn record(&self, name: &str, duration_us: u64, is_error: bool) {
        let canonical = if name.is_empty() {
            "__anonymous__"
        } else {
            name
        };

        // Check if already tracked
        if let Some(entry) = self.operations.get(canonical) {
            entry.record(duration_us, is_error);
            return;
        }

        // Check cardinality limit before inserting
        if self.operations.len() >= self.max_operations {
            self.overflow.record(duration_us, is_error);
            return;
        }

        // Insert and record (race-safe: entry() handles concurrent inserts)
        self.operations
            .entry(canonical.to_owned())
            .or_insert_with(OperationMetrics::new)
            .record(duration_us, is_error);
    }

    /// Render all per-operation metrics in Prometheus text exposition format.
    #[must_use]
    pub fn to_prometheus_format(&self) -> String {
        let mut out = String::new();

        // Collect entries for deterministic output (sorted by name)
        let mut entries: Vec<(String, u64, u64, u64, [u64; 11])> = self
            .operations
            .iter()
            .map(|e| {
                let buckets: [u64; 11] =
                    std::array::from_fn(|i| e.value().bucket_counts[i].load(Ordering::Relaxed));
                (
                    e.key().clone(),
                    e.value().count.load(Ordering::Relaxed),
                    e.value().duration_us.load(Ordering::Relaxed),
                    e.value().error_count.load(Ordering::Relaxed),
                    buckets,
                )
            })
            .collect();

        // Add overflow if it has data
        let overflow_count = self.overflow.count.load(Ordering::Relaxed);
        if overflow_count > 0 {
            let buckets: [u64; 11] =
                std::array::from_fn(|i| self.overflow.bucket_counts[i].load(Ordering::Relaxed));
            entries.push((
                "__overflow__".to_owned(),
                overflow_count,
                self.overflow.duration_us.load(Ordering::Relaxed),
                self.overflow.error_count.load(Ordering::Relaxed),
                buckets,
            ));
        }

        if entries.is_empty() {
            return out;
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Duration histogram
        out.push_str(
            "\n# HELP fraiseql_query_duration_seconds Per-operation query duration histogram\n\
             # TYPE fraiseql_query_duration_seconds histogram\n",
        );
        for (name, count, duration_us, _, buckets) in &entries {
            let mut cumulative: u64 = 0;
            for (i, &bucket_count) in buckets.iter().enumerate() {
                cumulative += bucket_count;
                let _ = writeln!(
                    out,
                    "fraiseql_query_duration_seconds_bucket{{operation=\"{name}\",le=\"{}\"}} \
                     {cumulative}",
                    HISTOGRAM_LE_LABELS[i],
                );
            }
            let _ = writeln!(
                out,
                "fraiseql_query_duration_seconds_bucket{{operation=\"{name}\",le=\"+Inf\"}} \
                 {count}",
            );
            #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for metrics reporting
            let sum_secs = *duration_us as f64 / 1_000_000.0;
            let _ = writeln!(
                out,
                "fraiseql_query_duration_seconds_sum{{operation=\"{name}\"}} {sum_secs:.6}",
            );
            let _ = writeln!(
                out,
                "fraiseql_query_duration_seconds_count{{operation=\"{name}\"}} {count}",
            );
        }

        // Error counter
        out.push_str(
            "\n# HELP fraiseql_query_errors_total Per-operation query error count\n\
             # TYPE fraiseql_query_errors_total counter\n",
        );
        for (name, _, _, error_count, _) in &entries {
            let _ =
                writeln!(out, "fraiseql_query_errors_total{{operation=\"{name}\"}} {error_count}",);
        }

        out
    }
}

impl Default for OperationMetricsRegistry {
    fn default() -> Self {
        Self::new(500)
    }
}

/// General-purpose histogram using the standard 11-bucket scheme.
#[derive(Debug)]
pub struct Histogram {
    count:         AtomicU64,
    sum_us:        AtomicU64,
    bucket_counts: [AtomicU64; 11],
}

impl Histogram {
    /// Create a new empty histogram.
    #[must_use]
    pub fn new() -> Self {
        Self {
            count:         AtomicU64::new(0),
            sum_us:        AtomicU64::new(0),
            bucket_counts: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Observe a duration in microseconds.
    pub fn observe_us(&self, duration_us: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_us.fetch_add(duration_us, Ordering::Relaxed);
        for (i, &bound) in HISTOGRAM_BUCKET_BOUNDS_US.iter().enumerate() {
            if duration_us <= bound {
                self.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    /// Render as Prometheus text format with the given metric name.
    #[allow(clippy::cast_precision_loss)] // Reason: microsecond precision loss at >2^53 us (~285 years) is acceptable
    #[must_use]
    pub fn to_prometheus_lines(&self, name: &str, help: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "\n# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} histogram");
        let count = self.count.load(Ordering::Relaxed);
        let sum_us = self.sum_us.load(Ordering::Relaxed);
        let mut cumulative = 0u64;
        for (i, le) in HISTOGRAM_LE_LABELS.iter().enumerate() {
            cumulative += self.bucket_counts[i].load(Ordering::Relaxed);
            let _ = writeln!(out, "{name}_bucket{{le=\"{le}\"}} {cumulative}");
        }
        let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} {count}");
        let sum_secs = sum_us as f64 / 1_000_000.0;
        let _ = writeln!(out, "{name}_sum {sum_secs:.6}");
        let _ = writeln!(out, "{name}_count {count}");
        out
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    /// Estimate the value at a given quantile (0.0–1.0) from bucket data.
    ///
    /// Uses linear interpolation within the bucket that contains the target
    /// quantile. Returns 0 if the histogram is empty.
    #[allow(clippy::cast_precision_loss)] // Reason: u64 counters in practice are < 2^53
    #[allow(clippy::cast_possible_truncation)] // Reason: product of u64 count × quantile (0..1) is always ≤ original count
    #[allow(clippy::cast_sign_loss)] // Reason: quantile is always in [0, 1], so product is non-negative
    #[must_use]
    pub fn estimate_quantile_us(&self, quantile: f64) -> u64 {
        let total = self.count.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }

        let target = (total as f64 * quantile) as u64;
        let mut cumulative = 0u64;

        for (i, &bound) in HISTOGRAM_BUCKET_BOUNDS_US.iter().enumerate() {
            let bucket_count = self.bucket_counts[i].load(Ordering::Relaxed);
            cumulative += bucket_count;
            if cumulative >= target {
                // Target falls in this bucket — return the bucket upper bound
                // as the estimate (conservative: overestimates latency slightly).
                return bound;
            }
        }

        // Beyond all buckets — return last bucket bound
        HISTOGRAM_BUCKET_BOUNDS_US[HISTOGRAM_BUCKET_BOUNDS_US.len() - 1]
    }
}

/// Guard for timing metrics.
pub struct TimingGuard {
    start:           Instant,
    duration_atomic: Arc<AtomicU64>,
}

impl TimingGuard {
    /// Create new timing guard.
    pub fn new(duration_atomic: Arc<AtomicU64>) -> Self {
        Self {
            start: Instant::now(),
            duration_atomic,
        }
    }

    /// Record duration in microseconds and consume guard.
    pub fn record(self) {
        #[allow(clippy::cast_possible_truncation)] // Reason: microsecond counter cannot exceed u64 in any practical uptime
        let duration_us = self.start.elapsed().as_micros() as u64;
        self.duration_atomic.fetch_add(duration_us, Ordering::Relaxed);
    }
}

/// Prometheus metrics output format.
#[derive(Debug)]
pub struct PrometheusMetrics {
    /// Total GraphQL queries executed
    pub queries_total:              u64,
    /// Successful GraphQL queries
    pub queries_success:            u64,
    /// Failed GraphQL queries
    pub queries_error:              u64,
    /// Average query duration in milliseconds
    pub queries_avg_duration_ms:    f64,
    /// Total database queries executed
    pub db_queries_total:           u64,
    /// Average database query duration in milliseconds
    pub db_queries_avg_duration_ms: f64,
    /// Total validation errors
    pub validation_errors_total:    u64,
    /// Total parse errors
    pub parse_errors_total:         u64,
    /// Total execution errors
    pub execution_errors_total:     u64,
    /// Total HTTP requests processed
    pub http_requests_total:        u64,
    /// HTTP 2xx responses
    pub http_responses_2xx:         u64,
    /// HTTP 4xx responses
    pub http_responses_4xx:         u64,
    /// HTTP 5xx responses
    pub http_responses_5xx:         u64,
    /// Cache hit count
    pub cache_hits:                 u64,
    /// Cache miss count
    pub cache_misses:               u64,
    /// Cache hit ratio (0.0 to 1.0)
    pub cache_hit_ratio:            f64,
}

impl PrometheusMetrics {
    /// Generate Prometheus text format output.
    #[must_use]
    pub fn to_prometheus_format(&self) -> String {
        format!(
            r"# HELP fraiseql_graphql_queries_total Total GraphQL queries executed
# TYPE fraiseql_graphql_queries_total counter
fraiseql_graphql_queries_total {}

# HELP fraiseql_graphql_queries_success Total successful GraphQL queries
# TYPE fraiseql_graphql_queries_success counter
fraiseql_graphql_queries_success {}

# HELP fraiseql_graphql_queries_error Total failed GraphQL queries
# TYPE fraiseql_graphql_queries_error counter
fraiseql_graphql_queries_error {}

# HELP fraiseql_graphql_query_duration_ms Average query execution time in milliseconds
# TYPE fraiseql_graphql_query_duration_ms gauge
fraiseql_graphql_query_duration_ms {}

# HELP fraiseql_database_queries_total Total database queries executed
# TYPE fraiseql_database_queries_total counter
fraiseql_database_queries_total {}

# HELP fraiseql_database_query_duration_ms Average database query time in milliseconds
# TYPE fraiseql_database_query_duration_ms gauge
fraiseql_database_query_duration_ms {}

# HELP fraiseql_validation_errors_total Total validation errors
# TYPE fraiseql_validation_errors_total counter
fraiseql_validation_errors_total {}

# HELP fraiseql_parse_errors_total Total parse errors
# TYPE fraiseql_parse_errors_total counter
fraiseql_parse_errors_total {}

# HELP fraiseql_execution_errors_total Total execution errors
# TYPE fraiseql_execution_errors_total counter
fraiseql_execution_errors_total {}

# HELP fraiseql_http_requests_total Total HTTP requests
# TYPE fraiseql_http_requests_total counter
fraiseql_http_requests_total {}

# HELP fraiseql_http_responses_2xx Total 2xx HTTP responses
# TYPE fraiseql_http_responses_2xx counter
fraiseql_http_responses_2xx {}

# HELP fraiseql_http_responses_4xx Total 4xx HTTP responses
# TYPE fraiseql_http_responses_4xx counter
fraiseql_http_responses_4xx {}

# HELP fraiseql_http_responses_5xx Total 5xx HTTP responses
# TYPE fraiseql_http_responses_5xx counter
fraiseql_http_responses_5xx {}

# HELP fraiseql_cache_hits Total cache hits
# TYPE fraiseql_cache_hits counter
fraiseql_cache_hits {}

# HELP fraiseql_cache_misses Total cache misses
# TYPE fraiseql_cache_misses counter
fraiseql_cache_misses {}

# HELP fraiseql_cache_hit_ratio Cache hit ratio (0-1)
# TYPE fraiseql_cache_hit_ratio gauge
fraiseql_cache_hit_ratio {:.3}
",
            self.queries_total,
            self.queries_success,
            self.queries_error,
            self.queries_avg_duration_ms,
            self.db_queries_total,
            self.db_queries_avg_duration_ms,
            self.validation_errors_total,
            self.parse_errors_total,
            self.execution_errors_total,
            self.http_requests_total,
            self.http_responses_2xx,
            self.http_responses_4xx,
            self.http_responses_5xx,
            self.cache_hits,
            self.cache_misses,
            self.cache_hit_ratio,
        )
    }
}

impl From<&MetricsCollector> for PrometheusMetrics {
    fn from(collector: &MetricsCollector) -> Self {
        let queries_total = collector.queries_total.load(Ordering::Relaxed);
        let queries_success = collector.queries_success.load(Ordering::Relaxed);
        let queries_error = collector.queries_error.load(Ordering::Relaxed);
        let queries_duration_us = collector.queries_duration_us.load(Ordering::Relaxed);

        let db_queries_total = collector.db_queries_total.load(Ordering::Relaxed);
        let db_queries_duration_us = collector.db_queries_duration_us.load(Ordering::Relaxed);

        let cache_hits = collector.cache_hits.load(Ordering::Relaxed);
        let cache_misses = collector.cache_misses.load(Ordering::Relaxed);
        let cache_total = cache_hits + cache_misses;

        Self {
            queries_total,
            queries_success,
            queries_error,
            #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for metrics/statistics
            queries_avg_duration_ms: if queries_total > 0 {
                (queries_duration_us as f64 / queries_total as f64) / 1000.0
            } else {
                0.0
            },
            db_queries_total,
            #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for metrics/statistics
            db_queries_avg_duration_ms: if db_queries_total > 0 {
                (db_queries_duration_us as f64 / db_queries_total as f64) / 1000.0
            } else {
                0.0
            },
            validation_errors_total: collector.validation_errors_total.load(Ordering::Relaxed),
            parse_errors_total: collector.parse_errors_total.load(Ordering::Relaxed),
            execution_errors_total: collector.execution_errors_total.load(Ordering::Relaxed),
            http_requests_total: collector.http_requests_total.load(Ordering::Relaxed),
            http_responses_2xx: collector.http_responses_2xx.load(Ordering::Relaxed),
            http_responses_4xx: collector.http_responses_4xx.load(Ordering::Relaxed),
            http_responses_5xx: collector.http_responses_5xx.load(Ordering::Relaxed),
            cache_hits,
            cache_misses,
            #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for metrics/statistics
            cache_hit_ratio: if cache_total > 0 {
                cache_hits as f64 / cache_total as f64
            } else {
                0.0
            },
        }
    }
}

