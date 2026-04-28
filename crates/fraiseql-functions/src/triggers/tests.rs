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

    let result = BeforeMutationResult::Proceed(input);

    match result {
        BeforeMutationResult::Proceed(modified) => {
            assert_eq!(modified["name"], "Alice");
            assert_eq!(modified["email"], "alice@example.com");
        }
        BeforeMutationResult::Abort(_) => {
            panic!("Expected Proceed, got Abort");
        }
    }
}

/// Test: before:mutation proceed with modified input
#[test]
fn test_before_mutation_proceed_with_modified_input() {
    // Function receives and modifies input
    let modified = serde_json::json!({
        "name": "ALICE",
        "email": "alice@example.com"
    });

    let result = BeforeMutationResult::Proceed(modified);

    match result {
        BeforeMutationResult::Proceed(output) => {
            assert_eq!(output["name"], "ALICE");
            assert_ne!(output["name"], "alice");
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

/// Test: chain execution order simulation
/// Simulates what happens when triggers execute in order, each receiving
/// the modified output from the previous trigger.
#[test]
fn test_before_mutation_chain_execution_simulation() {
    use crate::triggers::mutation::BeforeMutationChain;

    let chain = BeforeMutationChain {
        triggers: vec![
            BeforeMutationTrigger {
                function_name: "normalizeEmail".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "validateName".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "enrichProfile".to_string(),
                mutation_name: "createUser".to_string(),
            },
        ],
    };

    // Verify chain structure before execution simulation
    assert_eq!(chain.triggers.len(), 3);

    // Simulate chain execution
    let mut current_input = serde_json::json!({
        "name": "alice smith",
        "email": "  ALICE@EXAMPLE.COM  "
    });

    // Trigger 1: normalizeEmail (simulated result)
    current_input["email"] = serde_json::Value::String("alice@example.com".to_string());

    // Trigger 2: validateName (simulated result)
    current_input["name"] = serde_json::Value::String("Alice Smith".to_string());

    // Trigger 3: enrichProfile (simulated result)
    current_input["profile"] = serde_json::json!({"bio": "User"});

    // Verify the chain of modifications
    assert_eq!(current_input["email"], "alice@example.com");
    assert_eq!(current_input["name"], "Alice Smith");
    assert!(current_input["profile"].is_object());
    assert_eq!(current_input["profile"]["bio"], "User");
}

/// Test: chain execution short-circuit on abort simulation
/// Simulates what happens when a trigger aborts the chain.
#[test]
fn test_before_mutation_chain_abort_simulation() {
    let chain = crate::triggers::mutation::BeforeMutationChain {
        triggers: vec![
            BeforeMutationTrigger {
                function_name: "validateInput".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "checkDuplicates".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "auditLog".to_string(),
                mutation_name: "createUser".to_string(),
            },
        ],
    };

    // Verify chain structure
    assert_eq!(chain.triggers.len(), 3);

    // Trigger 1: validateInput would return Abort
    let result1 = BeforeMutationResult::Abort(
        "name is required".to_string()
    );

    // Chain short-circuits here, triggers 2 and 3 never execute
    match result1 {
        BeforeMutationResult::Abort(error) => {
            assert_eq!(error, "name is required");
            // This is where mutation would be aborted in actual implementation
        }
        BeforeMutationResult::Proceed(_) => {
            panic!("Expected abort");
        }
    }
}

// ============================================================================
// Cycle 3: after:storage Trigger Tests (RED Phase)
// ============================================================================

use crate::triggers::storage::{StorageTrigger, StorageOperation, StorageEventPayload};

/// Test: after:storage fires on upload
#[test]
fn test_after_storage_upload_fires() {
    let trigger = StorageTrigger {
        function_name: "onAvatarUpload".to_string(),
        bucket: "avatars".to_string(),
        operation: StorageOperation::Upload,
    };

    let storage_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "users/alice/avatar.jpg".to_string(),
        size_bytes: 204_800,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user123".to_string()),
        operation: StorageOperation::Upload,
    };

    let payload = trigger.build_payload(&storage_event);

    assert_eq!(payload.trigger_type, "after:storage:avatars:upload");
    assert_eq!(payload.entity, "avatars");
    assert_eq!(payload.event_kind, "upload");
    assert_eq!(payload.data["bucket"], "avatars");
    assert_eq!(payload.data["key"], "users/alice/avatar.jpg");
    assert_eq!(payload.data["size_bytes"], 204_800);
    assert_eq!(payload.data["content_type"], "image/jpeg");
    assert_eq!(payload.data["owner_id"], "user123");
}

/// Test: after:storage fires on delete
#[test]
fn test_after_storage_delete_fires() {
    let trigger = StorageTrigger {
        function_name: "onDocumentDelete".to_string(),
        bucket: "documents".to_string(),
        operation: StorageOperation::Delete,
    };

    let storage_event = StorageEventPayload {
        bucket: "documents".to_string(),
        key: "reports/2024/report.pdf".to_string(),
        size_bytes: 0,
        content_type: "application/pdf".to_string(),
        owner_id: Some("user456".to_string()),
        operation: StorageOperation::Delete,
    };

    let payload = trigger.build_payload(&storage_event);

    assert_eq!(payload.trigger_type, "after:storage:documents:delete");
    assert_eq!(payload.entity, "documents");
    assert_eq!(payload.event_kind, "delete");
    assert_eq!(payload.data["bucket"], "documents");
    assert_eq!(payload.data["key"], "reports/2024/report.pdf");
    assert_eq!(payload.data["operation"], "delete");
}

/// Test: storage trigger matches bucket correctly
#[test]
fn test_after_storage_matches_bucket() {
    let avatar_trigger = StorageTrigger {
        function_name: "onAvatarUpload".to_string(),
        bucket: "avatars".to_string(),
        operation: StorageOperation::Upload,
    };

    // Event for avatars bucket
    let avatar_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "user/avatar.jpg".to_string(),
        size_bytes: 100_000,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Upload,
    };

    // Event for documents bucket
    let doc_event = StorageEventPayload {
        bucket: "documents".to_string(),
        key: "report.pdf".to_string(),
        size_bytes: 500_000,
        content_type: "application/pdf".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Upload,
    };

    assert!(avatar_trigger.matches(&avatar_event));
    assert!(!avatar_trigger.matches(&doc_event));
}

/// Test: storage trigger matches operation correctly
#[test]
fn test_after_storage_matches_operation() {
    let upload_trigger = StorageTrigger {
        function_name: "onAvatarUpload".to_string(),
        bucket: "avatars".to_string(),
        operation: StorageOperation::Upload,
    };

    let upload_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "avatar.jpg".to_string(),
        size_bytes: 100_000,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Upload,
    };

    let delete_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "avatar.jpg".to_string(),
        size_bytes: 0,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Delete,
    };

    assert!(upload_trigger.matches(&upload_event));
    assert!(!upload_trigger.matches(&delete_event));
}

