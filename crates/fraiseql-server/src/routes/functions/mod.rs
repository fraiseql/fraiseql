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
//! [`FunctionResult`] on success.
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
use sha2::Digest as _;
use serde_json::json;

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
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": e.to_string() }).to_string(),
            )
                .into_response();
        }
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
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": e.to_string() }).to_string(),
        )
            .into_response(),
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
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(missing_docs)] // Reason: test code

    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use fraiseql_functions::{InMemoryFunctionStore, types::RuntimeType};
    use tower::ServiceExt as _;

    use super::*;

    /// Build a test router with a given store and noop runtime.
    fn make_test_state(store: Arc<dyn FunctionStore>) -> FunctionsRouteState {
        struct NoopRuntime;
        impl SendFunctionRuntime for NoopRuntime {
            fn invoke_raw(
                &self,
                _module: &FunctionModule,
                _event: EventPayload,
                _limits: ResourceLimits,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = fraiseql_error::Result<fraiseql_functions::types::FunctionResult>> + Send + '_>,
            > {
                use fraiseql_functions::types::{FunctionResult, LogEntry};
                Box::pin(async {
                    Ok(FunctionResult {
                        value: Some(serde_json::json!({"ok": true})),
                        logs: Vec::<LogEntry>::new(),
                        duration: std::time::Duration::from_millis(1),
                        memory_peak_bytes: 0,
                    })
                })
            }
            fn supported_extensions(&self) -> &[&str] {
                &[]
            }
            fn supports_hot_reload(&self) -> bool {
                false
            }
            fn name(&self) -> &'static str {
                "noop"
            }
        }

        FunctionsRouteState {
            store,
            runtime: Arc::new(NoopRuntime),
        }
    }

    #[tokio::test]
    async fn test_invoke_returns_404_for_unknown_function() {
        let store = Arc::new(InMemoryFunctionStore::new());
        let state = make_test_state(store);
        let app = functions_router(state);

        let req = Request::builder()
            .method(Method::POST)
            .uri("/functions/v1/ghost")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_invoke_returns_200_for_known_function() {
        let store = Arc::new(InMemoryFunctionStore::new());
        store
            .store_function("echo", RuntimeType::Wasm, bytes::Bytes::from_static(b"fake"))
            .await
            .unwrap();

        let state = make_test_state(store);
        let app = functions_router(state);

        let req = Request::builder()
            .method(Method::POST)
            .uri("/functions/v1/echo")
            .body(Body::from(r#"{"hello":"world"}"#))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
