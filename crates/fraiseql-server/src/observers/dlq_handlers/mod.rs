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

use super::runtime::{InMemoryDlq, ObserverRuntime};

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
    pub running:          bool,
    /// Number of loaded observers.
    pub observer_count:   usize,
    /// Total events processed since startup.
    pub events_processed: u64,
    /// Total errors since startup.
    pub errors:           u64,
    /// Number of items currently in the DLQ.
    pub dlq_count:          usize,
    /// Number of function-dispatch failures currently in the DLQ (after:mutation
    /// functions that exhausted their retries), counted separately from
    /// observer-action failures.
    pub function_dlq_count: usize,
    /// Number of failed entries dropped because the DLQ was at capacity
    /// (drop-newest, controlled by `[observers.runtime] max_dlq_size`). A
    /// non-zero value signals sustained delivery failure against a capped DLQ.
    pub dlq_dropped:        usize,
}

/// A single DLQ item in the HTTP response.
#[derive(Debug, Serialize)]
pub struct DlqItemResponse {
    /// Unique DLQ item ID.
    pub id:            Uuid,
    /// ID of the event that triggered the action.
    pub event_id:      Uuid,
    /// Entity type (e.g. "Order", "User").
    pub entity_type:   String,
    /// Entity instance ID.
    pub entity_id:     Uuid,
    /// Event type (INSERT, UPDATE, DELETE).
    pub event_type:    String,
    /// Action type that failed (e.g. "webhook", "email").
    pub action_type:   String,
    /// Error message from the last attempt.
    pub error_message: String,
    /// Number of retry attempts made.
    pub attempts:      u32,
}

impl From<&fraiseql_observers::DlqItem> for DlqItemResponse {
    fn from(item: &fraiseql_observers::DlqItem) -> Self {
        Self {
            id:            item.id,
            event_id:      item.event.id,
            entity_type:   item.event.entity_type.clone(),
            entity_id:     item.event.entity_id,
            event_type:    item.event.event_type.as_str().to_string(),
            action_type:   action_type_str(&item.action).to_string(),
            error_message: item.error_message.clone(),
            attempts:      item.attempts,
        }
    }
}

/// Paginated DLQ list response.
#[derive(Debug, Serialize)]
pub struct DlqListResponse {
    /// Items on the current page.
    pub items:  Vec<DlqItemResponse>,
    /// Total number of items matching the filter.
    pub total:  usize,
    /// Limit requested.
    pub limit:  usize,
    /// Offset applied.
    pub offset: usize,
}

