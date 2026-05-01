//! Metrics summary endpoint for the Studio dashboard.
//!
//! `GET /admin/v1/metrics/summary` returns a structured JSON summary
//! formatted for SPA consumption. This is complementary to the Prometheus
//! `/metrics` endpoint — it does NOT replace it.

use axum::{Json, extract::State, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Metrics structs — agreed with Luxen UI author
// ---------------------------------------------------------------------------

/// Request latency percentiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Median (P50) latency in milliseconds.
    pub p50_ms: u64,
    /// 95th-percentile latency in milliseconds.
    pub p95_ms: u64,
    /// 99th-percentile latency in milliseconds.
    pub p99_ms: u64,
}

/// Error rates over sliding windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRates {
    /// Errors per request, averaged over the last 5 minutes.
    pub rate_5m: f64,
    /// Errors per request, averaged over the last 1 hour.
    pub rate_1h: f64,
    /// Errors per request, averaged over the last 24 hours.
    pub rate_24h: f64,
}

/// Database connection pool stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Active (in-use) connections.
    pub active: u32,
    /// Idle connections.
    pub idle: u32,
    /// Maximum pool size.
    pub max: u32,
    /// Utilization ratio (active / max).
    pub utilization: f64,
}

/// Query result cache stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Cache hit rate (0–1).
    pub hit_rate: f64,
    /// Current number of cached entries.
    pub entries: u64,
}

/// Active subscription stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionStats {
    /// Number of active GraphQL subscriptions.
    pub active: u32,
}

/// Full metrics summary response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Request latency percentiles.
    pub latency: LatencyStats,
    /// Error rates over sliding windows.
    pub errors: ErrorRates,
    /// Database pool health.
    pub pool: PoolStats,
    /// Query cache stats.
    pub cache: CacheStats,
    /// Subscription stats.
    pub subscriptions: SubscriptionStats,
}

impl MetricsSummary {
    /// Build a zero-value summary (used as placeholder until real collectors are wired).
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            latency: LatencyStats {
                p50_ms: 0,
                p95_ms: 0,
                p99_ms: 0,
            },
            errors: ErrorRates {
                rate_5m: 0.0,
                rate_1h: 0.0,
                rate_24h: 0.0,
            },
            pool: PoolStats {
                active: 0,
                idle: 0,
                max: 0,
                utilization: 0.0,
            },
            cache: CacheStats {
                hit_rate: 0.0,
                entries: 0,
            },
            subscriptions: SubscriptionStats { active: 0 },
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /admin/v1/metrics/summary` — structured JSON metrics snapshot.
///
/// Reformats existing metric collectors into a SPA-friendly shape.
/// Does NOT replace the `/metrics` Prometheus endpoint.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn summary_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Placeholder — reads from MetricsCollector in Cycle 9.
    Json(MetricsSummary::zero())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
