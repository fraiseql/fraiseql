//! Metrics summary endpoint for the Studio dashboard.
//!
//! `GET /admin/v1/metrics/summary` returns a structured JSON summary
//! formatted for SPA consumption. This is complementary to the Prometheus
//! `/metrics` endpoint — it does NOT replace it.

use std::sync::atomic::Ordering;

use axum::{Json, extract::State, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::{metrics_server::MetricsCollector, routes::graphql::app_state::AppState};

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

/// Error rates.
///
/// The three windowed rates used to be served as three distinct numbers that were
/// all the *same* lifetime ratio. A dashboard showing "5m error rate" that is really
/// the all-time average is at its most misleading during an incident, which is the
/// only time anyone reads it. They are `Option` and `None` until windowed counters
/// exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRates {
    /// Errors per request over the process lifetime.
    pub lifetime: f64,
    /// Errors per request over the last 5 minutes, or `None` if not tracked.
    pub rate_5m:  Option<f64>,
    /// Errors per request over the last hour, or `None` if not tracked.
    pub rate_1h:  Option<f64>,
    /// Errors per request over the last 24 hours, or `None` if not tracked.
    pub rate_24h: Option<f64>,
}

/// Database connection pool stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Active (in-use) connections.
    pub active:      u32,
    /// Idle connections.
    pub idle:        u32,
    /// Maximum pool size.
    pub max:         u32,
    /// Utilization ratio (active / max).
    pub utilization: f64,
}

/// Query result cache stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Cache hit rate (0–1).
    pub hit_rate: f64,
    /// Current number of cached entries, or `None` — `MetricsCollector` does not
    /// track it, and `0` would read as "the cache is empty".
    pub entries:  Option<u64>,
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
    pub latency:       LatencyStats,
    /// Error rates over sliding windows.
    pub errors:        ErrorRates,
    /// Database pool health, or `None` when the summary has no pool handle.
    pub pool:          Option<PoolStats>,
    /// Query cache stats.
    pub cache:         CacheStats,
    /// Subscription stats, or `None` when not tracked.
    pub subscriptions: Option<SubscriptionStats>,
}

impl MetricsSummary {
    /// A summary for a process that has served nothing yet.
    ///
    /// Genuine zeros where zero is the truth (no requests ⇒ no latency, no errors),
    /// `None` where the number is simply not collected.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            latency:       LatencyStats {
                p50_ms: 0,
                p95_ms: 0,
                p99_ms: 0,
            },
            errors:        ErrorRates {
                lifetime: 0.0,
                rate_5m:  None,
                rate_1h:  None,
                rate_24h: None,
            },
            pool:          None,
            cache:         CacheStats {
                hit_rate: 0.0,
                entries:  None,
            },
            subscriptions: None,
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
pub async fn summary_handler<A>(State(state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(build_summary(&state.metrics))
}

/// Build a `MetricsSummary` from the live `MetricsCollector`.
pub(crate) fn build_summary(m: &MetricsCollector) -> MetricsSummary {
    let latency = LatencyStats {
        p50_ms: m.http_request_duration.estimate_quantile_us(0.5) / 1_000,
        p95_ms: m.http_request_duration.estimate_quantile_us(0.95) / 1_000,
        p99_ms: m.http_request_duration.estimate_quantile_us(0.99) / 1_000,
    };

    // Lifetime error rate. Windowed rates are reported as absent, not as this value.
    let total = m.queries_total.load(Ordering::Relaxed);
    let errors = m.queries_error.load(Ordering::Relaxed);
    #[allow(clippy::cast_precision_loss)] // Reason: counter values in practice are < 2^53
    let error_rate = if total > 0 {
        errors as f64 / total as f64
    } else {
        0.0
    };
    let errors_stats = ErrorRates {
        lifetime: error_rate,
        rate_5m:  None,
        rate_1h:  None,
        rate_24h: None,
    };

    // Cache stats
    let hits = m.cache_hits.load(Ordering::Relaxed);
    let misses = m.cache_misses.load(Ordering::Relaxed);
    #[allow(clippy::cast_precision_loss)] // Reason: counter values in practice are < 2^53
    let hit_rate = if hits + misses > 0 {
        hits as f64 / (hits + misses) as f64
    } else {
        0.0
    };
    let cache = CacheStats {
        hit_rate,
        entries: None, // Not tracked by MetricsCollector — `0` would mean "empty".
    };

    // Pool stats need adapter-level access this handler does not have. `None` says
    // "unknown"; the previous all-zero PoolStats said "no connections, no capacity",
    // which is what a saturated pool also looks like.
    let pool = None;

    // Subscription count is not tracked in AppState metrics.
    let subscriptions = None;

    MetricsSummary {
        latency,
        errors: errors_stats,
        pool,
        cache,
        subscriptions,
    }
}
