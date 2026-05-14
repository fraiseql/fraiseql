//! HTTP handlers for observer Dead Letter Queue (DLQ) endpoints.
//!
//! Exposes the in-memory DLQ over HTTP so applications can observe delivery
//! health and retry failed items without direct database or Redis access.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::runtime::ObserverRuntime;

// ── State ────────────────────────────────────────────────────────────────────

/// Shared state for DLQ HTTP handlers.
#[derive(Clone)]
pub struct DlqState {
    /// The observer runtime (provides DLQ access and event re-processing).
    pub runtime: Arc<RwLock<ObserverRuntime>>,
}

// ── Response types ───────────────────────────────────────────────────────────

/// Summary of observer delivery health.
#[derive(Debug, Serialize)]
pub struct DeliveryStatusSummary {
    /// Whether the observer runtime is running.
    pub running: bool,
    /// Number of loaded observers.
    pub observer_count: usize,
    /// Total events processed since startup.
    pub events_processed: u64,
    /// Total errors since startup.
    pub errors: u64,
    /// Number of items currently in the DLQ.
    pub dlq_count: usize,
}

/// A single DLQ item in the HTTP response.
#[derive(Debug, Serialize)]
pub struct DlqItemResponse {
    /// Unique DLQ item ID.
    pub id: Uuid,
    /// ID of the event that triggered the action.
    pub event_id: Uuid,
    /// Entity type (e.g. "Order", "User").
    pub entity_type: String,
    /// Entity instance ID.
    pub entity_id: Uuid,
    /// Event type (INSERT, UPDATE, DELETE).
    pub event_type: String,
    /// Action type that failed (e.g. "webhook", "email").
    pub action_type: String,
    /// Error message from the last attempt.
    pub error_message: String,
    /// Number of retry attempts made.
    pub attempts: u32,
}

impl From<&fraiseql_observers::DlqItem> for DlqItemResponse {
    fn from(item: &fraiseql_observers::DlqItem) -> Self {
        Self {
            id: item.id,
            event_id: item.event.id,
            entity_type: item.event.entity_type.clone(),
            entity_id: item.event.entity_id,
            event_type: item.event.event_type.as_str().to_string(),
            action_type: action_type_str(&item.action).to_string(),
            error_message: item.error_message.clone(),
            attempts: item.attempts,
        }
    }
}

/// Paginated DLQ list response.
#[derive(Debug, Serialize)]
pub struct DlqListResponse {
    /// Items on the current page.
    pub items: Vec<DlqItemResponse>,
    /// Total number of items matching the filter.
    pub total: usize,
    /// Limit requested.
    pub limit: usize,
    /// Offset applied.
    pub offset: usize,
}

/// Query parameters for `GET /api/observers/dlq`.
#[derive(Debug, Deserialize)]
pub struct DlqListQuery {
    /// Maximum items to return (default 50).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination (default 0).
    #[serde(default)]
    pub offset: usize,
    /// Optional filter: action type (e.g. "webhook").
    pub action: Option<String>,
    /// Optional filter: entity type (e.g. "Order").
    pub object_type: Option<String>,
}

const fn default_limit() -> usize {
    50
}

/// Result of retrying a single DLQ item.
#[derive(Debug, Serialize)]
pub struct RetryResponse {
    /// Whether the retry succeeded.
    pub success: bool,
    /// The DLQ item ID that was retried.
    pub item_id: Uuid,
    /// Human-readable result message.
    pub message: String,
}

