//! Tests for `routes/studio/` modules.
#![allow(unused_imports)] // Reason: shared test-prelude import used by only some cfg combinations

mod admin_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::admin::*;

    #[test]
    fn test_admin_health_response_serializes() {
        let resp = AdminHealthResponse {
            uptime_secs:    10,
            version:        "test".to_string(),
            pool_active:    Some(1),
            pool_idle:      Some(4),
            pool_max:       Some(5),
            cache_hit_rate: Some(0.8),
            cache_entries:  Some(100),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("uptime_secs"));
        assert!(json.contains("cache_hit_rate"));
    }

    #[test]
    fn test_admin_schema_response_serializes() {
        let resp = AdminSchemaResponse {
            schema: serde_json::json!({"types": []}),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"schema\""));
    }
}

mod metrics_summary_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    #![allow(clippy::float_cmp)] // Reason: testing exact 0.0 from zero-division guards
    use std::sync::atomic::Ordering;

    use super::super::metrics_summary::*;
    use crate::metrics_server::MetricsCollector;

    #[test]
    fn test_metrics_summary_serializes() {
        let m = MetricsSummary::zero();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"latency\""));
        assert!(json.contains("\"p50_ms\""));
        assert!(json.contains("\"errors\""));
        assert!(json.contains("\"pool\""));
        assert!(json.contains("\"cache\""));
        assert!(json.contains("\"subscriptions\""));
        // Unmeasured figures serialize as null, never as a plausible-looking zero.
        assert!(json.contains("\"pool\":null"), "unmeasured pool must be null: {json}");
        assert!(
            json.contains("\"subscriptions\":null"),
            "unmeasured subscription count must be null: {json}"
        );
    }

    #[test]
    fn test_build_summary_latency_from_histogram() {
        let collector = MetricsCollector::new();
        // Record requests in the 5ms bucket (5000 us)
        for _ in 0..100 {
            collector.http_request_duration.observe_us(4_000);
        }
        // Record a few slow requests in the 100ms bucket
        for _ in 0..5 {
            collector.http_request_duration.observe_us(80_000);
        }

        let summary = build_summary(&collector, None);
        // P50 should be in the 5ms bucket (5000 us → 5 ms)
        assert_eq!(summary.latency.p50_ms, 5);
        // P99 should be in the 100ms bucket
        assert_eq!(summary.latency.p99_ms, 100);
    }

    #[test]
    fn test_build_summary_empty_histogram_returns_zero() {
        let collector = MetricsCollector::new();
        let summary = build_summary(&collector, None);
        assert_eq!(summary.latency.p50_ms, 0);
        assert_eq!(summary.latency.p95_ms, 0);
        assert_eq!(summary.latency.p99_ms, 0);
    }

    #[test]
    fn test_build_summary_error_rate() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(100, Ordering::Relaxed);
        collector.queries_error.store(10, Ordering::Relaxed);

        let summary = build_summary(&collector, None);
        assert!((summary.errors.lifetime - 0.1).abs() < f64::EPSILON);
        // The windowed rates are absent, not a copy of the lifetime rate. This test
        // used to assert `rate_5m == 0.1` — naming a five-minute window over a value
        // that was the all-time average.
        assert_eq!(summary.errors.rate_5m, None);
        assert_eq!(summary.errors.rate_1h, None);
        assert_eq!(summary.errors.rate_24h, None);
    }

    #[test]
    fn test_build_summary_cache_hit_rate() {
        let collector = MetricsCollector::new();
        let cache = fraiseql_core::db::ResultCacheStats {
            entries: 4,
            hits: 75,
            misses: 25,
            ..Default::default()
        };

        let summary = build_summary(&collector, Some(cache));
        assert!((summary.cache.hit_rate - 0.75).abs() < f64::EPSILON);
        assert_eq!(summary.cache.entries, Some(4));
    }

    #[test]
    fn test_build_summary_zero_division_safe() {
        let collector = MetricsCollector::new();
        let summary = build_summary(&collector, None);
        assert_eq!(summary.errors.lifetime, 0.0);
        assert_eq!(summary.cache.hit_rate, 0.0);
    }
}

mod mod_tests {
    use super::super::{mime_for_filename, studio_shell_html};

    #[test]
    fn test_shell_contains_l_tabs() {
        let html = studio_shell_html();
        assert!(html.contains("<l-tabs"), "shell must contain <l-tabs>");
    }

    #[test]
    fn test_shell_contains_all_sections() {
        let html = studio_shell_html();
        for s in [
            "Schema",
            "Data",
            "Auth",
            "Storage",
            "Functions",
            "Observers",
            "Logs",
            "Metrics",
        ] {
            assert!(html.contains(s), "shell must contain section '{s}'");
        }
    }

    /// #373 — every shell tab has a renderer in the SPA source, and each tab with
    /// an admin API is wired to its real endpoint. `include_str!` makes a
    /// moved/renamed SPA source a build error, not a silent skip, and an
    /// endpoint rename (server-side) must be made here too or this pins it.
    #[test]
    fn spa_wires_every_tab_to_its_admin_endpoint() {
        const APP_TS: &str = include_str!("../../../studio/src/app.ts");
        for (section, endpoint) in [
            ("renderSchema", "/admin/v1/schema"),
            ("renderObservers", "/api/observers/dlq"),
            ("renderObservers", "/api/observers/delivery/health"),
            ("renderLogs", "/api/observers/logs"),
            ("renderData", "/admin/v1/schema"),
            ("renderMetrics", "/admin/v1/metrics/summary"),
        ] {
            assert!(APP_TS.contains(section), "SPA must define {section}");
            assert!(APP_TS.contains(endpoint), "SPA must call {endpoint}");
        }
        // The tabs with no admin API say so, and call none of the removed routes.
        for section in ["renderAuth", "renderStorage", "renderFunctions"] {
            assert!(
                APP_TS.contains(&format!("const {section} = notServedYet(")),
                "SPA must render {section} as not served yet"
            );
        }
        for removed in [
            "/admin/v1/users'",
            "/admin/v1/storage/",
            "/admin/v1/functions",
            "/admin/v1/data/",
        ] {
            assert!(!APP_TS.contains(removed), "SPA must not call the removed route {removed}");
        }
        // The DLQ retry ACTIONS are wired (the backend effect is pinned by
        // observers::dlq_handlers::tests::concurrent_retry_dispatches_at_most_once).
        assert!(APP_TS.contains("/retry"), "SPA must wire the DLQ retry action");
        assert!(APP_TS.contains("dlq/retry-all"), "SPA must wire retry-all");
        // Every shell tab value has a SECTIONS renderer entry.
        for tab in [
            "schema",
            "data",
            "auth",
            "storage",
            "functions",
            "observers",
            "logs",
            "metrics",
        ] {
            assert!(
                APP_TS.contains(&format!("{tab}:")) || APP_TS.contains(&format!("{tab}: ")),
                "SECTIONS must route tab '{tab}'"
            );
        }
    }

    #[test]
    fn test_shell_references_app_js() {
        let html = studio_shell_html();
        assert!(html.contains("app.js"), "shell must reference app.js");
    }

    #[test]
    fn test_mime_for_js() {
        assert!(mime_for_filename("app.js").contains("javascript"));
    }

    #[test]
    fn test_mime_for_css() {
        assert!(mime_for_filename("app.css").contains("css"));
    }
}
