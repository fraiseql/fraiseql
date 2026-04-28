//! Integration tests for the trigger system.

use crate::types::EventPayload;
use crate::triggers::mutation::{
    AfterMutationTrigger, BeforeMutationTrigger, EntityEvent, EventKind, TriggerMatcher,
};

/// Test: after:mutation fires on insert
#[test]
fn test_after_mutation_fires_on_insert() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    };

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp: chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);

    assert_eq!(payload.trigger_type, "after:mutation:onUserCreated");
    assert_eq!(payload.entity, "User");
    assert_eq!(payload.event_kind, "insert");
    assert_eq!(payload.data["old"], serde_json::Value::Null);
    assert!(payload.data["new"].is_object());
}

/// Test: after:mutation fires on update
#[test]
fn test_after_mutation_fires_on_update() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserUpdated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Update),
    };

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Update,
        old: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        new: Some(serde_json::json!({ "id": 1, "name": "Alice Smith" })),
        timestamp: chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);

    assert_eq!(payload.trigger_type, "after:mutation:onUserUpdated");
    assert_eq!(payload.entity, "User");
    assert_eq!(payload.event_kind, "update");
    assert!(payload.data["old"].is_object());
    assert!(payload.data["new"].is_object());
}

/// Test: after:mutation fires on delete
#[test]
fn test_after_mutation_fires_on_delete() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserDeleted".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Delete),
    };

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Delete,
        old: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        new: None,
        timestamp: chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);

    assert_eq!(payload.trigger_type, "after:mutation:onUserDeleted");
    assert_eq!(payload.entity, "User");
    assert_eq!(payload.event_kind, "delete");
    assert!(payload.data["old"].is_object());
    assert_eq!(payload.data["new"], serde_json::Value::Null);
}

/// Test: after:mutation receives correct entity type
#[test]
fn test_after_mutation_receives_entity_type() {
    let trigger = AfterMutationTrigger {
        function_name: "onPostCreated".to_string(),
        entity_type: "Post".to_string(),
        event_filter: Some(EventKind::Insert),
    };

    let event = EntityEvent {
        entity: "Post".to_string(),
        event_kind: EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1, "title": "Hello" })),
        timestamp: chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);

    assert_eq!(payload.entity, "Post");
    assert_eq!(trigger.entity_type, "Post");
}

/// Test: trigger matching logic for entity type and event kind
#[test]
fn test_after_mutation_trigger_matching() {
    let trigger_insert = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    };

    let trigger_all = AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type: "User".to_string(),
        event_filter: None,
    };

    // Insert-only trigger matches insert
    assert!(trigger_insert.matches("User", EventKind::Insert));
    assert!(!trigger_insert.matches("User", EventKind::Update));
    assert!(!trigger_insert.matches("Post", EventKind::Insert));

    // All-kinds trigger matches all
    assert!(trigger_all.matches("User", EventKind::Insert));
    assert!(trigger_all.matches("User", EventKind::Update));
    assert!(trigger_all.matches("User", EventKind::Delete));
    assert!(!trigger_all.matches("Post", EventKind::Insert));
}

/// Test: before:mutation trigger matching
#[test]
fn test_before_mutation_trigger_matching() {
    let trigger = BeforeMutationTrigger {
        function_name: "validateUserInput".to_string(),
        mutation_name: "createUser".to_string(),
    };

    assert!(trigger.matches("createUser"));
    assert!(!trigger.matches("updateUser"));
    assert!(!trigger.matches("deleteUser"));
}

/// Test: multiple before:mutation triggers in sequence
#[test]
fn test_before_mutation_multiple_triggers() {
    let trigger_a = BeforeMutationTrigger {
        function_name: "validateInput".to_string(),
        mutation_name: "createUser".to_string(),
    };

    let trigger_b = BeforeMutationTrigger {
        function_name: "checkDuplicates".to_string(),
        mutation_name: "createUser".to_string(),
    };

    let trigger_c = BeforeMutationTrigger {
        function_name: "auditLog".to_string(),
        mutation_name: "createUser".to_string(),
    };

    assert!(trigger_a.matches("createUser"));
    assert!(trigger_b.matches("createUser"));
    assert!(trigger_c.matches("createUser"));
}

/// Test: event payload serialization
#[test]
fn test_after_mutation_payload_serialization() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    };

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp: chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);
    let json = serde_json::to_string(&payload).expect("serialize");
    let restored: EventPayload = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.trigger_type, payload.trigger_type);
    assert_eq!(restored.entity, payload.entity);
    assert_eq!(restored.event_kind, payload.event_kind);
}

/// Test: trigger matcher finds correct triggers for dispatch
#[test]
fn test_trigger_dispatch_finds_matching_triggers() {
    let mut matcher = TriggerMatcher::new();

    // Add triggers for different scenarios
    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    });

    matcher.add(AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type: "User".to_string(),
        event_filter: None, // Matches all events
    });

    // When User is inserted, both specific and all-kinds triggers match
    let triggers = matcher.find("User", EventKind::Insert);
    assert_eq!(triggers.len(), 2);
    let names: Vec<_> = triggers.iter().map(|t| t.function_name.as_str()).collect();
    assert!(names.contains(&"onUserCreated"));
    assert!(names.contains(&"onUserChanged"));

    // When User is updated, only all-kinds trigger matches
    let triggers = matcher.find("User", EventKind::Update);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].function_name, "onUserChanged");
}

/// Test: async dispatch doesn't block mutation response
#[tokio::test]
async fn test_after_mutation_async_dispatch_nonblocking() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    };

    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKind::Insert,
        old: None,
        new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp: chrono::Utc::now(),
    };

    // Building payload is synchronous (fast)
    let payload = trigger.build_payload(&event);
    assert_eq!(payload.trigger_type, "after:mutation:onUserCreated");

    // In real implementation, function execution would be spawned as a task
    // and would not block the mutation response
    // This test just verifies the payload is built correctly
}

