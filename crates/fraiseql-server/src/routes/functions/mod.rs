//! Edge-function HTTP routes.
//!
//! Mounts a single invocation endpoint:
//!
//! | Method | Path | Operation |
//! |--------|------|-----------|
//! | `POST` | `/functions/v1/{name}` | Invoke a deployed function by name |
//!
//! The request body is forwarded as the `data` field of an [`EventPayload`]
//! with `trigger_type = "http"`. The response is the JSON-encoded
//! `FunctionResult` on success.
//!
//! Routes are only mounted when a store and runtime have been attached via
//! [`Server::with_functions`](crate::server::Server::with_functions).

use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use fraiseql_functions::{
    FunctionStore,
    runtime::SendFunctionRuntime,
    types::{EventPayload, FunctionModule, ResourceLimits},
};
use serde_json::json;
use sha2::Digest as _;

/// Shared state for function route handlers.
#[derive(Clone)]
pub struct FunctionsRouteState {
    /// Deployment store — provides bytecode lookup by function name.
    pub store: Arc<dyn FunctionStore>,
    /// Execution runtime — runs the bytecode and returns a result.
    pub runtime: Arc<dyn SendFunctionRuntime>,
}

/// `POST /functions/v1/{name}` — invoke a deployed function.
///
/// The request body is treated as raw JSON and forwarded as the event payload's
/// `data` field. An empty body is passed as `null`.
///
/// # Responses
///
/// | Status | Body | Meaning |
/// |--------|------|---------|
/// | 200 | `FunctionResult` JSON | Successful invocation |
/// | 404 | error message | No active function with that name |
/// | 500 | error message | Load or execution error |
pub async fn invoke_function_handler(
    State(state): State<FunctionsRouteState>,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    // Resolve function record (bytecode + metadata) from the store.
    let record = match state.store.get_function(&name).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                json!({ "error": format!("function '{name}' not found") }).to_string(),
            )
                .into_response();
        },
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": e.to_string() }).to_string(),
            )
                .into_response();
        },
    };

    // Convert the stored record into an executable module.
    let module = FunctionModule {
        name: record.name.clone(),
        source_hash: format!("{:x}", sha2::Sha256::digest(&record.bytecode)),
        bytecode: record.bytecode,
        runtime: record.runtime,
    };

    // Parse the request body as JSON event data, defaulting to null for empty bodies.
    let data: serde_json::Value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
    };

    let event = EventPayload {
        trigger_type: "http".to_string(),
        entity: name.clone(),
        event_kind: "invoke".to_string(),
        data,
        timestamp: chrono::Utc::now(),
    };

    // Execute the function and serialize the result.
    match state.runtime.invoke_raw(&module, event, ResourceLimits::default()).await {
        Ok(result) => {
            let body = json!({
                "value": result.value,
                "logs": result.logs,
                "duration_ms": result.duration.as_millis(),
                "memory_peak_bytes": result.memory_peak_bytes,
            });
            (StatusCode::OK, body.to_string()).into_response()
        },
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e.to_string() }).to_string())
                .into_response()
        },
    }
}

/// Build the functions sub-router and attach `state` to all routes.
///
/// Register this router with [`Router::merge`] after the main application
/// router is built. All routes use the `/functions/v1/` prefix.
pub fn functions_router(state: FunctionsRouteState) -> Router {
    Router::new()
        .route("/functions/v1/{name}", post(invoke_function_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests;
