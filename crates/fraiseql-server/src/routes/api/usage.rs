//! Usage statistics API endpoint.
//!
//! Exposes in-memory mutation counters accumulated by the `MutationAuditLayer`
//! tracing subscriber.  Counters are keyed by `(tenant_id, period, entity_type)`
//! and reset to zero on process restart.
//!
//! ## Endpoint
//!
//! ```text
//! GET /api/v1/admin/usage?tenant_id=<str>&period=<YYYY-MM>
//! ```
//!
//! Protected by the admin bearer token (same as all other admin read routes).
//!
//! ## Response
//!
//! ```json
//! {
//!   "tenant_id": "acme",
//!   "period": "2026-05",
//!   "usage": {
//!     "mutations": { "User": 42, "Order": 7 }
//!   }
//! }
//! ```
//!
//! Unknown `tenant_id` / `period` combinations return 200 with
//! `"usage": { "mutations": {} }` — never 404.
//!
//! Invalid `period` (not `YYYY-MM`) returns 400 with
//! `{"error": "invalid period format"}`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::{
    routes::graphql::AppState,
    usage::aggregator::{UsageSummary, validate_period},
};

// ── Query parameters ───────────────────────────────────────────────────────

/// Query parameters for the usage endpoint.
#[derive(Debug, Deserialize)]
pub struct UsageQueryParams {
    /// Tenant identifier to query.
    pub tenant_id: String,
    /// Billing period in `YYYY-MM` format (e.g. `"2026-05"`).
    pub period:    String,
}

// ── Response types ─────────────────────────────────────────────────────────

/// Successful usage query response.
#[derive(Debug, Serialize)]
pub struct UsageResponse {
    /// The queried tenant identifier.
    pub tenant_id: String,
    /// The queried period (`YYYY-MM`).
    pub period:    String,
    /// Mutation counts for the queried period.
    pub usage:     UsageSummary,
}

// ── Handler ────────────────────────────────────────────────────────────────

/// Query mutation usage statistics for a tenant and period.
///
/// Returns 400 when `period` is not a valid `YYYY-MM` string.  Returns 200
/// with empty `mutations` for unknown tenant/period combinations.
///
/// # Errors
///
/// Returns `(400, {"error": "invalid period format"})` when the `period`
/// query parameter is not in `YYYY-MM` format.
pub async fn usage_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<UsageResponse>, (StatusCode, Json<serde_json::Value>)> {
    if !validate_period(&params.period) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid period format"})),
        ));
    }

    let usage = state.usage.query(&params.tenant_id, &params.period);

    Ok(Json(UsageResponse {
        tenant_id: params.tenant_id,
        period:    params.period,
        usage,
    }))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode},
        middleware,
        routing::get,
    };
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::{
        middleware::{BearerAuthState, bearer_auth_middleware},
        usage::aggregator::UsageAggregator,
    };

    // ── Stub adapter ──────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct StubAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    // ── Test helpers ──────────────────────────────────────────────────────

    fn make_state_with_usage(usage: Arc<UsageAggregator>) -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor).with_usage(usage)
    }

    fn make_router(usage: Arc<UsageAggregator>) -> Router {
        let state = make_state_with_usage(usage);
        Router::new()
            .route("/api/v1/admin/usage", get(usage_handler::<StubAdapter>))
            .with_state(state)
    }

    fn make_authed_router(usage: Arc<UsageAggregator>) -> Router {
        let state = make_state_with_usage(usage);
        let auth_state = BearerAuthState::new("secret-token".to_string());
        Router::new()
            .route("/api/v1/admin/usage", get(usage_handler::<StubAdapter>))
            .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
            .with_state(state)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_usage_invalid_period_returns_400() {
        let router = make_router(Arc::new(UsageAggregator::new()));

        for bad_period in &["2026", "26-04", "2026/04", "2026-13", "2026-00", ""] {
            let req = Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/v1/admin/usage?tenant_id=acme&period={bad_period}"
                ))
                .body(Body::empty())
                .unwrap();

            let resp = router.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "expected 400 for period {bad_period:?}"
            );

            let json = body_json(resp).await;
            assert_eq!(json["error"], "invalid period format");
        }
    }

    #[tokio::test]
    async fn test_usage_happy_path_response_shape() {
        let usage = Arc::new(UsageAggregator::new());
        // Pre-populate with 3 User mutations and 2 Order mutations for acme in 2026-05.
        let event = |entity: &str| crate::usage::events::MutationAuditEvent {
            mutation_name: format!("create_{entity}"),
            entity_type:   entity.to_owned(),
            operation:     "create".to_owned(),
            tenant_id:     "acme".to_owned(),
            period:        "2026-05".to_owned(),
        };
        for _ in 0..3 {
            usage.record(&event("User"));
        }
        for _ in 0..2 {
            usage.record(&event("Order"));
        }

        let router = make_router(Arc::clone(&usage));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert_eq!(json["tenant_id"], "acme");
        assert_eq!(json["period"], "2026-05");
        assert_eq!(json["usage"]["mutations"]["User"], 3);
        assert_eq!(json["usage"]["mutations"]["Order"], 2);
    }

    #[tokio::test]
    async fn test_usage_unknown_tenant_returns_empty_mutations() {
        let usage = Arc::new(UsageAggregator::new());
        let router = make_router(Arc::clone(&usage));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=nobody&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert_eq!(json["tenant_id"], "nobody");
        assert_eq!(json["period"], "2026-05");
        // mutations must be an empty object, not null or missing
        assert!(json["usage"]["mutations"].is_object());
        assert_eq!(json["usage"]["mutations"].as_object().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_usage_unknown_period_returns_empty_mutations() {
        let usage = Arc::new(UsageAggregator::new());
        // Populate one period, query a different one.
        usage.record(&crate::usage::events::MutationAuditEvent {
            mutation_name: "create_user".to_owned(),
            entity_type:   "User".to_owned(),
            operation:     "create".to_owned(),
            tenant_id:     "acme".to_owned(),
            period:        "2026-04".to_owned(),
        });

        let router = make_router(usage);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert!(json["usage"]["mutations"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_usage_unauthenticated_returns_401() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        // No Authorization header → bearer auth middleware rejects.
        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_usage_wrong_token_returns_403() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        // Wrong token → 403 Forbidden (bearer auth middleware distinguishes
        // missing header (401) from invalid token (403)).
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_usage_correct_token_returns_200() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .header("Authorization", "Bearer secret-token")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