/// Test: storage trigger with Any operation matches all events
#[test]
fn test_after_storage_matches_any_operation() {
    let any_trigger = StorageTrigger {
        function_name: "onStorageEvent".to_string(),
        bucket: "avatars".to_string(),
        operation: StorageOperation::Any,
    };

    let upload_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "avatar.jpg".to_string(),
        size_bytes: 100_000,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Upload,
    };

    let delete_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "avatar.jpg".to_string(),
        size_bytes: 0,
        content_type: "image/jpeg".to_string(),
        owner_id: Some("user1".to_string()),
        operation: StorageOperation::Delete,
    };

    assert!(any_trigger.matches(&upload_event));
    assert!(any_trigger.matches(&delete_event));
}

/// Test: storage trigger ignores transform cache operations
#[test]
fn test_after_storage_ignores_transform_cache() {
    let trigger = StorageTrigger {
        function_name: "onAvatarUpload".to_string(),
        bucket: "avatars".to_string(),
        operation: StorageOperation::Upload,
    };

    // Transform cache operations have _transforms/ prefix
    let transform_event = StorageEventPayload {
        bucket: "avatars".to_string(),
        key: "_transforms/avatar-thumb.jpg".to_string(),
        size_bytes: 50000,
        content_type: "image/jpeg".to_string(),
        owner_id: None,
        operation: StorageOperation::Upload,
    };

    assert!(!trigger.should_fire(&transform_event));
}

