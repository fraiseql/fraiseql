//! Tests for `routes/studio/` modules.
#![allow(unused_imports)]

mod admin_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::admin::*;

    #[test]
    fn test_extract_bearer_token_valid() {
        assert_eq!(extract_bearer_token(Some("Bearer abc123")), Some("abc123"));
    }

    #[test]
    fn test_extract_bearer_token_no_header() {
        assert_eq!(extract_bearer_token(None), None);
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        assert_eq!(extract_bearer_token(Some("Basic abc")), None);
    }

    #[test]
    fn test_admin_health_response_serializes() {
        let resp = AdminHealthResponse {
            uptime_secs: 10,
            version: "test".to_string(),
            pool_active: 1,
            pool_idle: 4,
            pool_max: 5,
            cache_hit_rate: Some(0.8),
            cache_entries: Some(100),
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

mod auth_users_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::auth_users::*;

    #[test]
    fn test_user_list_response_serializes() {
        let resp = UserListResponse {
            users: vec![],
            total: 0,
            page: 1,
            page_size: 50,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"users\""));
    }

    #[test]
    fn test_user_invite_request_parses() {
        let input = r#"{"email":"a@b.com"}"#;
        let req: UserInviteRequest = serde_json::from_str(input).unwrap();
        assert_eq!(req.email, "a@b.com");
    }
}

mod data_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::data::*;

    #[test]
    fn test_data_query_response_serializes() {
        let resp = DataQueryResponse {
            rows: vec![serde_json::json!({"id": 1})],
            total: 1,
            page: 1,
            page_size: 50,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"rows\""));
        assert!(json.contains("\"total\""));
    }

    #[test]
    fn test_filter_op_round_trips() {
        for (raw, expected) in [
            ("\"eq\"", FilterOp::Eq),
            ("\"contains\"", FilterOp::Contains),
        ] {
            let op: FilterOp = serde_json::from_str(raw).unwrap();
            assert_eq!(op, expected);
        }
    }

    #[test]
    fn test_defaults() {
        let q: DataBrowserQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.page, 1);
        assert_eq!(q.page_size, 50);
    }
}

mod function_ops_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::function_ops::*;

    #[test]
    fn test_function_list_serializes() {
        let resp = FunctionListResponse { functions: vec![] };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"functions\""));
    }

    #[test]
    fn test_secret_set_request_parses() {
        let input = r#"{"value":"s3cr3t"}"#;
        let req: SecretSetRequest = serde_json::from_str(input).unwrap();
        assert_eq!(req.value, "s3cr3t");
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

        let summary = build_summary(&collector);
        // P50 should be in the 5ms bucket (5000 us → 5 ms)
        assert_eq!(summary.latency.p50_ms, 5);
        // P99 should be in the 100ms bucket
        assert_eq!(summary.latency.p99_ms, 100);
    }

    #[test]
    fn test_build_summary_empty_histogram_returns_zero() {
        let collector = MetricsCollector::new();
        let summary = build_summary(&collector);
        assert_eq!(summary.latency.p50_ms, 0);
        assert_eq!(summary.latency.p95_ms, 0);
        assert_eq!(summary.latency.p99_ms, 0);
    }

    #[test]
    fn test_build_summary_error_rate() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(100, Ordering::Relaxed);
        collector.queries_error.store(10, Ordering::Relaxed);

        let summary = build_summary(&collector);
        assert!((summary.errors.rate_5m - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_build_summary_cache_hit_rate() {
        let collector = MetricsCollector::new();
        collector.cache_hits.store(75, Ordering::Relaxed);
        collector.cache_misses.store(25, Ordering::Relaxed);

        let summary = build_summary(&collector);
        assert!((summary.cache.hit_rate - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_build_summary_zero_division_safe() {
        let collector = MetricsCollector::new();
        let summary = build_summary(&collector);
        assert_eq!(summary.errors.rate_5m, 0.0);
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
            "Data",
            "Auth",
            "Storage",
            "Functions",
            "Realtime",
            "Metrics",
        ] {
            assert!(html.contains(s), "shell must contain section '{s}'");
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

mod realtime_monitor_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::realtime_monitor::*;

    #[test]
    fn test_realtime_stats_serializes() {
        let resp = RealtimeStatsResponse {
            connections: 5,
            channels: vec!["users".to_string()],
            presence_rooms: vec![PresenceRoom {
                room: "lobby".to_string(),
                members: 3,
            }],
            cdc_lag_ms: Some(10),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"connections\""));
        assert!(json.contains("\"channels\""));
        assert!(json.contains("\"presence_rooms\""));
        assert!(json.contains("\"cdc_lag_ms\""));
    }
}

mod storage_browser_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::super::storage_browser::*;

    #[test]
    fn test_object_list_response_serializes() {
        let resp = ObjectListResponse {
            objects: vec![],
            total: 0,
            page: 1,
            page_size: 50,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"objects\""));
    }

    #[test]
    fn test_presign_request_parses() {
        let input = r#"{"bucket":"b","key":"k","expires_in_secs":60}"#;
        let req: PresignRequest = serde_json::from_str(input).unwrap();
        assert_eq!(req.bucket, "b");
        assert_eq!(req.expires_in_secs, 60);
    }
}
