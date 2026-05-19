//! Audit logging for encryption/decryption operations
//!
//! Provides comprehensive logging of all field-level encryption operations
//! for compliance (HIPAA, PCI-DSS, GDPR, SOC 2) and security monitoring.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::secrets_manager::SecretsError;

/// RFC 4180 CSV quoting: wrap the field in double-quotes and escape internal
/// double-quotes by doubling them. This prevents injection via fields that
/// contain commas, newlines, or quote characters.
fn csv_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' {
            out.push('"'); // double the quote
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Encryption operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
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
    timestamp: DateTime<Utc>,
    /// User performing operation
    user_id: String,
    /// Field name being encrypted/decrypted
    field_name: String,
    /// Operation type
    operation: OperationType,
    /// Success or failure
    status: EventStatus,
    /// Error message if failed
    error_message: Option<String>,
    /// Request ID for correlation
    request_id: String,
    /// Session ID for tracking
    session_id: String,
    /// Additional context data
    context: HashMap<String, String>,
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
    #[must_use]
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
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get user ID
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get field name
    #[must_use]
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Get operation type
    #[must_use]
    pub const fn operation(&self) -> OperationType {
        self.operation
    }

    /// Get status
    #[must_use]
    pub const fn status(&self) -> EventStatus {
        self.status
    }

    /// Get error message
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Get request ID
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Get session ID
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get context data
    #[must_use]
    pub const fn context(&self) -> &HashMap<String, String> {
        &self.context
    }

    /// Convert to CSV for logging.
    ///
    /// All user-controlled fields are RFC 4180 quoted to prevent CSV injection.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let error = self.error_message.as_deref().unwrap_or("");
        format!(
            "{},{},{},{},{},{},{},{}",
            csv_quote(&self.timestamp.to_rfc3339()),
            csv_quote(&self.user_id),
            csv_quote(&self.field_name),
            csv_quote(&self.operation.to_string()),
            csv_quote(&self.status.to_string()),
            csv_quote(error),
            csv_quote(&self.request_id),
            csv_quote(&self.session_id),
        )
    }

    /// Convert to JSON for logging.
    ///
    /// Uses `serde_json` to produce well-formed JSON with correct escaping,
    /// preventing injection via user-controlled `user_id` or `field_name` values.
    #[must_use]
    pub fn to_json_like(&self) -> String {
        serde_json::json!({
            "timestamp":  self.timestamp.to_rfc3339(),
            "user_id":    self.user_id,
            "field_name": self.field_name,
            "operation":  self.operation.to_string(),
            "status":     self.status.to_string(),
            "error":      self.error_message.as_deref().unwrap_or(""),
            "request_id": self.request_id,
            "session_id": self.session_id,
        })
        .to_string()
    }
}

/// Audit logger for encryption operations
///
/// Handles logging of all encryption/decryption events for compliance.
pub struct AuditLogger {
    /// In-memory log entries (for testing)
    entries: Vec<AuditLogEntry>,
    /// Maximum entries to keep in memory
    max_entries: usize,
}

impl AuditLogger {
    /// Create new audit logger
    #[must_use]
    pub const fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    /// Log encryption operation
    ///
    /// # Errors
    ///
    /// This function currently never returns an error; it always succeeds after evicting
    /// the oldest entry if the log is at capacity.
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
    #[must_use]
    pub fn recent_entries(&self, count: usize) -> Vec<AuditLogEntry> {
        let start = if self.entries.len() > count {
            self.entries.len() - count
        } else {
            0
        };
        self.entries[start..].to_vec()
    }

    /// Get entries for specific user
    #[must_use]
    pub fn entries_for_user(&self, user_id: &str) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.user_id == user_id)
    }

    /// Get entries for specific field
    #[must_use]
    pub fn entries_for_field(&self, field_name: &str) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.field_name == field_name)
    }

    /// Get entries for specific operation type
    #[must_use]
    pub fn entries_for_operation(&self, operation: OperationType) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.operation == operation)
    }

    /// Get failed operations
    #[must_use]
    pub fn failed_entries(&self) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.status == EventStatus::Failure)
    }

    /// Get successful operations
    #[must_use]
    pub fn successful_entries(&self) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.status == EventStatus::Success)
    }

    /// Get entries for specific user and operation
    #[must_use]
    pub fn entries_for_user_operation(
        &self,
        user_id: &str,
        operation: OperationType,
    ) -> Vec<AuditLogEntry> {
        self.filter_entries(|e| e.user_id == user_id && e.operation == operation)
    }

    /// Get entry count
    #[must_use]
    pub const fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests;
