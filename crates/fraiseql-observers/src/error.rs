//! Observer error types and error code definitions.

use thiserror::Error;

/// Observer error type with structured error codes.
#[derive(Debug, Error, Clone)]
pub enum ObserverError {
    /// OB001: Observer configuration is invalid
    #[error("OB001: Invalid observer configuration: {message}")]
    InvalidConfig {
        /// Detailed error message
        message: String,
    },

    /// OB002: Event does not match any observers
    #[error("OB002: Event type '{event_type}' does not match configured observers")]
    NoMatchingObservers {
        /// The event type that didn't match
        event_type: String,
    },

    /// OB003: Condition syntax is invalid
    #[error("OB003: Invalid condition syntax: {reason}")]
    InvalidCondition {
        /// Reason for invalid condition
        reason: String,
    },

    /// OB004: Condition evaluation failed
    #[error("OB004: Condition evaluation failed: {reason}")]
    ConditionEvaluationFailed {
        /// Reason for evaluation failure
        reason: String,
    },

    /// OB005: Action configuration is invalid
    #[error("OB005: Invalid action configuration: {reason}")]
    InvalidActionConfig {
        /// Reason for invalid configuration
        reason: String,
    },

    /// OB006: Action execution failed (transient)
    #[error("OB006: Action execution failed (transient): {reason}")]
    ActionExecutionFailed {
        /// Reason for execution failure
        reason: String,
    },

    /// OB007: Action execution permanently failed
    #[error("OB007: Action execution permanently failed: {reason}")]
    ActionPermanentlyFailed {
        /// Reason for permanent failure
        reason: String,
    },

    /// OB008: Template rendering failed
    #[error("OB008: Template rendering failed: {reason}")]
    TemplateRenderingFailed {
        /// Reason for rendering failure
        reason: String,
    },

    /// OB009: Database operation failed
    #[error("OB009: Database operation failed: {reason}")]
    DatabaseError {
        /// Reason for database error
        reason: String,
    },

    /// OB010: PostgreSQL LISTEN connection error
    #[error("OB010: PostgreSQL LISTEN connection failed: {reason}")]
    ListenerConnectionFailed {
        /// Reason for connection failure
        reason: String,
    },

    /// OB011: Event channel backpressure - events dropped
    #[error("OB011: Event channel backpressure - events dropped (capacity exceeded)")]
    ChannelFull,

    /// OB012: Dead letter queue operation failed
    #[error("OB012: Dead letter queue operation failed: {reason}")]
    DlqError {
        /// Reason for DLQ operation failure
        reason: String,
    },

    /// OB013: Retry logic exhausted all attempts
    #[error("OB013: Retry logic exhausted all attempts: {reason}")]
    RetriesExhausted {
        /// Reason for retry exhaustion
        reason: String,
    },

    /// OB014: Unsupported action type
    #[error("OB014: Unsupported action type: {action_type}")]
    UnsupportedActionType {
        /// The action type that is not supported
        action_type: String,
    },

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Database query error from the underlying SQLx driver.
    #[error("Database query error: {0}")]
    SqlxError(String),

    /// OB015: Circuit breaker is open - fast fail
    #[error("OB015: Circuit breaker is open: {message}")]
    CircuitBreakerOpen {
        /// Message describing why circuit is open
        message: String,
    },

    /// OB016: Event transport connection failed
    #[error("OB016: Event transport connection failed: {reason}")]
    TransportConnectionFailed {
        /// Reason for transport connection failure
        reason: String,
    },

    /// OB017: Event transport publish failed
    #[error("OB017: Event transport publish failed: {reason}")]
    TransportPublishFailed {
        /// Reason for publish failure
        reason: String,
    },

    /// OB018: Event transport subscribe failed
    #[error("OB018: Event transport subscribe failed: {reason}")]
    TransportSubscribeFailed {
        /// Reason for subscribe failure
        reason: String,
    },

    /// OB019: Event storage operation failed
    #[error("OB019: Event storage operation failed: {reason}")]
    StorageError {
        /// Reason for storage operation failure
        reason: String,
    },

    /// OB020: Event payload could not be deserialized.
    ///
    /// The raw bytes are preserved so the caller can route the message to a
    /// dead-letter queue without losing the original payload.
    #[error("OB020: Event deserialization failed: {reason}")]
    DeserializationError {
        /// Raw bytes of the unparseable message
        raw: Vec<u8>,
        /// Human-readable reason (e.g. the serde_json error message)
        reason: String,
    },

