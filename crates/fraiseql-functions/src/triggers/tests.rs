//! Integration tests for the trigger system.

use crate::types::EventPayload;
use crate::triggers::mutation::{
    AfterMutationTrigger, BeforeMutationTrigger, EntityEvent, EventKind,
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
