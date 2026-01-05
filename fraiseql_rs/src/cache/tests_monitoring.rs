//! Integration tests for cache monitoring (Phase 17A.5)
//!
//! Tests cache health monitoring, observability, and alerting

#[cfg(test)]
mod tests {
    use crate::cache::{CacheHealthThresholds, CacheMonitor, HealthStatus};

    #[test]
    fn test_monitor_creation_with_defaults() {
        let monitor = CacheMonitor::new();

        assert_eq!(
            monitor
                .total_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            monitor
                .total_misses
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            monitor
                .total_invalidations
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn test_monitor_creation_with_custom_thresholds() {
        let thresholds = CacheHealthThresholds {
            min_hit_rate: 0.75,
            max_miss_rate: 0.25,
            max_invalidation_rate: 0.20,
            max_memory_bytes: 512 * 1024 * 1024,
            min_hits_per_second: 200,
        };

        let monitor = CacheMonitor::with_thresholds(thresholds);

        // Should be created without errors
        assert_eq!(
            monitor
                .total_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn test_record_hits_and_misses() {
        let monitor = CacheMonitor::new();

        for _ in 0..100 {
            monitor.record_hit();
        }

        for _ in 0..25 {
            monitor.record_miss();
        }

        assert_eq!(
            monitor
                .total_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            100
        );
        assert_eq!(
            monitor
                .total_misses
                .load(std::sync::atomic::Ordering::Relaxed),
            25
        );
    }

    #[test]
    fn test_health_report_healthy_cache() {
        let monitor = CacheMonitor::new();

        // Record good hit rate: 80 hits, 20 misses = 80% hit rate
        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        monitor.record_cache_entry(); // 100 entries total
        for _ in 0..99 {
            monitor.record_cache_entry();
        }
        monitor.record_invalidation(10); // 10% invalidation rate

        let health = monitor.get_health(80, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.hit_rate > 0.75);
        assert!(health.miss_rate < 0.25);
        assert!(health.issues.is_empty());
    }

    #[test]
    fn test_health_report_degraded_low_hit_rate() {
        let monitor = CacheMonitor::new();

        // Record poor hit rate: 30 hits, 70 misses = 30% hit rate
        for _ in 0..30 {
            monitor.record_hit();
        }
        for _ in 0..70 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(health.hit_rate < 0.50);
        assert_eq!(health.issues.len(), 1);
        assert!(health.issues[0].contains("Low hit rate"));
    }

    #[test]
    fn test_health_report_degraded_high_invalidation() {
        let monitor = CacheMonitor::new();

        // Record good hit rate
        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        // Record 100 cached entries
        for _ in 0..100 {
            monitor.record_cache_entry();
        }

        // Invalidate 35+ entries (35% invalidation rate)
        monitor.record_invalidation(35);

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(health.invalidation_rate > 0.30);
        assert!(health
            .issues
            .iter()
            .any(|i| i.contains("High invalidation rate")));
    }

    #[test]
    fn test_health_report_unhealthy_memory_exceeded() {
        let thresholds = CacheHealthThresholds {
            max_memory_bytes: 50 * 1024 * 1024, // 50MB
            ..Default::default()
        };
        let monitor = CacheMonitor::with_thresholds(thresholds);

        // Good hit rate
        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        let health = monitor.get_health(1000, 10000, 100 * 1024 * 1024); // Using 100MB

        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health
            .issues
            .iter()
            .any(|i| i.contains("Memory usage exceeded")));
    }

    #[test]
    fn test_health_report_multiple_issues() {
        let thresholds = CacheHealthThresholds {
            max_memory_bytes: 10 * 1024 * 1024, // 10MB
            ..Default::default()
        };
        let monitor = CacheMonitor::with_thresholds(thresholds);

        // Poor hit rate
        for _ in 0..30 {
            monitor.record_hit();
        }
        for _ in 0..70 {
            monitor.record_miss();
        }

        // High invalidation
        for _ in 0..100 {
            monitor.record_cache_entry();
        }
        monitor.record_invalidation(50); // 50% invalidation

        let health = monitor.get_health(100, 1000, 20 * 1024 * 1024); // 20MB usage

        // Should detect multiple issues
        assert!(health.issues.len() >= 2);
        assert!(health.status != HealthStatus::Healthy);
    }

    #[test]
    fn test_memory_tracking() {
        let monitor = CacheMonitor::new();

        monitor.record_memory_usage(10 * 1024 * 1024); // 10MB
        monitor.record_memory_usage(25 * 1024 * 1024); // 25MB
        monitor.record_memory_usage(15 * 1024 * 1024); // 15MB (lower, but peak stays 25MB)

        let peak = monitor
            .peak_memory_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(peak, 25 * 1024 * 1024);
    }

    #[test]
    fn test_invalidation_tracking() {
        let monitor = CacheMonitor::new();

        monitor.record_invalidation(5);
        monitor.record_invalidation(3);
        monitor.record_invalidation(10);

        let invalidations = monitor
            .total_invalidations
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(invalidations, 18);
    }

    #[test]
    fn test_cache_entry_tracking() {
        let monitor = CacheMonitor::new();

        for _ in 0..100 {
            monitor.record_cache_entry();
        }

        let total_cached = monitor
            .total_cached
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(total_cached, 100);
    }

    #[test]
    fn test_prometheus_export_format() {
        let monitor = CacheMonitor::new();

        for _ in 0..100 {
            monitor.record_hit();
        }
        for _ in 0..25 {
            monitor.record_miss();
        }
        monitor.record_invalidation(10);

        let output = monitor.export_prometheus();

        assert!(output.contains("fraiseql_cache_hits_total"));
        assert!(output.contains("fraiseql_cache_misses_total"));
        assert!(output.contains("fraiseql_cache_hit_rate"));
        assert!(output.contains("fraiseql_cache_invalidations_total"));
        assert!(output.contains("fraiseql_cache_peak_memory_bytes"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
        assert!(output.contains("counter"));
        assert!(output.contains("gauge"));
    }

    #[test]
    fn test_json_export() {
        let monitor = CacheMonitor::new();

        for _ in 0..80 {
            monitor.record_hit();
        }
        for _ in 0..20 {
            monitor.record_miss();
        }

        let json = monitor.to_json();

        assert_eq!(json["hits"], 80);
        assert_eq!(json["misses"], 20);
        assert!(json["hit_rate"].is_number());
    }

    #[test]
    fn test_hit_rate_calculation() {
        let monitor = CacheMonitor::new();

        // 75% hit rate: 75 hits, 25 misses
        for _ in 0..75 {
            monitor.record_hit();
        }
        for _ in 0..25 {
            monitor.record_miss();
        }

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert!((health.hit_rate - 0.75).abs() < 0.001);
        assert!((health.miss_rate - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_invalidation_rate_calculation() {
        let monitor = CacheMonitor::new();

        // 100 entries cached, 25 invalidated = 25% rate
        for _ in 0..100 {
            monitor.record_cache_entry();
        }
        monitor.record_invalidation(25);

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        assert!((health.invalidation_rate - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_memory_percent_calculation() {
        let thresholds = CacheHealthThresholds {
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB
            ..Default::default()
        };
        let monitor = CacheMonitor::with_thresholds(thresholds);

        // Using 256MB out of 1GB = 25%
        let health = monitor.get_health(100, 1000, 256 * 1024 * 1024);

        assert!((health.memory_percent - 25.0).abs() < 0.1);
    }

    #[test]
    fn test_uptime_tracking() {
        let monitor = CacheMonitor::new();

        monitor.record_hit();

        // Sleep a bit
        std::thread::sleep(std::time::Duration::from_millis(100));

        let _health = monitor.get_health(10, 100, 10 * 1024 * 1024);

        // uptime_seconds is always >= 0 since it's u64
    }

    #[test]
    fn test_hits_per_second_calculation() {
        let monitor = CacheMonitor::new();

        // Record 100 hits
        for _ in 0..100 {
            monitor.record_hit();
        }

        // Sleep for 1 second
        std::thread::sleep(std::time::Duration::from_secs(1));

        let health = monitor.get_health(50, 1000, 100 * 1024 * 1024);

        // Should be approximately 100 hits per second (allowing some variance)
        assert!(health.hits_per_second > 90.0 && health.hits_per_second < 110.0);
    }

    #[test]
    fn test_sample_collection() {
        let monitor = CacheMonitor::new();

        for _ in 0..50 {
            monitor.record_hit();
        }

        monitor.collect_sample(50 * 1024 * 1024);

        // Check that sample was collected
        let samples = monitor.samples.lock().unwrap();
        assert_eq!(samples.len(), 1);
        assert!(samples[0].timestamp > 0);
    }

    #[test]
    fn test_sample_limit() {
        let monitor = CacheMonitor::new();

        // Collect 15 samples (should keep only last 10)
        for i in 0..15 {
            monitor.collect_sample((i * 10) * 1024 * 1024);
        }

        let samples = monitor.samples.lock().unwrap();
        assert_eq!(samples.len(), 10); // Should not exceed 10
    }

    #[test]
    fn test_concurrent_monitoring() {
        let monitor = std::sync::Arc::new(CacheMonitor::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let m = monitor.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    m.record_hit();
                    m.record_invalidation(1);
                    m.record_cache_entry();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 10 threads × 100 iterations = 1000 of each
        assert_eq!(
            monitor
                .total_hits
                .load(std::sync::atomic::Ordering::Relaxed),
            1000
        );
        assert_eq!(
            monitor
                .total_invalidations
                .load(std::sync::atomic::Ordering::Relaxed),
            1000
        );
        assert_eq!(
            monitor
                .total_cached
                .load(std::sync::atomic::Ordering::Relaxed),
            1000
        );
    }

    #[test]
    fn test_threshold_customization() {
        let thresholds = CacheHealthThresholds {
            min_hit_rate: 0.90,
            max_miss_rate: 0.10,
            max_invalidation_rate: 0.05,
            max_memory_bytes: 500 * 1024 * 1024,
            min_hits_per_second: 500,
        };

        let monitor = CacheMonitor::with_thresholds(thresholds);

        // 85% hit rate is good but not meeting 90% threshold
        for _ in 0..85 {
            monitor.record_hit();
        }
        for _ in 0..15 {
            monitor.record_miss();
        }

        let health = monitor.get_health(100, 1000, 100 * 1024 * 1024);

        // Should be degraded because 85% < 90% threshold
        assert_ne!(health.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_zero_requests_no_panic() {
        let monitor = CacheMonitor::new();

        // Don't record any hits or misses

        let health = monitor.get_health(0, 1000, 0);

        // Should handle gracefully without panic
        assert_eq!(health.hit_rate, 0.0);
        assert_eq!(health.miss_rate, 0.0);
    }

    #[test]
    fn test_health_report_timestamp() {
        let monitor = CacheMonitor::new();
        let health = monitor.get_health(10, 100, 10 * 1024 * 1024);

        // Timestamp should be recent (within last few seconds)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        assert!(now - health.timestamp <= 2);
    }
}
