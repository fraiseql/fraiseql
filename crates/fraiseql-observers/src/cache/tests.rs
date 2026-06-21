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

// ── #428: pure Redis glob escaping for the cache-invalidation action ──
//
// These run in the standard `test` leg (no `caching` feature needed) so the
// security-critical escape-then-substitute boundary is covered on every push.
mod glob_tests {
    use serde_json::json;

    use crate::cache::glob::{escape_redis_glob, has_unescaped_glob, render_key_pattern};

    #[test]
    fn escapes_full_redis_glob_metaclass() {
        // Every operator AND the escape char itself must be backslash-escaped.
        assert_eq!(escape_redis_glob("a*b"), r"a\*b");
        assert_eq!(escape_redis_glob("a?b"), r"a\?b");
        assert_eq!(escape_redis_glob("a[b]c"), r"a\[b\]c");
        assert_eq!(escape_redis_glob(r"a\b"), r"a\\b");
        assert_eq!(escape_redis_glob("plain"), "plain");
    }

    #[test]
    fn detects_unescaped_glob_operators() {
        assert!(has_unescaped_glob("app:user:*"));
        assert!(has_unescaped_glob("app:user:?"));
        assert!(has_unescaped_glob("app:user:[12]"));
    }

    #[test]
    fn escaped_operators_are_not_treated_as_glob() {
        // An operator that came from an escaped event value is literal.
        assert!(!has_unescaped_glob(r"app:order:\*"));
        assert!(!has_unescaped_glob(r"app:order:\?"));
        assert!(!has_unescaped_glob(r"app:order:\["));
        assert!(!has_unescaped_glob("app:order:123"));
        // A lone `]` is not an operator on its own.
        assert!(!has_unescaped_glob("app:order:1]"));
    }

    #[test]
    fn render_substitutes_top_level_fields() {
        let data = json!({ "id": 123, "name": "alice" });
        assert_eq!(render_key_pattern("app:order:{{ id }}", &data, false), "app:order:123");
        assert_eq!(render_key_pattern("app:user:{{ name }}", &data, false), "app:user:alice");
    }

    #[test]
    fn escape_then_substitute_neutralizes_value_globs() {
        // A wildcard in the (untrusted) event value must be escaped so it cannot
        // widen the match — while the author's trailing `*` survives as a glob.
        let data = json!({ "id": "a*b" });
        let escaped = render_key_pattern("app:order:{{ id }}:*", &data, true);
        assert_eq!(escaped, r"app:order:a\*b:*");
        // Only the author's trailing `*` is an operator; the value's `*` is escaped.
        assert!(has_unescaped_glob(&escaped));
    }

    #[test]
    fn value_only_glob_takes_the_direct_path() {
        // key_pattern with no template glob + a malicious value => no surviving
        // glob after escaping => direct UNLINK of the literal key, no broad wipe.
        let data = json!({ "id": "*" });
        let escaped = render_key_pattern("app:order:{{ id }}", &data, true);
        assert_eq!(escaped, r"app:order:\*");
        assert!(!has_unescaped_glob(&escaped));
        // The literal key actually targeted (raw render) is the real key name.
        assert_eq!(render_key_pattern("app:order:{{ id }}", &data, false), "app:order:*");
    }

    #[test]
    fn template_glob_with_no_value_glob_takes_the_scan_path() {
        let data = json!({ "id": 7 });
        let escaped = render_key_pattern("app:user:{{ id }}:*", &data, true);
        assert_eq!(escaped, "app:user:7:*");
        assert!(has_unescaped_glob(&escaped));
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
