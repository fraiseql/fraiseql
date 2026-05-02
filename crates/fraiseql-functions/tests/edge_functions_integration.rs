//! Integration tests for the edge functions runtime.
//!
//! These tests exercise the full pipeline:
//!   deploy → invoke → response
//!
//! using in-memory implementations so no external services are required.

#![cfg(feature = "runtime-wasm")]
#![allow(clippy::unwrap_used)] // Reason: integration tests use unwrap for concise assertions

use std::path::PathBuf;
use std::time::Duration;

use fraiseql_functions::{
    EventPayload, FunctionModule, FunctionStore, InMemoryFunctionStore,
    ResourceLimits, RuntimeType,
};
use fraiseql_functions::runtime::wasm::{WasmConfig, WasmRuntime};
use fraiseql_functions::runtime::FunctionRuntime;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions")
        .join(name)
}

fn load_wasm(name: &str) -> bytes::Bytes {
    let path = fixture_path(name);
    bytes::Bytes::from(
        std::fs::read(&path)
            .unwrap_or_else(|_| panic!("Failed to read WASM fixture: {}", path.display())),
    )
}

fn make_event(entity: &str) -> EventPayload {
    EventPayload {
        trigger_type: "mutation".to_string(),
        entity: entity.to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"id": 1}),
        timestamp: chrono::Utc::now(),
    }
}

// ── Deploy → invoke → response ────────────────────────────────────────────────

/// Full pipeline: store a function, load it, invoke it, verify the result.
#[tokio::test]
async fn test_deploy_invoke_identity_function() {
    let store = InMemoryFunctionStore::new();
    let runtime = WasmRuntime::new(&WasmConfig::default()).unwrap();

    // Deploy
    let bytecode = load_wasm("guest-identity.wasm");
    let record = store
        .store_function("identity", RuntimeType::Wasm, bytecode.clone())
        .await
        .unwrap();

    assert_eq!(record.name, "identity");
    assert_eq!(record.version, 1);

    // Load from store
    let loaded = store.get_function("identity").await.unwrap().unwrap();
    let module = FunctionModule::from_bytecode(loaded.name.clone(), loaded.bytecode);

    // Invoke
    let event = make_event("User");
    let host = fraiseql_functions::NoopHostContext::new(event.clone());
    let result = runtime
        .invoke(&module, event.clone(), &host, ResourceLimits::default())
        .await
        .unwrap();

    assert!(result.value.is_some());
    let val = result.value.unwrap();
    assert_eq!(val["entity"].as_str().unwrap(), "User");
}

/// Deploy and invoke the transform function (verifies different artifact).
#[tokio::test]
async fn test_deploy_invoke_transform_function() {
    let store = InMemoryFunctionStore::new();
    let runtime = WasmRuntime::new(&WasmConfig::default()).unwrap();

    let bytecode = load_wasm("guest-transform.wasm");
    let _record = store
        .store_function("transform", RuntimeType::Wasm, bytecode.clone())
        .await
        .unwrap();

    let loaded = store.get_function("transform").await.unwrap().unwrap();
    let module = FunctionModule::from_bytecode(loaded.name.clone(), loaded.bytecode);
    let event = make_event("Post");
    let host = fraiseql_functions::NoopHostContext::new(event.clone());

    let result = runtime
        .invoke(&module, event, &host, ResourceLimits::default())
        .await
        .unwrap();

    let val = result.value.expect("transform must return a value");
    assert!(val["transformed"].as_bool().unwrap_or(false));
}

// ── Module cache hit on second invocation ─────────────────────────────────────

