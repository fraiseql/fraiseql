//! Each admission step, through the transports that run it.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body, routing::post};
use chrono::{Duration, Utc};
use fraiseql_core::{
    db::types::JsonbValue,
    runtime::Executor,
    schema::FieldType,
    security::{AuthenticatedUser, SecurityContext},
    types::UserId,
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use http::{HeaderMap, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{AdmissionPolicy, identify};
use crate::routes::graphql::{
    AppState, TenantExecutorRegistry, graphql_get_handler, graphql_handler,
    tenant_registry::TenantQuota,
};

/// An executor whose `users` query answers one row with this `id`.
fn executor(id: i64) -> Arc<Executor> {
    let schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_users")
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .with_sql_source("v_users")
                .build(),
        )
        .build();
    let adapter =
        FailingAdapter::new().with_response("v_users", vec![JsonbValue::new(json!({ "id": id }))]);
    Arc::new(Executor::new(schema, Arc::new(adapter)))
}

/// `/graphql` on a default executor answering `id: 1`, with tenant `acme` registered on
/// one answering `id: 2`, under `quota`.
fn router(quota: TenantQuota, adjust: impl FnOnce(&TenantExecutorRegistry)) -> Router {
    let mut state = AppState::new(executor(1));
    let registry = TenantExecutorRegistry::new(Arc::clone(&state.executor));
    registry.upsert_with_quota("acme", executor(2), quota);
    adjust(&registry);
    state = state.with_tenant_registry(Arc::new(registry));
    state.graphql_incremental_enabled = true;
    Router::new()
        .route("/graphql", post(graphql_handler).get(graphql_get_handler))
        .with_state(state)
}

/// POST `query` to `/graphql` as tenant `acme`, with `accept` as the Accept header.
async fn post_as_acme(router: &Router, query: &str, accept: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("accept", accept)
        .header("x-tenant-id", "acme")
        .body(Body::from(serde_json::to_vec(&json!({ "query": query })).unwrap()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn a_request_runs_on_its_tenants_executor() {
    let router = router(TenantQuota::default(), |_| {});
    let (status, body) = post_as_acme(&router, "{ users { id } }", "application/json").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["data"]["users"], json!([{ "id": 2 }]), "acme's rows, not the default's");
}

#[tokio::test]
async fn a_suspended_tenant_is_refused() {
    let router = router(TenantQuota::default(), |registry| registry.suspend("acme").unwrap());
    let (status, body) = post_as_acme(&router, "{ users { id } }", "application/json").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}

#[tokio::test]
async fn an_operation_over_its_tenants_cost_budget_is_refused() {
    let quota = TenantQuota {
        cost_budget: Some(0),
        ..TenantQuota::default()
    };
    let router = router(quota, |_| {});
    let (_, body) = post_as_acme(&router, "{ users { id } }", "application/json").await;
    assert!(body.contains("OPERATION_COST_EXCEEDED"), "a refusal, not an answer: {body}");
}

/// Every batch of a streamed delivery runs on the tenant's executor, the first included.
#[tokio::test]
async fn a_streamed_delivery_runs_on_its_tenants_executor() {
    let router = router(TenantQuota::default(), |_| {});
    let (status, body) =
        post_as_acme(&router, "{ users @stream(initialCount: 1) { id } }", "text/event-stream")
            .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains(r#""id":2"#) && !body.contains(r#""id":1"#),
        "acme's rows, not the default's: {body}"
    );
}

fn principal() -> SecurityContext {
    let user = AuthenticatedUser {
        user_id:      UserId::new("u-1"),
        scopes:       Vec::new(),
        expires_at:   Utc::now() + Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::new(),
    };
    SecurityContext::from_user(&user, "req-1".to_string())
}

/// The request's trace is stamped onto its principal where the transport's policy says
/// so, and nowhere else.
#[tokio::test]
async fn a_principal_carries_the_requests_trace_where_its_policy_stamps_it() {
    let state = AppState::new(executor(1));
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".parse().unwrap(),
    );
    for (policy, expected) in [
        (AdmissionPolicy::GRAPHQL, Some("4bf92f3577b34da6a3ce929d0e0e4736")),
        (AdmissionPolicy::ASYNC_OPERATIONS, None),
    ] {
        let identified = identify(&state, &headers, Some(principal()), &policy).await.unwrap();
        let trace = identified.security_context().and_then(SecurityContext::trace_id);
        assert_eq!(trace, expected, "{policy:?}");
    }
}