/// Result of retrying all DLQ items.
#[derive(Debug, Serialize)]
pub struct RetryAllResponse {
    /// Number of items successfully re-processed.
    pub items_retried: usize,
    /// Number of items that failed re-processing.
    pub items_failed: usize,
    /// Human-readable result message.
    pub message: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/observers/delivery/health`
///
/// Returns a summary of observer delivery health including DLQ counts.
pub async fn delivery_health_handler(
    State(state): State<DlqState>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let health = runtime.health();
    let dlq = runtime.dlq();

    let summary = DeliveryStatusSummary {
        running: health.running,
        observer_count: health.observer_count,
        events_processed: health.events_processed,
        errors: health.errors,
        dlq_count: dlq.count(),
    };

    (StatusCode::OK, Json(summary))
}

/// `GET /api/observers/dlq`
///
/// Returns a paginated, optionally filtered list of DLQ items.
pub async fn dlq_list_handler(
    State(state): State<DlqState>,
    Query(query): Query<DlqListQuery>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();
    let all_items = dlq.list_all();

    // Apply filters
    let filtered: Vec<_> = all_items
        .iter()
        .filter(|item| {
            if let Some(ref action_filter) = query.action {
                if action_type_str(&item.action) != action_filter.as_str() {
                    return false;
                }
            }
            if let Some(ref object_type) = query.object_type {
                if item.event.entity_type != *object_type {
                    return false;
                }
            }
            true
        })
        .collect();

    let total = filtered.len();

    // Apply pagination
    let page: Vec<DlqItemResponse> = filtered
        .iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|item| DlqItemResponse::from(*item))
        .collect();

    let response = DlqListResponse {
        items: page,
        total,
        limit: query.limit,
        offset: query.offset,
    };

    (StatusCode::OK, Json(response))
}

/// `GET /api/observers/dlq/:id`
///
/// Returns a single DLQ item by ID.
pub async fn dlq_get_handler(
    State(state): State<DlqState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();

    match dlq.get(id) {
        Some(item) => (StatusCode::OK, Json(serde_json::json!(DlqItemResponse::from(&item))))
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "DLQ item not found" })),
        )
            .into_response(),
    }
}

/// `POST /api/observers/dlq/:id/retry`
///
/// Re-processes a single DLQ item through the observer executor, then
/// removes it from the DLQ on success.
pub async fn dlq_retry_handler(
    State(state): State<DlqState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();

    let Some(item) = dlq.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(RetryResponse {
                success: false,
                item_id: id,
                message: "DLQ item not found".to_string(),
            }),
        )
            .into_response();
    };

    // Re-process the event through the executor
    let executor_guard = runtime.executor_ref().read().await;
    let Some(executor) = executor_guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RetryResponse {
                success: false,
                item_id: id,
                message: "Observer executor not available".to_string(),
            }),
        )
            .into_response();
    };

    match executor.process_event(&item.event).await {
        Ok(_summary) => {
            dlq.remove(id);
            (
                StatusCode::OK,
                Json(RetryResponse {
                    success: true,
                    item_id: id,
                    message: "Item re-processed successfully".to_string(),
                }),
            )
                .into_response()
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RetryResponse {
                success: false,
                item_id: id,
                message: format!("Retry failed: {e}"),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/observers/dlq/retry-all`
///
/// Re-processes all DLQ items. Successfully retried items are removed from the DLQ.
pub async fn dlq_retry_all_handler(
    State(state): State<DlqState>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();
    let items = dlq.list_all();

    if items.is_empty() {
        return (
            StatusCode::OK,
            Json(RetryAllResponse {
                items_retried: 0,
                items_failed: 0,
                message: "No items in DLQ".to_string(),
            }),
        );
    }

    let executor_guard = runtime.executor_ref().read().await;
    let Some(executor) = executor_guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RetryAllResponse {
                items_retried: 0,
                items_failed: items.len(),
                message: "Observer executor not available".to_string(),
            }),
        );
    };

    let mut retried = 0;
    let mut failed = 0;

    for item in &items {
        match executor.process_event(&item.event).await {
            Ok(_) => {
                dlq.remove(item.id);
                retried += 1;
            },
            Err(e) => {
                tracing::warn!(item_id = %item.id, error = %e, "DLQ retry failed");
                failed += 1;
            },
        }
    }

    (
        StatusCode::OK,
        Json(RetryAllResponse {
            items_retried: retried,
            items_failed: failed,
            message: format!("Batch retry completed: {retried} retried, {failed} failed"),
        }),
    )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract a human-readable action type string from an `ActionConfig`.
const fn action_type_str(action: &fraiseql_observers::ActionConfig) -> &'static str {
    action.action_type()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_status_summary_serializes() {
        let summary = DeliveryStatusSummary {
            running: true,
            observer_count: 3,
            events_processed: 42,
            errors: 1,
            dlq_count: 2,
        };
        let json = serde_json::to_value(&summary).expect("serialize");
        assert_eq!(json["running"], true);
        assert_eq!(json["dlq_count"], 2);
        assert_eq!(json["events_processed"], 42);
    }

    #[test]
    fn dlq_list_response_serializes() {
        let response = DlqListResponse {
            items: vec![],
            total: 0,
            limit: 50,
            offset: 0,
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["total"], 0);
        assert_eq!(json["limit"], 50);
    }

    #[test]
    fn retry_response_serializes() {
        let response = RetryResponse {
            success: true,
            item_id: Uuid::nil(),
            message: "ok".to_string(),
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["success"], true);
    }

    #[test]
    fn retry_all_response_serializes() {
        let response = RetryAllResponse {
            items_retried: 5,
            items_failed: 1,
            message: "done".to_string(),
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["items_retried"], 5);
        assert_eq!(json["items_failed"], 1);
    }

    #[test]
    fn default_limit_is_50() {
        assert_eq!(default_limit(), 50);
    }
}