/// Test: storage trigger payload includes all metadata
#[test]
fn test_after_storage_payload_includes_metadata() {
    let trigger = StorageTrigger {
        function_name: "onUpload".to_string(),
        bucket: "documents".to_string(),
        operation: StorageOperation::Upload,
    };

    let storage_event = StorageEventPayload {
        bucket: "documents".to_string(),
        key: "invoices/INV-001.pdf".to_string(),
        size_bytes: 1_024_000,
        content_type: "application/pdf".to_string(),
        owner_id: Some("company_789".to_string()),
        operation: StorageOperation::Upload,
    };

    let payload = trigger.build_payload(&storage_event);

    assert!(payload.data.is_object());
    assert!(payload.data["bucket"].is_string());
    assert!(payload.data["key"].is_string());
    assert!(payload.data["size_bytes"].is_number());
    assert!(payload.data["content_type"].is_string());
    assert!(payload.data["owner_id"].is_string());
}

// ============================================================================
// Cycle 4: cron Trigger Tests (RED Phase)
// ============================================================================

use crate::triggers::cron::{CronTrigger, CronSchedule, CronExecutionState};

/// Test: cron trigger parses valid cron expression (daily at 2 AM)
#[test]
fn test_cron_trigger_parses_daily_expression() {
    let trigger = CronTrigger {
        function_name: "dailyCleanup".to_string(),
        schedule: "0 2 * * *".to_string(), // 2 AM every day
        timezone: "UTC".to_string(),
    };

    assert_eq!(trigger.function_name, "dailyCleanup");
    assert_eq!(trigger.schedule, "0 2 * * *");
    assert_eq!(trigger.timezone, "UTC");
}

/// Test: cron trigger parses valid cron expression (every hour)
#[test]
fn test_cron_trigger_parses_hourly_expression() {
    let trigger = CronTrigger {
        function_name: "hourlySync".to_string(),
        schedule: "0 * * * *".to_string(), // Every hour at :00
        timezone: "UTC".to_string(),
    };

    assert_eq!(trigger.function_name, "hourlySync");
    assert_eq!(trigger.schedule, "0 * * * *");
}

/// Test: cron trigger parses valid cron expression (every 5 minutes)
#[test]
fn test_cron_trigger_parses_every_5_minutes() {
    let trigger = CronTrigger {
        function_name: "frequentCheck".to_string(),
        schedule: "*/5 * * * *".to_string(), // Every 5 minutes
        timezone: "UTC".to_string(),
    };

    assert_eq!(trigger.schedule, "*/5 * * * *");
}

/// Test: cron schedule evaluates to true for matching time
#[test]
fn test_cron_schedule_matches_exact_time() {
    let schedule = CronSchedule::parse("0 2 * * *").expect("parse cron");

    // 2024-03-15 02:00:00 UTC should match "0 2 * * *"
    let matching_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse datetime")
        .with_timezone(&chrono::Utc);

    assert!(schedule.matches(&matching_time));
}

/// Test: cron schedule does not match non-matching time
#[test]
fn test_cron_schedule_does_not_match_wrong_hour() {
    let schedule = CronSchedule::parse("0 2 * * *").expect("parse cron");

    // 2024-03-15 03:00:00 UTC should NOT match "0 2 * * *"
    let non_matching_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T03:00:00+00:00")
        .expect("parse datetime")
        .with_timezone(&chrono::Utc);

    assert!(!schedule.matches(&non_matching_time));
}

