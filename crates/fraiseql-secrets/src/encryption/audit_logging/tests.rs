#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_audit_log_entry_creation() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
    assert_eq!(entry.user_id(), "user123");
    assert_eq!(entry.field_name(), "email");
    assert_eq!(entry.operation(), OperationType::Insert);
    assert_eq!(entry.status(), EventStatus::Success);
}

#[test]
fn test_audit_log_entry_with_failure() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Select, "req456", "sess789")
        .with_failure("Decryption failed: wrong key");
    assert_eq!(entry.status(), EventStatus::Failure);
    assert_eq!(entry.error_message(), Some("Decryption failed: wrong key"));
}

#[test]
fn test_audit_log_entry_with_context() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Update, "req456", "sess789")
        .with_context("ip_address", "192.168.1.1")
        .with_context("user_role", "admin");
    assert_eq!(entry.context().get("ip_address"), Some(&"192.168.1.1".to_string()));
    assert_eq!(entry.context().get("user_role"), Some(&"admin".to_string()));
}

#[test]
fn test_audit_log_entry_to_csv() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
    let csv = entry.to_csv();
    assert!(csv.contains("user123"));
    assert!(csv.contains("email"));
    assert!(csv.contains("insert"));
    assert!(csv.contains("success"));
}

#[test]
fn test_audit_log_entry_to_json_like() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Select, "req456", "sess789");
    let json = entry.to_json_like();
    assert!(json.contains("user123"));
    assert!(json.contains("email"));
    assert!(json.contains("select"));
}

#[test]
fn test_audit_logger_logging() {
    let mut logger = AuditLogger::new(10);
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
    let result = logger.log_entry(entry);
    result.unwrap_or_else(|e| panic!("expected Ok from log_entry: {e}"));
    assert_eq!(logger.entry_count(), 1);
}

#[test]
fn test_audit_logger_recent_entries() {
    let mut logger = AuditLogger::new(10);
    for i in 0..5 {
        let entry = AuditLogEntry::new(
            format!("user{}", i),
            "email",
            OperationType::Insert,
            "req456",
            "sess789",
        );
        let _ = logger.log_entry(entry);
    }
    let recent = logger.recent_entries(2);
    assert_eq!(recent.len(), 2);
}

#[test]
fn test_audit_logger_entries_for_user() {
    let mut logger = AuditLogger::new(10);
    for i in 0..3 {
        let entry = AuditLogEntry::new(
            "user123",
            format!("field{}", i),
            OperationType::Insert,
            "req456",
            "sess789",
        );
        let _ = logger.log_entry(entry);
    }
    for i in 0..2 {
        let entry = AuditLogEntry::new(
            "user456",
            format!("field{}", i),
            OperationType::Select,
            "req456",
            "sess789",
        );
        let _ = logger.log_entry(entry);
    }
    let user_entries = logger.entries_for_user("user123");
    assert_eq!(user_entries.len(), 3);
}

#[test]
fn test_audit_logger_entries_for_field() {
    let mut logger = AuditLogger::new(10);
    for i in 0..3 {
        let entry = AuditLogEntry::new(
            format!("user{}", i),
            "email",
            OperationType::Insert,
            "req456",
            "sess789",
        );
        let _ = logger.log_entry(entry);
    }
    let email_entries = logger.entries_for_field("email");
    assert_eq!(email_entries.len(), 3);
}

#[test]
fn test_audit_logger_failed_entries() {
    let mut logger = AuditLogger::new(10);
    let success =
        AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
    let failure =
        AuditLogEntry::new("user456", "phone", OperationType::Select, "req789", "sess123")
            .with_failure("Key not found");
    let _ = logger.log_entry(success);
    let _ = logger.log_entry(failure);
    let failed = logger.failed_entries();
    assert_eq!(failed.len(), 1);
}

