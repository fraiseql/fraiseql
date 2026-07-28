//! Admin API endpoints for the Studio dashboard.
//!
//! All routes are grouped under `/admin/v1/*` and protected by the existing
//! `bearer_auth_middleware` (reusing the same admin token from `ServerConfig`).

use axum::{Json, extract::State};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Response shapes (agreed with Luxen UI author per phase spec)
// ---------------------------------------------------------------------------

/// Response from `GET /admin/v1/health/detailed`.
///
/// Every field that the server cannot currently measure is `Option` and answered as
/// `null`, never as `0`. A zero pool size reads as "the pool is exhausted"; a zero
/// cache hit rate reads as "the cache never hits". Both were being reported by a
/// server that simply had no access to the numbers.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminHealthResponse {
    /// Server uptime in seconds since startup.
    pub uptime_secs:    u64,
    /// Binary version string (e.g. `"2.2.0"`).
    pub version:        String,
    /// Number of active database connections, or `None` when not measurable here.
    pub pool_active:    Option<u32>,
    /// Number of idle database connections, or `None` when not measurable here.
    pub pool_idle:      Option<u32>,
    /// Maximum pool size, or `None` when not measurable here.
    pub pool_max:       Option<u32>,
    /// Query cache hit rate (0–1), or `None` if cache is disabled or unmeasured.
    pub cache_hit_rate: Option<f64>,
    /// Current cache entry count, or `None` if cache is disabled or unmeasured.
    pub cache_entries:  Option<u64>,
}

/// Response from `GET /admin/v1/schema`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminSchemaResponse {
    /// Compiled schema as raw JSON value.
    pub schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Token extraction helper (public for testing)
// ---------------------------------------------------------------------------

/// Extract the bearer token from an `Authorization` header value.
///
/// Returns `Some(token)` for `"Bearer <token>"` headers; `None` otherwise.
#[must_use]
pub fn extract_bearer_token(auth_header: Option<&str>) -> Option<&str> {
    let header = auth_header?;
    header.strip_prefix("Bearer ")
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/v1/schema` — compiled schema as JSON.
///
/// Protected by `bearer_auth_middleware` applied in the router layer.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn schema_handler<A>(
    State(state): State<AppState<A>>,
) -> impl axum::response::IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let schema = state.executor.load().schema().clone();
    let value = serde_json::to_value(&schema).unwrap_or(serde_json::Value::Null);
    Json(AdminSchemaResponse { schema: value })
}

/// `GET /admin/v1/health/detailed` — pool stats, cache stats, uptime, version.
///
/// Protected by `bearer_auth_middleware` applied in the router layer.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn health_handler<A>(
    State(state): State<AppState<A>>,
) -> impl axum::response::IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Was `SystemTime::now() - UNIX_EPOCH`, i.e. the current Unix timestamp: a
    // server that had been up for four seconds reported ~1.8 billion seconds of
    // uptime. `AppState::started_at` is set when the state is built at boot.
    let uptime_secs = state.started_at.elapsed().as_secs();

    Json(AdminHealthResponse {
        uptime_secs,
        version: env!("CARGO_PKG_VERSION").to_string(),
        // `None`, not `0`: this handler has no pool handle, and reporting a
        // zero-sized pool to an operator diagnosing saturation is worse than
        // reporting nothing.
        pool_active: None,
        pool_idle: None,
        pool_max: None,
        cache_hit_rate: None,
        cache_entries: None,
    })
}
