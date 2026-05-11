#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod cache_tests {
    use crate::cache::*;

    #[test]
    fn test_cached_action_result_new() {
        let result =
            CachedActionResult::new("email".to_string(), true, "Email sent".to_string(), 125.5);

        assert_eq!(result.action_type, "email");
        assert!(result.success);
        assert!((result.duration_ms - 125.5).abs() < f64::EPSILON);
        assert!(result.cached_at_unix > 0);
    }

    #[test]
    fn test_cached_action_result_is_fresh() {
        let fresh =
            CachedActionResult::new("cache".to_string(), true, "From cache".to_string(), 5.0);
        assert!(fresh.is_fresh());

        let not_fresh =
            CachedActionResult::new("api".to_string(), true, "From API".to_string(), 100.0);
        assert!(!not_fresh.is_fresh());
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_cache_stats_new() {
        let stats = CacheStats::new();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_cache_stats_record_hit() {
        let mut stats = CacheStats::new();
        stats.record(true, 2.0);

        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 1.0);
        assert!((stats.avg_hit_latency_ms - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_cache_stats_record_miss() {
        let mut stats = CacheStats::new();
        stats.record(false, 150.0);

        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.hit_rate, 0.0);
        assert!((stats.avg_miss_latency_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_stats_hit_rate_calculation() {
        let mut stats = CacheStats::new();
        // 8 hits at ~2ms each
        for _ in 0..8 {
            stats.record(true, 2.0);
        }
        // 2 misses at ~150ms each
        for _ in 0..2 {
            stats.record(false, 150.0);
        }

        assert_eq!(stats.total_requests, 10);
        assert_eq!(stats.cache_hits, 8);
        assert_eq!(stats.cache_misses, 2);
        assert!((stats.hit_rate - 0.8).abs() < f64::EPSILON);
        assert!((stats.avg_hit_latency_ms - 2.0).abs() < f64::EPSILON);
        assert!((stats.avg_miss_latency_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_cache_stats_reset() {
        let mut stats = CacheStats::new();
        stats.record(true, 2.0);
        stats.record(false, 150.0);

        stats.reset();

        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
        assert_eq!(stats.avg_hit_latency_ms, 0.0);
        assert_eq!(stats.avg_miss_latency_ms, 0.0);
    }
}

#[cfg(feature = "caching")]
mod redis_tests {
    use crate::cache::redis::*;

    #[test]
    fn test_cache_key_generation() {
        let key = RedisCacheBackend::cache_key("email_action:order:123");
        assert_eq!(key, "cache:v1:email_action:order:123");
    }

    #[test]
    fn test_redis_cache_backend_clone() {
        // Ensure RedisCacheBackend is Clone-able
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisCacheBackend>();
        // Note: This test verifies the struct is Clone
        // Actual Redis tests require a Redis server
    }
}
