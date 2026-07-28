//! Metrics summary endpoint /admin/v1/metrics/summary
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use fraiseql_server::routes::studio::metrics_summary::MetricsSummary;

#[test]
fn test_metrics_summary_shape() {
    let resp = MetricsSummary {
        latency:       fraiseql_server::routes::studio::metrics_summary::LatencyStats {
            p50_ms: 2,
            p95_ms: 18,
            p99_ms: 45,
        },
        errors:        fraiseql_server::routes::studio::metrics_summary::ErrorRates {
            lifetime: 0.0015,
            rate_5m:  Some(0.002),
            rate_1h:  Some(0.001),
            rate_24h: Some(0.0008),
        },
        pool:          Some(fraiseql_server::routes::studio::metrics_summary::PoolStats {
            active:      4,
            idle:        12,
            max:         20,
            utilization: 0.25,
        }),
        cache:         fraiseql_server::routes::studio::metrics_summary::CacheStats {
            hit_rate: 0.91,
            entries:  Some(1024),
        },
        subscriptions: Some(fraiseql_server::routes::studio::metrics_summary::SubscriptionStats {
            active: 38,
        }),
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"latency\""));
    assert!(json.contains("\"p50_ms\""));
    assert!(json.contains("\"errors\""));
    assert!(json.contains("\"pool\""));
    assert!(json.contains("\"cache\""));
    assert!(json.contains("\"subscriptions\""));
}
