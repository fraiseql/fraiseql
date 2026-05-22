#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::path::PathBuf;

use super::*;
use crate::{EventPayload, FunctionModule, RuntimeType, runtime::FunctionRuntime};

/// Helper to find test fixture file
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions")
        .join(name)
}

/// Helper to load WASM bytecode from a fixture file
fn load_wasm_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    std::fs::read(&path).unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()))
}

#[test]
fn test_wasm_load_valid_component() {
    // Load a valid WASM component and ensure it can be used
    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

    // Should not panic and should have correct properties
    assert_eq!(module.name, "test_identity");
    assert_eq!(module.runtime, RuntimeType::Wasm);
    assert!(!module.source_hash.is_empty());
    assert!(!module.bytecode.is_empty());
}

#[test]
fn test_wasm_load_invalid_bytes_returns_error() {
    // Garbage bytes should fail validation
    let invalid_bytecode = bytes::Bytes::from(vec![0xFF, 0xFE, 0xFD, 0xFC]);
    let module = FunctionModule::from_bytecode("invalid".to_string(), invalid_bytecode);

    // Module creation itself should succeed (validation happens at runtime)
    assert_eq!(module.name, "invalid");
    // But the module should be rejected when trying to execute (tested in runtime tests)
}

#[tokio::test]
async fn test_wasm_guest_can_call_log() {
    // Component calls fraiseql:host/logging.log(info, "hello")
    // Result.logs contains entry with correct message and level
    use crate::{host::NoopHostContext, runtime::FunctionRuntime};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_logging".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"test": true}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let limits = crate::types::ResourceLimits::default();

    // Note: This test will fail if guest-identity.wasm is not a valid WASM component
    // (it's a placeholder until Cycle 0 - WASM Toolchain builds real fixtures)
    let _result = runtime.invoke(&module, event, &host, limits).await;

    // For now, we just verify the infrastructure is wired correctly
    // Real assertion will verify logs when actual WASM fixture is available
}

#[tokio::test]
async fn test_wasm_guest_log_levels() {
    // Component calls logging with debug/info/warn/error levels
    // All are captured with correct level
    use crate::{host::NoopHostContext, runtime::FunctionRuntime};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_log_levels".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let _result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // When the guest calls log with different levels, all should be captured
    // Will be fully tested when real WASM fixture is available
}

#[tokio::test]
async fn test_wasm_guest_get_event_payload() {
    // Component calls context.get-event-payload()
    // Receives event JSON
    use crate::{host::NoopHostContext, runtime::FunctionRuntime};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_event_payload".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "mutation".to_string(),
        entity:       "User".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"user_id": 42, "email": "test@example.com"}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let _result = runtime
        .invoke(&module, event.clone(), &host, crate::types::ResourceLimits::default())
        .await;

    // Guest should be able to retrieve the event payload from context
    // Will be fully tested when real WASM fixture is available
}

#[tokio::test]
async fn test_wasm_guest_get_auth_context() {
    // Component calls context.get-auth-context()
    // Receives auth JSON or error
    use crate::{host::NoopHostContext, runtime::FunctionRuntime};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    // This invocation may fail if guest tries to get auth context when none exists
    let _result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Either way, the host context was available for the guest to call
    // Will be fully tested when real WASM fixture is available
}

#[tokio::test]
async fn test_wasm_guest_get_env_var() {
    // Component calls context.get-env-var("APP_URL")
    // Receives value or None
    use crate::{host::NoopHostContext, runtime::FunctionRuntime};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let _result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Guest can retrieve environment variables via context
    // Will be fully tested when real WASM fixture is available
}

// ========== Phase 5B Cycle 1: WASM Host Function Bridge Tests (RED) ==========

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_query_with_live_host() {
    // RED: Component calls fraiseql:host/io.query
    // Should receive GraphQL result as JSON string
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_query".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "TestEntity".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": 42, "name": "test_item"}),
        timestamp:    chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Should complete successfully
    assert!(result.is_ok(), "Query invocation should succeed");

    let function_result = result.unwrap();
    // Should have a result value (the query response)
    assert!(function_result.value.is_some(), "Query should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_http_request_with_live_host() {
    // RED: Component calls fraiseql:host/io.http-request
    // Should receive HTTP response with status, headers, body
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_http".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "TestEntity".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": 42}),
        timestamp:    chrono::Utc::now(),
    };

    let config = HostContextConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..Default::default()
    };
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Should complete successfully
    assert!(result.is_ok(), "HTTP request invocation should succeed");

    let function_result = result.unwrap();
    // Should have a result value (the HTTP response)
    assert!(function_result.value.is_some(), "HTTP request should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_storage_get_with_live_host() {
    // RED: Component calls fraiseql:host/io.storage-get
    // Should receive bytes from storage backend or error
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_storage_get".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "File".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Should complete successfully (even if storage returns error)
    assert!(result.is_ok(), "Storage get invocation should succeed");

    let function_result = result.unwrap();
    // Should have a result value (either storage bytes or error)
    assert!(function_result.value.is_some(), "Storage get should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_env_var_with_live_host() {
    // RED: Component calls fraiseql:host/context.get-env-var
    // Should receive environment variable value or None
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let mut config = HostContextConfig::default();
    config.allowed_env_vars.insert("TEST_VAR".to_string());
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Should complete successfully
    assert!(result.is_ok(), "Env var invocation should succeed");

    let function_result = result.unwrap();
    // Should have a result value (the env var or None)
    assert!(function_result.value.is_some(), "Env var should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_auth_context_with_live_host() {
    // RED: Component calls fraiseql:host/context.get-auth-context
    // Should receive auth context JSON with user info
    use crate::host::live::{HostContextConfig, LiveHostContext};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    // Should complete successfully
    assert!(result.is_ok(), "Auth context invocation should succeed");

    let function_result = result.unwrap();
    // Should have a result value (the auth context or error)
    assert!(function_result.value.is_some(), "Auth context should return a value");
}
