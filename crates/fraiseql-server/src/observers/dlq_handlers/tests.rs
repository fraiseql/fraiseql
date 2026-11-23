use super::*;

/// A request to the DLQ routes as the operator, through the router: each handler is reached
/// by its extractors, `OperatorOnly` among them, as a server reaches it.
async fn as_operator(
    state: DlqState,
    method: axum::http::Method,
    uri: &str,
) -> axum::response::Response {
    use tower::ServiceExt;

    let operator = fraiseql_core::security::AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new("admin-user"),
        scopes:       vec!["fraiseql:admin".to_owned()],
        expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: std::collections::HashMap::new(),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    };
    crate::observers::observer_dlq_routes(state)
        .layer(axum::Extension(crate::middleware::AuthUser(operator)))
        .layer(axum::Extension(crate::middleware::TenantClaim(Arc::from("tenant_id"))))
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible")
}

#[test]
fn delivery_status_summary_serializes() {
    let summary = DeliveryStatusSummary {
        running:            true,
        observer_count:     3,
        events_processed:   42,
        errors:             1,
        dlq_count:          2,
        function_dlq_count: 4,
        dlq_dropped:        5,
    };
    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json["running"], true);
    assert_eq!(json["dlq_count"], 2);
    assert_eq!(json["function_dlq_count"], 4);
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

    use axum::{http::Method, response::IntoResponse};
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
        url:                Some("http://localhost/hook".to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     None,
        signing_secret_env: None,
        headers_env:        std::collections::HashMap::new(),
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
    let retry = format!("/dlq/{id}/retry");
    let (r1, r2) = tokio::join!(
        as_operator(state.clone(), Method::POST, &retry),
        as_operator(state.clone(), Method::POST, &retry),
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

// ── #341: DELETE + stats endpoints ──────────────────────────────────────────

mod dlq_endpoints {
    use std::{collections::HashMap, sync::Arc};

    use axum::http::{Method, StatusCode};
    use fraiseql_observers::{ActionConfig, DeadLetterQueue, EntityEvent, EventKind};
    use sqlx::PgPool;
    use tokio::sync::RwLock;
    use uuid::Uuid;

    use super::{super::DlqState, as_operator};
    use crate::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};

    fn webhook_action() -> ActionConfig {
        ActionConfig::Webhook {
            url:                Some("http://localhost/hook".to_string()),
            url_env:            None,
            method:             None,
            headers:            HashMap::new(),
            body_template:      None,
            signing_secret:     None,
            signing_secret_env: None,
            headers_env:        std::collections::HashMap::new(),
        }
    }

    fn test_event() -> EntityEvent {
        EntityEvent::new(EventKind::Created, "T".to_string(), Uuid::new_v4(), serde_json::json!({}))
    }

    fn empty_runtime() -> ObserverRuntime {
        // A lazy pool never connects; the DLQ is in-memory, so these endpoint
        // tests need no database.
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test").expect("lazy pool");
        ObserverRuntime::new(ObserverRuntimeConfig::new(pool))
    }

    #[tokio::test]
    async fn delete_present_item_returns_200_and_removes() {
        let runtime = empty_runtime();
        let id = runtime
            .dlq()
            .push(test_event(), webhook_action(), "boom".to_string())
            .await
            .expect("push");
        let state = DlqState {
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let resp = as_operator(state.clone(), Method::DELETE, &format!("/dlq/{id}")).await;
        assert_eq!(resp.status(), StatusCode::OK, "deleting a present item returns 200");
        assert_eq!(state.runtime.read().await.dlq().count(), 0, "the item is removed");
    }

    #[tokio::test]
    async fn delete_absent_item_returns_404() {
        let state = DlqState {
            runtime: Arc::new(RwLock::new(empty_runtime())),
        };

        let resp = as_operator(state, Method::DELETE, &format!("/dlq/{}", Uuid::new_v4())).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "deleting an absent item returns 404");
    }

    #[tokio::test]
    async fn stats_reports_real_counts() {
        let runtime = empty_runtime();
        // Two webhook failures; bump one's attempts so total_retries is non-zero.
        runtime
            .dlq()
            .push(test_event(), webhook_action(), "boom".to_string())
            .await
            .expect("push");
        let id2 = runtime
            .dlq()
            .push(test_event(), webhook_action(), "boom".to_string())
            .await
            .expect("push");
        runtime.dlq().mark_retry_failed(id2, "again").await.expect("mark");
        let state = DlqState {
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let resp = as_operator(state, Method::GET, "/dlq/stats").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(json["total_items"], 2, "two items in the DLQ");
        assert_eq!(json["total_retries"], 1, "one item had a recorded retry attempt");
        assert_eq!(json["by_action"]["webhook"], 2, "both failures are webhook actions");
    }
}
