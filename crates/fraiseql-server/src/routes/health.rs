//! Health check endpoint.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use tracing::{debug, error};

use crate::routes::graphql::AppState;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Server status.
    pub status: String,

    /// Database status.
    pub database: DatabaseStatus,

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
            version:     "2.0.0-a1".to_string(),
            schema_hash: Some("abc123def456abc1".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("PostgreSQL"));
    }
}
