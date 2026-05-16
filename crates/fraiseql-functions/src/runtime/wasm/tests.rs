#![allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions

use std::path::PathBuf;

use crate::{EventPayload, FunctionModule, RuntimeType, runtime::FunctionRuntime};

/// Helper to find test fixture file.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions")
        .join(name)
}

/// Helper to load WASM bytecode from a fixture file.
fn load_wasm_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    std::fs::read(&path).unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()))
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

#[tokio::test]
async fn test_wasm_guest_identity_roundtrip() {
    use crate::host::NoopHostContext;

    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"test": true}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let limits = crate::types::ResourceLimits::default();

    let result = runtime
        .invoke(&module, event.clone(), &host, limits)
        .await
        .expect("invoke should succeed");

    // The identity guest returns the event unchanged
    let returned = result.value.expect("should have a value");
    let expected = serde_json::to_value(&event).unwrap();
    assert_eq!(returned, expected, "identity guest should echo event back");
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
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"test": true}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let limits = crate::types::ResourceLimits::default();

    let result = runtime.invoke(&module, event, &host, limits).await;
    assert!(result.is_ok(), "invocation should succeed");
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
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;
    assert!(result.is_ok());
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
        entity:       "User".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"user_id": 42, "email": "test@example.com"}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;
    assert!(result.is_ok());
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
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;
    assert!(result.is_ok());
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
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event.clone());
    let result = runtime
        .invoke(&module, event, &host, crate::types::ResourceLimits::default())
        .await;
    assert!(result.is_ok());
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_query_with_live_host() {
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

    assert!(result.is_ok(), "Query invocation should succeed");

    let function_result = result.unwrap();
    assert!(function_result.value.is_some(), "Query should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_http_request_with_live_host() {
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

    assert!(result.is_ok(), "HTTP request invocation should succeed");

    let function_result = result.unwrap();
    assert!(function_result.value.is_some(), "HTTP request should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_storage_get_with_live_host() {
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

    assert!(result.is_ok(), "Storage get invocation should succeed");

    let function_result = result.unwrap();
    assert!(function_result.value.is_some(), "Storage get should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_env_var_with_live_host() {
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

    assert!(result.is_ok(), "Env var invocation should succeed");

    let function_result = result.unwrap();
    assert!(function_result.value.is_some(), "Env var should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_wasm_guest_calls_auth_context_with_live_host() {
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

    assert!(result.is_ok(), "Auth context invocation should succeed");

    let function_result = result.unwrap();
    assert!(function_result.value.is_some(), "Auth context should return a value");
}

/// Mock host context that responds to all operations for integration testing.
struct MockFullBridgeHost {
    event:   EventPayload,
    storage: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl MockFullBridgeHost {
    fn new(event: EventPayload) -> Self {
        Self {
            event,
            storage: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl super::host_bridge::DynHostContext for MockFullBridgeHost {
    fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + '_,
        >,
    > {
        Box::pin(async { Ok(serde_json::json!({"data": {"users": [{"id": 1}]}})) })
    }

    fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<Vec<serde_json::Value>>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async { Ok(vec![]) })
    }

    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<crate::host::HttpResponse>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async {
            Ok(crate::host::HttpResponse {
                status:  200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body:    b"{}".to_vec(),
            })
        })
    }

    fn storage_get(
        &self,
        _bucket: &str,
        key: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<u8>>> + Send + '_>,
    > {
        let data = self.storage.lock().expect("lock").get(key).cloned().unwrap_or_default();
        Box::pin(async move { Ok(data) })
    }

    fn storage_put(
        &self,
        _bucket: &str,
        key: &str,
        body: &[u8],
        _content_type: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<()>> + Send + '_>>
    {
        self.storage.lock().expect("lock").insert(key.to_string(), body.to_vec());
        Box::pin(async { Ok(()) })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "sub": "test-user",
            "roles": ["admin"],
        }))
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        if name == "FRAISEQL_TEST_VAR" {
            Ok(Some("test_value".to_string()))
        } else {
            Ok(None)
        }
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::types::LogLevel, _message: &str) {}
}

#[tokio::test]
async fn test_wasm_guest_full_bridge_exercises_all_ops() {
    let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
        .expect("Failed to create WasmRuntime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-full-bridge.wasm"));
    let module = FunctionModule::from_bytecode("full_bridge".to_string(), bytecode);

    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "FullBridge".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": 1}),
        timestamp:    chrono::Utc::now(),
    };

    let host = std::sync::Arc::new(MockFullBridgeHost::new(event.clone()));

    let result = runtime
        .invoke_with_context(&module, event, host, crate::types::ResourceLimits::default())
        .await
        .expect("full bridge invocation should succeed");

    let value = result.value.expect("should have result value");

    // Verify all operations reported success
    assert_eq!(value["logging"], "ok", "logging ops should succeed");
    assert_eq!(value["event_payload"], true, "event payload should be non-empty");
    assert_eq!(value["auth_context"]["ok"], true, "auth context should succeed");
    assert_eq!(value["env_var"]["found"], true, "env var should be found");
    assert_eq!(value["env_var"]["value"], "test_value", "env var should have correct value");
    assert_eq!(value["http_request"]["ok"], true, "http request should succeed");
    assert_eq!(value["http_request"]["status"], 200, "http status should be 200");
    assert_eq!(value["query"]["ok"], true, "GraphQL query should succeed");
    assert_eq!(value["storage_put"]["ok"], true, "storage put should succeed");
    assert_eq!(value["storage_get"]["ok"], true, "storage get should succeed");
}

#[cfg(feature = "runtime-wasm")]
#[tokio::test]
async fn test_before_mutation_chain_with_wasm_function_proceeds() {
    use std::collections::HashMap;

    use crate::{
        host::NoopHostContext,
        observer::FunctionObserver,
        triggers::mutation::{BeforeMutationChain, BeforeMutationResult, BeforeMutationTrigger},
    };

    // Set up observer with WASM runtime
    let mut observer = FunctionObserver::new();
    let runtime =
        super::WasmRuntime::new(&super::WasmConfig::default()).expect("create wasm runtime");
    observer.register_runtime(RuntimeType::Wasm, runtime);

    // Load the identity guest (returns event unchanged → Proceed with unchanged input)
    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("validateInput".to_string(), bytecode);

    let mut modules = HashMap::new();
    modules.insert("validateInput".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "validateInput".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({"name": "Alice", "email": "alice@example.com"});
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "User".to_string(),
        event_kind:   "created".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let host = NoopHostContext::new(event);
    let limits = crate::types::ResourceLimits::default();

    let result = chain.execute(input.clone(), &modules, &observer, &host, limits).await;

    match result.expect("chain should succeed") {
        BeforeMutationResult::Proceed(output) => {
            // Identity guest echoes event → no "input" key → chain proceeds with original input
            assert_eq!(output, input, "should proceed with unchanged input");
        },
        BeforeMutationResult::Abort(msg) => {
            panic!("Expected Proceed, got Abort: {msg}");
        },
    }
}

#[tokio::test]
async fn test_wasm_performance_baseline() {
    use crate::host::NoopHostContext;

    let runtime =
        super::WasmRuntime::new(&super::WasmConfig::default()).expect("create wasm runtime");

    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("perf_test".to_string(), bytecode);
    let limits = crate::types::ResourceLimits::default();

    // Cold start: first invocation (includes component compilation)
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Perf".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": 1}),
        timestamp:    chrono::Utc::now(),
    };
    let host = NoopHostContext::new(event.clone());

    let cold_start = std::time::Instant::now();
    let result = runtime.invoke(&module, event.clone(), &host, limits.clone()).await;
    let cold_duration = cold_start.elapsed();
    assert!(result.is_ok(), "cold start should succeed");

    // Warm starts: subsequent invocations (engine caches compilation)
    let mut warm_durations = Vec::new();
    for _ in 0..10 {
        let host = NoopHostContext::new(event.clone());
        let warm_start = std::time::Instant::now();
        let result = runtime.invoke(&module, event.clone(), &host, limits.clone()).await;
        warm_durations.push(warm_start.elapsed());
        assert!(result.is_ok(), "warm start should succeed");
    }

    let avg_warm = warm_durations.iter().sum::<std::time::Duration>() / 10;

    // Log performance numbers for baseline tracking
    eprintln!("=== WASM Performance Baseline ===");
    eprintln!("  Cold start:     {:?}", cold_duration);
    eprintln!("  Avg warm start: {:?} (10 iterations)", avg_warm);

    // Relaxed assertions — these are guardrails, not benchmarks.
    // Cold start includes WASM compilation, should be under 500ms on CI.
    assert!(cold_duration.as_millis() < 500, "cold start too slow: {cold_duration:?}");
    // Warm start should be significantly faster.
    assert!(avg_warm.as_millis() < 100, "warm start too slow: {avg_warm:?}");
}

