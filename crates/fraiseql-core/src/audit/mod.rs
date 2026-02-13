//! Audit logging infrastructure
//!
//! Provides audit event structures and backend trait for multi-backend audit logging.
//!
//! # Architecture
//!
//! Audit events capture security-relevant operations:
//! - User authentication and authorization
//! - Data mutations (create, update, delete)
//! - Administrative actions
//! - Configuration changes
//!
//! Multiple backends support different deployments:
//! - File: JSON lines to local files
//! - PostgreSQL: Relational storage with indexing
//! - Syslog: Centralized logging infrastructure
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::audit::AuditEvent;
//!
//! let event = AuditEvent::new_user_action(
//!     "user123",
//!     "alice",
//!     "192.168.1.1",
//!     "users",
//!     "create",
//!     "success",
//! );
//!
//! // Log to backend
//! backend.log_event(event).await?;
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Audit event representing a security-relevant operation.
///
/// Captures detailed information about user actions, system events,
/// and data mutations for compliance and security auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event identifier (UUID)
    pub id: String,

    /// ISO 8601 timestamp of the event
    pub timestamp: String,

    /// Event type (e.g., "user_login", "data_modification", "access_denied")
    pub event_type: String,

    /// User ID (None for system events)
    pub user_id: String,

    /// Username for human readability
    pub username: String,

    /// IP address of the request origin
    pub ip_address: String,

    /// Resource type affected (e.g., "users", "posts", "admin_config")
    pub resource_type: String,

    /// Resource ID (None for bulk operations or system events)
    pub resource_id: Option<String>,

    /// Action performed (e.g., "create", "update", "delete", "read")
    pub action: String,

    /// State before modification (None for read operations)
    pub before_state: Option<JsonValue>,

    /// State after modification (None for deletions or reads)
    pub after_state: Option<JsonValue>,

    /// Event status: "success", "failure", or "denied"
    pub status: String,

    /// Error message if status is "failure" or "denied"
    pub error_message: Option<String>,

    /// Tenant ID for multi-tenant deployments
    pub tenant_id: Option<String>,

    /// Additional context as JSON (user_agent, correlation_id, etc.)
    pub metadata: JsonValue,
}

impl AuditEvent {
    /// Create a new audit event for a user action.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User performing the action
    /// * `username` - User's name for readability
    /// * `ip_address` - Request origin IP
    /// * `resource_type` - Type of resource affected
    /// * `action` - Action performed
    /// * `status` - Result status (success/failure/denied)
    #[must_use]
    pub fn new_user_action(
        user_id: impl Into<String>,
        username: impl Into<String>,
        ip_address: impl Into<String>,
        resource_type: impl Into<String>,
        action: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        let resource_type_str = resource_type.into();
        let action_str = action.into();

        Self {
            id:            Uuid::new_v4().to_string(),
            timestamp:     Utc::now().to_rfc3339(),
            event_type:    format!(
                "{}_{}",
                resource_type_str.to_lowercase(),
                action_str.to_lowercase()
            ),
            user_id:       user_id.into(),
            username:      username.into(),
            ip_address:    ip_address.into(),
            resource_type: resource_type_str,
            resource_id:   None,
            action:        action_str,
            before_state:  None,
            after_state:   None,
            status:        status.into(),
            error_message: None,
            tenant_id:     None,
            metadata:      JsonValue::Object(serde_json::Map::new()),
        }
    }

    /// Add resource ID to the event.
    #[must_use]
    pub fn with_resource_id(mut self, id: impl Into<String>) -> Self {
        self.resource_id = Some(id.into());
        self
    }

    /// Add before state to track modifications.
    #[must_use]
    pub fn with_before_state(mut self, state: JsonValue) -> Self {
        self.before_state = Some(state);
        self
    }

    /// Add after state to track modifications.
    #[must_use]
    pub fn with_after_state(mut self, state: JsonValue) -> Self {
        self.after_state = Some(state);
        self
    }

    /// Add error message for failed operations.
    #[must_use]
    pub fn with_error(mut self, message: impl Into<String>) -> Self {
        self.error_message = Some(message.into());
        self
    }

    /// Set tenant ID for multi-tenant tracking.
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Add metadata (user agent, correlation ID, etc.).
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: JsonValue) -> Self {
        if let JsonValue::Object(ref mut map) = self.metadata {
            map.insert(key.into(), value);
        }
        self
    }

    /// Validate the audit event.
    pub fn validate(&self) -> AuditResult<()> {
        // Validate required fields
        if self.user_id.is_empty() {
            return Err(AuditError::ValidationError("user_id cannot be empty".to_string()));
        }

        // Validate status is one of allowed values
        match self.status.as_str() {
            "success" | "failure" | "denied" => {},
            _ => {
                return Err(AuditError::ValidationError(format!(
                    "Invalid status: {}",
                    self.status
                )));
            },
        }

        // Validate that status=failure has error_message
        if self.status == "failure" && self.error_message.is_none() {
            return Err(AuditError::ValidationError(
                "failure status requires error_message".to_string(),
            ));
        }

        Ok(())
    }
}

/// Result type for audit operations
pub type AuditResult<T> = Result<T, AuditError>;

/// Error type for audit operations
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// File I/O error
    #[error("File error: {0}")]
    FileError(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Audit backend trait - implement for each storage backend.
#[async_trait::async_trait]
pub trait AuditBackend: Send + Sync {
    /// Log an audit event to this backend.
    async fn log_event(&self, event: AuditEvent) -> AuditResult<()>;

    /// Query audit events from this backend.
    async fn query_events(&self, filters: AuditQueryFilters) -> AuditResult<Vec<AuditEvent>>;
}

/// Filters for querying audit events
#[derive(Debug, Clone)]
pub struct AuditQueryFilters {
    /// Filter by event type
    pub event_type: Option<String>,

    /// Filter by user ID
    pub user_id: Option<String>,

    /// Filter by resource type
    pub resource_type: Option<String>,

    /// Filter by status
    pub status: Option<String>,

    /// Filter by tenant ID
    pub tenant_id: Option<String>,

    /// Start time (ISO 8601)
    pub start_time: Option<String>,

    /// End time (ISO 8601)
    pub end_time: Option<String>,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}

impl Default for AuditQueryFilters {
    fn default() -> Self {
        Self {
            event_type:    None,
            user_id:       None,
            resource_type: None,
            status:        None,
            tenant_id:     None,
            start_time:    None,
            end_time:      None,
            limit:         Some(100),
            offset:        None,
        }
    }
}

/// File-based audit backend
pub mod file_backend;

/// PostgreSQL audit backend
pub mod postgres_backend;

/// Syslog audit backend
pub mod syslog_backend;

// Re-export backends for convenience
pub use file_backend::FileAuditBackend;
pub use postgres_backend::PostgresAuditBackend;
pub use syslog_backend::SyslogAuditBackend;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod file_backend_tests;

#[cfg(test)]
mod postgres_backend_tests;

#[cfg(test)]
mod syslog_backend_tests;
