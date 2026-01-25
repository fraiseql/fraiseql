//! HTTP handlers for observer management endpoints.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;

use super::{
    CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery, ObserverRepository,
    PaginatedResponse, UpdateObserverRequest,
};

/// Application state for observer handlers.
#[derive(Clone)]
pub struct ObserverState {
    pub repository: ObserverRepository,
}

/// List observers with optional filters.
///
/// GET /api/observers
pub async fn list_observers(
    State(state): State<ObserverState>,
    Query(query): Query<ListObserversQuery>,
) -> impl IntoResponse {
    // TODO: Extract customer_org from auth context
    let customer_org: Option<i64> = None;

    match state.repository.list(&query, customer_org).await {
        Ok((observers, total_count)) => {
            let response =
                PaginatedResponse::new(observers, query.page, query.page_size, total_count);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to list observers: {}", error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Get a single observer by ID.
///
/// GET /api/observers/:id
pub async fn get_observer(
    State(state): State<ObserverState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let customer_org: Option<i64> = None;

    match state.repository.get_by_id(id, customer_org).await {
        Ok(Some(observer)) => (StatusCode::OK, Json(observer)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Observer not found" })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to get observer {}: {}", id, error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Create a new observer.
///
/// POST /api/observers
pub async fn create_observer(
    State(state): State<ObserverState>,
    Json(request): Json<CreateObserverRequest>,
) -> impl IntoResponse {
    // Validate request
    if request.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Name is required" })),
        )
            .into_response();
    }

    if request.actions.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "At least one action is required" })),
        )
            .into_response();
    }

    // Validate event_type if provided
    if let Some(ref event_type) = request.event_type {
        let valid_types = ["INSERT", "UPDATE", "DELETE", "CUSTOM"];
        if !valid_types.contains(&event_type.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid event_type '{}'. Must be one of: {:?}", event_type, valid_types)
                })),
            )
                .into_response();
        }
    }

    let customer_org: Option<i64> = None;
    // TODO: Extract created_by from auth context
    let created_by: Option<&str> = None;

    match state.repository.create(&request, customer_org, created_by).await {
        Ok(observer) => (StatusCode::CREATED, Json(observer)).into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to create observer: {}", error_msg);
            let status = if error_msg.contains("already exists") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(serde_json::json!({ "error": error_msg }))).into_response()
        },
    }
}

/// Update an existing observer.
///
/// PATCH /api/observers/:id
pub async fn update_observer(
    State(state): State<ObserverState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateObserverRequest>,
) -> impl IntoResponse {
    // Validate event_type if provided
    if let Some(ref event_type) = request.event_type {
        let valid_types = ["INSERT", "UPDATE", "DELETE", "CUSTOM"];
        if !valid_types.contains(&event_type.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid event_type '{}'. Must be one of: {:?}", event_type, valid_types)
                })),
            )
                .into_response();
        }
    }

    let customer_org: Option<i64> = None;
    // TODO: Extract updated_by from auth context
    let updated_by: Option<&str> = None;

    match state.repository.update(id, &request, customer_org, updated_by).await {
        Ok(Some(observer)) => (StatusCode::OK, Json(observer)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Observer not found" })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to update observer {}: {}", id, error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Delete an observer (soft delete).
///
/// DELETE /api/observers/:id
pub async fn delete_observer(
    State(state): State<ObserverState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let customer_org: Option<i64> = None;

    match state.repository.delete(id, customer_org).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Observer not found" })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to delete observer {}: {}", id, error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Get observer statistics.
///
/// GET /api/observers/stats
/// GET /api/observers/:id/stats
pub async fn get_observer_stats(
    State(state): State<ObserverState>,
    observer_id: Option<Path<Uuid>>,
) -> impl IntoResponse {
    let customer_org: Option<i64> = None;
    let id = observer_id.map(|p| p.0);

    match state.repository.get_stats(id, customer_org).await {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to get observer stats: {}", error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// List observer execution logs.
///
/// GET /api/observers/logs
/// GET /api/observers/:id/logs
pub async fn list_observer_logs(
    State(state): State<ObserverState>,
    Path(observer_id): Path<Option<Uuid>>,
    Query(mut query): Query<ListObserverLogsQuery>,
) -> impl IntoResponse {
    // If observer_id is in path, use it
    if let Some(id) = observer_id {
        query.observer_id = Some(id);
    }

    let customer_org: Option<i64> = None;

    match state.repository.list_logs(&query, customer_org).await {
        Ok((logs, total_count)) => {
            let response = PaginatedResponse::new(logs, query.page, query.page_size, total_count);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to list observer logs: {}", error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Enable an observer.
///
/// POST /api/observers/:id/enable
pub async fn enable_observer(
    State(state): State<ObserverState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let request = UpdateObserverRequest {
        enabled: Some(true),
        ..Default::default()
    };

    let customer_org: Option<i64> = None;
    let updated_by: Option<&str> = None;

    match state.repository.update(id, &request, customer_org, updated_by).await {
        Ok(Some(observer)) => (StatusCode::OK, Json(observer)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Observer not found" })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to enable observer {}: {}", id, error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

/// Disable an observer.
///
/// POST /api/observers/:id/disable
pub async fn disable_observer(
    State(state): State<ObserverState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let request = UpdateObserverRequest {
        enabled: Some(false),
        ..Default::default()
    };

    let customer_org: Option<i64> = None;
    let updated_by: Option<&str> = None;

    match state.repository.update(id, &request, customer_org, updated_by).await {
        Ok(Some(observer)) => (StatusCode::OK, Json(observer)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Observer not found" })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to disable observer {}: {}", id, error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}

// ============================================================================
// Runtime Health Check Handlers
// ============================================================================

use std::sync::Arc;

use tokio::sync::RwLock;

/// State for runtime health checks
#[derive(Clone)]
pub struct RuntimeHealthState {
    /// Reference to the observer runtime (wrapped in RwLock for thread safety)
    pub runtime: Arc<RwLock<super::ObserverRuntime>>,
}

/// Get observer runtime health status.
///
/// GET /api/observers/runtime/health
pub async fn get_runtime_health(State(state): State<RuntimeHealthState>) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let health = runtime.health();

    let status = if health.running {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    // Convert RuntimeHealth to JSON-serializable format
    let health_json = serde_json::json!({
        "running": health.running,
        "observer_count": health.observer_count,
        "last_checkpoint": health.last_checkpoint,
        "events_processed": health.events_processed,
        "errors": health.errors
    });

    (status, Json(health_json)).into_response()
}

/// Reload observers from database.
///
/// POST /api/observers/runtime/reload
pub async fn reload_observers(State(state): State<RuntimeHealthState>) -> impl IntoResponse {
    let runtime = state.runtime.read().await;

    match runtime.reload_observers().await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Observers reloaded successfully",
                "count": count
            })),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to reload observers: {}", error_msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        },
    }
}
