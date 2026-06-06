use super::*;

#[test]
fn delivery_status_summary_serializes() {
    let summary = DeliveryStatusSummary {
        running:          true,
        observer_count:   3,
        events_processed: 42,
        errors:           1,
        dlq_count:        2,
        dlq_dropped:      5,
    };
    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json["running"], true);
    assert_eq!(json["dlq_count"], 2);
    assert_eq!(json["dlq_dropped"], 5);
    assert_eq!(json["events_processed"], 42);
}

#[test]
fn dlq_list_response_serializes() {
    let response = DlqListResponse {
        items:  vec![],
        total:  0,
        limit:  50,
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
        items_failed:  1,
        message:       "done".to_string(),
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["items_retried"], 5);
    assert_eq!(json["items_failed"], 1);
}

#[test]
fn default_limit_is_50() {
    assert_eq!(default_limit(), 50);
}

// ── #344: concurrent retry dispatches at most once ──────────────────────────

/// Two concurrent `POST /dlq/{id}/retry` on the same item must dispatch the
/// action at most once: exactly one response succeeds (200) and the other finds
/// nothing to claim (404). The item ends up removed.
#[tokio::test]
async fn concurrent_retry_dispatches_at_most_once() {
    use std::{collections::HashMap, sync::Arc};

    use axum::{
        extract::{Path, State},
        response::IntoResponse,
    };
    use fraiseql_observers::{
        ActionConfig, DeadLetterQueue, EntityEvent, EventKind, EventMatcher, ObserverExecutor,
    };
    use sqlx::PgPool;
    use tokio::sync::RwLock;

    use crate::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};

    let pool = PgPool::connect_lazy("postgres://test:test@localhost/test").expect("lazy pool");
    let runtime = ObserverRuntime::new(ObserverRuntimeConfig::new(pool));

    // One failed item in the DLQ.
    let event = EntityEvent::new(
        EventKind::Created,
        "T".to_string(),
        Uuid::new_v4(),
        serde_json::json!({}),
    );
    let action = ActionConfig::Webhook {
        url:           Some("http://localhost/hook".to_string()),
        url_env:       None,
        headers:       HashMap::new(),
        body_template: None,
    };
    let id = runtime.dlq().push(event, action, "boom".to_string()).await.expect("push");

    // Inject a no-op executor (empty matcher → process_event is a successful
    // no-op) that shares the runtime's DLQ.
    let matcher = EventMatcher::build(HashMap::new()).expect("build matcher");
    let shared_dlq: Arc<dyn DeadLetterQueue> = runtime.dlq().clone();
    let executor = Arc::new(ObserverExecutor::new(matcher, shared_dlq));
    *runtime.executor_ref().write().await = Some(executor);

    let state = DlqState {
        runtime: Arc::new(RwLock::new(runtime)),
    };

    // Fire two concurrent retries on the same id.
    let (r1, r2) = tokio::join!(
        dlq_retry_handler(State(state.clone()), Path(id)),
        dlq_retry_handler(State(state.clone()), Path(id)),
    );

    let mut codes = [
        r1.into_response().status().as_u16(),
        r2.into_response().status().as_u16(),
    ];
    codes.sort_unstable();
    assert_eq!(codes, [200, 404], "one retry succeeds, the other finds nothing to claim");

    assert_eq!(
        state.runtime.read().await.dlq().count(),
        0,
        "the item is claimed and dispatched exactly once, then gone"
    );
}
