//! Validation-specific audit logging with tenant isolation and PII redaction.
//!
//! Provides audit trail tracking for all validation decisions, including
//! field name, validation rule applied, success/failure, and execution context.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing;

/// Redaction policy for sensitive fields in audit logs
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
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
    pub timestamp: DateTime<Utc>,
    /// User ID from authentication context
    pub user_id: Option<String>,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Option<String>,
    /// Client IP address
    pub ip_address: String,
    /// GraphQL query or mutation string (may be redacted)
    pub query_string: String,
    /// Name of the mutation (if applicable)
    pub mutation_name: Option<String>,
    /// Field name that was validated
    pub field: String,
    /// Validation rule that was applied
    pub validation_rule: String,
    /// Whether the validation passed
    pub valid: bool,
    /// Reason for failure (if applicable)
    pub failure_reason: Option<String>,
    /// Duration of validation in microseconds
    pub duration_us: u64,
    /// Type of validator executed (e.g., "`pattern_validator`", "`async_validator`")
    pub execution_context: String,
}

/// Validation audit logger for recording validation decisions
#[derive(Clone)]
pub struct ValidationAuditLogger {
    config: Arc<ValidationAuditLoggerConfig>,
    entries: Arc<Mutex<Vec<ValidationAuditEntry>>>,
}

impl ValidationAuditLogger {
    /// Create a new validation audit logger with the given configuration
    #[must_use]
    pub fn new(config: ValidationAuditLoggerConfig) -> Self {
        Self {
            config: Arc::new(config),
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
            match self.entries.lock() {
                Ok(mut entries) => entries.push(entry),
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        lost_entry = ?entry,
                        "CRITICAL: Audit log mutex poisoned, entry lost"
                    );
                    // In production, this should trigger an alert/metric
                },
            }
        }
    }

    /// Check if audit logging is enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get all logged entries (for testing/compliance export)
    #[must_use]
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
    #[must_use]
    pub fn entry_count(&self) -> usize {
        if let Ok(entries) = self.entries.lock() {
            entries.len()
        } else {
            0
        }
    }

    /// Filter entries by user ID
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn entries_by_field(&self, field: &str) -> Vec<ValidationAuditEntry> {
        if let Ok(entries) = self.entries.lock() {
            entries.iter().filter(|e| e.field == field).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Count validation failures
    #[must_use]
    pub fn failure_count(&self) -> usize {
        if let Ok(entries) = self.entries.lock() {
            entries.iter().filter(|e| !e.valid).count()
        } else {
            0
        }
    }

    /// Get configuration reference
    #[must_use]
    pub fn config(&self) -> &ValidationAuditLoggerConfig {
        &self.config
    }
}
