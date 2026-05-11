#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod dedup_tests {
    use crate::dedup::*;

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_new() {
        let stats = DeduplicationStats::new();
        assert_eq!(stats.total_checked, 0);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_record_new_event() {
        let mut stats = DeduplicationStats::new();
        stats.record(false);

        assert_eq!(stats.total_checked, 1);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 1);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_dedup_stats_record_duplicate() {
        let mut stats = DeduplicationStats::new();
        stats.record(false);
        stats.record(true);

        assert_eq!(stats.total_checked, 2);
        assert_eq!(stats.duplicates_skipped, 1);
        assert_eq!(stats.new_events, 1);
        assert!((stats.hit_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dedup_stats_hit_rate() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..8 {
            stats.record(true); // duplicates
        }
        for _ in 0..2 {
            stats.record(false); // new events
        }

        assert_eq!(stats.total_checked, 10);
        assert_eq!(stats.duplicates_skipped, 8);
        assert_eq!(stats.new_events, 2);
        assert!((stats.hit_rate - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_reset() {
        let mut stats = DeduplicationStats::new();
        stats.record(true);
        stats.record(false);

        stats.reset();

        assert_eq!(stats.total_checked, 0);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    // =========================================================================
    // Additional tests for dedup/mod.rs coverage
    // =========================================================================

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_default_equals_new() {
        let stats_default = DeduplicationStats::default();
        let stats_new = DeduplicationStats::new();
        assert_eq!(stats_default.total_checked, stats_new.total_checked);
        assert_eq!(stats_default.duplicates_skipped, stats_new.duplicates_skipped);
        assert_eq!(stats_default.new_events, stats_new.new_events);
        assert_eq!(stats_default.hit_rate, stats_new.hit_rate);
    }

    #[test]
    fn test_dedup_stats_only_duplicates_hit_rate_one() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..5 {
            stats.record(true);
        }
        assert!((stats.hit_rate - 1.0).abs() < f64::EPSILON, "All duplicates → hit_rate = 1.0");
        assert_eq!(stats.new_events, 0);
        assert_eq!(stats.duplicates_skipped, 5);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_only_new_events_hit_rate_zero() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..5 {
            stats.record(false);
        }
        assert_eq!(stats.hit_rate, 0.0, "All new events → hit_rate = 0.0");
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.new_events, 5);
    }

    #[test]
    fn test_dedup_stats_increments_total_checked_on_each_record() {
        let mut stats = DeduplicationStats::new();
        stats.record(false);
        assert_eq!(stats.total_checked, 1);
        stats.record(true);
        assert_eq!(stats.total_checked, 2);
        stats.record(false);
        assert_eq!(stats.total_checked, 3);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_reset_clears_hit_rate() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..10 {
            stats.record(true);
        }
        assert!((stats.hit_rate - 1.0).abs() < f64::EPSILON);
        stats.reset();
        assert_eq!(stats.hit_rate, 0.0, "After reset hit_rate should be 0.0");
    }

    #[test]
    fn test_dedup_stats_single_new_single_dup_equals_50_percent() {
        let mut stats = DeduplicationStats::new();
        stats.record(false); // new
        stats.record(true); // duplicate
        assert!((stats.hit_rate - 0.5).abs() < f64::EPSILON, "50% hit rate");
        assert_eq!(stats.total_checked, 2);
    }

    #[test]
    fn test_dedup_stats_many_records_large_scale() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..1000 {
            stats.record(false); // new events
        }
        for _ in 0..1000 {
            stats.record(true); // duplicates
        }
        assert_eq!(stats.total_checked, 2000);
        assert_eq!(stats.new_events, 1000);
        assert_eq!(stats.duplicates_skipped, 1000);
        assert!((stats.hit_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_dedup_stats_reset_then_reuse() {
        let mut stats = DeduplicationStats::new();
        stats.record(true);
        stats.record(true);
        stats.reset();

        // After reset, stats should behave as freshly created
        stats.record(false);
        assert_eq!(stats.total_checked, 1);
        assert_eq!(stats.new_events, 1);
        assert_eq!(stats.duplicates_skipped, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_dedup_stats_precision_three_of_ten_duplicates() {
        let mut stats = DeduplicationStats::new();
        for _ in 0..7 {
            stats.record(false);
        }
        for _ in 0..3 {
            stats.record(true);
        }
        assert_eq!(stats.total_checked, 10);
        assert_eq!(stats.duplicates_skipped, 3);
        let expected = 0.3_f64;
        assert!(
            (stats.hit_rate - expected).abs() < 1e-10,
            "Expected hit_rate ≈ 0.3, got {}",
            stats.hit_rate
        );
    }

    #[test]
    fn test_dedup_stats_clone() {
        let mut stats = DeduplicationStats::new();
        stats.record(true);
        stats.record(false);
        let cloned = stats.clone();
        assert_eq!(cloned.total_checked, stats.total_checked);
        assert_eq!(cloned.duplicates_skipped, stats.duplicates_skipped);
        assert_eq!(cloned.new_events, stats.new_events);
        assert!((cloned.hit_rate - stats.hit_rate).abs() < f64::EPSILON);
    }
}

#[cfg(feature = "dedup")]
mod redis_tests {
    use crate::dedup::redis::*;

    #[test]
    fn test_dedup_key_generation() {
        let key = RedisDeduplicationStore::dedup_key("order:123:created");
        assert_eq!(key, "dedup:v1:order:123:created");
    }

    #[test]
    fn test_redis_dedup_store_clone() {
        // Ensure RedisDeduplicationStore is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisDeduplicationStore>();
        // Note: This test only verifies the struct is Clone-able
        // Actual Redis tests require a Redis server
    }
}
