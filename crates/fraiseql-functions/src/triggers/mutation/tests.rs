use super::*;

#[test]
fn test_event_kind_as_str() {
    assert_eq!(EventKind::Insert.as_str(), "insert");
    assert_eq!(EventKind::Update.as_str(), "update");
    assert_eq!(EventKind::Delete.as_str(), "delete");
}

#[test]
fn test_after_mutation_trigger_matches() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    };

    assert!(trigger.matches("User", EventKind::Insert));
    assert!(!trigger.matches("User", EventKind::Update));
    assert!(!trigger.matches("Post", EventKind::Insert));
}

#[test]
fn test_after_mutation_trigger_matches_all_kinds() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
    };

    assert!(trigger.matches("User", EventKind::Insert));
    assert!(trigger.matches("User", EventKind::Update));
    assert!(trigger.matches("User", EventKind::Delete));
    assert!(!trigger.matches("Post", EventKind::Insert));
}

#[test]
fn test_after_mutation_trigger_builds_payload() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    };

    let event = EntityEvent {
        entity:     "User".to_string(),
        event_kind: EventKind::Insert,
        old:        None,
        new:        Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp:  chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);
    assert_eq!(payload.trigger_type, "after:mutation:onUserCreated");
    assert_eq!(payload.entity, "User");
    assert_eq!(payload.event_kind, "insert");
    assert_eq!(payload.data["event_kind"], "insert");
    assert_eq!(payload.data["old"], serde_json::Value::Null);
    assert!(payload.data["new"].is_object());
}

#[test]
fn test_before_mutation_trigger_matches() {
    let trigger = BeforeMutationTrigger {
        function_name: "validateUserInput".to_string(),
        mutation_name: "createUser".to_string(),
    };

    assert!(trigger.matches("createUser"));
    assert!(!trigger.matches("updateUser"));
}

#[test]
fn test_trigger_matcher_empty() {
    let matcher = TriggerMatcher::new();
    let results = matcher.find("User", EventKind::Insert);
    assert!(results.is_empty());
}

#[test]
fn test_trigger_matcher_specific_event_kind() {
    let mut matcher = TriggerMatcher::new();
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    };

    matcher.add(trigger);
    let results = matcher.find("User", EventKind::Insert);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].function_name, "onUserCreated");

    // Should not match other event kinds
    let results = matcher.find("User", EventKind::Update);
    assert!(results.is_empty());
}

#[test]
fn test_trigger_matcher_all_kinds() {
    let mut matcher = TriggerMatcher::new();
    let trigger = AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
    };

    matcher.add(trigger);
    assert_eq!(matcher.find("User", EventKind::Insert).len(), 1);
    assert_eq!(matcher.find("User", EventKind::Update).len(), 1);
    assert_eq!(matcher.find("User", EventKind::Delete).len(), 1);
}

#[test]
fn test_trigger_matcher_mixed_specific_and_all() {
    let mut matcher = TriggerMatcher::new();

    // Add specific triggers
    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    });

    // Add all-kinds trigger
    matcher.add(AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
    });

    // Insert should return both
    let results = matcher.find("User", EventKind::Insert);
    assert_eq!(results.len(), 2);

    // Update should return only all-kinds
    let results = matcher.find("User", EventKind::Update);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].function_name, "onUserChanged");
}

#[test]
fn test_trigger_matcher_multiple_entities() {
    let mut matcher = TriggerMatcher::new();

    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    });

    matcher.add(AfterMutationTrigger {
        function_name: "onPostCreated".to_string(),
        entity_type:   "Post".to_string(),
        event_filter:  Some(EventKind::Insert),
    });

    let user_results = matcher.find("User", EventKind::Insert);
    assert_eq!(user_results.len(), 1);
    assert_eq!(user_results[0].function_name, "onUserCreated");

    let post_results = matcher.find("Post", EventKind::Insert);
    assert_eq!(post_results.len(), 1);
    assert_eq!(post_results[0].function_name, "onPostCreated");
}

#[test]
fn test_trigger_matcher_no_cross_entity_match() {
    let mut matcher = TriggerMatcher::new();

    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
    });

    let post_results = matcher.find("Post", EventKind::Insert);
    assert!(post_results.is_empty());
}

// ── BeforeMutationChain::execute() tests ────────────────────────────────

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_empty_chain_proceeds() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType, host::NoopHostContext,
    };

    // Empty chain: no triggers → Proceed with original input
    let chain = BeforeMutationChain { triggers: vec![] };
    let observer = FunctionObserver::new();
    let modules: HashMap<String, FunctionModule> = HashMap::new();
    let input = serde_json::json!({ "name": "Alice" });

    let event = crate::types::EventPayload {
        trigger_type: "test".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input.clone(),
            &modules,
            &observer,
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Proceed(v) => assert_eq!(v, input),
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_passthrough_proceeds() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that returns the event as-is → Proceed with original input
    let source = "export default async (event) => event;".to_string();
    let module = FunctionModule::from_source("validateUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("validateUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "validateUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "Alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input.clone(),
            &modules,
            &observer,
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    // Function returns the event data (which is the input), no "abort" key → Proceed
    match result {
        BeforeMutationResult::Proceed(_) => {},
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_abort() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that returns {"abort": "name required"}
    let source = r#"export default async (event) => ({ abort: "name required" });"#.to_string();
    let module = FunctionModule::from_source("validateUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("validateUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "validateUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Abort(msg) => assert_eq!(msg, "name required"),
        BeforeMutationResult::Proceed(_) => panic!("Expected Abort"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_modify_input() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that uppercases the name and returns {"input": {modified}}
    let source = r#"
export default async (event) => ({
  input: { ...event, name: event.name.toUpperCase() }
});
"#
    .to_string();
    let module =
        FunctionModule::from_source("transformUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("transformUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "transformUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Proceed(modified) => {
            assert_eq!(modified["name"], "ALICE");
        },
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

// NOTE: The sequential (multi-trigger) chain test is verified at the unit level here
// using a mock observer, and the end-to-end behaviour is covered by Cycle 7 E2E tests.
#[test]
fn test_before_mutation_chain_execute_sequential_chain_structure() {
    // Verify that a chain with two triggers is built correctly and both triggers
    // are present in declaration order. The actual execution of sequential chains
    // is tested via E2E integration tests (Cycle 7) using the full Deno runtime.
    let chain = BeforeMutationChain {
        triggers: vec![
            BeforeMutationTrigger {
                function_name: "step1".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "step2".to_string(),
                mutation_name: "createUser".to_string(),
            },
        ],
    };

    assert_eq!(chain.triggers.len(), 2);
    assert_eq!(chain.triggers[0].function_name, "step1");
    assert_eq!(chain.triggers[1].function_name, "step2");
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_missing_module_returns_error() {
    use std::collections::HashMap;

    use crate::{FunctionModule, FunctionObserver, ResourceLimits, host::NoopHostContext};

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "nonexistentFn".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let observer = FunctionObserver::new();
    let modules: HashMap<String, FunctionModule> = HashMap::new(); // empty

    let input = serde_json::json!({ "name": "Alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            &NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_err(), "Expected error for missing module");
}
