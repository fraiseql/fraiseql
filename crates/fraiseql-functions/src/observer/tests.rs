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
    // Functions receive event.data (the entity payload), not the full EventPayload wrapper.
    let event_data = event.data.clone();
    let result = observer
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and result should come from Deno runtime
    assert!(result.is_ok(), "Observer should dispatch to Deno runtime");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_data));
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
    // Functions receive event.data (the entity payload), not the full EventPayload wrapper.
    let event_data = event.data.clone();
    let result = observer
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and result should come from Deno runtime
    assert!(result.is_ok(), "Observer should dispatch to Deno runtime for TypeScript");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_data));
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
    let event_data = event.data.clone();
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
    assert_eq!(js_result.value, Some(event_data));
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

// ── Cycle 5: dispatch_entity_event tests ─────────────────────────────────────

#[test]
fn test_dispatch_entity_event_no_triggers_returns_empty() {
    use crate::triggers::{mutation::EntityEvent, TriggerRegistry};
    use std::collections::HashMap;

    // Empty registry → no after:mutation triggers → dispatch returns empty vec
    let observer = FunctionObserver::new();
    let registry = TriggerRegistry::new();
    let modules: HashMap<String, FunctionModule> = HashMap::new();

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: crate::triggers::mutation::EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp: Utc::now(),
    };

    let matching = observer.find_after_mutation_triggers(&registry, &event);
    assert!(matching.is_empty(), "empty registry → no matching triggers");
    // No function to invoke means dispatch_count = 0
    let _ = modules; // unused in this test
}

#[test]
fn test_dispatch_entity_event_finds_matching_triggers() {
    use crate::{
        FunctionDefinition,
        triggers::{mutation::{EntityEvent, EventKind}, TriggerRegistry},
    };

    // Registry with an after:mutation:User trigger
    let defs = vec![
        FunctionDefinition::new("onUserCreated", "after:mutation:User:insert", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();

    let observer = FunctionObserver::new();

    // Insert event → trigger matches
    let insert_event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1 })),
        timestamp: Utc::now(),
    };
    let matching = observer.find_after_mutation_triggers(&registry, &insert_event);
    assert_eq!(matching.len(), 1, "should match 1 trigger for User insert");

    // Update event → no trigger (trigger is insert-only)
    let update_event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Update,
        old: Some(serde_json::json!({ "id": 1, "name": "Old" })),
        new: Some(serde_json::json!({ "id": 1, "name": "New" })),
        timestamp: Utc::now(),
    };
    let matching = observer.find_after_mutation_triggers(&registry, &update_event);
    assert!(matching.is_empty(), "update event should not match insert-only trigger");
}
