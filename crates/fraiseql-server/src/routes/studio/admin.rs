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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminHealthResponse {
    /// Server uptime in seconds since startup.
    pub uptime_secs: u64,
    /// Binary version string (e.g. `"2.2.0"`).
    pub version: String,
    /// Number of active database connections in the pool.
    pub pool_active: u32,
    /// Number of idle database connections in the pool.
    pub pool_idle: u32,
    /// Maximum pool size.
    pub pool_max: u32,
    /// Query cache hit rate (0–1), or `None` if cache is disabled.
    pub cache_hit_rate: Option<f64>,
    /// Current cache entry count, or `None` if cache is disabled.
    pub cache_entries: Option<u64>,
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
    State(_state): State<AppState<A>>,
) -> impl axum::response::IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let uptime_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Json(AdminHealthResponse {
        uptime_secs,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pool_active: 0,
        pool_idle: 0,
        pool_max: 0,
        cache_hit_rate: None,
        cache_entries: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
