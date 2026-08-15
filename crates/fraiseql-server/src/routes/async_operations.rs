//! HTTP surface for durable long-running operations (#391):
//! `POST /operations/v1/{operation}` · `GET /operations/v1/{op_id}` ·
//! `DELETE /operations/v1/{op_id}`.
//!
//! Mounted only when `[async_operations]` is configured (a new surface is an
//! operator decision), behind the deployment's configured auth layer — and the
//! handlers additionally hard-require an authenticated principal: a submission
//! snapshots the caller's `SecurityContext` for the worker, so an anonymous
//! submission has nothing to execute as.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::graphql::{AppState, tenant_dispatch};
use crate::{
    async_operations::AsyncOperationsRuntime,
    extractors::OptionalSecurityContext,
    routes::idempotency::{IdempotencyCheck, IdempotencyScope, StoredResponse, hash_body},
};

/// Router state: the shared runtime plus the app state (tenant resolution,
/// idempotency store, cost budgets).
#[derive(Clone)]
pub struct AsyncOperationsState<A: DatabaseAdapter> {
    /// Store + validated configuration.
    pub runtime: AsyncOperationsRuntime,
    /// The application state (same instance the GraphQL transport uses).
    pub app:     AppState<A>,
}

/// Build the `/operations/v1` router.
///
/// One route registration for all three verbs: axum refuses two registrations
/// of the same path shape under different capture names (a `Router::route`
/// panic at boot — the #316 class the route-syntax gate exists for). `POST`
/// reads the capture as the operation name; `GET`/`DELETE` parse it as the
/// operation id.
pub fn router<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AsyncOperationsState<A>,
) -> Router {
    Router::new()
        .route(
            "/operations/v1/{id_or_operation}",
            post(submit::<A>).get(status::<A>).delete(cancel::<A>),
        )
        .with_state(state)
}

#[cfg(test)]
mod router_construction {
    use super::*;

    /// axum validates path-capture syntax inside `Router::route`, so a bad
    /// literal panics HERE, in `cargo test`, not at first server boot (the
    /// axum-bump checklist's construction-test rule).
    #[tokio::test]
    #[allow(clippy::unwrap_used)] // Reason: test code — a lazy-pool ctor error must surface
    async fn async_operations_router_constructs() {
        use std::sync::Arc;

        use fraiseql_core::runtime::Executor;
        use fraiseql_test_utils::failing_adapter::FailingAdapter;

        // Construction needs state but never a live database: the pool is
        // lazy and the adapter is a stub.
        let app = AppState::new(Arc::new(Executor::new(
            fraiseql_core::schema::CompiledSchema::new(),
            Arc::new(FailingAdapter::new()),
        )));
        let runtime = AsyncOperationsRuntime {
            store:  Arc::new(crate::async_operations::AsyncOperationStore::new(
                sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap(),
            )),
            config: Arc::new(crate::server_config::AsyncOperationsConfig::default()),
        };
        let _router = router(AsyncOperationsState { runtime, app });
    }
}

/// Submission body: the GraphQL document to execute asynchronously.
#[derive(Debug, Deserialize)]
struct SubmitBody {
    /// The GraphQL document; its root field must equal the path's `{operation}`
    /// and be on the configured allowlist.
    query:     String,
    /// Document variables.
    #[serde(default)]
    variables: Option<Value>,
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Project a stored operation row into the status envelope.
fn op_json(op: &crate::async_operations::AsyncOperation) -> Value {
    json!({
        "op_id": op.op_id,
        "operation": op.operation,
        "status": op.state,
        "cancellation_requested": op.cancellation_requested,
        "attempts": op.attempts,
        "created_at": op.created_at,
        "started_at": op.started_at,
        "finished_at": op.finished_at,
        "result": op.result,
        "error": op.error,
    })
}

async fn submit<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AsyncOperationsState<A>>,
    Path(operation): Path<String>,
    OptionalSecurityContext(ctx): OptionalSecurityContext,
    headers: HeaderMap,
    Json(body): Json<SubmitBody>,
) -> Response {
    // A submission is executed later, as the submitter — no principal, nothing
    // to execute as. Refused outright, never queued unattributed.
    let Some(ctx) = ctx else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "async operations require an authenticated caller — the submission snapshots your \
             security context for the background execution",
        );
    };

    // The allowlist is the operator's explicit surface (fail-closed).
    if !state.runtime.config.operations.iter().any(|o| o == &operation) {
        return error_response(
            StatusCode::NOT_FOUND,
            "unknown async operation — the [async_operations] allowlist does not include it",
        );
    }

    // The document must parse, be a query or mutation (a subscription cannot
    // fire-and-poll), and its root field must be the operation the path names —
    // one identifier for routing, allowlisting and storage, never two (#857's
    // lesson).
    let parsed = match fraiseql_core::graphql::parse_query(&body.query) {
        Ok(p) => p,
        Err(e) => {
            return error_response(StatusCode::BAD_REQUEST, &format!("invalid GraphQL: {e}"));
        },
    };
    if parsed.operation_type == "subscription" {
        return error_response(
            StatusCode::BAD_REQUEST,
            "subscriptions cannot run as async operations — they have no final result to poll",
        );
    }
    if parsed.root_field != operation {
        return error_response(
            StatusCode::BAD_REQUEST,
            "the document's root field does not match the operation named in the path",
        );
    }

    let tenant_key = match tenant_dispatch::resolve_tenant_key(&state.app, Some(&ctx), &headers) {
        Ok(k) => k,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    // Idempotent submission (#747): the same Idempotency-Key with the same body
    // replays the original response — the same op_id, not a duplicate
    // operation. A different body under the same key is a conflict.
    let idempotency = headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(|k| {
        let scope = IdempotencyScope {
            tenant: tenant_key.clone(),
            method: "POST".to_string(),
            path:   format!("/operations/v1/{operation}"),
        };
        let body_hash = hash_body(&json!({ "query": body.query, "variables": body.variables }));
        (scope.key(k), body_hash)
    });
    if let Some((ref key, body_hash)) = idempotency {
        match state.app.idempotency_store.check(key, body_hash).await {
            IdempotencyCheck::Replay(stored) => {
                let status = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::ACCEPTED);
                return (status, Json(stored.body.unwrap_or(Value::Null))).into_response();
            },
            IdempotencyCheck::Conflict => {
                return error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Idempotency-Key was already used with a different request body",
                );
            },
            IdempotencyCheck::New => {},
        }
    }

    // Cost is charged at submission (#379/#391): a queue must not be a budget
    // bypass, and a never-executable submission should be refused before it is
    // stored.
    let cost = tenant_dispatch::estimate_request_cost(
        &body.query,
        body.variables.as_ref(),
        &state.app.executor(),
    );
    if let Err(e) =
        tenant_dispatch::charge_cost_budget(&state.app, tenant_key.as_deref(), Some(&ctx), cost)
    {
        return error_response(StatusCode::TOO_MANY_REQUESTS, &e.to_string());
    }

    let ctx_json = match serde_json::to_value(&ctx) {
        Ok(v) => v,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("could not snapshot the security context: {e}"),
            );
        },
    };
    let op_id = match state
        .runtime
        .store
        .submit(
            tenant_key.as_deref(),
            ctx.user_id.as_str(),
            &operation,
            &body.query,
            body.variables.as_ref(),
            &ctx_json,
            state.runtime.config.max_attempts,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let response_body = json!({ "op_id": op_id, "status": "queued" });
    if let Some((key, body_hash)) = idempotency {
        state
            .app
            .idempotency_store
            .store(
                key,
                body_hash,
                StoredResponse {
                    status:  StatusCode::ACCEPTED.as_u16(),
                    headers: Vec::new(),
                    body:    Some(response_body.clone()),
                },
            )
            .await;
    }
    (StatusCode::ACCEPTED, Json(response_body)).into_response()
}

