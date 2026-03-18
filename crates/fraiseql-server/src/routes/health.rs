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

    /// Federation circuit breaker state (present when federation is configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub federation: Option<FederationHealth>,

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

/// Federation circuit breaker health snapshot.
#[derive(Debug, Serialize)]
pub struct FederationHealth {
    /// Whether federation is configured at all.
    pub configured: bool,
    /// Per-entity circuit breaker state.
    pub subgraphs:  Vec<crate::federation::circuit_breaker::SubgraphCircuitHealth>,
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

    // Bind executor guard once for consistency within this handler
    let executor = state.executor();

    // Perform real database health check
    let health_result = executor.adapter().health_check().await;
    let db_healthy = health_result.is_ok();

    let adapter = executor.adapter();
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

    let schema_hash = Some(executor.schema().content_hash());

    let federation = state.circuit_breaker.as_ref().map(|cb| FederationHealth {
        configured: true,
        subgraphs:  cb.health_snapshot(),
    });

    // Probe observer health when the runtime is attached to AppState.
    #[cfg(feature = "observers")]
    let observers = if let Some(ref runtime) = state.observer_runtime {
        let rt = runtime.read().await;
        let health = rt.health();
        Some(ObserverHealth {
            running:        health.running,
            pending_events: health.events_processed as usize,
            last_error:     if health.errors > 0 {
                Some(format!("{} errors encountered", health.errors))
            } else {
                None
            },
        })
    } else {
        None
    };
    #[cfg(not(feature = "observers"))]
    let observers: Option<ObserverHealth> = None;

    // Probe cache health when the Arrow cache is attached to AppState.
    #[cfg(feature = "arrow")]
    let cache = state.cache.as_ref().map(|_| CacheHealth {
        connected: true, // In-memory cache is always "connected"
        backend:   "in-memory".to_string(),
    });
    #[cfg(not(feature = "arrow"))]
    let cache: Option<CacheHealth> = None;

    // Probe secrets backend health.
    #[cfg(feature = "secrets")]
    let secrets = if let Some(ref sm) = state.secrets_manager {
        let connected = sm.health_check().await.is_ok();
        Some(SecretsHealth {
            connected,
            backend: sm.backend_name().to_string(),
        })
    } else {
        None
    };
    #[cfg(not(feature = "secrets"))]
    let secrets: Option<SecretsHealth> = None;

    let status = determine_status(db_healthy, observers.as_ref(), secrets.as_ref(), federation.as_ref());

    let response = HealthResponse {
        status: status.to_string(),
        database,
        observers,
        cache,
        secrets,
        federation,
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_hash,
    };

    let status_code = if status == "unhealthy" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
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

    let db_healthy = state.executor().adapter().health_check().await.is_ok();

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

    let executor = state.executor();
    let schema = executor.schema();
    let (status, status_code) = match schema.federation.as_ref() {
        Some(fed) if fed.enabled => ("healthy", StatusCode::OK),
        _ => ("not_configured", StatusCode::OK),
    };

    let subgraphs = state.circuit_breaker.as_ref().map_or_else(Vec::new, |cb| {
        cb.health_snapshot()
            .into_iter()
            .map(|entry| {
                let available = matches!(
                    entry.state,
                    crate::federation::circuit_breaker::CircuitHealthState::Closed
                        | crate::federation::circuit_breaker::CircuitHealthState::HalfOpen
                );
                crate::federation::SubgraphHealthStatus {
                    name: entry.subgraph,
                    available,
                    latency_ms: 0.0,
                    last_check: chrono::Utc::now().to_rfc3339(),
                    error_count_last_60s: 0,
                    error_rate_percent: 0.0,
                }
            })
            .collect()
    });

