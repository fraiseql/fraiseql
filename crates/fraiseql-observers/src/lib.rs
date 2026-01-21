#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cast_possible_truncation)]

//! FraiseQL Observer System - Post-Mutation Side Effects
//!
//! This crate implements the observer pattern for FraiseQL, allowing applications
//! to define side effects that execute after database mutations (INSERT, UPDATE, DELETE).
//!
//! # Architecture
//!
//! The observer system is built on PostgreSQL LISTEN/NOTIFY:
//!
//! ```text
//! Database mutation (INSERT/UPDATE/DELETE)
//!     ↓
//! PostgreSQL pg_notify('fraiseql_events', event_data)
//!     ↓
//! EventListener (separate connection) receives NOTIFY
//!     ↓
//! Events sent to bounded mpsc::channel (backpressure)
//!     ↓
//! ObserverExecutor processes in parallel worker pool
//!     ├─ Condition evaluation (skip if condition false)
//!     ├─ Action execution (webhook, email, Slack, etc.)
//!     ├─ Retry logic (exponential/linear/fixed backoff)
//!     └─ Dead Letter Queue (failed actions for manual retry)
//! ```
//!
//! # Key Features
//!
//! - **Flexible Actions**: Webhook, email, Slack, SMS, push notifications, cache invalidation, search indexing
//! - **Conditions**: DSL for conditional action execution (e.g., `status_changed_to('shipped') && total > 100`)
//! - **Reliability**: Retry logic with exponential/linear/fixed backoff
//! - **Dead Letter Queue**: Failed actions stored for manual retry
//! - **Backpressure**: Configurable overflow policies (drop, block, drop-oldest)
//! - **Observable**: Structured logging, Prometheus metrics
//! - **Testable**: All external dependencies abstracted as traits with mock implementations

pub mod condition;
pub mod config;
pub mod error;
pub mod event;
pub mod listener;
pub mod matcher;
pub mod traits;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

// Re-export common types at crate level
pub use condition::{ConditionAst, ConditionParser};
pub use config::{
    ActionConfig, BackoffStrategy, FailurePolicy, ObserverDefinition, ObserverRuntimeConfig,
    OverflowPolicy, RetryConfig,
};
pub use error::{ObserverError, ObserverErrorCode, Result};
pub use event::{EntityEvent, EventKind, FieldChanges};
pub use listener::{EventListener, ListenerConfig};
pub use matcher::EventMatcher;
pub use traits::{
    ActionExecutor, ActionResult, ConditionEvaluator, DeadLetterQueue, DlqItem, EventSource,
    TemplateRenderer,
};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn test_entity_event_creation() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        );

        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.data["total"], 100);
    }

    #[test]
    fn test_observer_error_codes() {
        let err = ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };

        assert!(err.is_transient());
        assert_eq!(err.code(), ObserverErrorCode::ActionExecutionFailed);
    }

    #[test]
    fn test_webhook_action_validation() {
        let invalid = ActionConfig::Webhook {
            url: None,
            url_env: None,
            headers: std::collections::HashMap::new(),
            body_template: None,
        };

        assert!(invalid.validate().is_err());

        let valid = ActionConfig::Webhook {
            url: Some("https://example.com".to_string()),
            url_env: None,
            headers: std::collections::HashMap::new(),
            body_template: Some("{}".to_string()),
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 30000);
    }
}