async fn status<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AsyncOperationsState<A>>,
    Path(op_id): Path<String>,
    OptionalSecurityContext(ctx): OptionalSecurityContext,
    headers: HeaderMap,
) -> Response {
    let (ctx, op_id) = match caller_and_id(ctx, &op_id) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };
    let tenant_key = match tenant_dispatch::resolve_tenant_key(&state.app, Some(&ctx), &headers) {
        Ok(k) => k,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    // The status is READ from the stored row — never inferred (P19 mode 6) —
    // and scoped to the submitter, so another principal's op_id reads as
    // absent rather than becoming an existence oracle.
    match state
        .runtime
        .store
        .get_scoped(op_id, ctx.user_id.as_str(), tenant_key.as_deref())
        .await
    {
        Ok(Some(op)) => (StatusCode::OK, Json(op_json(&op))).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "no such operation"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn cancel<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AsyncOperationsState<A>>,
    Path(op_id): Path<String>,
    OptionalSecurityContext(ctx): OptionalSecurityContext,
    headers: HeaderMap,
) -> Response {
    let (ctx, op_id) = match caller_and_id(ctx, &op_id) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };
    let tenant_key = match tenant_dispatch::resolve_tenant_key(&state.app, Some(&ctx), &headers) {
        Ok(k) => k,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let submitter = ctx.user_id.as_str();
    let tenant = tenant_key.as_deref();

    // Truthful cancellation (#746): a queued operation is cancelled outright
    // and reported as such; a running one gets a REQUEST (honoured at the
    // worker's next safe point) and is reported as exactly that — never as an
    // accomplished cancellation.
    match state.runtime.store.cancel_queued(op_id, submitter, tenant).await {
        Ok(true) => {
            return (StatusCode::OK, Json(json!({ "op_id": op_id, "status": "cancelled" })))
                .into_response();
        },
        Ok(false) => {},
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
    match state.runtime.store.request_cancel(op_id, submitter, tenant).await {
        Ok(true) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "op_id": op_id,
                "status": "cancel_requested",
                "detail": "the operation is running; cancellation is honoured at the next safe \
                           point — poll the status to observe the outcome",
            })),
        )
            .into_response(),
        Ok(false) => match state.runtime.store.get_scoped(op_id, submitter, tenant).await {
            Ok(Some(op)) => error_response(
                StatusCode::CONFLICT,
                &format!("operation is already terminal ({}); nothing to cancel", op.state),
            ),
            Ok(None) => error_response(StatusCode::NOT_FOUND, "no such operation"),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Shared guard: an authenticated caller and a UUID-shaped id.
#[allow(clippy::result_large_err)] // Reason: the Err IS the handler's HTTP response; boxing it buys nothing
fn caller_and_id(
    ctx: Option<fraiseql_core::security::SecurityContext>,
    op_id: &str,
) -> std::result::Result<(fraiseql_core::security::SecurityContext, Uuid), Response> {
    let Some(ctx) = ctx else {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "async operations require an authenticated caller",
        ));
    };
    let Ok(op_id) = Uuid::parse_str(op_id) else {
        return Err(error_response(StatusCode::BAD_REQUEST, "op_id must be a UUID"));
    };
    Ok((ctx, op_id))
}