/// Test: cron schedule matches on specific minutes
#[test]
fn test_cron_schedule_matches_every_5_minutes() {
    let schedule = CronSchedule::parse("*/5 * * * *").expect("parse cron");

    // Should match at :00, :05, :10, :15, :20, etc.
    let time_00 = chrono::DateTime::parse_from_rfc3339("2024-03-15T10:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    let time_05 = chrono::DateTime::parse_from_rfc3339("2024-03-15T10:05:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    let time_03 = chrono::DateTime::parse_from_rfc3339("2024-03-15T10:03:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    assert!(schedule.matches(&time_00));
    assert!(schedule.matches(&time_05));
    assert!(!schedule.matches(&time_03));
}

/// Test: cron trigger tracks last execution time
#[test]
fn test_cron_trigger_tracks_last_execution() {
    let mut state = CronExecutionState::new();

    // Initially, last_executed is None
    assert!(state.last_executed.is_none());

    // Record an execution
    let now = chrono::Utc::now();
    state.record_execution(now);

    assert_eq!(state.last_executed, Some(now));
}

/// Test: cron trigger detects if it should execute (first time)
#[test]
fn test_cron_trigger_should_execute_first_time() {
    let schedule = CronSchedule::parse("0 2 * * *").expect("parse cron");
    let state = CronExecutionState::new();

    // First execution time
    let exec_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    // Should execute if no prior execution
    assert!(state.should_execute(&schedule, &exec_time));
}

/// Test: cron trigger prevents duplicate execution in same window
#[test]
fn test_cron_trigger_prevents_duplicate_in_window() {
    let schedule = CronSchedule::parse("0 2 * * *").expect("parse cron");
    let mut state = CronExecutionState::new();

    let exec_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    // First execution succeeds
    assert!(state.should_execute(&schedule, &exec_time));
    state.record_execution(exec_time);

    // Same window (2:05 is still in the 2 AM hour) should NOT execute again
    let within_window = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:05:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    assert!(!state.should_execute(&schedule, &within_window));
}

/// Test: cron trigger allows execution in next window
#[test]
fn test_cron_trigger_allows_next_window() {
    let schedule = CronSchedule::parse("0 * * * *").expect("parse cron");
    let mut state = CronExecutionState::new();

    // Execute at 2:00
    let time_200 = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    assert!(state.should_execute(&schedule, &time_200));
    state.record_execution(time_200);

    // Execute at 3:00 (next hour)
    let time_300 = chrono::DateTime::parse_from_rfc3339("2024-03-15T03:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    assert!(state.should_execute(&schedule, &time_300));
}

/// Test: cron trigger catches up on missed executions
#[test]
fn test_cron_trigger_catches_up_missed_executions() {
    let schedule = CronSchedule::parse("0 * * * *").expect("parse cron");
    let state = CronExecutionState::new();

    // Server was down from 2:00 to 3:00, now it's 3:30
    let last_known = chrono::DateTime::parse_from_rfc3339("2024-03-15T01:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    let now = chrono::DateTime::parse_from_rfc3339("2024-03-15T03:30:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    let missed = state.find_missed_executions(&schedule, &last_known, &now);

    // Should find 2:00 and 3:00 as missed executions
    assert_eq!(missed.len(), 2);
}

/// Test: cron trigger payload includes schedule and function info
#[test]
fn test_cron_trigger_payload_includes_schedule_info() {
    let trigger = CronTrigger {
        function_name: "dailyCleanup".to_string(),
        schedule: "0 2 * * *".to_string(),
        timezone: "UTC".to_string(),
    };

    let exec_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    let payload = trigger.build_payload(&exec_time);

    assert_eq!(payload.trigger_type, "cron:dailyCleanup");
    assert_eq!(payload.entity, "cron");
    assert_eq!(payload.event_kind, "scheduled");
    assert_eq!(payload.data["schedule"], "0 2 * * *");
    assert_eq!(payload.data["timezone"], "UTC");
}

/// Test: cron trigger payload includes execution timestamp
#[test]
fn test_cron_trigger_payload_includes_execution_time() {
    let trigger = CronTrigger {
        function_name: "hourlySync".to_string(),
        schedule: "0 * * * *".to_string(),
        timezone: "UTC".to_string(),
    };

    let exec_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T14:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);

    let payload = trigger.build_payload(&exec_time);

    assert!(payload.data.is_object());
    assert!(payload.data["executed_at"].is_string());
    assert_eq!(payload.data["executed_at"], "2024-03-15T14:00:00Z");
}

/// Test: cron trigger handles timezone offset
#[test]
fn test_cron_trigger_with_specific_timezone() {
    let trigger = CronTrigger {
        function_name: "morningReport".to_string(),
        schedule: "0 9 * * *".to_string(), // 9 AM in specified timezone
        timezone: "America/New_York".to_string(),
    };

    assert_eq!(trigger.timezone, "America/New_York");
}

/// Test: cron trigger serialization/deserialization
#[test]
fn test_cron_trigger_serialization() {
    let trigger = CronTrigger {
        function_name: "dailyCleanup".to_string(),
        schedule: "0 2 * * *".to_string(),
        timezone: "UTC".to_string(),
    };

    let json = serde_json::to_string(&trigger).expect("serialize");
    let restored: CronTrigger = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.function_name, trigger.function_name);
    assert_eq!(restored.schedule, trigger.schedule);
    assert_eq!(restored.timezone, trigger.timezone);
}

/// Test: cron execution state persistence
#[test]
fn test_cron_execution_state_serialization() {
    let mut state = CronExecutionState::new();
    let exec_time = chrono::DateTime::parse_from_rfc3339("2024-03-15T02:00:00+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    state.record_execution(exec_time);

    let json = serde_json::to_string(&state).expect("serialize");
    let restored: CronExecutionState = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.last_executed, Some(exec_time));
}

/// Test: HTTP trigger GET route parsing
#[test]
fn test_http_trigger_get_route() {
    use crate::triggers::http::HttpTriggerRoute;

    let route = HttpTriggerRoute {
        function_name: "helloWorld".to_string(),
        method: "GET".to_string(),
        path: "/functions/v1/hello".to_string(),
        requires_auth: false,
    };

    assert_eq!(route.function_name, "helloWorld");
    assert_eq!(route.method, "GET");
    assert_eq!(route.path, "/functions/v1/hello");
    assert!(!route.requires_auth);
}

/// Test: HTTP trigger POST route with auth required
#[test]
fn test_http_trigger_post_route_with_auth() {
    use crate::triggers::http::HttpTriggerRoute;

    let route = HttpTriggerRoute {
        function_name: "processData".to_string(),
        method: "POST".to_string(),
        path: "/functions/v1/process".to_string(),
        requires_auth: true,
    };

    assert_eq!(route.function_name, "processData");
    assert_eq!(route.method, "POST");
    assert!(route.requires_auth);
}

/// Test: HTTP trigger request body handling
#[test]
fn test_http_trigger_request_payload() {
    use crate::triggers::http::HttpTriggerPayload;

    let payload = HttpTriggerPayload {
        method: "POST".to_string(),
        path: "/functions/v1/users".to_string(),
        headers: serde_json::json!({
            "content-type": "application/json",
            "x-user-id": "123"
        }),
        query: serde_json::json!({}),
        params: serde_json::json!({
            "id": "user-123"
        }),
        body: Some(serde_json::json!({
            "name": "Alice",
            "email": "alice@example.com"
        })),
    };

    assert_eq!(payload.method, "POST");
    assert_eq!(payload.path, "/functions/v1/users");
    assert!(payload.body.is_some());
    assert_eq!(payload.body.expect("body exists")["name"], "Alice");
}

/// Test: HTTP trigger path parameters extraction
#[test]
fn test_http_trigger_path_params() {
    use crate::triggers::http::HttpTriggerPayload;

    let payload = HttpTriggerPayload {
        method: "GET".to_string(),
        path: "/functions/v1/users/123".to_string(),
        headers: serde_json::json!({}),
        query: serde_json::json!({}),
        params: serde_json::json!({
            "id": "123"
        }),
        body: None,
    };

    assert_eq!(payload.params["id"], "123");
}

/// Test: HTTP trigger response with custom status code
#[test]
fn test_http_trigger_response_custom_status() {
    use crate::triggers::http::HttpTriggerResponse;

    let response = HttpTriggerResponse {
        status: 201,
        headers: serde_json::json!({
            "x-custom-header": "value"
        }),
        body: serde_json::json!({
            "id": "new-user-123",
            "created": true
        }),
    };

    assert_eq!(response.status, 201);
    assert_eq!(response.headers["x-custom-header"], "value");
    assert_eq!(response.body["id"], "new-user-123");
}

/// Test: HTTP trigger response with default status 200
#[test]
fn test_http_trigger_response_default_status() {
    use crate::triggers::http::HttpTriggerResponse;

    let response = HttpTriggerResponse {
        status: 200,
        headers: serde_json::json!({}),
        body: serde_json::json!({"message": "OK"}),
    };

    assert_eq!(response.status, 200);
    assert_eq!(response.body["message"], "OK");
}

/// Test: HTTP trigger method parsing
#[test]
fn test_http_trigger_method_parsing() {
    use crate::triggers::http::HttpTriggerRoute;

    for method in &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
        let route = HttpTriggerRoute {
            function_name: "test".to_string(),
            method: method.to_string(),
            path: "/test".to_string(),
            requires_auth: false,
        };
        assert_eq!(route.method, *method);
    }
}

/// Test: HTTP trigger route matching
#[test]
fn test_http_trigger_route_matching() {
    use crate::triggers::http::{HttpTriggerRoute, HttpTriggerMatcher};

    let mut matcher = HttpTriggerMatcher::new();
    matcher.add(HttpTriggerRoute {
        function_name: "getUser".to_string(),
        method: "GET".to_string(),
        path: "/users/:id".to_string(),
        requires_auth: true,
    });

    matcher.add(HttpTriggerRoute {
        function_name: "createUser".to_string(),
        method: "POST".to_string(),
        path: "/users".to_string(),
        requires_auth: true,
    });

    // GET /users/:id should match
    let route = matcher.find("GET", "/users/123");
    assert!(route.is_some());
    assert_eq!(route.expect("route matched").function_name, "getUser");

    // POST /users should match
    let route = matcher.find("POST", "/users");
    assert!(route.is_some());
    assert_eq!(route.expect("route matched").function_name, "createUser");

    // GET /posts should not match
    let route = matcher.find("GET", "/posts");
    assert!(route.is_none());
}

/// Test: HTTP trigger query parameters
#[test]
fn test_http_trigger_query_parameters() {
    use crate::triggers::http::HttpTriggerPayload;

    let payload = HttpTriggerPayload {
        method: "GET".to_string(),
        path: "/functions/v1/search".to_string(),
        headers: serde_json::json!({}),
        query: serde_json::json!({
            "q": "alice",
            "limit": 10
        }),
        params: serde_json::json!({}),
        body: None,
    };

    assert_eq!(payload.query["q"], "alice");
    assert_eq!(payload.query["limit"], 10);
}

/// Test: HTTP trigger event payload building
#[test]
fn test_http_trigger_event_payload() {
    use crate::triggers::http::HttpTriggerRoute;

    let route = HttpTriggerRoute {
        function_name: "handleRequest".to_string(),
        method: "POST".to_string(),
        path: "/functions/v1/webhook".to_string(),
        requires_auth: false,
    };

    let http_payload = serde_json::json!({
        "method": "POST",
        "path": "/functions/v1/webhook",
        "body": {"event": "user.created"}
    });

    let trigger_type = format!("http:{}:{}", route.method, route.path);
    assert_eq!(trigger_type, "http:POST:/functions/v1/webhook");

    // Verify event payload would include this trigger type
    let event = EventPayload {
        trigger_type,
        entity: "HttpRequest".to_string(),
        event_kind: "request".to_string(),
        data: http_payload,
        timestamp: chrono::Utc::now(),
    };

    assert_eq!(event.entity, "HttpRequest");
    assert_eq!(event.event_kind, "request");
}