    /// OB021: Event tenant does not match the configured scope.
    ///
    /// The event carried a `tenant_id` that is not permitted by the executor's
    /// `TenantScope`, or the event lacked a `tenant_id` when one is required.
    #[error(
        "OB021: Tenant violation — event tenant {event_tenant:?} not permitted by scope \
         '{required_scope}'"
    )]
    TenantViolation {
        /// The `tenant_id` carried by the event (`None` if absent)
        event_tenant:   Option<String>,
        /// Human-readable description of the configured scope (e.g. `"Single(acme)"`)
        required_scope: String,
    },
}

/// Error code with classification for retry/DLQ decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverErrorCode {
    /// OB001: Invalid observer configuration
    InvalidConfig,
    /// OB002: No matching observers
    NoMatchingObservers,
    /// OB003: Invalid condition syntax
    InvalidCondition,
    /// OB004: Condition evaluation failed
    ConditionEvaluationFailed,
    /// OB005: Invalid action configuration
    InvalidActionConfig,
    /// OB006: Action execution failed (transient)
    ActionExecutionFailed,
    /// OB007: Action execution permanently failed
    ActionPermanentlyFailed,
    /// OB008: Template rendering failed
    TemplateRenderingFailed,
    /// OB009: Database operation failed
    DatabaseError,
    /// OB010: PostgreSQL LISTEN connection error
    ListenerConnectionFailed,
    /// OB011: Channel full - backpressure
    ChannelFull,
    /// OB012: Dead letter queue operation failed
    DlqError,
    /// OB013: Retries exhausted
    RetriesExhausted,
    /// OB014: Unsupported action type
    UnsupportedActionType,
    /// OB015: Circuit breaker is open
    CircuitBreakerOpen,
    /// OB016: Event transport connection failed
    TransportConnectionFailed,
    /// OB017: Event transport publish failed
    TransportPublishFailed,
    /// OB018: Event transport subscribe failed
    TransportSubscribeFailed,
    /// OB019: Event storage operation failed
    StorageError,
    /// OB020: Event deserialization failed
    DeserializationError,
    /// OB021: Tenant scope violation
    TenantViolation,
}

impl ObserverErrorCode {
    /// Returns true if this error is transient (retryable)
    #[must_use]
    pub const fn is_transient(self) -> bool {
        matches!(
            self,
            ObserverErrorCode::ActionExecutionFailed
                | ObserverErrorCode::DatabaseError
                | ObserverErrorCode::ListenerConnectionFailed
                | ObserverErrorCode::TransportConnectionFailed
                | ObserverErrorCode::TransportPublishFailed
                | ObserverErrorCode::TransportSubscribeFailed
        )
    }

    /// Returns true if this error should go to dead letter queue
    #[must_use]
    pub const fn should_dlq(self) -> bool {
        matches!(
            self,
            ObserverErrorCode::ActionPermanentlyFailed
                | ObserverErrorCode::TemplateRenderingFailed
                | ObserverErrorCode::InvalidActionConfig
                | ObserverErrorCode::DeserializationError
        )
    }
}

impl From<sqlx::Error> for ObserverError {
    fn from(err: sqlx::Error) -> Self {
        Self::SqlxError(err.to_string())
    }
}

#[cfg(any(feature = "dedup", feature = "caching", feature = "queue", feature = "redis-lease"))]
impl From<redis::RedisError> for ObserverError {
    fn from(err: redis::RedisError) -> Self {
        Self::DatabaseError {
            reason: format!("Redis error: {err}"),
        }
    }
}

