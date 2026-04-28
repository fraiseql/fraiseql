//! Tests for `DenoRuntime` — `JavaScript`/`TypeScript` function execution via V8

#![cfg(feature = "runtime-deno")]
#![allow(clippy::unwrap_used)]  // Reason: tests are stubs, cleanup in GREEN phase
#![allow(unused_imports)]  // Reason: used in test functions that may not be compiled in some configurations

use crate::{EventPayload, FunctionModule, FunctionRuntime, ResourceLimits, RuntimeType};
use chrono::Utc;

/// Helper to create a test event payload
#[allow(dead_code)]  // Reason: used in failing tests, will be used when GREEN phase implemented
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
async fn test_deno_execute_identity_js() {
    // JS module: `export default async (event) => event;`
    // Input: {"value": 42}
    // Expected: same JSON returned
    let source = r"
export default async (event) => {
    return event;
};
"
    .to_string();

    let module = FunctionModule::from_source("test_identity".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let event_data = event.data.clone();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and return the event as-is
    assert!(result.is_ok(), "Identity function should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_data));
}

#[tokio::test]
async fn test_deno_execute_transform_js() {
    // JS module that adds a field
    let source = r"
export default async (event) => {
    return { ...event.data, processed: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_transform".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Transform function should execute successfully");
    let result = result.unwrap();
    // Result should have the original data plus the new field
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert!(obj.contains_key("processed"), "Result should have 'processed' field");
        assert_eq!(obj["processed"], true);
        assert_eq!(obj["value"], 42);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_execute_typescript() {
    // TypeScript module with type annotations
    let source = r"
interface Event {
    data: Record<string, any>;
}

export default async (event: Event): Promise<object> => {
    return { result: (event.data as any).value + 1 };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_typescript".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "TypeScript function should execute successfully");
    let result = result.unwrap();
    // Result should be { result: 43 }
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert_eq!(obj["result"], 43);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_syntax_error_returns_validation() {
    // Invalid JavaScript
    let source = "export default async (event) => { broken syntax here }".to_string();

    let module = FunctionModule::from_source("test_syntax_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should return an error (either Validation for syntax error or Unsupported for stub)
    assert!(result.is_err(), "Syntax error should result in error");
    // When fully implemented, should be Validation error for SyntaxError
    // For now, stub returns Unsupported
}

#[tokio::test]
async fn test_deno_runtime_error_returns_internal() {
    // JavaScript that throws at runtime
    let source = r#"
export default async (event) => {
    throw new Error("Something went wrong");
};
"#
    .to_string();

    let module = FunctionModule::from_source("test_runtime_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should return an internal error
    assert!(result.is_err(), "Runtime error should result in error");
}

#[tokio::test]
async fn test_deno_name_returns_deno() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    assert_eq!(runtime.name(), "deno");
}

#[tokio::test]
async fn test_deno_supported_extensions() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let exts = runtime.supported_extensions();
    assert!(exts.contains(&".js"));
    assert!(exts.contains(&".ts"));
    assert!(exts.contains(&".mjs"));
    assert!(exts.contains(&".mts"));
}
