//! Audit logging for encryption/decryption operations
//!
//! Provides comprehensive logging of all field-level encryption operations
//! for compliance (HIPAA, PCI-DSS, GDPR, SOC 2) and security monitoring.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::secrets_manager::SecretsError;

/// Encryption operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// INSERT operation
    Insert,
    /// SELECT operation
    Select,
    /// UPDATE operation
    Update,
    /// DELETE operation
    Delete,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insert => write!(f, "insert"),
            Self::Select => write!(f, "select"),
            Self::Update => write!(f, "update"),
            Self::Delete => write!(f, "delete"),
        }
    }
}

/// Encryption event status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    /// Operation succeeded
    Success,
    /// Operation failed
    Failure,
}

impl std::fmt::Display for EventStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
        }
    }
}

/// Single audit log entry for encryption operation
#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    /// Timestamp of operation
    timestamp:     DateTime<Utc>,
    /// User performing operation
    user_id:       String,
    /// Field name being encrypted/decrypted
    field_name:    String,
    /// Operation type
    operation:     OperationType,
    /// Success or failure
    status:        EventStatus,
    /// Error message if failed
    error_message: Option<String>,
    /// Request ID for correlation
    request_id:    String,
    /// Session ID for tracking
    session_id:    String,
    /// Additional context data
    context:       HashMap<String, String>,
}

impl AuditLogEntry {
    /// Create new audit log entry
    pub fn new(
        user_id: impl Into<String>,
        field_name: impl Into<String>,
        operation: OperationType,
        request_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            user_id: user_id.into(),
            field_name: field_name.into(),
            operation,
            status: EventStatus::Success,
            error_message: None,
            request_id: request_id.into(),
            session_id: session_id.into(),
            context: HashMap::new(),
        }
    }

    /// Mark entry as failed
    pub fn with_failure(mut self, error: impl Into<String>) -> Self {
        self.status = EventStatus::Failure;
        self.error_message = Some(error.into());
        self
    }

    /// Add context data
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Add common security context data
    pub fn with_security_context(self, ip_address: Option<&str>, user_role: Option<&str>) -> Self {
        let mut entry = self;
        if let Some(ip) = ip_address {
            entry = entry.with_context("ip_address", ip);
        }
        if let Some(role) = user_role {
            entry = entry.with_context("user_role", role);
        }
        entry
    }

    /// Get timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get user ID
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get field name
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Get operation type
    pub fn operation(&self) -> OperationType {
        self.operation
    }

    /// Get status
    pub fn status(&self) -> EventStatus {
        self.status
    }

    /// Get error message
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Get request ID
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get context data
    pub fn context(&self) -> &HashMap<String, String> {
        &self.context
    }

    /// Convert to CSV for logging
    pub fn to_csv(&self) -> String {
        let error = self.error_message.as_deref().unwrap_or("");
        format!(
            "{},{},{},{},{},{},{},{}",
            self.timestamp.to_rfc3339(),
            self.user_id,
            self.field_name,
            self.operation,
            self.status,
            error,
            self.request_id,
            self.session_id
        )
    }

    /// Convert to JSON-like string for logging
    pub fn to_json_like(&self) -> String {
        format!(
            "{{ \"timestamp\": \"{}\", \"user_id\": \"{}\", \"field_name\": \"{}\", \
             \"operation\": \"{}\", \"status\": \"{}\", \"error\": \"{}\", \
             \"request_id\": \"{}\", \"session_id\": \"{}\" }}",
            self.timestamp.to_rfc3339(),
            self.user_id,
            self.field_name,
            self.operation,
            self.status,
            self.error_message.as_deref().unwrap_or(""),
            self.request_id,
            self.session_id
        )
    }
}

/// Audit logger for encryption operations
///
/// Handles logging of all encryption/decryption events for compliance.
pub struct AuditLogger {
    /// In-memory log entries (for testing)
    entries:     Vec<AuditLogEntry>,
    /// Maximum entries to keep in memory
    max_entries: usize,
}

impl AuditLogger {
    /// Create new audit logger
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    /// Log encryption operation
    pub fn log_entry(&mut self, entry: AuditLogEntry) -> Result<(), SecretsError> {
        // Keep bounded history
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }

        self.entries.push(entry);
        Ok(())
    }

    /// Internal filter helper for reducing duplication
    fn filter_entries<F>(&self, predicate: F) -> Vec<AuditLogEntry>
    where
        F: Fn(&&AuditLogEntry) -> bool,
    {
        self.entries.iter().filter(predicate).cloned().collect()
    }

    /// Get recent entries
    pub fn recent_entries(&self, count: usize) -> Vec<AuditLogEntry> {
        let start = if self.entries.len() > count {
            self.entries.len() - count
        } else {
            0
        };
        self.entries[start..].to_vec()
    }

    /// Get entries for specific user
    pub fn entries_for_user(&self, user_id: &str) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.user_id == user_id)
    }

    /// Get entries for specific field
    pub fn entries_for_field(&self, field_name: &str) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.field_name == field_name)
    }

    /// Get entries for specific operation type
    pub fn entries_for_operation(&self, operation: OperationType) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.operation == operation)
    }

    /// Get failed operations
    pub fn failed_entries(&self) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.status == EventStatus::Failure)
    }

    /// Get successful operations
    pub fn successful_entries(&self) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.status == EventStatus::Success)
    }

    /// Get entries for specific user and operation
    pub fn entries_for_user_operation(
        &self,
        user_id: &str,
        operation: OperationType,
    ) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.user_id == user_id && e.operation == operation)
    }

    /// Get entry count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_entry_creation() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
        assert_eq!(entry.user_id(), "user123");
        assert_eq!(entry.field_name(), "email");
        assert_eq!(entry.operation(), OperationType::Insert);
        assert_eq!(entry.status(), EventStatus::Success);
    }

    #[test]
    fn test_audit_log_entry_with_failure() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Select, "req456", "sess789")
                .with_failure("Decryption failed: wrong key");
        assert_eq!(entry.status(), EventStatus::Failure);
        assert_eq!(entry.error_message(), Some("Decryption failed: wrong key"));
    }

    #[test]
    fn test_audit_log_entry_with_context() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Update, "req456", "sess789")
                .with_context("ip_address", "192.168.1.1")
                .with_context("user_role", "admin");
        assert_eq!(entry.context().get("ip_address"), Some(&"192.168.1.1".to_string()));
        assert_eq!(entry.context().get("user_role"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_audit_log_entry_to_csv() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
        let csv = entry.to_csv();
        assert!(csv.contains("user123"));
        assert!(csv.contains("email"));
        assert!(csv.contains("insert"));
        assert!(csv.contains("success"));
    }

    #[test]
    fn test_audit_log_entry_to_json_like() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Select, "req456", "sess789");
        let json = entry.to_json_like();
        assert!(json.contains("user123"));
        assert!(json.contains("email"));
        assert!(json.contains("select"));
    }

    #[test]
    fn test_audit_logger_logging() {
        let mut logger = AuditLogger::new(10);
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
        let result = logger.log_entry(entry);
        assert!(result.is_ok());
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
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789");
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
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789")
                .with_security_context(Some("192.168.1.1"), Some("admin"));
        assert_eq!(entry.context().get("ip_address"), Some(&"192.168.1.1".to_string()));
        assert_eq!(entry.context().get("user_role"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_audit_log_entry_with_partial_security_context() {
        let entry =
            AuditLogEntry::new("user123", "email", OperationType::Insert, "req456", "sess789")
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
}