impl ObserverError {
    /// Get the error code for this error
    #[must_use]
    pub const fn code(&self) -> ObserverErrorCode {
        match self {
            ObserverError::InvalidConfig { .. } => ObserverErrorCode::InvalidConfig,
            ObserverError::NoMatchingObservers { .. } => ObserverErrorCode::NoMatchingObservers,
            ObserverError::InvalidCondition { .. } => ObserverErrorCode::InvalidCondition,
            ObserverError::ConditionEvaluationFailed { .. } => {
                ObserverErrorCode::ConditionEvaluationFailed
            },
            ObserverError::InvalidActionConfig { .. } => ObserverErrorCode::InvalidActionConfig,
            ObserverError::ActionExecutionFailed { .. } => ObserverErrorCode::ActionExecutionFailed,
            ObserverError::ActionPermanentlyFailed { .. } => {
                ObserverErrorCode::ActionPermanentlyFailed
            },
            ObserverError::TemplateRenderingFailed { .. } => {
                ObserverErrorCode::TemplateRenderingFailed
            },
            ObserverError::DatabaseError { .. } => ObserverErrorCode::DatabaseError,
            ObserverError::ListenerConnectionFailed { .. } => {
                ObserverErrorCode::ListenerConnectionFailed
            },
            ObserverError::ChannelFull => ObserverErrorCode::ChannelFull,
            ObserverError::DlqError { .. } => ObserverErrorCode::DlqError,
            ObserverError::RetriesExhausted { .. } => ObserverErrorCode::RetriesExhausted,
            ObserverError::UnsupportedActionType { .. } => ObserverErrorCode::UnsupportedActionType,
            ObserverError::SerializationError(_) => ObserverErrorCode::InvalidConfig,
            ObserverError::SqlxError(_) => ObserverErrorCode::DatabaseError,
            ObserverError::CircuitBreakerOpen { .. } => ObserverErrorCode::CircuitBreakerOpen,
            ObserverError::TransportConnectionFailed { .. } => {
                ObserverErrorCode::TransportConnectionFailed
            },
            ObserverError::TransportPublishFailed { .. } => {
                ObserverErrorCode::TransportPublishFailed
            },
            ObserverError::TransportSubscribeFailed { .. } => {
                ObserverErrorCode::TransportSubscribeFailed
            },
            ObserverError::StorageError { .. } => ObserverErrorCode::StorageError,
            ObserverError::DeserializationError { .. } => ObserverErrorCode::DeserializationError,
            ObserverError::TenantViolation { .. } => ObserverErrorCode::TenantViolation,
        }
    }

    /// Returns true if this error is transient (retryable)
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        self.code().is_transient()
    }

    /// Returns true if this error should go to dead letter queue
    #[must_use]
    pub const fn should_dlq(&self) -> bool {
        self.code().should_dlq()
    }
}

/// Result type alias for observer operations
pub type Result<T> = std::result::Result<T, ObserverError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_is_transient() {
        assert!(ObserverErrorCode::ActionExecutionFailed.is_transient());
        assert!(ObserverErrorCode::DatabaseError.is_transient());
        assert!(ObserverErrorCode::ListenerConnectionFailed.is_transient());

        assert!(!ObserverErrorCode::InvalidConfig.is_transient());
        assert!(!ObserverErrorCode::ActionPermanentlyFailed.is_transient());
    }

    #[test]
    fn test_error_code_should_dlq() {
        assert!(ObserverErrorCode::ActionPermanentlyFailed.should_dlq());
        assert!(ObserverErrorCode::TemplateRenderingFailed.should_dlq());
        assert!(ObserverErrorCode::InvalidActionConfig.should_dlq());

        assert!(!ObserverErrorCode::ActionExecutionFailed.should_dlq());
        assert!(!ObserverErrorCode::DatabaseError.should_dlq());
    }

    #[test]
    fn test_observer_error_code_method() {
        let err = ObserverError::InvalidConfig {
            message: "test".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::InvalidConfig);
        assert!(!err.is_transient());
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_transient_action_failure() {
        let err = ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        assert!(err.is_transient());
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_permanent_action_failure() {
        let err = ObserverError::ActionPermanentlyFailed {
            reason: "invalid config".to_string(),
        };
        assert!(!err.is_transient());
        assert!(err.should_dlq());
    }

    #[test]
    fn test_deserialization_error_routes_to_dlq() {
        let err = ObserverError::DeserializationError {
            raw:    b"not valid json {{".to_vec(),
            reason: "invalid json: expected value at line 1 column 1".to_string(),
        };
        // Not transient — retrying the same broken bytes cannot succeed.
        assert!(!err.is_transient());
        // Should be routed to DLQ so the raw payload is preserved.
        assert!(err.should_dlq());
        assert_eq!(err.code(), ObserverErrorCode::DeserializationError);
    }

    #[test]
    fn test_deserialization_error_should_dlq_code() {
        assert!(ObserverErrorCode::DeserializationError.should_dlq());
        assert!(!ObserverErrorCode::DeserializationError.is_transient());
    }

    #[test]
    fn test_tenant_violation_error_code() {
        let err = ObserverError::TenantViolation {
            event_tenant:   Some("other-tenant".to_string()),
            required_scope: "Single(acme)".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::TenantViolation);
        // Not retryable — the tenant policy won't change between attempts.
        assert!(!err.is_transient());
        // Handled internally by DedupedObserverExecutor; not routed via should_dlq().
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_tenant_violation_none_tenant() {
        let err = ObserverError::TenantViolation {
            event_tenant:   None,
            required_scope: "Single(acme)".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::TenantViolation);
        assert!(!err.is_transient());
    }
}
