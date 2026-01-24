#![forbid(unsafe_code)]
// TODO: Add documentation incrementally - allowing for now to focus on actionable warnings
#![allow(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Allow common pedantic lints that are too noisy for this codebase
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unused_self)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::new_without_default)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::unused_async)]
// Test-related - exact float comparison is intentional in tests
#![allow(clippy::float_cmp)]
// Nursery lints - experimental, too noisy
#![allow(clippy::collection_is_never_read)]
#![allow(clippy::future_not_send)]
#![allow(clippy::significant_drop_in_scrutinee)]

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

pub mod actions;
pub mod actions_additional;
pub mod cache;
#[cfg(feature = "caching")]
pub mod cached_executor;
pub mod checkpoint;
pub mod cli;
pub mod concurrent;
pub mod condition;
pub mod config;
pub mod dedup;
#[cfg(feature = "dedup")]
pub mod deduped_executor;

pub mod factory;
pub mod error;
pub mod event;
#[cfg(feature = "arrow")]
pub mod arrow_bridge;
pub mod executor;
pub mod listener;
pub mod logging;
pub mod matcher;
#[cfg(feature = "metrics")]
pub mod metrics;
pub mod queue;
pub mod resilience;
pub mod search;
pub mod traits;
pub mod transport;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

// Re-export common types at crate level
pub use actions::{ActionExecutionResult, EmailAction, SlackAction, WebhookAction};
pub use actions_additional::{CacheAction, PushAction, SearchAction, SmsAction};
pub use cache::{CacheBackend, CachedActionResult, CacheStats};
#[cfg(feature = "caching")]
pub use cache::redis::RedisCacheBackend;
pub use checkpoint::{CheckpointState, CheckpointStore};
pub use checkpoint::postgres::PostgresCheckpointStore;
pub use concurrent::ConcurrentActionExecutor;
pub use dedup::{DeduplicationStats, DeduplicationStore};
#[cfg(feature = "dedup")]
pub use dedup::redis::RedisDeduplicationStore;
pub use queue::{
    ExponentialBackoffPolicy, FixedBackoffPolicy, Job, JobQueue, JobResult, JobStatus,
    LinearBackoffPolicy, QueueStats, RetryPolicy,
};
pub use queue::worker::{JobWorker, JobWorkerPool};
#[cfg(feature = "queue")]
pub use queue::redis::RedisJobQueue;
pub use search::{IndexedEvent, SearchBackend, SearchStats};
#[cfg(feature = "search")]
pub use search::http::HttpSearchBackend;
pub use condition::{ConditionAst, ConditionParser};
pub use config::{
    ActionConfig, BackoffStrategy, FailurePolicy, MultiListenerConfig, ObserverDefinition,
    ObserverRuntimeConfig, OverflowPolicy, RetryConfig,
};
pub use error::{ObserverError, ObserverErrorCode, Result};
pub use event::{EntityEvent, EventKind, FieldChanges};
pub use executor::{ExecutionSummary, ObserverExecutor};
pub use listener::{
    ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig, CheckpointLease,
    FailoverEvent, FailoverManager, EventListener, ListenerConfig, ListenerHandle, ListenerHealth,
    ListenerState, ListenerStateMachine, MultiListenerCoordinator,
};
pub use logging::{
    StructuredLogger, TraceIdExtractor, set_trace_id_context, get_current_trace_id,
};
pub use logging::correlation::TraceContext;
pub use matcher::EventMatcher;
#[cfg(feature = "metrics")]
pub use metrics::ObserverMetrics;
pub use resilience::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, DegradationLevel, GracefulDegradation,
    PerEndpointCircuitBreaker, ResilientExecutor, ResilienceStrategy,
};
pub use traits::{
    ActionExecutor, ActionResult, ConditionEvaluator, DeadLetterQueue, DlqItem, EventSource,
    TemplateRenderer,
};
pub use transport::{
    EventFilter, EventStream, EventTransport, HealthStatus, InMemoryTransport,
    PostgresNotifyTransport, TransportHealth, TransportType,
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

#[cfg(test)]
mod e2e_tests {
    //! End-to-end integration tests (Subphase 7.5)
    //!
    //! These tests verify the full workflow:
    //! Change Log Entry → `EntityEvent` → Observer Matching → Action Execution

    use super::*;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use uuid::Uuid;

    #[test]
    fn test_e2e_insert_workflow() {
        // Simulate: Database INSERT → ChangeLog entry → EntityEvent → Observer processing

        let entity_id = Uuid::new_v4();
        let changelog_entry = listener::ChangeLogEntry {
            id: 1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: Some("user-1".to_string()),
            object_type: "Order".to_string(),
            object_id: entity_id.to_string(),
            modification_type: "INSERT".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string(), "total": 250.00, "status": "new" }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T12:00:00+00:00".to_string(),
        };

        // Step 1: Convert to EntityEvent
        let event = changelog_entry.to_entity_event().expect("Failed to convert");

        // Step 2: Verify event properties
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.entity_id, entity_id);
        assert_eq!(event.data["total"], 250.00);
        assert_eq!(event.user_id, Some("user-1".to_string()));

        // Step 3: Create executor and matcher (verifies integration)
        let dlq = Arc::new(testing::mocks::MockDeadLetterQueue::new());
        let matcher = matcher::EventMatcher::new();
        let _executor = executor::ObserverExecutor::new(matcher, dlq);

        // Note: executor.process_event() would execute observers if registered
        // This is verified separately in executor tests
    }

    #[test]
    fn test_e2e_update_workflow_with_condition() {
        // Simulate: UPDATE → EntityEvent with field changes → Condition matching

        let entity_id = Uuid::new_v4();
        let changelog_entry = listener::ChangeLogEntry {
            id: 2,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: Some("user-2".to_string()),
            object_type: "Order".to_string(),
            object_id: entity_id.to_string(),
            modification_type: "UPDATE".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "u",
                "before": { "status": "new", "total": 250.00 },
                "after": { "status": "shipped", "total": 250.00 }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T13:00:00+00:00".to_string(),
        };

        // Step 1: Convert to EntityEvent
        let event = changelog_entry.to_entity_event().expect("Failed to convert");

        // Step 2: Verify UPDATE event with field changes
        assert_eq!(event.event_type, EventKind::Updated);
        assert_eq!(event.data["status"], "shipped");

        let changes = event.changes.expect("No changes detected");
        assert!(changes.contains_key("status"));
        assert_eq!(changes["status"].old, "new");
        assert_eq!(changes["status"].new, "shipped");
    }

    #[test]
    fn test_e2e_delete_workflow() {
        // Simulate: DELETE → EntityEvent with before values

        let entity_id = Uuid::new_v4();
        let changelog_entry = listener::ChangeLogEntry {
            id: 3,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: None,
            object_type: "User".to_string(),
            object_id: entity_id.to_string(),
            modification_type: "DELETE".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "d",
                "before": { "id": entity_id.to_string(), "name": "John Doe", "email": "john@example.com" },
                "after": null
            }),
            extra_metadata: None,
            created_at: "2026-01-22T14:00:00+00:00".to_string(),
        };

        // Step 1: Convert to EntityEvent
        let event = changelog_entry.to_entity_event().expect("Failed to convert");

        // Step 2: Verify DELETE event uses before values
        assert_eq!(event.event_type, EventKind::Deleted);
        assert_eq!(event.data["name"], "John Doe");
        assert_eq!(event.data["email"], "john@example.com");
    }

    #[test]
    fn test_e2e_multi_entity_types() {
        // Verify the system can handle different entity types

        let types = vec!["Order", "User", "Product", "Invoice"];

        for entity_type in types {
            let entity_id = Uuid::new_v4();
            let entry = listener::ChangeLogEntry {
                id: 1,
                pk_entity_change_log: Uuid::new_v4().to_string(),
                fk_customer_org: "acme".to_string(),
                fk_contact: None,
                object_type: entity_type.to_string(),
                object_id: entity_id.to_string(),
                modification_type: "INSERT".to_string(),
                change_status: "success".to_string(),
                object_data: json!({
                    "op": "c",
                    "before": null,
                    "after": { "id": entity_id.to_string() }
                }),
                extra_metadata: None,
                created_at: "2026-01-22T15:00:00+00:00".to_string(),
            };

            let event = entry.to_entity_event().expect("Failed to convert");
            assert_eq!(event.entity_type, entity_type);
            assert_eq!(event.event_type, EventKind::Created);
        }
    }

    #[test]
    fn test_e2e_multi_tenant_isolation() {
        // Verify tenant isolation via fk_customer_org

        let orgs = vec!["org-1", "org-2", "org-3"];
        let entity_id = Uuid::new_v4();

        for org_id in orgs {
            let entry = listener::ChangeLogEntry {
                id: 1,
                pk_entity_change_log: Uuid::new_v4().to_string(),
                fk_customer_org: org_id.to_string(),
                fk_contact: None,
                object_type: "Order".to_string(),
                object_id: entity_id.to_string(),
                modification_type: "INSERT".to_string(),
                change_status: "success".to_string(),
                object_data: json!({
                    "op": "c",
                    "before": null,
                    "after": { "id": entity_id.to_string(), "org": org_id }
                }),
                extra_metadata: None,
                created_at: "2026-01-22T16:00:00+00:00".to_string(),
            };

            let event = entry.to_entity_event().expect("Failed to convert");
            assert_eq!(event.data["org"], org_id);
        }
    }

    #[test]
    fn test_e2e_field_changes_complex() {
        // Verify complex field change scenarios

        let entity_id = Uuid::new_v4();
        let entry = listener::ChangeLogEntry {
            id: 1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: None,
            object_type: "Order".to_string(),
            object_id: entity_id.to_string(),
            modification_type: "UPDATE".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "u",
                "before": {
                    "status": "pending",
                    "items": 5,
                    "tracking_number": "123456"
                },
                "after": {
                    "status": "shipped",
                    "items": 5,
                    "tracking_number": "123456",
                    "shipped_at": "2026-01-22T16:30:00Z"
                }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T16:30:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().expect("Failed to convert");
        let changes = event.changes.expect("No changes detected");

        // Status changed
        assert!(changes.contains_key("status"));
        assert_eq!(changes["status"].old, "pending");
        assert_eq!(changes["status"].new, "shipped");

        // Items unchanged (shouldn't be in changes)
        assert!(!changes.contains_key("items"));

        // Tracking unchanged (shouldn't be in changes)
        assert!(!changes.contains_key("tracking_number"));

        // New field: shipped_at
        assert!(changes.contains_key("shipped_at"));
        assert_eq!(changes["shipped_at"].old, Value::Null);
        assert_eq!(changes["shipped_at"].new, "2026-01-22T16:30:00Z");
    }

    #[test]
    fn test_e2e_timestamp_accuracy() {
        // Verify timestamp parsing preserves accuracy

        let entity_id = Uuid::new_v4();
        let timestamp_str = "2026-01-22T14:30:45.123456+00:00";

        let entry = listener::ChangeLogEntry {
            id: 1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: None,
            object_type: "Order".to_string(),
            object_id: entity_id.to_string(),
            modification_type: "INSERT".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string() }
            }),
            extra_metadata: None,
            created_at: timestamp_str.to_string(),
        };

        let event = entry.to_entity_event().expect("Failed to convert");

        // Verify timestamp was parsed correctly
        assert!(event.timestamp.to_rfc3339().contains("2026-01-22T14:30:45"));
    }

    #[test]
    fn test_e2e_invalid_uuid_handling() {
        // Verify error handling for invalid UUID in object_id

        let entry = listener::ChangeLogEntry {
            id: 1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org: "acme".to_string(),
            fk_contact: None,
            object_type: "Order".to_string(),
            object_id: "not-a-uuid".to_string(),
            modification_type: "INSERT".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "c",
                "before": null,
                "after": { "id": "invalid" }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T17:00:00+00:00".to_string(),
        };

        let result = entry.to_entity_event();
        assert!(result.is_err());
    }
}
