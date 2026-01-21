use std::sync::Arc;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::runtime_state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub checks: Vec<HealthCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Liveness probe - is the process running?
pub async fn liveness_handler() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness probe - is the service ready to accept traffic?
pub async fn readiness_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    // Check if shutting down
    if state.shutdown.is_shutting_down() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: HealthStatus::Unhealthy,
                checks: vec![HealthCheck {
                    name: "shutdown".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Service is shutting down".to_string()),
                    latency_ms: None,
                }],
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        ).into_response();
    }

    // Perform health checks
    let mut checks = Vec::new();
    let mut overall_status = HealthStatus::Healthy;

    // Database check (if database feature is enabled)
    #[cfg(feature = "database")]
    {
        let db_check = check_database(&state).await;
        if db_check.status != HealthStatus::Healthy {
            overall_status = HealthStatus::Degraded;
        }
        checks.push(db_check);
    }

    // Cache check (if configured)
    if state.cache.is_some() {
        let cache_check = check_cache(&state).await;
        if cache_check.status == HealthStatus::Unhealthy {
            overall_status = HealthStatus::Degraded;
        }
        checks.push(cache_check);
    }

    let status_code = match overall_status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK, // Still accept traffic
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (
        status_code,
        Json(HealthResponse {
            status: overall_status,
            checks,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    ).into_response()
}

#[cfg(feature = "database")]
async fn check_database(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    match sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => HealthCheck {
            name: "database".to_string(),
            status: HealthStatus::Healthy,
            message: None,
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
        Err(e) => HealthCheck {
            name: "database".to_string(),
            status: HealthStatus::Unhealthy,
            message: Some(format!("Connection failed: {}", e)),
            latency_ms: Some(start.elapsed().as_millis() as u64),
        },
    }
}

async fn check_cache(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    if let Some(cache) = &state.cache {
        match cache.ping().await {
            Ok(_) => HealthCheck {
                name: "cache".to_string(),
                status: HealthStatus::Healthy,
                message: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
            Err(e) => HealthCheck {
                name: "cache".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("Connection failed: {}", e)),
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
        }
    } else {
        HealthCheck {
            name: "cache".to_string(),
            status: HealthStatus::Healthy,
            message: Some("Not configured".to_string()),
            latency_ms: None,
        }
    }
}

/// Startup probe handler for slow-starting services
pub async fn startup_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    // Check critical dependencies only
    #[cfg(feature = "database")]
    {
        let db_check = check_database(&state).await;

        if db_check.status == HealthStatus::Healthy {
            StatusCode::OK.into_response()
        } else {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }

    #[cfg(not(feature = "database"))]
    {
        // Without database, just check if we're not shutting down
        if state.shutdown.is_shutting_down() {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            StatusCode::OK.into_response()
        }
    }
}