    let response = FederationHealthResponse {
        status: status.to_string(),
        subgraphs,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    (status_code, Json(response))
}

/// Determines overall health status.
///
/// - `"unhealthy"` (503): database is down
/// - `"degraded"` (200): database is up but one or more optional subsystems are failing
/// - `"healthy"` (200): all enabled subsystems are operational
fn determine_status(
    db_healthy: bool,
    observers: Option<&ObserverHealth>,
    secrets: Option<&SecretsHealth>,
    federation: Option<&FederationHealth>,
) -> &'static str {
    if !db_healthy {
        return "unhealthy";
    }

    let observers_degraded = observers.is_some_and(|o| !o.running);
    let secrets_degraded = secrets.is_some_and(|s| !s.connected);
    let federation_degraded = federation.is_some_and(|f| {
        f.configured
            && f.subgraphs.iter().any(|sg| {
                matches!(
                    sg.state,
                    crate::federation::circuit_breaker::CircuitHealthState::Open
                )
            })
    });

    if observers_degraded || secrets_degraded || federation_degraded {
        "degraded"
    } else {
        "healthy"
    }
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
    fn test_determine_status_all_healthy() {
        assert_eq!(determine_status(true, None, None, None), "healthy");
    }

    #[test]
    fn test_determine_status_db_down_is_unhealthy() {
        assert_eq!(determine_status(false, None, None, None), "unhealthy");
    }

    #[test]
    fn test_determine_status_observers_not_running_is_degraded() {
        let observers = Some(ObserverHealth {
            running:        false,
            pending_events: 0,
            last_error:     None,
        });
        assert_eq!(determine_status(true, observers.as_ref(), None, None), "degraded");
    }

    #[test]
    fn test_determine_status_secrets_disconnected_is_degraded() {
        let secrets = Some(SecretsHealth {
            connected: false,
            backend:   "vault".to_string(),
        });
        assert_eq!(determine_status(true, None, secrets.as_ref(), None), "degraded");
    }

    #[test]
    fn test_determine_status_federation_circuit_open_is_degraded() {
        use crate::federation::circuit_breaker::{CircuitHealthState, SubgraphCircuitHealth};

        let federation = Some(FederationHealth {
            configured: true,
            subgraphs:  vec![SubgraphCircuitHealth {
                subgraph: "Product".to_string(),
                state:    CircuitHealthState::Open,
            }],
        });
        assert_eq!(
            determine_status(true, None, None, federation.as_ref()),
            "degraded"
        );
    }

    #[test]
    fn test_determine_status_db_down_overrides_degraded() {
        let secrets = Some(SecretsHealth {
            connected: false,
            backend:   "vault".to_string(),
        });
        assert_eq!(
            determine_status(false, None, secrets.as_ref(), None),
            "unhealthy"
        );
    }

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
            federation:  None,
            version:     "2.0.0-a1".to_string(),
            schema_hash: Some("abc123def456abc1".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("PostgreSQL"));
    }

    #[test]
    fn test_health_response_omits_federation_when_none() {
        let response = HealthResponse {
            status:      "healthy".to_string(),
            database:    DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: None,
                idle_connections:   None,
            },
            observers:   None,
            cache:       None,
            secrets:     None,
            federation:  None,
            version:     "2.0.0".to_string(),
            schema_hash: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("federation"), "federation key must be absent when field is None");
    }

    #[test]
    fn test_health_response_includes_federation_when_present() {
        use crate::federation::circuit_breaker::{CircuitHealthState, SubgraphCircuitHealth};

        let response = HealthResponse {
            status:      "healthy".to_string(),
            database:    DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: None,
                idle_connections:   None,
            },
            observers:   None,
            cache:       None,
            secrets:     None,
            federation:  Some(FederationHealth {
                configured: true,
                subgraphs:  vec![SubgraphCircuitHealth {
                    subgraph: "Product".to_string(),
                    state:    CircuitHealthState::Open,
                }],
            }),
            version:     "2.0.0".to_string(),
            schema_hash: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("federation"), "federation key must be present");
        assert!(json.contains("configured"), "configured field must appear");
        assert!(json.contains("Product"), "subgraph name must appear");
        assert!(json.contains("open"), "circuit state must be serialised as snake_case");
    }
}
