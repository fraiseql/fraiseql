//! Integration tests for v2.3.0 server features.
//!
//! Covers:
//!
//! - Schema metadata endpoint (`GET /api/v1/schema/metadata`)
//! - Usage aggregation endpoint (`GET /api/v1/admin/usage`)
//! - Mutation audit tracing end-to-end
//! - Federation plan visualisation (`GET /admin/v1/federation/plan`)
//!
//! Subscription forwarder and `GET /auth/me` tests require additional
//! infrastructure (mock `WebSocket` subgraph, JWT issuance) and are covered
//! separately in dedicated test modules.
//!
//! **Execution engine:** none (in-memory schema + `FailingAdapter`)
//! **Infrastructure:** none (no database, no Redis)
//! **Parallelism:** safe (all tests use `oneshot`, no shared state)

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected

mod common;

use std::sync::Arc;

use axum::{Router, body::Body, routing::get};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::{
    routes::{
        api::{metadata::metadata_handler, usage::usage_handler},
        graphql::AppState,
    },
    usage::{aggregator::UsageAggregator, events::MutationAuditEvent},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_state() -> AppState<FailingAdapter> {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    AppState::new(Arc::new(Executor::new(schema, adapter)))
}

fn make_state_with_usage(usage: Arc<UsageAggregator>) -> AppState<FailingAdapter> {
    make_state().with_usage(usage)
}

async fn get_json(router: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ── Schema Metadata Endpoint ─────────────────────────────────────────────────

/// Integration: metadata endpoint returns 200 with correct envelope structure.
#[tokio::test]
async fn test_metadata_endpoint_returns_200_with_envelope() {
    let router = Router::new()
        .route("/api/v1/schema/metadata", get(metadata_handler::<FailingAdapter>))
        .with_state(make_state());

    let (status, body) = get_json(&router, "/api/v1/schema/metadata").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "success");
    assert!(body["data"]["metadata"].is_object(), "data.metadata must be an object");
}

/// Integration: metadata endpoint returns empty map for a schema with no annotations.
#[tokio::test]
async fn test_metadata_endpoint_empty_schema_returns_empty_map() {
    let router = Router::new()
        .route("/api/v1/schema/metadata", get(metadata_handler::<FailingAdapter>))
        .with_state(make_state());

    let (status, body) = get_json(&router, "/api/v1/schema/metadata").await;

    assert_eq!(status, StatusCode::OK);
    let metadata = &body["data"]["metadata"];
    assert_eq!(
        metadata.as_object().unwrap().len(),
        0,
        "empty schema should produce empty metadata map"
    );
}

/// Integration: metadata endpoint is publicly accessible by default (no auth required).
///
/// The auth gating behaviour (`metadata_require_auth`) is tested at the unit level
/// in `introspection_security_test.rs`.  This test confirms the route exists and is
/// reachable when no OIDC middleware is configured.
#[tokio::test]
async fn test_metadata_endpoint_accessible_without_auth_by_default() {
    let router = Router::new()
        .route("/api/v1/schema/metadata", get(metadata_handler::<FailingAdapter>))
        .with_state(make_state());

    let (status, _) = get_json(&router, "/api/v1/schema/metadata").await;
    assert_eq!(status, StatusCode::OK, "metadata must be reachable without auth by default");
}

// ── Usage Aggregation Endpoint ───────────────────────────────────────────────

fn make_usage_router(usage: Arc<UsageAggregator>) -> Router {
    Router::new()
        .route("/api/v1/admin/usage", get(usage_handler::<FailingAdapter>))
        .with_state(make_state_with_usage(usage))
}

/// Integration: usage endpoint returns empty map for a fresh aggregator.
#[tokio::test]
async fn test_usage_endpoint_empty_aggregator_returns_empty_mutations() {
    let router = make_usage_router(Arc::new(UsageAggregator::new()));

    let (status, body) =
        get_json(&router, "/api/v1/admin/usage?tenant_id=acme&period=2026-05").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tenant_id"], "acme");
    assert_eq!(body["period"], "2026-05");
    let mutations = &body["usage"]["mutations"];
    assert!(mutations.is_object());
    assert_eq!(mutations.as_object().unwrap().len(), 0);
}

/// Integration: usage endpoint returns recorded counters.
#[tokio::test]
async fn test_usage_endpoint_reflects_recorded_events() {
    let usage = Arc::new(UsageAggregator::new());

    // Record 5 User and 3 Order mutations for tenant_a in 2026-05
    for _ in 0..5 {
        usage.record(&MutationAuditEvent::new(
            "create_user",
            "User",
            "create",
            "tenant_a",
            "2026-05",
        ));
    }
    for _ in 0..3 {
        usage.record(&MutationAuditEvent::new(
            "create_order",
            "Order",
            "create",
            "tenant_a",
            "2026-05",
        ));
    }

    let router = make_usage_router(Arc::clone(&usage));
    let (status, body) =
        get_json(&router, "/api/v1/admin/usage?tenant_id=tenant_a&period=2026-05").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["usage"]["mutations"]["User"], 5);
    assert_eq!(body["usage"]["mutations"]["Order"], 3);
}

