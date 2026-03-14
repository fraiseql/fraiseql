//! Health check endpoint.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use tracing::{debug, error};

use crate::routes::graphql::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall server status: `"healthy"`, `"degraded"`, or `"unhealthy"`.
    ///
    /// - `"healthy"` — database and all enabled subsystems are reachable.
    /// - `"degraded"` — database is healthy but an optional subsystem is failing. Returns HTTP 200
    ///   so load balancers keep the pod in rotation; alert on the field value.
    /// - `"unhealthy"` — database is unreachable. Returns HTTP 503.
    pub status: String,

    /// Database status.
    pub database: DatabaseStatus,

    /// Observer runtime health (present when the `observers` feature is compiled in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observers: Option<ObserverHealth>,

    /// Cache / Redis health (present when a Redis cache backend is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheHealth>,

    /// Secrets backend health (present when Vault or another secrets backend is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<SecretsHealth>,

    /// Server version.
    pub version: String,

    /// 32-character hex SHA-256 content hash of the compiled schema.
    ///
    /// Operators can compare this value across server instances to verify
    /// all instances are running the same schema. Different values indicate
    /// a schema mismatch (e.g. partial rollout or stale deployment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
}

/// Observer runtime health snapshot.
#[derive(Debug, Serialize)]
pub struct ObserverHealth {
    /// Whether the observer runtime is currently running.
    pub running:        bool,
    /// Approximate number of events pending in the internal queue.
    pub pending_events: usize,
    /// Last error message from the observer runtime, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error:     Option<String>,
}

/// Cache subsystem health.
#[derive(Debug, Serialize)]
pub struct CacheHealth {
    /// Whether the cache backend is reachable (Redis ping succeeded, or always
    /// `true` for the in-memory backend).
    pub connected: bool,
    /// Cache backend type: `"redis"` or `"in-memory"`.
    pub backend:   String,
}

/// Secrets backend health.
#[derive(Debug, Serialize)]
pub struct SecretsHealth {
    /// Whether the secrets backend is reachable and the token is valid.
    pub connected: bool,
    /// Backend type: `"vault"`, `"env"`, `"aws-secrets"`, etc.
    pub backend:   String,
}

/// Readiness response (subset of HealthResponse).
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    /// `"ready"` or `"not_ready"`.
    pub status: String,
    /// Human-readable reason when `status = "not_ready"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Database status.
#[derive(Debug, Serialize)]
pub struct DatabaseStatus {
    /// Connection status.
    pub connected: bool,

    /// Database type.
    pub database_type: String,

    /// Active connections (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_connections: Option<usize>,

    /// Idle connections (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_connections: Option<usize>,
}

/// Federation health response.
#[derive(Debug, Serialize)]
pub struct FederationHealthResponse {
    /// Overall federation status: healthy, degraded, unhealthy, unknown
    pub status: String,

    /// Per-subgraph status
    pub subgraphs: Vec<crate::federation::SubgraphHealthStatus>,

    /// Response timestamp
    pub timestamp: String,
}

/// Health check handler.
///
/// Returns server and database health status.
///
/// # Response Codes
///
/// - 200: Everything healthy
/// - 503: Database connection failed
pub async fn health_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Health check requested");

    // Perform real database health check
    let health_result = state.executor.adapter().health_check().await;
    let db_healthy = health_result.is_ok();

    let adapter = state.executor.adapter();
    let db_type = adapter.database_type();
    let metrics = adapter.pool_metrics();

    let database = if db_healthy {
        DatabaseStatus {
            connected:          true,
            database_type:      format!("{db_type:?}"),
            active_connections: Some(metrics.active_connections as usize),
            idle_connections:   Some(metrics.idle_connections as usize),
        }
    } else {
        error!("Database health check failed: {:?}", health_result.err());
        DatabaseStatus {
            connected:          false,
            database_type:      format!("{db_type:?}"),
            active_connections: Some(metrics.active_connections as usize),
            idle_connections:   Some(metrics.idle_connections as usize),
        }
    };

    let status = if db_healthy { "healthy" } else { "unhealthy" };

    let schema_hash = Some(state.executor.schema().content_hash());

    let response = HealthResponse {
        status: status.to_string(),
        database,
        observers: None, // Populated when observer health probe is wired into AppState
        cache: None,     // Populated when Redis cache probe is wired into AppState
        secrets: None,   // Populated when Vault probe is wired into AppState
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_hash,
    };

    let status_code = if db_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
}

/// Readiness probe handler.
///
/// Returns `200 OK` when the server can serve traffic (database reachable),
/// or `503 Service Unavailable` when it cannot.
///
/// Kubernetes usage:
/// - `livenessProbe` → `GET /health` (always 200 while process is alive)
/// - `readinessProbe` → `GET /readiness` (503 while not ready to serve traffic)
pub async fn readiness_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Readiness check requested");

    let db_healthy = state.executor.adapter().health_check().await.is_ok();

    if db_healthy {
        (
            StatusCode::OK,
            Json(ReadinessResponse {
                status: "ready".to_string(),
                reason: None,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                status: "not_ready".to_string(),
                reason: Some("Database connection unavailable".to_string()),
            }),
        )
    }
}

/// Federation health check handler.
///
/// Returns federation-specific health status.
///
/// When federation is configured in the compiled schema, reports `healthy`.
/// When federation is not configured, reports `not_configured`.
///
/// # Response Codes
///
/// - 200: Federation status retrieved
pub async fn federation_health_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Federation health check requested");

    let schema = state.executor.schema();
    let (status, status_code) = match schema.federation.as_ref() {
        Some(fed) if fed.enabled => ("healthy", StatusCode::OK),
        _ => ("not_configured", StatusCode::OK),
    };

    let response = FederationHealthResponse {
        status:    status.to_string(),
        subgraphs: vec![],
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    (status_code, Json(response))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status:      "healthy".to_string(),
            database:    DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: Some(2),
                idle_connections:   Some(8),
            },
            observers:   None,
            cache:       None,
            secrets:     None,
            version:     "2.0.0-a1".to_string(),
            schema_hash: Some("abc123def456abc1".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("PostgreSQL"));
    }
}
