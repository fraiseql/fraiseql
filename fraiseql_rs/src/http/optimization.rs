//! Performance optimization and tuning for HTTP layer
//!
//! Provides additional monitoring, health checks, and performance tuning
//! capabilities to optimize the HTTP server for production workloads.

/// Rate limiting configuration for fine-tuned control
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests allowed per second (base limit)
    pub requests_per_second: u32,

    /// Burst allowance (token bucket algorithm)
    pub burst_size: u32,

    /// Time window for rate limiting in milliseconds
    pub window_size_ms: u32,

    /// How often to clean up old entries in milliseconds
    pub cleanup_interval_ms: u32,
}

impl RateLimitConfig {
    /// Create default rate limiting configuration
    ///
    /// Defaults: 1000 req/s, burst of 100, 1 second window
    #[must_use]
    pub const fn default() -> Self {
        Self {
            requests_per_second: 1000,
            burst_size: 100,
            window_size_ms: 1000,
            cleanup_interval_ms: 60000, // Cleanup every 60 seconds
        }
    }

    /// Create permissive rate limiting (for testing)
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            requests_per_second: 10000,
            burst_size: 1000,
            window_size_ms: 1000,
            cleanup_interval_ms: 60000,
        }
    }

    /// Create strict rate limiting (for protection)
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 20,
            window_size_ms: 1000,
            cleanup_interval_ms: 30000,
        }
    }
}

/// HTTP server optimization settings
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,

    /// Enable compression (Brotli/Zstd)
    pub enable_compression: bool,

    /// Minimum response size for compression (bytes)
    pub compression_threshold_bytes: usize,

    /// Connection timeout in seconds
    pub connection_timeout_secs: u32,

    /// Idle connection timeout in seconds
    pub idle_timeout_secs: u32,

    /// Keep-alive interval in seconds
    pub keepalive_interval_secs: u32,

    /// Request buffer size in bytes
    pub request_buffer_size: usize,

    /// Response buffer size in bytes
    pub response_buffer_size: usize,

    /// Maximum header size in bytes
    pub max_header_size: usize,
}

impl OptimizationConfig {
    /// Create default optimization configuration (balanced)
    #[must_use]
    pub const fn default() -> Self {
        Self {
            rate_limit: RateLimitConfig::default(),
            enable_compression: true,
            compression_threshold_bytes: 1024,
            connection_timeout_secs: 30,
            idle_timeout_secs: 60,
            keepalive_interval_secs: 30,
            request_buffer_size: 8192,   // 8KB
            response_buffer_size: 16384, // 16KB
            max_header_size: 16384,      // 16KB
        }
    }

    /// Create performance-optimized configuration
    #[must_use]
    pub const fn high_performance() -> Self {
        Self {
            rate_limit: RateLimitConfig::permissive(),
            enable_compression: true,
            compression_threshold_bytes: 2048,
            connection_timeout_secs: 60,
            idle_timeout_secs: 120,
            keepalive_interval_secs: 60,
            request_buffer_size: 16384,  // 16KB
            response_buffer_size: 32768, // 32KB
            max_header_size: 32768,      // 32KB
        }
    }

    /// Create security-focused configuration
    #[must_use]
    pub const fn high_security() -> Self {
        Self {
            rate_limit: RateLimitConfig::strict(),
            enable_compression: false, // Disable to prevent timing attacks
            compression_threshold_bytes: 0,
            connection_timeout_secs: 15,
            idle_timeout_secs: 30,
            keepalive_interval_secs: 15,
            request_buffer_size: 4096,  // 4KB
            response_buffer_size: 8192, // 8KB
            max_header_size: 8192,      // 8KB
        }
    }
}

/// Health check status information
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Overall health status (healthy, degraded, unhealthy)
    pub status: String,

    /// Server uptime in seconds
    pub uptime_secs: u64,

    /// Currently active connections
    pub active_connections: u64,

    /// Total requests processed
    pub total_requests: u64,

    /// Error rate (0.0 - 1.0)
    pub error_rate: f64,

    /// Memory usage estimate (bytes)
    pub memory_bytes: u64,

    /// Average response time (milliseconds)
    pub avg_response_time_ms: f64,
}

impl HealthStatus {
    /// Determine health status from metrics
    #[must_use]
    pub fn from_metrics(
        uptime_secs: u64,
        active_connections: u64,
        total_requests: u64,
        successful_requests: u64,
        total_duration_ms: u64,
        memory_bytes: u64,
    ) -> Self {
        let error_rate = if total_requests > 0 {
            1.0 - (successful_requests as f64 / total_requests as f64)
        } else {
            0.0
        };

        let avg_response_time_ms = if total_requests > 0 {
            total_duration_ms as f64 / total_requests as f64
        } else {
            0.0
        };

        let status = if error_rate > 0.1 {
            "unhealthy".to_string()
        } else if error_rate > 0.05 || avg_response_time_ms > 20.0 {
            "degraded".to_string()
        } else {
            "healthy".to_string()
        };

        Self {
            status,
            uptime_secs,
            active_connections,
            total_requests,
            error_rate,
            memory_bytes,
            avg_response_time_ms,
        }
    }
}