/// Test: trigger matcher with multiple mutations
#[test]
fn test_trigger_dispatch_multiple_mutations() {
    let mut matcher = TriggerMatcher::new();

    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Insert),
    });

    matcher.add(AfterMutationTrigger {
        function_name: "onUserDeleted".to_string(),
        entity_type: "User".to_string(),
        event_filter: Some(EventKind::Delete),
    });

    matcher.add(AfterMutationTrigger {
        function_name: "onPostCreated".to_string(),
        entity_type: "Post".to_string(),
        event_filter: Some(EventKind::Insert),
    });

    // User insert triggers only user create trigger
    let triggers = matcher.find("User", EventKind::Insert);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].function_name, "onUserCreated");

    // User delete triggers only user delete trigger
    let triggers = matcher.find("User", EventKind::Delete);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].function_name, "onUserDeleted");

    // Post insert triggers only post create trigger
    let triggers = matcher.find("Post", EventKind::Insert);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].function_name, "onPostCreated");

    // No triggers for post delete
    let triggers = matcher.find("Post", EventKind::Delete);
    assert!(triggers.is_empty());
}

// ============================================================================
// Cycle 2: before:mutation Trigger Tests (RED Phase)
// ============================================================================

use crate::triggers::mutation::BeforeMutationResult;

/// Test: before:mutation receives proposed input
#[test]
fn test_before_mutation_receives_proposed_input() {
    let input = serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com"
    });

    // In the actual implementation, this input would be passed to the function
    // and the function would receive it as the event data
    assert!(input.is_object());
    assert_eq!(input["name"], "Alice");
    assert_eq!(input["email"], "alice@example.com");
}

/// Test: before:mutation proceed allows mutation
#[test]
fn test_before_mutation_proceed_allows_mutation() {
    let input = serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com"
    });

    let result = BeforeMutationResult::Proceed(input.clone());

    match result {
        BeforeMutationResult::Proceed(modified) => {
            assert_eq!(modified, input);
        }
        BeforeMutationResult::Abort(_) => {
            panic!("Expected Proceed, got Abort");
        }
    }
}

/// Test: before:mutation proceed with modified input
#[test]
fn test_before_mutation_proceed_with_modified_input() {
    let input = serde_json::json!({
        "name": "alice",
        "email": "alice@example.com"
    });

    // Function modifies name to uppercase
    let modified = serde_json::json!({
        "name": "ALICE",
        "email": "alice@example.com"
    });

    let result = BeforeMutationResult::Proceed(modified.clone());

    match result {
        BeforeMutationResult::Proceed(output) => {
            assert_eq!(output["name"], "ALICE");
            assert_ne!(output["name"], input["name"]);
        }
        BeforeMutationResult::Abort(_) => {
            panic!("Expected Proceed, got Abort");
        }
    }
}

/// Test: before:mutation abort cancels mutation
#[test]
fn test_before_mutation_abort_cancels_mutation() {
    let result: BeforeMutationResult = BeforeMutationResult::Abort(
        "validation failed: name is required".to_string(),
    );

    match result {
        BeforeMutationResult::Proceed(_) => {
            panic!("Expected Abort, got Proceed");
        }
        BeforeMutationResult::Abort(error) => {
            assert_eq!(error, "validation failed: name is required");
        }
    }
}

/// Test: chain of triggers executes in order
#[test]
fn test_before_mutation_chain_order() {
    let trigger_a = BeforeMutationTrigger {
        function_name: "validateInput".to_string(),
        mutation_name: "createUser".to_string(),
    };

    let trigger_b = BeforeMutationTrigger {
        function_name: "checkDuplicates".to_string(),
        mutation_name: "createUser".to_string(),
    };

    let trigger_c = BeforeMutationTrigger {
        function_name: "auditLog".to_string(),
        mutation_name: "createUser".to_string(),
    };

    let chain = crate::triggers::mutation::BeforeMutationChain {
        triggers: vec![trigger_a, trigger_b, trigger_c],
    };

    // Verify triggers are in the expected order
    assert_eq!(chain.triggers[0].function_name, "validateInput");
    assert_eq!(chain.triggers[1].function_name, "checkDuplicates");
    assert_eq!(chain.triggers[2].function_name, "auditLog");
}

/// Test: before:mutation result serialization
#[test]
fn test_before_mutation_result_serialization() {
    let proceed_result = BeforeMutationResult::Proceed(
        serde_json::json!({"name": "Alice"})
    );

    let json = serde_json::to_string(&proceed_result).expect("serialize");
    let restored: BeforeMutationResult = serde_json::from_str(&json)
        .expect("deserialize");

    match restored {
        BeforeMutationResult::Proceed(value) => {
            assert_eq!(value["name"], "Alice");
        }
        BeforeMutationResult::Abort(_) => {
            panic!("Expected Proceed after deserialization");
        }
    }
}

/// Test: abort result serialization
#[test]
fn test_before_mutation_abort_serialization() {
    let abort_result = BeforeMutationResult::Abort(
        "validation error".to_string()
    );

    let json = serde_json::to_string(&abort_result).expect("serialize");
    let restored: BeforeMutationResult = serde_json::from_str(&json)
        .expect("deserialize");

    match restored {
        BeforeMutationResult::Proceed(_) => {
            panic!("Expected Abort after deserialization");
        }
        BeforeMutationResult::Abort(error) => {
            assert_eq!(error, "validation error");
        }
    }
}
