//! Tests for audit event structure (Phase 11.3 - GREEN)

use super::*;
use serde_json::json;

// ============================================================================
// Test 1: Audit Event Creation
// ============================================================================

/// Test basic audit event creation
#[test]
fn test_audit_event_new_user_action() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    assert_eq!(event.user_id, "user123");
    assert_eq!(event.username, "alice");
    assert_eq!(event.ip_address, "192.168.1.1");
    assert_eq!(event.resource_type, "users");
    assert_eq!(event.action, "create");
    assert_eq!(event.status, "success");
    assert!(!event.id.is_empty());
    assert!(!event.timestamp.is_empty());
}

/// Test audit event builder methods
#[test]
fn test_audit_event_builder() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "update",
        "success",
    )
    .with_resource_id("usr-456")
    .with_tenant_id("acme-corp")
    .with_before_state(json!({"name": "Alice"}))
    .with_after_state(json!({"name": "Alice Smith"}))
    .with_metadata("user_agent", json!("Mozilla/5.0..."));

    assert_eq!(event.resource_id, Some("usr-456".to_string()));
    assert_eq!(event.tenant_id, Some("acme-corp".to_string()));
    assert!(event.before_state.is_some());
    assert!(event.after_state.is_some());
    assert!(event.metadata.get("user_agent").is_some());
}

/// Test audit event with error
#[test]
fn test_audit_event_with_error() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "delete",
        "failure",
    )
    .with_error("User not found");

    assert_eq!(event.status, "failure");
    assert_eq!(event.error_message, Some("User not found".to_string()));
}

// ============================================================================
// Test 2: Audit Event Validation
// ============================================================================

/// Test valid audit event passes validation
#[test]
fn test_audit_event_valid() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    assert!(event.validate().is_ok());
}

/// Test missing user_id fails validation
#[test]
fn test_audit_event_missing_user_id() {
    let mut event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );
    event.user_id = String::new();

    assert!(event.validate().is_err());
}

/// Test invalid status fails validation
#[test]
fn test_audit_event_invalid_status() {
    let mut event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );
    event.status = "invalid_status".to_string();

    assert!(event.validate().is_err());
}

/// Test failure status without error message fails validation
#[test]
fn test_audit_event_failure_without_error() {
    let mut event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "failure",
    );
    event.error_message = None;

    assert!(event.validate().is_err());
}

// ============================================================================
// Test 3: Audit Event Serialization
// ============================================================================

/// Test audit event serializes to JSON
#[test]
fn test_audit_event_serialization() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    )
    .with_resource_id("usr-456")
    .with_tenant_id("acme-corp");

    let json = serde_json::to_string(&event).expect("Should serialize");
    assert!(json.contains("user123"));
    assert!(json.contains("alice"));
    assert!(json.contains("192.168.1.1"));
}

/// Test audit event deserializes from JSON
#[test]
fn test_audit_event_deserialization() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    let json = serde_json::to_string(&event).expect("Should serialize");
    let deserialized: AuditEvent =
        serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.user_id, event.user_id);
    assert_eq!(deserialized.username, event.username);
}

// ============================================================================
// Test 4: Audit Query Filters
// ============================================================================

/// Test default query filters
#[test]
fn test_audit_query_filters_default() {
    let filters = AuditQueryFilters::default();

    assert_eq!(filters.event_type, None);
    assert_eq!(filters.user_id, None);
    assert_eq!(filters.limit, Some(100));
}

/// Test query filters with values
#[test]
fn test_audit_query_filters_with_values() {
    let filters = AuditQueryFilters {
        event_type: Some("users_create".to_string()),
        user_id: Some("user123".to_string()),
        status: Some("success".to_string()),
        limit: Some(50),
        ..Default::default()
    };

    assert_eq!(filters.event_type, Some("users_create".to_string()));
    assert_eq!(filters.user_id, Some("user123".to_string()));
    assert_eq!(filters.status, Some("success".to_string()));
    assert_eq!(filters.limit, Some(50));
}

// ============================================================================
// Test 5: Audit Event Edge Cases
// ============================================================================

/// Test audit event with optional fields
#[test]
fn test_audit_event_optional_fields() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "read",
        "success",
    );

    assert_eq!(event.resource_id, None);
    assert_eq!(event.before_state, None);
    assert_eq!(event.after_state, None);
    assert_eq!(event.error_message, None);
    assert_eq!(event.tenant_id, None);
}

/// Test audit event with all valid statuses
#[test]
fn test_audit_event_all_statuses() {
    let statuses = vec!["success", "failure", "denied"];

    for status in statuses {
        let mut event = AuditEvent::new_user_action(
            "user123",
            "alice",
            "192.168.1.1",
            "users",
            "action",
            status,
        );

        if status == "failure" {
            event.error_message = Some("Error message".to_string());
        }

        assert!(event.validate().is_ok());
    }
}

/// Test audit event timestamp format
#[test]
fn test_audit_event_timestamp_format() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    // Should be ISO 8601 format
    assert!(event.timestamp.contains("T"));
    assert!(event.timestamp.contains("Z") || event.timestamp.contains("+"));
}

/// Test audit event ID is UUID
#[test]
fn test_audit_event_id_is_uuid() {
    let event = AuditEvent::new_user_action(
        "user123",
        "alice",
        "192.168.1.1",
        "users",
        "create",
        "success",
    );

    // Should be valid UUID format
    let id_parts: Vec<&str> = event.id.split('-').collect();
    assert_eq!(id_parts.len(), 5);
}
