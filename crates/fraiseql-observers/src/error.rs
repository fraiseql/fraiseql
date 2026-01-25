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

    /// Phase 8: Database query/connection error (from sqlx)
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
        )
    }
}

impl From<sqlx::Error> for ObserverError {
    fn from(err: sqlx::Error) -> Self {
        Self::SqlxError(err.to_string())
    }
}

#[cfg(any(feature = "dedup", feature = "caching", feature = "queue"))]
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
}