/// Query parameters for `GET /api/observers/dlq`.
#[derive(Debug, Deserialize)]
pub struct DlqListQuery {
    /// Maximum items to return (default 50).
    #[serde(default = "default_limit")]
    pub limit:       usize,
    /// Offset for pagination (default 0).
    #[serde(default)]
    pub offset:      usize,
    /// Optional filter: action type (e.g. "webhook").
    pub action:      Option<String>,
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
    pub items_failed:  usize,
    /// Human-readable result message.
    pub message:       String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/observers/delivery/health`
///
/// Returns a summary of observer delivery health including DLQ counts.
pub async fn delivery_health_handler(State(state): State<DlqState>) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let health = runtime.health();
    let dlq = runtime.dlq();

    let summary = DeliveryStatusSummary {
        running:            health.running,
        observer_count:     health.observer_count,
        events_processed:   health.events_processed,
        errors:             health.errors,
        dlq_count:          dlq.count(),
        function_dlq_count: dlq.function_count(),
        dlq_dropped:        dlq.overflow_count(),
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

/// `GET /api/observers/dlq/{id}`
///
/// Returns a single DLQ item by ID.
pub async fn dlq_get_handler(
    State(state): State<DlqState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();

    match dlq.get(id) {
        Some(item) => {
            (StatusCode::OK, Json(serde_json::json!(DlqItemResponse::from(&item)))).into_response()
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "DLQ item not found" })),
        )
            .into_response(),
    }
}

/// Outcome of atomically claiming and re-processing a single DLQ item.
enum RetryOutcome {
    /// No item with this id was present to claim (already retried, or never existed).
    NotFound,
    /// The claimed item was re-dispatched successfully and stays removed.
    Succeeded,
    /// Re-dispatch failed; the item was re-inserted (cap-bypassing) and the
    /// error message is carried back for the response.
    Failed(String),
}

/// Atomically claim a DLQ item and re-dispatch it through the executor.
///
/// The claim ([`InMemoryDlq::try_claim`]) removes the item under a single lock,
/// so two concurrent retries on the same id dispatch the action **at most once**
/// (#344): the winner processes, the loser sees [`RetryOutcome::NotFound`]. On a
/// failed redispatch the item is re-inserted via the **cap-bypassing** path
/// ([`InMemoryDlq::reinsert`]) with its `attempts` bumped, so a failed retry is
/// never silently lost — even if the DLQ refilled to capacity during the claim
/// (#343/#344).
async fn claim_and_process(
    dlq: &InMemoryDlq,
    executor: &fraiseql_observers::ObserverExecutor,
    id: Uuid,
) -> RetryOutcome {
    let Some(mut item) = dlq.try_claim(id) else {
        return RetryOutcome::NotFound;
    };

    match executor.process_event(&item.event).await {
        Ok(_summary) => RetryOutcome::Succeeded,
        Err(e) => {
            item.attempts = item.attempts.saturating_add(1);
            item.error_message = format!("Retry failed: {e}");
            dlq.reinsert(item);
            RetryOutcome::Failed(e.to_string())
        },
    }
}

/// `POST /api/observers/dlq/{id}/retry`
///
/// Atomically claims a single DLQ item and re-processes it through the observer
/// executor. The claim guarantees the action is re-dispatched at most once even
/// under concurrent retries; a failed redispatch re-inserts the item.
pub async fn dlq_retry_handler(
    State(state): State<DlqState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;

    // Acquire the executor before claiming: if it is unavailable we return 503
    // without removing the item.
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

    match claim_and_process(runtime.dlq(), executor, id).await {
        RetryOutcome::NotFound => (
            StatusCode::NOT_FOUND,
            Json(RetryResponse {
                success: false,
                item_id: id,
                message: "DLQ item not found".to_string(),
            }),
        )
            .into_response(),
        RetryOutcome::Succeeded => (
            StatusCode::OK,
            Json(RetryResponse {
                success: true,
                item_id: id,
                message: "Item re-processed successfully".to_string(),
            }),
        )
            .into_response(),
        RetryOutcome::Failed(e) => (
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
/// Re-processes all DLQ items. Each item is claimed atomically (a single-lock
/// remove-and-return), so an item that a per-item retry claims concurrently is
/// not also dispatched by this loop. Successfully retried items stay removed;
/// failed ones are re-inserted.
pub async fn dlq_retry_all_handler(State(state): State<DlqState>) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();

    // Snapshot the ids to drain; each is then claimed atomically below.
    let ids: Vec<Uuid> = dlq.list_all().into_iter().map(|item| item.id).collect();

    if ids.is_empty() {
        return (
            StatusCode::OK,
            Json(RetryAllResponse {
                items_retried: 0,
                items_failed:  0,
                message:       "No items in DLQ".to_string(),
            }),
        );
    }

    let executor_guard = runtime.executor_ref().read().await;
    let Some(executor) = executor_guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RetryAllResponse {
                items_retried: 0,
                items_failed:  ids.len(),
                message:       "Observer executor not available".to_string(),
            }),
        );
    };

    let mut retried = 0;
    let mut failed = 0;

    for id in ids {
        match claim_and_process(dlq, executor, id).await {
            // Claimed and dispatched by a racing per-item retry — not our work.
            RetryOutcome::NotFound => {},
            RetryOutcome::Succeeded => retried += 1,
            RetryOutcome::Failed(e) => {
                tracing::warn!(item_id = %id, error = %e, "DLQ retry failed");
                failed += 1;
            },
        }
    }

    (
        StatusCode::OK,
        Json(RetryAllResponse {
            items_retried: retried,
            items_failed:  failed,
            message:       format!("Batch retry completed: {retried} retried, {failed} failed"),
        }),
    )
}

/// `DELETE /api/observers/dlq/{id}`
///
/// Removes a single DLQ item. Returns 200 when an item was present and removed,
/// 404 when absent. The removal goes through the atomic claim
/// (`InMemoryDlq::try_claim`, #344), so a concurrent retry and delete cannot
/// both act on the same item — exactly one wins, the other gets 404. This is the
/// real backing for the `fraiseql-observers dlq remove` CLI command (#341).
pub async fn dlq_delete_handler(
    State(state): State<DlqState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();

    if dlq.try_claim(id).is_some() {
        (StatusCode::OK, Json(serde_json::json!({ "deleted": id }))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "DLQ item not found" })),
        )
            .into_response()
    }
}

/// DLQ statistics response.
#[derive(Debug, Serialize)]
pub struct DlqStatsResponse {
    /// Number of items currently in the DLQ.
    pub total_items:   usize,
    /// Sum of retry attempts recorded across all current items.
    pub total_retries: u64,
    /// Entries dropped because the DLQ was at capacity (drop-newest, controlled
    /// by `[observers.runtime] max_dlq_size`).
    pub dropped:       usize,
    /// Item counts grouped by the failed action type (e.g. `webhook`, `email`).
    pub by_action:     std::collections::BTreeMap<String, usize>,
}

/// `GET /api/observers/dlq/stats`
///
/// Returns real aggregate statistics computed from the in-memory DLQ — total
/// items, total recorded retry attempts, count dropped at capacity, and a
/// per-action breakdown. Backs the `fraiseql-observers dlq stats` CLI command
/// (#341) with real data instead of fabricated numbers.
pub async fn dlq_stats_handler(State(state): State<DlqState>) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let dlq = runtime.dlq();
    let items = dlq.list_all();

    let total_items = items.len();
    let total_retries: u64 = items.iter().map(|item| u64::from(item.attempts)).sum();
    let mut by_action: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for item in &items {
        *by_action.entry(action_type_str(&item.action).to_string()).or_default() += 1;
    }

    let response = DlqStatsResponse {
        total_items,
        total_retries,
        dropped: dlq.overflow_count(),
        by_action,
    };

    (StatusCode::OK, Json(response))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract a human-readable action type string from an `ActionConfig`.
const fn action_type_str(action: &fraiseql_observers::ActionConfig) -> &'static str {
    action.action_type()
}

#[cfg(test)]
mod tests;