/// Integration: usage endpoint enforces tenant isolation (`tenant_b` cannot see `tenant_a`'s
/// counters).
#[tokio::test]
async fn test_usage_endpoint_tenant_isolation() {
    let usage = Arc::new(UsageAggregator::new());

    usage.record(&MutationAuditEvent::new("create_user", "User", "create", "tenant_a", "2026-05"));

    let router = make_usage_router(Arc::clone(&usage));

    // tenant_b should see empty results even though tenant_a has data
    let (status, body) =
        get_json(&router, "/api/v1/admin/usage?tenant_id=tenant_b&period=2026-05").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["usage"]["mutations"].as_object().unwrap().len(), 0);
}

/// Integration: usage endpoint returns 400 for invalid period format.
#[tokio::test]
async fn test_usage_endpoint_rejects_invalid_period() {
    let router = make_usage_router(Arc::new(UsageAggregator::new()));

    let (status, body) =
        get_json(&router, "/api/v1/admin/usage?tenant_id=acme&period=invalid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string(), "error response must have an 'error' field");
}

// ── Mutation Audit Tracing End-to-End ────────────────────────────────────────

/// Integration: `MutationAuditLayer` records events when tracing events are emitted.
///
/// This test wires up the full pipeline:
///   `tracing::info!(target: "fraiseql::mutation_audit", ...)` → `MutationAuditLayer` →
/// `UsageAggregator`
#[tokio::test]
async fn test_audit_layer_records_tracing_events() {
    use fraiseql_server::usage::{aggregator::UsageAggregator, layer::MutationAuditLayer};
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    let aggregator = Arc::new(UsageAggregator::new());
    let layer = MutationAuditLayer::new(Arc::clone(&aggregator));

    let subscriber = Registry::default().with(layer);

    // Emit a mutation audit event within this subscriber scope. The layer derives the
    // period from the current month (`Utc::now`), not from any `period` field.
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            target: "fraiseql::mutation_audit",
            mutation_name = "create_user",
            entity_type   = "User",
            operation     = "create",
            tenant_id     = "audit_tenant",
        );
    });

    // Verify the aggregator recorded the event under the current period.
    let period = chrono::Utc::now().format("%Y-%m").to_string();
    let summary = aggregator.query("audit_tenant", &period);
    assert_eq!(
        summary.mutations.get("User").copied(),
        Some(1),
        "MutationAuditLayer should have captured the tracing event"
    );
}

/// Integration: `MutationAuditLayer` ignores events from other tracing targets.
#[tokio::test]
async fn test_audit_layer_ignores_non_mutation_events() {
    use fraiseql_server::usage::{aggregator::UsageAggregator, layer::MutationAuditLayer};
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    let aggregator = Arc::new(UsageAggregator::new());
    let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        // This event uses the wrong target and should NOT be captured
        tracing::info!(
            target: "app::some_other_target",
            mutation_name = "create_user",
            entity_type   = "User",
            operation     = "create",
            tenant_id     = "ignored_tenant",
            period        = "2026-05",
        );
    });

    let summary = aggregator.query("ignored_tenant", "2026-05");
    assert_eq!(
        summary.mutations.get("User").copied(),
        None,
        "wrong-target events must not be counted"
    );
}

/// Integration: audit pipeline end-to-end — emit events then query via HTTP.
#[tokio::test]
async fn test_audit_pipeline_emit_then_query_via_http() {
    use fraiseql_server::usage::{aggregator::UsageAggregator, layer::MutationAuditLayer};
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    let aggregator = Arc::new(UsageAggregator::new());
    let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
    let subscriber = Registry::default().with(layer);

    // Emit 2 events via tracing (the layer derives the period from the current month).
    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..2_u8 {
            tracing::info!(
                target: "fraiseql::mutation_audit",
                mutation_name = "create_product",
                entity_type   = "Product",
                operation     = "create",
                tenant_id     = "shop",
            );
        }
    });

    // Query via the HTTP endpoint, using the current period.
    let period = chrono::Utc::now().format("%Y-%m").to_string();
    let router = make_usage_router(Arc::clone(&aggregator));
    let (status, body) =
        get_json(&router, &format!("/api/v1/admin/usage?tenant_id=shop&period={period}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["usage"]["mutations"]["Product"], 2);
}

// ── Federation Plan Visualisation ────────────────────────────────────────────

#[cfg(feature = "federation")]
mod federation_plan_tests {
    use fraiseql_server::routes::api::federation::plan_handler;

    use super::*;

    /// Integration: federation plan endpoint returns 200 with plan structure for a basic query.
    #[tokio::test]
    async fn test_federation_plan_endpoint_returns_200() {
        let router = Router::new()
            .route("/admin/v1/federation/plan", get(plan_handler::<FailingAdapter>))
            .with_state(make_state());

        let query = urlencoding::encode("{ __typename }");
        let uri = format!("/admin/v1/federation/plan?query={query}");
        let (status, body) = get_json(&router, &uri).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "success");
        assert!(body["data"].is_object(), "response must contain a data object");
    }

    /// Integration: federation plan endpoint returns 400 for missing query parameter.
    #[tokio::test]
    async fn test_federation_plan_endpoint_missing_query_returns_400() {
        let router = Router::new()
            .route("/admin/v1/federation/plan", get(plan_handler::<FailingAdapter>))
            .with_state(make_state());

        let (status, _body) = get_json(&router, "/admin/v1/federation/plan").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
