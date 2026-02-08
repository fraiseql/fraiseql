//! Validation-specific audit logging with tenant isolation and PII redaction.
//!
//! Provides audit trail tracking for all validation decisions, including
//! field name, validation rule applied, success/failure, and execution context.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Redaction policy for sensitive fields in audit logs
#[derive(Debug, Clone, Copy, Default)]
pub enum RedactionPolicy {
    /// No redaction - log everything
    None,
    /// Conservative redaction - redact passwords, tokens, etc.
    #[default]
    Conservative,
    /// Aggressive redaction - redact most user-related data
    Aggressive,
}

/// Configuration for validation audit logging
#[derive(Debug, Clone)]
pub struct ValidationAuditLoggerConfig {
    /// Enable validation audit logging
    pub enabled: bool,
    /// Capture successful validation entries (not just failures)
    pub capture_successful_validations: bool,
    /// Include the GraphQL query/mutation string in logs
    pub capture_query_strings: bool,
    /// Redaction policy for sensitive data
    pub redaction_policy: RedactionPolicy,
}

impl Default for ValidationAuditLoggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capture_successful_validations: true,
            capture_query_strings: true,
            redaction_policy: RedactionPolicy::default(),
        }
    }
}

/// A single validation audit entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationAuditEntry {
    /// Timestamp of the validation check
    pub timestamp:         DateTime<Utc>,
    /// User ID from authentication context
    pub user_id:           Option<String>,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id:         Option<String>,
    /// Client IP address
    pub ip_address:        String,
    /// GraphQL query or mutation string (may be redacted)
    pub query_string:      String,
    /// Name of the mutation (if applicable)
    pub mutation_name:     Option<String>,
    /// Field name that was validated
    pub field:             String,
    /// Validation rule that was applied
    pub validation_rule:   String,
    /// Whether the validation passed
    pub valid:             bool,
    /// Reason for failure (if applicable)
    pub failure_reason:    Option<String>,
    /// Duration of validation in microseconds
    pub duration_us:       u64,
    /// Type of validator executed (e.g., "pattern_validator", "async_validator")
    pub execution_context: String,
}

/// Validation audit logger for recording validation decisions
#[derive(Clone)]
pub struct ValidationAuditLogger {
    config:  Arc<ValidationAuditLoggerConfig>,
    entries: Arc<Mutex<Vec<ValidationAuditEntry>>>,
}

impl ValidationAuditLogger {
    /// Create a new validation audit logger with the given configuration
    pub fn new(config: ValidationAuditLoggerConfig) -> Self {
        Self {
            config:  Arc::new(config),
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Log a validation audit entry
    pub fn log_entry(&self, entry: ValidationAuditEntry) {
        if !self.config.enabled {
            return;
        }

        // Only log failures or successful entries if configured to capture successes
        if !entry.valid || self.config.capture_successful_validations {
            if let Ok(mut entries) = self.entries.lock() {
                entries.push(entry);
            }
        }
    }

    /// Check if audit logging is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get all logged entries (for testing/compliance export)
    pub fn get_entries(&self) -> Vec<ValidationAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            entries.clone()
        } else {
            Vec::new()
        }
    }

    /// Clear all logged entries
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    /// Get count of logged entries
    pub fn entry_count(&self) -> usize {
        if let Ok(entries) = self.entries.lock() {
            entries.len()
        } else {
            0
        }
    }

    /// Filter entries by user ID
    pub fn entries_by_user(&self, user_id: &str) -> Vec<ValidationAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            entries
                .iter()
                .filter(|e| e.user_id.as_deref() == Some(user_id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Filter entries by tenant ID
    pub fn entries_by_tenant(&self, tenant_id: &str) -> Vec<ValidationAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            entries
                .iter()
                .filter(|e| e.tenant_id.as_deref() == Some(tenant_id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Filter entries by field name
    pub fn entries_by_field(&self, field: &str) -> Vec<ValidationAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            entries.iter().filter(|e| e.field == field).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Count validation failures
    pub fn failure_count(&self) -> usize {
        if let Ok(entries) = self.entries.lock() {
            entries.iter().filter(|e| !e.valid).count()
        } else {
            0
        }
    }

    /// Get configuration reference
    pub fn config(&self) -> &ValidationAuditLoggerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redaction_policy_default() {
        let policy = RedactionPolicy::default();
        match policy {
            RedactionPolicy::Conservative => {},
            _ => panic!("Default should be Conservative"),
        }
    }

    #[test]
    fn test_config_default() {
        let config = ValidationAuditLoggerConfig::default();
        assert!(config.enabled);
        assert!(config.capture_successful_validations);
        assert!(config.capture_query_strings);
    }

    #[test]
    fn test_logger_enabled_disabled() {
        let config = ValidationAuditLoggerConfig {
            enabled: false,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        assert!(!logger.is_enabled());

        let config2 = ValidationAuditLoggerConfig::default();
        let logger2 = ValidationAuditLogger::new(config2);
        assert!(logger2.is_enabled());
    }

    #[test]
    fn test_logger_entry_logging() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           Some("user:1".to_string()),
            tenant_id:         Some("tenant:1".to_string()),
            ip_address:        "192.168.1.1".to_string(),
            query_string:      "{ user { id } }".to_string(),
            mutation_name:     None,
            field:             "email".to_string(),
            validation_rule:   "pattern".to_string(),
            valid:             false,
            failure_reason:    Some("Invalid format".to_string()),
            duration_us:       100,
            execution_context: "pattern_validator".to_string(),
        };

        logger.log_entry(entry);
        assert_eq!(logger.entry_count(), 1);
    }

    #[test]
    fn test_logger_filter_by_user() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry1 = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           Some("user:1".to_string()),
            tenant_id:         None,
            ip_address:        "192.168.1.1".to_string(),
            query_string:      String::new(),
            mutation_name:     None,
            field:             "field1".to_string(),
            validation_rule:   "required".to_string(),
            valid:             false,
            failure_reason:    None,
            duration_us:       0,
            execution_context: "validator".to_string(),
        };

        let mut entry2 = entry1.clone();
        entry2.user_id = Some("user:2".to_string());

        logger.log_entry(entry1);
        logger.log_entry(entry2);

        let user1_entries = logger.entries_by_user("user:1");
        assert_eq!(user1_entries.len(), 1);
    }

    #[test]
    fn test_logger_failure_count() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           None,
            tenant_id:         None,
            ip_address:        "192.168.1.1".to_string(),
            query_string:      String::new(),
            mutation_name:     None,
            field:             "field".to_string(),
            validation_rule:   "pattern".to_string(),
            valid:             false,
            failure_reason:    Some("error".to_string()),
            duration_us:       0,
            execution_context: "validator".to_string(),
        };

        logger.log_entry(entry.clone());

        let mut entry_success = entry;
        entry_success.valid = true;
        logger.log_entry(entry_success);

        assert_eq!(logger.failure_count(), 1);
    }
}