#[test]
fn test_audit_logger_bounded_history() {
    let mut logger = AuditLogger::new(3);
    for i in 0..5 {
        let entry = AuditLogEntry::new(
            format!("user{}", i),
            "email",
            OperationType::Insert,
            "req456",
            "sess789",
        );
        let _ = logger.log_entry(entry);
    }
    assert_eq!(logger.entry_count(), 3);
}

#[test]
fn test_audit_logger_clear() {
    let mut logger = AuditLogger::new(10);
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
    let _ = logger.log_entry(entry);
    assert_eq!(logger.entry_count(), 1);
    logger.clear();
    assert_eq!(logger.entry_count(), 0);
}

#[test]
fn test_operation_type_display() {
    assert_eq!(OperationType::Insert.to_string(), "insert");
    assert_eq!(OperationType::Select.to_string(), "select");
    assert_eq!(OperationType::Update.to_string(), "update");
    assert_eq!(OperationType::Delete.to_string(), "delete");
}

#[test]
fn test_event_status_display() {
    assert_eq!(EventStatus::Success.to_string(), "success");
    assert_eq!(EventStatus::Failure.to_string(), "failure");
}

#[test]
fn test_audit_log_entry_with_security_context() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789")
        .with_security_context(Some("192.168.1.1"), Some("admin"));
    assert_eq!(entry.context().get("ip_address"), Some(&"192.168.1.1".to_string()));
    assert_eq!(entry.context().get("user_role"), Some(&"admin".to_string()));
}

#[test]
fn test_audit_log_entry_with_partial_security_context() {
    let entry = AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789")
        .with_security_context(Some("192.168.1.1"), None);
    assert_eq!(entry.context().get("ip_address"), Some(&"192.168.1.1".to_string()));
    assert!(!entry.context().contains_key("user_role"));
}

#[test]
fn test_audit_logger_entries_for_operation() {
    let mut logger = AuditLogger::new(10);
    let entry1 = AuditLogEntry::new("user1", "email", OperationType::Insert, "req1", "sess1");
    let entry2 = AuditLogEntry::new("user2", "phone", OperationType::Select, "req2", "sess2");
    let entry3 = AuditLogEntry::new("user3", "ssn", OperationType::Insert, "req3", "sess3");
    let _ = logger.log_entry(entry1);
    let _ = logger.log_entry(entry2);
    let _ = logger.log_entry(entry3);
    let inserts = logger.entries_for_operation(OperationType::Insert);
    assert_eq!(inserts.len(), 2);
    let selects = logger.entries_for_operation(OperationType::Select);
    assert_eq!(selects.len(), 1);
}

#[test]
fn test_audit_logger_successful_entries() {
    let mut logger = AuditLogger::new(10);
    let success = AuditLogEntry::new("user1", "email", OperationType::Insert, "req1", "sess1");
    let failure = AuditLogEntry::new("user2", "phone", OperationType::Select, "req2", "sess2")
        .with_failure("Key not found");
    let _ = logger.log_entry(success);
    let _ = logger.log_entry(failure);
    let successful = logger.successful_entries();
    assert_eq!(successful.len(), 1);
    assert_eq!(successful[0].user_id(), "user1");
}

#[test]
fn test_audit_logger_entries_for_user_operation() {
    let mut logger = AuditLogger::new(10);
    let entry1 = AuditLogEntry::new("user1", "email", OperationType::Insert, "req1", "sess1");
    let entry2 = AuditLogEntry::new("user1", "phone", OperationType::Select, "req2", "sess2");
    let entry3 = AuditLogEntry::new("user2", "email", OperationType::Insert, "req3", "sess3");
    let _ = logger.log_entry(entry1);
    let _ = logger.log_entry(entry2);
    let _ = logger.log_entry(entry3);
    let user1_inserts = logger.entries_for_user_operation("user1", OperationType::Insert);
    assert_eq!(user1_inserts.len(), 1);
    let user1_selects = logger.entries_for_user_operation("user1", OperationType::Select);
    assert_eq!(user1_selects.len(), 1);
}