/// Performance statistics for monitoring
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// P50 latency (milliseconds)
    pub p50_latency_ms: f64,

    /// P95 latency (milliseconds)
    pub p95_latency_ms: f64,

    /// P99 latency (milliseconds)
    pub p99_latency_ms: f64,

    /// Maximum latency observed (milliseconds)
    pub max_latency_ms: u64,

    /// Requests per second
    pub requests_per_sec: f64,

    /// Throughput (bytes per second)
    pub throughput_bytes_per_sec: u64,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0,
            requests_per_sec: 0.0,
            throughput_bytes_per_sec: 0,
        }
    }
}

/// Rate limit information for response headers
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Total limit
    pub limit: u32,

    /// Remaining requests
    pub remaining: u32,

    /// Unix timestamp when limit resets
    pub reset_at: u64,
}

impl RateLimitInfo {
    /// Create new rate limit info
    #[must_use]
    pub const fn new(limit: u32, remaining: u32, reset_at: u64) -> Self {
        Self {
            limit,
            remaining,
            reset_at,
        }
    }

    /// Get HTTP headers for rate limit info
    #[must_use]
    pub fn to_headers(&self) -> [(String, String); 3] {
        [
            ("X-RateLimit-Limit".to_string(), self.limit.to_string()),
            (
                "X-RateLimit-Remaining".to_string(),
                self.remaining.to_string(),
            ),
            ("X-RateLimit-Reset".to_string(), self.reset_at.to_string()),
        ]
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    /// Total connections in pool
    pub total: u64,

    /// Active connections
    pub active: u64,

    /// Idle connections
    pub idle: u64,

    /// Waiting for connection
    pub waiting: u64,
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total cache hits
    pub hits: u64,

    /// Total cache misses
    pub misses: u64,

    /// Cache hit ratio (0.0 - 1.0)
    pub hit_ratio: f64,

    /// Cache size in bytes
    pub size_bytes: u64,
}

impl CacheStats {
    /// Calculate hit ratio
    #[must_use]
    pub fn calculate_hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_second, 1000);
        assert_eq!(config.burst_size, 100);
    }

    #[test]
    fn test_rate_limit_config_permissive() {
        let config = RateLimitConfig::permissive();
        assert_eq!(config.requests_per_second, 10000);
    }

    #[test]
    fn test_rate_limit_config_strict() {
        let config = RateLimitConfig::strict();
        assert_eq!(config.requests_per_second, 100);
    }

    #[test]
    fn test_optimization_config_default() {
        let config = OptimizationConfig::default();
        assert!(config.enable_compression);
        assert_eq!(config.connection_timeout_secs, 30);
    }

    #[test]
    fn test_optimization_config_high_performance() {
        let config = OptimizationConfig::high_performance();
        assert!(config.enable_compression);
        assert!(config.request_buffer_size > OptimizationConfig::default().request_buffer_size);
    }

    #[test]
    fn test_optimization_config_high_security() {
        let config = OptimizationConfig::high_security();
        assert!(!config.enable_compression);
        assert_eq!(config.rate_limit.requests_per_second, 100);
    }

    #[test]
    fn test_health_status_healthy() {
        let status = HealthStatus::from_metrics(
            3600,       // uptime
            10,         // active connections
            1000,       // total requests
            990,        // successful (99% success)
            5000,       // total duration (5ms avg)
            45_000_000, // memory (45MB)
        );

        assert_eq!(status.status, "healthy");
        assert!(status.error_rate < 0.01);
    }

    #[test]
    fn test_health_status_degraded() {
        let status = HealthStatus::from_metrics(
            3600, 100, 1000, 900,   // 90% success (degraded)
            15000, // 15ms avg (slow)
            60_000_000,
        );

        assert_eq!(status.status, "degraded");
    }

    #[test]
    fn test_health_status_unhealthy() {
        let status = HealthStatus::from_metrics(
            3600,
            500,
            1000,
            750, // 75% success (bad)
            25000,
            100_000_000,
        );

        assert_eq!(status.status, "unhealthy");
        assert!(status.error_rate > 0.1);
    }

    #[test]
    fn test_rate_limit_info_headers() {
        let info = RateLimitInfo::new(1000, 999, 1234567890);
        let headers = info.to_headers();

        assert_eq!(headers[0].0, "X-RateLimit-Limit");
        assert_eq!(headers[0].1, "1000");
        assert_eq!(headers[1].0, "X-RateLimit-Remaining");
        assert_eq!(headers[1].1, "999");
        assert_eq!(headers[2].0, "X-RateLimit-Reset");
        assert_eq!(headers[2].1, "1234567890");
    }

    #[test]
    fn test_cache_stats_hit_ratio() {
        let stats = CacheStats {
            hits: 800,
            misses: 200,
            hit_ratio: 0.0,
            size_bytes: 1_000_000,
        };

        assert!((stats.calculate_hit_ratio() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_cache_stats_empty() {
        let stats = CacheStats {
            hits: 0,
            misses: 0,
            hit_ratio: 0.0,
            size_bytes: 0,
        };

        assert_eq!(stats.calculate_hit_ratio(), 0.0);
    }
}