/// Second invocation must use the cached compiled component (no recompilation).
#[tokio::test]
async fn test_module_cache_hit_on_second_invocation() {
    use fraiseql_functions::WasmModuleCache;

    let cache = WasmModuleCache::new(8);
    let runtime = WasmRuntime::with_module_cache(&WasmConfig::default(), cache).unwrap();

    let bytecode = load_wasm("guest-identity.wasm");
    let module = FunctionModule::from_bytecode("identity_cached".to_string(), bytecode);

    let event = make_event("User");
    let host = fraiseql_functions::NoopHostContext::new(event.clone());

    // First invocation: cache miss → compile and cache
    let _r1 = runtime
        .invoke(&module, event.clone(), &host, ResourceLimits::default())
        .await
        .unwrap();

    assert_eq!(runtime.module_cache().len(), 1, "module should be cached after first invoke");

    // Second invocation: cache hit
    let host2 = fraiseql_functions::NoopHostContext::new(event.clone());
    let r2 = runtime
        .invoke(&module, event.clone(), &host2, ResourceLimits::default())
        .await
        .unwrap();

    assert!(r2.value.is_some(), "second invocation should succeed");
    // Cache should still have exactly 1 entry (not 2 — it's the same hash)
    assert_eq!(runtime.module_cache().len(), 1);
}

/// Different functions with different bytecode occupy separate cache entries.
#[tokio::test]
async fn test_module_cache_stores_multiple_functions() {
    use fraiseql_functions::WasmModuleCache;

    let cache = WasmModuleCache::new(8);
    let runtime = WasmRuntime::with_module_cache(&WasmConfig::default(), cache).unwrap();

    let identity_bytes = load_wasm("guest-identity.wasm");
    let transform_bytes = load_wasm("guest-transform.wasm");

    let identity = FunctionModule::from_bytecode("identity".to_string(), identity_bytes);
    let transform = FunctionModule::from_bytecode("transform".to_string(), transform_bytes);

    let e1 = make_event("A");
    let h1 = fraiseql_functions::NoopHostContext::new(e1.clone());
    runtime.invoke(&identity, e1, &h1, ResourceLimits::default()).await.unwrap();

    let e2 = make_event("B");
    let h2 = fraiseql_functions::NoopHostContext::new(e2.clone());
    runtime.invoke(&transform, e2, &h2, ResourceLimits::default()).await.unwrap();

    assert_eq!(
        runtime.module_cache().len(),
        2,
        "two distinct modules should produce two cache entries"
    );
}

// ── CPU timeout enforcement ───────────────────────────────────────────────────

/// Invoking with a very short timeout must return a timeout error.
#[tokio::test]
async fn test_timeout_returns_error() {
    let runtime = WasmRuntime::new(&WasmConfig::default()).unwrap();
    let bytecode = load_wasm("guest-identity.wasm");
    let module = FunctionModule::from_bytecode("timeout_test".to_string(), bytecode);

    let event = make_event("User");
    let host = fraiseql_functions::NoopHostContext::new(event.clone());

    // 1 nanosecond timeout — guaranteed to fire
    let limits = ResourceLimits {
        max_duration: Duration::from_nanos(1),
        ..Default::default()
    };

    let result = runtime.invoke(&module, event, &host, limits).await;
    assert!(result.is_err(), "invocation should fail with timeout");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("timed out") || err.contains("timeout"),
        "error should mention timeout, got: {err}"
    );
}

// ── Concurrent invocations cap ────────────────────────────────────────────────

/// Concurrent invocations beyond the cap are immediately rejected.
#[tokio::test]
async fn test_concurrency_limiter_rejects_over_cap() {
    use fraiseql_functions::ConcurrencyLimiter;

    let limiter = ConcurrencyLimiter::new(2);

    let p1 = limiter.try_acquire().unwrap();
    let p2 = limiter.try_acquire().unwrap();

    // Third attempt over cap must be rejected
    let rejected = limiter.try_acquire();
    assert!(rejected.is_err(), "third acquire on cap-2 limiter must fail");

    // Drop a permit and verify we can acquire again
    drop(p1);
    let p3 = limiter.try_acquire();
    assert!(p3.is_ok(), "after dropping a permit, acquire must succeed");

    drop(p2);
    drop(p3);
    assert_eq!(limiter.available_permits(), 2);
}

/// Registry isolates per-function caps.
#[tokio::test]
async fn test_concurrency_registry_per_function_isolation() {
    use fraiseql_functions::ConcurrencyLimiterRegistry;

    let registry = ConcurrencyLimiterRegistry::new(1);
    let fn_a = registry.get_or_create("fn_a");
    let fn_b = registry.get_or_create("fn_b");

    let _p_a = fn_a.try_acquire().unwrap(); // fn_a at cap
    assert!(fn_a.try_acquire().is_err(), "fn_a should be at cap");
    assert!(fn_b.try_acquire().is_ok(), "fn_b cap is independent of fn_a");
}

