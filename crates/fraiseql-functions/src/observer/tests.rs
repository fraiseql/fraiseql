//! Tests for `FunctionObserver` dispatch logic

#![allow(clippy::unwrap_used)]  // Reason: tests use unwrap for clarity

use crate::{
    host::NoopHostContext, observer::FunctionObserver, EventPayload, FunctionModule,
    ResourceLimits, RuntimeType,
};
use chrono::Utc;

/// Helper to create a test event payload
fn test_event() -> EventPayload {
    EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"value": 42}),
        timestamp: Utc::now(),
    }
}

#[tokio::test]
#[cfg(feature = "runtime-deno")]
async fn test_function_observer_dispatches_js_to_deno() {
    // Create a simple JS function
    let source = "export default async (event) => event;".to_string();

    let module = FunctionModule::from_source("test_js".to_string(), source, RuntimeType::Deno);

    // Create observer and register Deno runtime
    let mut observer = FunctionObserver::new();
    let deno_runtime = crate::runtime::deno::DenoRuntime::new(&crate::runtime::deno::DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, deno_runtime);

    // Execute through observer
    let event = test_event();
    let event_json = serde_json::to_value(&event).unwrap();
    let result = observer
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and result should come from Deno runtime
    assert!(result.is_ok(), "Observer should dispatch to Deno runtime");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_json));
}

#[tokio::test]
#[cfg(feature = "runtime-deno")]
async fn test_function_observer_dispatches_ts_to_deno() {
    // Create a TypeScript function
    let source = "export default async (event) => event;".to_string();

    let module = FunctionModule::from_source("test_ts".to_string(), source, RuntimeType::Deno);

    // Create observer and register Deno runtime
    let mut observer = FunctionObserver::new();
    let deno_runtime = crate::runtime::deno::DenoRuntime::new(&crate::runtime::deno::DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, deno_runtime);

    // Execute through observer
    let event = test_event();
    let event_json = serde_json::to_value(&event).unwrap();
    let result = observer
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and result should come from Deno runtime
    assert!(result.is_ok(), "Observer should dispatch to Deno runtime for TypeScript");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_json));
}

#[tokio::test]
#[cfg(all(feature = "runtime-wasm", feature = "runtime-deno"))]
async fn test_function_observer_wasm_and_deno_coexist() {
    // Create a simple JS function
    let js_source = "export default async (event) => event;".to_string();
    let js_module = FunctionModule::from_source("test_js".to_string(), js_source, RuntimeType::Deno);

    // Create observer and register both runtimes
    let mut observer = FunctionObserver::new();
    let deno_runtime = crate::runtime::deno::DenoRuntime::new(&crate::runtime::deno::DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, deno_runtime);

    let wasm_runtime = crate::runtime::wasm::WasmRuntime::new(&crate::runtime::wasm::WasmConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Wasm, wasm_runtime);

    // Execute JS (Deno) function
    let event = test_event();
    let event_json = serde_json::to_value(&event).unwrap();
    let js_result = observer
        .invoke(
            &js_module,
            event.clone(),
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should succeed with Deno
    assert!(js_result.is_ok(), "Observer should dispatch JS to Deno runtime");
    let js_result = js_result.unwrap();
    assert_eq!(js_result.value, Some(event_json));
}

#[tokio::test]
async fn test_function_observer_unknown_runtime_returns_error() {
    // Create a module with Deno runtime type
    let source = "export default async (event) => event;".to_string();
    let module = FunctionModule::from_source("test_unknown".to_string(), source, RuntimeType::Deno);

    // Create observer WITHOUT registering Deno runtime
    let observer = FunctionObserver::new();

    // Execute should fail with Unsupported error
    let event = test_event();
    let result = observer
        .invoke(
            &module,
            event.clone(),
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should fail because no runtime is registered
    assert!(result.is_err(), "Observer should return error for unregistered runtime");
    let err = result.unwrap_err();
    assert!(
        matches!(err, fraiseql_error::FraiseQLError::Unsupported { .. }),
        "Error should be Unsupported, got: {:?}",
        err
    );
}
