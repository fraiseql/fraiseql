//! Tests for `routes/functions/` module.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use fraiseql_functions::{
    FunctionStore,
    InMemoryFunctionStore,
    runtime::SendFunctionRuntime,
    types::{EventPayload, FunctionModule, FunctionResult, LogEntry, ResourceLimits, RuntimeType},
};
use tower::ServiceExt as _;

use super::{FunctionsRouteState, functions_router};

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
            Box<
                dyn std::future::Future<
                        Output = fraiseql_error::Result<FunctionResult>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(FunctionResult {
                    value:             Some(serde_json::json!({"ok": true})),
                    logs:              Vec::<LogEntry>::new(),
                    duration:          std::time::Duration::from_millis(1),
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