// ── Secret injection round-trip ───────────────────────────────────────────────

/// Secrets set for a function are readable via the host context.
#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_secret_injection_round_trip() {
    use std::sync::Arc;
    use fraiseql_functions::InMemorySecretsStore;
    use fraiseql_functions::host::live::{HostContextConfig, LiveHostContext};
    use fraiseql_functions::HostContext as _;

    let store: Arc<dyn fraiseql_functions::FunctionSecretsStore> =
        Arc::new(InMemorySecretsStore::new());

    // Store a secret for "my_function"
    store
        .set_secret("my_function", "DB_PASSWORD", "s3cr3t")
        .await
        .unwrap();

    // Build a LiveHostContext with the secrets store wired in
    let payload = make_event("User");
    let ctx = LiveHostContext::new(payload, HostContextConfig::default())
        .with_secrets(Arc::clone(&store), "my_function");

    // Retrieve via get_secret
    let val = ctx.get_secret("DB_PASSWORD").await.unwrap();
    assert_eq!(val, Some("s3cr3t".to_string()));

    // A different key returns None
    let missing = ctx.get_secret("MISSING_KEY").await.unwrap();
    assert!(missing.is_none());

    // Another function's context cannot see these secrets
    let other_payload = make_event("Post");
    let other_ctx = LiveHostContext::new(other_payload, HostContextConfig::default())
        .with_secrets(Arc::clone(&store), "other_function");

    let cross_fn = other_ctx.get_secret("DB_PASSWORD").await.unwrap();
    assert!(cross_fn.is_none(), "cross-function secret access must return None");
}

// ── Store CRUD ─────────────────────────────────────────────────────────────────

/// Full deploy lifecycle: store, list, delete.
#[tokio::test]
async fn test_function_store_full_lifecycle() {
    let store = InMemoryFunctionStore::new();

    // Deploy
    let bytecode = load_wasm("guest-identity.wasm");
    let rec = store
        .store_function("my_fn", RuntimeType::Wasm, bytecode.clone())
        .await
        .unwrap();

    assert_eq!(rec.name, "my_fn");
    assert_eq!(rec.version, 1);

    // List
    let list = store.list_functions().await.unwrap();
    assert_eq!(list.len(), 1);

    // Version bump on re-deploy
    let rec2 = store
        .store_function("my_fn", RuntimeType::Wasm, bytecode)
        .await
        .unwrap();
    assert_eq!(rec2.version, 2, "re-deploy should bump version");

    // Delete
    let deleted = store.delete_function("my_fn").await.unwrap();
    assert!(deleted);

    let after_delete = store.get_function("my_fn").await.unwrap();
    assert!(after_delete.is_none());
}

/// `get_function` returns the highest version.
#[tokio::test]
async fn test_function_store_get_returns_latest_version() {
    let store = InMemoryFunctionStore::new();
    let bytecode = load_wasm("guest-identity.wasm");

    for _ in 0..3 {
        store
            .store_function("versioned_fn", RuntimeType::Wasm, bytecode.clone())
            .await
            .unwrap();
    }

    let loaded = store.get_function("versioned_fn").await.unwrap().unwrap();
    assert_eq!(loaded.version, 3);
}

/// `list_functions` returns one entry per function name (latest version only).
#[tokio::test]
async fn test_function_store_list_latest_only() {
    let store = InMemoryFunctionStore::new();
    let identity = load_wasm("guest-identity.wasm");
    let transform = load_wasm("guest-transform.wasm");

    store
        .store_function("fn_a", RuntimeType::Wasm, identity.clone())
        .await
        .unwrap();
    store
        .store_function("fn_a", RuntimeType::Wasm, identity)
        .await
        .unwrap();
    store
        .store_function("fn_b", RuntimeType::Wasm, transform)
        .await
        .unwrap();

    let list = store.list_functions().await.unwrap();
    // Should list one entry per function name
    assert_eq!(list.len(), 2, "list should contain one entry per function name");

    let fn_a = list.iter().find(|r| r.name == "fn_a").unwrap();
    assert_eq!(fn_a.version, 2, "listed version should be the latest");
}
