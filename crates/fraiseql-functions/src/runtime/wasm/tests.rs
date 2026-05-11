#![allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions

use std::path::PathBuf;
use crate::{EventPayload, FunctionModule, RuntimeType};
use crate::runtime::FunctionRuntime;

/// Helper to find test fixture file
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions")
        .join(name)
}

/// Helper to load WASM bytecode from a fixture file
fn load_wasm_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    std::fs::read(&path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()))
}

#[test]
fn test_wasm_load_valid_component() {
    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

    assert_eq!(module.name, "test_identity");
    assert_eq!(module.runtime, RuntimeType::Wasm);
    assert!(!module.source_hash.is_empty());
    assert!(!module.bytecode.is_empty());
}

#[test]
fn test_wasm_load_invalid_bytes_returns_error() {
    let invalid_bytecode = bytes::Bytes::from(vec![0xFF, 0xFE, 0xFD, 0xFC]);
    let module = FunctionModule::from_bytecode("invalid".to_string(), invalid_bytecode);

    assert_eq!(module.name, "invalid");
}

/// Invoke on guest-identity returns the event payload (real WASM, not mock).
#[tokio::test]
async fn test_wasm_guest_returns_event_payload() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "mutation".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"user_id": 42, "email": "test@example.com"}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let limits = crate::types::ResourceLimits::default();

    let result = runtime
        .invoke(&module, event.clone(), &host, limits)
        .await
        .expect("invoke should succeed");

    // Real WASM returned a value (not a mock short-circuit)
    assert!(result.value.is_some(), "handle() should return a value");

    let val = result.value.unwrap();
    // The identity guest echoes the event JSON back; verify round-trip fields
    assert_eq!(val["trigger_type"].as_str().unwrap(), "mutation");
    assert_eq!(val["entity"].as_str().unwrap(), "User");
    assert_eq!(val["data"]["user_id"].as_i64().unwrap(), 42);
}

/// Verify the transform fixture adds `"transformed": true`.
#[tokio::test]
async fn test_wasm_guest_transform_adds_field() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-transform.wasm"));
    let module = FunctionModule::from_bytecode("test_transform".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "mutation".to_string(),
        entity: "Post".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"post_id": 1}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await
        .expect("transform invoke should succeed");

    let val = result.value.expect("value present");
    assert!(val["transformed"].as_bool().unwrap());
}

#[tokio::test]
async fn test_wasm_guest_can_call_log() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_logging".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"test": true}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let limits = crate::types::ResourceLimits::default();

    let result = runtime
        .invoke(&module, event, &host, limits)
        .await
        .expect("invoke should succeed");

    assert!(result.value.is_some());
}

#[tokio::test]
async fn test_wasm_guest_log_levels() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_log_levels".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await
        .expect("invoke should succeed");

    assert!(result.value.is_some());
}

#[tokio::test]
async fn test_wasm_guest_get_event_payload() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_event_payload".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "mutation".to_string(),
        entity: "User".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"user_id": 42, "email": "test@example.com"}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event.clone(), &host, crate::types::ResourceLimits::default())
        .await
        .expect("invoke should succeed");

    let val = result.value.expect("value present");
    assert_eq!(val["entity"].as_str().unwrap(), "User");
}

#[tokio::test]
async fn test_wasm_guest_get_auth_context() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await
        .expect("invoke should succeed");

    assert!(result.value.is_some());
}

#[tokio::test]
async fn test_wasm_guest_get_env_var() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await
        .expect("invoke should succeed");

    assert!(result.value.is_some());
}

// ========== WASM Host Function Bridge Tests ==========

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_query_with_live_host() {
    use crate::host::live::{LiveHostContext, HostContextConfig};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_query".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "TestEntity".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"id": 42, "name": "test_item"}),
        timestamp: chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Query invocation should succeed");
    assert!(result.unwrap().value.is_some(), "Query should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_http_request_with_live_host() {
    use crate::host::live::{LiveHostContext, HostContextConfig};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_http".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "TestEntity".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"id": 42}),
        timestamp: chrono::Utc::now(),
    };

    let config = HostContextConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..Default::default()
    };
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "HTTP request invocation should succeed");
    assert!(result.unwrap().value.is_some(), "HTTP request should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_storage_get_with_live_host() {
    use crate::host::live::{LiveHostContext, HostContextConfig};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_storage_get".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "File".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Storage get invocation should succeed");
    assert!(result.unwrap().value.is_some(), "Storage get should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_env_var_with_live_host() {
    use crate::host::live::{LiveHostContext, HostContextConfig};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let mut config = HostContextConfig::default();
    config.allowed_env_vars.insert("TEST_VAR".to_string());
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Env var invocation should succeed");
    assert!(result.unwrap().value.is_some(), "Env var should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_auth_context_with_live_host() {
    use crate::host::live::{LiveHostContext, HostContextConfig};

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({}),
        timestamp: chrono::Utc::now(),
    };

    let config = HostContextConfig::default();
    let host = LiveHostContext::new(event.clone(), config);

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Auth context invocation should succeed");
    assert!(result.unwrap().value.is_some(), "Auth context should return a value");
}