#[cfg(feature = "runtime-wasm")]
#[tokio::test]
async fn test_after_mutation_trigger_fires_without_blocking() {
    use crate::{
        host::NoopHostContext,
        observer::FunctionObserver,
        triggers::mutation::{AfterMutationTrigger, EntityEvent, EventKind},
    };

    // Set up observer with WASM runtime
    let mut observer = FunctionObserver::new();
    let runtime =
        super::WasmRuntime::new(&super::WasmConfig::default()).expect("create wasm runtime");
    observer.register_runtime(RuntimeType::Wasm, runtime);

    // Load the identity guest
    let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
    let module = FunctionModule::from_bytecode("onUserCreated".to_string(), bytecode);

    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    };

    let entity_event = EntityEvent {
        entity:     "User".to_string(),
        event_kind: EventKind::Insert,
        old:        None,
        new:        Some(serde_json::json!({"id": 1, "name": "Alice"})),
        timestamp:  chrono::Utc::now(),
    };

    // Build payload and invoke — fire-and-forget semantics mean we just
    // verify the invocation completes without error.
    let payload = trigger.build_payload(&entity_event);
    let host = NoopHostContext::new(payload.clone());
    let limits = crate::types::ResourceLimits::default();

    let start = std::time::Instant::now();
    let result = observer.invoke(&module, payload, &host, limits).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "after:mutation trigger should complete without error");
    // Verify it completed quickly (identity guest is fast)
    assert!(elapsed.as_millis() < 1000, "should complete quickly: {elapsed:?}");
}
