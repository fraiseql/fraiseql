//! Unit tests for `fraiseql-observers` — error types, event types, and config types.
//!
//! These tests exercise the public API without requiring a real database,
//! Redis, NATS, or any other external service. They complement the existing
//! inline tests by covering Display output, serde round-trips, and full
//! coverage of the error code classification matrix.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::collections::HashMap;

use fraiseql_observers::{
    EntityEvent, EventKind, FieldChanges, ObserverError, ObserverErrorCode,
    config::{BackoffStrategy, FailurePolicy, OverflowPolicy, RetryConfig},
};
use serde_json::json;
use uuid::Uuid;

// ── ObserverError Display ─────────────────────────────────────────────────────

#[test]
fn observer_error_invalid_config_display() {
    let e = ObserverError::InvalidConfig { message: "bad config".into() };
    let s = e.to_string();
    assert!(s.contains("OB001"), "got: {s}");
    assert!(s.contains("bad config"), "got: {s}");
}

#[test]
fn observer_error_no_matching_observers_display() {
    let e = ObserverError::NoMatchingObservers { event_type: "ORDER_SHIPPED".into() };
    let s = e.to_string();
    assert!(s.contains("OB002"), "got: {s}");
    assert!(s.contains("ORDER_SHIPPED"), "got: {s}");
}

#[test]
fn observer_error_invalid_condition_display() {
    let e = ObserverError::InvalidCondition { reason: "unexpected token".into() };
    let s = e.to_string();
    assert!(s.contains("OB003"), "got: {s}");
    assert!(s.contains("unexpected token"), "got: {s}");
}

#[test]
fn observer_error_condition_evaluation_failed_display() {
    let e = ObserverError::ConditionEvaluationFailed { reason: "null pointer".into() };
    let s = e.to_string();
    assert!(s.contains("OB004"), "got: {s}");
    assert!(s.contains("null pointer"), "got: {s}");
}

#[test]
fn observer_error_invalid_action_config_display() {
    let e = ObserverError::InvalidActionConfig { reason: "missing url".into() };
    let s = e.to_string();
    assert!(s.contains("OB005"), "got: {s}");
    assert!(s.contains("missing url"), "got: {s}");
}

#[test]
fn observer_error_action_execution_failed_display() {
    let e = ObserverError::ActionExecutionFailed { reason: "timeout".into() };
    let s = e.to_string();
    assert!(s.contains("OB006"), "got: {s}");
    assert!(s.contains("timeout"), "got: {s}");
}

#[test]
fn observer_error_action_permanently_failed_display() {
    let e = ObserverError::ActionPermanentlyFailed { reason: "endpoint gone".into() };
    let s = e.to_string();
    assert!(s.contains("OB007"), "got: {s}");
    assert!(s.contains("endpoint gone"), "got: {s}");
}

#[test]
fn observer_error_template_rendering_failed_display() {
    let e = ObserverError::TemplateRenderingFailed { reason: "undefined variable".into() };
    let s = e.to_string();
    assert!(s.contains("OB008"), "got: {s}");
    assert!(s.contains("undefined variable"), "got: {s}");
}

#[test]
fn observer_error_database_error_display() {
    let e = ObserverError::DatabaseError { reason: "connection refused".into() };
    let s = e.to_string();
    assert!(s.contains("OB009"), "got: {s}");
    assert!(s.contains("connection refused"), "got: {s}");
}

#[test]
fn observer_error_listener_connection_failed_display() {
    let e = ObserverError::ListenerConnectionFailed { reason: "host unreachable".into() };
    let s = e.to_string();
    assert!(s.contains("OB010"), "got: {s}");
    assert!(s.contains("host unreachable"), "got: {s}");
}

#[test]
fn observer_error_channel_full_display() {
    let e = ObserverError::ChannelFull;
    let s = e.to_string();
    assert!(s.contains("OB011"), "got: {s}");
    assert!(!s.is_empty());
}

#[test]
fn observer_error_dlq_error_display() {
    let e = ObserverError::DlqError { reason: "write failed".into() };
    let s = e.to_string();
    assert!(s.contains("OB012"), "got: {s}");
    assert!(s.contains("write failed"), "got: {s}");
}

#[test]
fn observer_error_retries_exhausted_display() {
    let e = ObserverError::RetriesExhausted { reason: "all 3 attempts failed".into() };
    let s = e.to_string();
    assert!(s.contains("OB013"), "got: {s}");
    assert!(s.contains("all 3 attempts failed"), "got: {s}");
}

#[test]
fn observer_error_unsupported_action_type_display() {
    let e = ObserverError::UnsupportedActionType { action_type: "kafka".into() };
    let s = e.to_string();
    assert!(s.contains("OB014"), "got: {s}");
    assert!(s.contains("kafka"), "got: {s}");
}

#[test]
fn observer_error_serialization_error_display() {
    let e = ObserverError::SerializationError("failed to serialize".into());
    let s = e.to_string();
    assert!(s.contains("failed to serialize"), "got: {s}");
}

#[test]
fn observer_error_sqlx_error_display() {
    let e = ObserverError::SqlxError("no rows returned".into());
    let s = e.to_string();
    assert!(s.contains("no rows returned"), "got: {s}");
}

#[test]
fn observer_error_circuit_breaker_open_display() {
    let e = ObserverError::CircuitBreakerOpen { message: "too many failures".into() };
    let s = e.to_string();
    assert!(s.contains("OB015"), "got: {s}");
    assert!(s.contains("too many failures"), "got: {s}");
}

#[test]
fn observer_error_transport_connection_failed_display() {
    let e = ObserverError::TransportConnectionFailed { reason: "NATS unreachable".into() };
    let s = e.to_string();
    assert!(s.contains("OB016"), "got: {s}");
    assert!(s.contains("NATS unreachable"), "got: {s}");
}

#[test]
fn observer_error_transport_publish_failed_display() {
    let e = ObserverError::TransportPublishFailed { reason: "subject invalid".into() };
    let s = e.to_string();
    assert!(s.contains("OB017"), "got: {s}");
    assert!(s.contains("subject invalid"), "got: {s}");
}

#[test]
fn observer_error_transport_subscribe_failed_display() {
    let e = ObserverError::TransportSubscribeFailed { reason: "permission denied".into() };
    let s = e.to_string();
    assert!(s.contains("OB018"), "got: {s}");
    assert!(s.contains("permission denied"), "got: {s}");
}

#[test]
fn observer_error_storage_error_display() {
    let e = ObserverError::StorageError { reason: "disk full".into() };
    let s = e.to_string();
    assert!(s.contains("OB019"), "got: {s}");
    assert!(s.contains("disk full"), "got: {s}");
}

#[test]
fn observer_error_deserialization_error_display() {
    let e = ObserverError::DeserializationError {
        raw:    b"bad bytes".to_vec(),
        reason: "not valid JSON".into(),
    };
    let s = e.to_string();
    assert!(s.contains("OB020"), "got: {s}");
    assert!(s.contains("not valid JSON"), "got: {s}");
}

#[test]
fn observer_error_tenant_violation_display_with_tenant() {
    let e = ObserverError::TenantViolation {
        event_tenant:   Some("acme".into()),
        required_scope: "Single(beta)".into(),
    };
    let s = e.to_string();
    assert!(s.contains("OB021"), "got: {s}");
    assert!(s.contains("acme"), "got: {s}");
    assert!(s.contains("Single(beta)"), "got: {s}");
}

#[test]
fn observer_error_tenant_violation_display_without_tenant() {
    let e = ObserverError::TenantViolation {
        event_tenant:   None,
        required_scope: "Single(acme)".into(),
    };
    let s = e.to_string();
    assert!(s.contains("OB021"), "got: {s}");
    assert!(s.contains("Single(acme)"), "got: {s}");
}

// ── ObserverError::code() — full variant coverage ────────────────────────────

#[test]
fn observer_error_code_no_matching_observers() {
    let e = ObserverError::NoMatchingObservers { event_type: "X".into() };
    assert_eq!(e.code(), ObserverErrorCode::NoMatchingObservers);
}

#[test]
fn observer_error_code_invalid_condition() {
    let e = ObserverError::InvalidCondition { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::InvalidCondition);
}

#[test]
fn observer_error_code_condition_evaluation_failed() {
    let e = ObserverError::ConditionEvaluationFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::ConditionEvaluationFailed);
}

#[test]
fn observer_error_code_invalid_action_config() {
    let e = ObserverError::InvalidActionConfig { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::InvalidActionConfig);
}

#[test]
fn observer_error_code_template_rendering_failed() {
    let e = ObserverError::TemplateRenderingFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::TemplateRenderingFailed);
}

#[test]
fn observer_error_code_database_error() {
    let e = ObserverError::DatabaseError { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::DatabaseError);
}

#[test]
fn observer_error_code_listener_connection_failed() {
    let e = ObserverError::ListenerConnectionFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::ListenerConnectionFailed);
}

#[test]
fn observer_error_code_channel_full() {
    assert_eq!(ObserverError::ChannelFull.code(), ObserverErrorCode::ChannelFull);
}

#[test]
fn observer_error_code_dlq_error() {
    let e = ObserverError::DlqError { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::DlqError);
}

#[test]
fn observer_error_code_retries_exhausted() {
    let e = ObserverError::RetriesExhausted { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::RetriesExhausted);
}

#[test]
fn observer_error_code_unsupported_action_type() {
    let e = ObserverError::UnsupportedActionType { action_type: "kafka".into() };
    assert_eq!(e.code(), ObserverErrorCode::UnsupportedActionType);
}

#[test]
fn observer_error_code_serialization_error_maps_to_invalid_config() {
    let e = ObserverError::SerializationError("bad".into());
    assert_eq!(e.code(), ObserverErrorCode::InvalidConfig);
}

#[test]
fn observer_error_code_sqlx_error_maps_to_database_error() {
    let e = ObserverError::SqlxError("bad".into());
    assert_eq!(e.code(), ObserverErrorCode::DatabaseError);
}

#[test]
fn observer_error_code_circuit_breaker_open() {
    let e = ObserverError::CircuitBreakerOpen { message: "open".into() };
    assert_eq!(e.code(), ObserverErrorCode::CircuitBreakerOpen);
}

#[test]
fn observer_error_code_transport_connection_failed() {
    let e = ObserverError::TransportConnectionFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::TransportConnectionFailed);
}

#[test]
fn observer_error_code_transport_publish_failed() {
    let e = ObserverError::TransportPublishFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::TransportPublishFailed);
}

#[test]
fn observer_error_code_transport_subscribe_failed() {
    let e = ObserverError::TransportSubscribeFailed { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::TransportSubscribeFailed);
}

#[test]
fn observer_error_code_storage_error() {
    let e = ObserverError::StorageError { reason: "bad".into() };
    assert_eq!(e.code(), ObserverErrorCode::StorageError);
}

// ── is_transient() — transport variants ──────────────────────────────────────

#[test]
fn transport_connection_failed_is_transient() {
    let e = ObserverError::TransportConnectionFailed { reason: "retry me".into() };
    assert!(e.is_transient());
    assert!(!e.should_dlq());
}

#[test]
fn transport_publish_failed_is_transient() {
    let e = ObserverError::TransportPublishFailed { reason: "retry me".into() };
    assert!(e.is_transient());
    assert!(!e.should_dlq());
}

#[test]
fn transport_subscribe_failed_is_transient() {
    let e = ObserverError::TransportSubscribeFailed { reason: "retry me".into() };
    assert!(e.is_transient());
    assert!(!e.should_dlq());
}

// ── should_dlq() — DLQ variants ──────────────────────────────────────────────

#[test]
fn invalid_action_config_should_dlq() {
    let e = ObserverError::InvalidActionConfig { reason: "bad url".into() };
    assert!(e.should_dlq());
    assert!(!e.is_transient());
}

#[test]
fn template_rendering_failed_should_dlq() {
    let e = ObserverError::TemplateRenderingFailed { reason: "missing field".into() };
    assert!(e.should_dlq());
    assert!(!e.is_transient());
}

#[test]
fn deserialization_error_should_dlq_and_not_transient() {
    let e = ObserverError::DeserializationError {
        raw:    b"garbage".to_vec(),
        reason: "invalid JSON".into(),
    };
    assert!(e.should_dlq());
    assert!(!e.is_transient());
}

// ── EventKind serde ───────────────────────────────────────────────────────────

#[test]
fn event_kind_created_serializes_to_insert() {
    let s = serde_json::to_string(&EventKind::Created).unwrap();
    assert_eq!(s, "\"INSERT\"");
}

#[test]
fn event_kind_updated_serializes_to_update() {
    let s = serde_json::to_string(&EventKind::Updated).unwrap();
    assert_eq!(s, "\"UPDATE\"");
}

#[test]
fn event_kind_deleted_serializes_to_delete() {
    let s = serde_json::to_string(&EventKind::Deleted).unwrap();
    assert_eq!(s, "\"DELETE\"");
}

#[test]
fn event_kind_custom_serializes_to_custom() {
    let s = serde_json::to_string(&EventKind::Custom).unwrap();
    assert_eq!(s, "\"CUSTOM\"");
}

#[test]
fn event_kind_round_trips_via_serde() {
    for kind in [EventKind::Created, EventKind::Updated, EventKind::Deleted, EventKind::Custom] {
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }
}

#[test]
fn event_kind_as_str_matches_serde_value() {
    assert_eq!(EventKind::Created.as_str(), "INSERT");
    assert_eq!(EventKind::Updated.as_str(), "UPDATE");
    assert_eq!(EventKind::Deleted.as_str(), "DELETE");
    assert_eq!(EventKind::Custom.as_str(), "CUSTOM");
}

// ── FieldChanges ─────────────────────────────────────────────────────────────

#[test]
fn field_changes_serde_round_trip() {
    let fc = FieldChanges { old: json!("pending"), new: json!("shipped") };
    let json = serde_json::to_string(&fc).unwrap();
    let decoded: FieldChanges = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.old, json!("pending"));
    assert_eq!(decoded.new, json!("shipped"));
}

// ── EntityEvent ───────────────────────────────────────────────────────────────

#[test]
fn entity_event_new_has_unique_ids() {
    let id = Uuid::new_v4();
    let e1 = EntityEvent::new(EventKind::Created, "User".into(), id, json!({}));
    let e2 = EntityEvent::new(EventKind::Created, "User".into(), id, json!({}));
    assert_ne!(e1.id, e2.id, "each event should have a unique id");
    assert_eq!(e1.entity_id, e2.entity_id);
}

#[test]
fn entity_event_defaults_have_no_user_or_tenant() {
    let e = EntityEvent::new(EventKind::Created, "Order".into(), Uuid::new_v4(), json!({}));
    assert!(e.user_id.is_none());
    assert!(e.tenant_id.is_none());
    assert!(e.changes.is_none());
}

#[test]
fn entity_event_custom_kind_neither_new_nor_deleted() {
    let e = EntityEvent::new(EventKind::Custom, "Report".into(), Uuid::new_v4(), json!({}));
    assert!(!e.is_new());
    assert!(!e.is_deleted());
}

#[test]
fn entity_event_updated_kind_neither_new_nor_deleted() {
    let e = EntityEvent::new(EventKind::Updated, "User".into(), Uuid::new_v4(), json!({}));
    assert!(!e.is_new());
    assert!(!e.is_deleted());
}

#[test]
fn entity_event_field_changed_returns_false_without_changes() {
    let e = EntityEvent::new(EventKind::Updated, "User".into(), Uuid::new_v4(), json!({}));
    assert!(!e.field_changed("email"));
}

#[test]
fn entity_event_field_changed_to_returns_false_without_changes() {
    let e = EntityEvent::new(EventKind::Updated, "User".into(), Uuid::new_v4(), json!({}));
    assert!(!e.field_changed_to("status", &json!("active")));
}

#[test]
fn entity_event_field_changed_from_returns_false_without_changes() {
    let e = EntityEvent::new(EventKind::Updated, "User".into(), Uuid::new_v4(), json!({}));
    assert!(!e.field_changed_from("status", &json!("pending")));
}

#[test]
fn entity_event_field_changed_detects_present_field() {
    let mut changes = HashMap::new();
    changes.insert("role".into(), FieldChanges { old: json!("user"), new: json!("admin") });
    let e = EntityEvent::new(EventKind::Updated, "User".into(), Uuid::new_v4(), json!({}))
        .with_changes(changes);
    assert!(e.field_changed("role"));
    assert!(!e.field_changed("email"));
}

#[test]
fn entity_event_field_changed_to_checks_new_value() {
    let mut changes = HashMap::new();
    changes.insert("status".into(), FieldChanges { old: json!("draft"), new: json!("published") });
    let e = EntityEvent::new(EventKind::Updated, "Post".into(), Uuid::new_v4(), json!({}))
        .with_changes(changes);
    assert!(e.field_changed_to("status", &json!("published")));
    assert!(!e.field_changed_to("status", &json!("draft")));
}

#[test]
fn entity_event_field_changed_from_checks_old_value() {
    let mut changes = HashMap::new();
    changes.insert("price".into(), FieldChanges { old: json!(9.99), new: json!(14.99) });
    let e = EntityEvent::new(EventKind::Updated, "Product".into(), Uuid::new_v4(), json!({}))
        .with_changes(changes);
    assert!(e.field_changed_from("price", &json!(9.99)));
    assert!(!e.field_changed_from("price", &json!(14.99)));
}

#[test]
fn entity_event_serde_round_trip() {
    let entity_id = Uuid::new_v4();
    let e = EntityEvent::new(
        EventKind::Created,
        "Order".into(),
        entity_id,
        json!({"total": 42, "status": "pending"}),
    )
    .with_user_id("user-abc".into())
    .with_tenant_id("tenant-xyz");

    let json = serde_json::to_string(&e).unwrap();
    let decoded: EntityEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.event_type, EventKind::Created);
    assert_eq!(decoded.entity_type, "Order");
    assert_eq!(decoded.entity_id, entity_id);
    assert_eq!(decoded.user_id, Some("user-abc".into()));
    assert_eq!(decoded.tenant_id, Some("tenant-xyz".into()));
    assert_eq!(decoded.data["total"], 42);
}

// ── RetryConfig defaults ──────────────────────────────────────────────────────

#[test]
fn retry_config_default_max_attempts() {
    assert_eq!(RetryConfig::default().max_attempts, 3);
}

#[test]
fn retry_config_default_initial_delay_ms() {
    assert_eq!(RetryConfig::default().initial_delay_ms, 100);
}

#[test]
fn retry_config_default_max_delay_ms() {
    assert_eq!(RetryConfig::default().max_delay_ms, 30_000);
}

#[test]
fn retry_config_default_backoff_strategy_is_exponential() {
    assert!(matches!(RetryConfig::default().backoff_strategy, BackoffStrategy::Exponential));
}

#[test]
fn retry_config_serde_round_trip_with_defaults() {
    let cfg = RetryConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let decoded: RetryConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.max_attempts, cfg.max_attempts);
    assert_eq!(decoded.initial_delay_ms, cfg.initial_delay_ms);
    assert_eq!(decoded.max_delay_ms, cfg.max_delay_ms);
}

// ── BackoffStrategy serde ─────────────────────────────────────────────────────

#[test]
fn backoff_strategy_exponential_is_default() {
    assert!(matches!(BackoffStrategy::default(), BackoffStrategy::Exponential));
}

#[test]
fn backoff_strategy_serde_exponential() {
    let json = serde_json::to_string(&BackoffStrategy::Exponential).unwrap();
    assert_eq!(json, "\"exponential\"");
    let decoded: BackoffStrategy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, BackoffStrategy::Exponential));
}

#[test]
fn backoff_strategy_serde_linear() {
    let json = serde_json::to_string(&BackoffStrategy::Linear).unwrap();
    assert_eq!(json, "\"linear\"");
    let decoded: BackoffStrategy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, BackoffStrategy::Linear));
}

#[test]
fn backoff_strategy_serde_fixed() {
    let json = serde_json::to_string(&BackoffStrategy::Fixed).unwrap();
    assert_eq!(json, "\"fixed\"");
    let decoded: BackoffStrategy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, BackoffStrategy::Fixed));
}

// ── FailurePolicy serde ───────────────────────────────────────────────────────

#[test]
fn failure_policy_log_is_default() {
    assert!(matches!(FailurePolicy::default(), FailurePolicy::Log));
}

#[test]
fn failure_policy_serde_log() {
    let json = serde_json::to_string(&FailurePolicy::Log).unwrap();
    assert_eq!(json, "\"log\"");
    let decoded: FailurePolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, FailurePolicy::Log));
}

#[test]
fn failure_policy_serde_alert() {
    let json = serde_json::to_string(&FailurePolicy::Alert).unwrap();
    assert_eq!(json, "\"alert\"");
    let decoded: FailurePolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, FailurePolicy::Alert));
}

#[test]
fn failure_policy_serde_dlq() {
    let json = serde_json::to_string(&FailurePolicy::Dlq).unwrap();
    assert_eq!(json, "\"dlq\"");
    let decoded: FailurePolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, FailurePolicy::Dlq));
}

// ── OverflowPolicy serde ──────────────────────────────────────────────────────

#[test]
fn overflow_policy_drop_is_default() {
    assert!(matches!(OverflowPolicy::default(), OverflowPolicy::Drop));
}

#[test]
fn overflow_policy_serde_drop() {
    let json = serde_json::to_string(&OverflowPolicy::Drop).unwrap();
    assert_eq!(json, "\"drop\"");
    let decoded: OverflowPolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, OverflowPolicy::Drop));
}

#[test]
fn overflow_policy_serde_block() {
    let json = serde_json::to_string(&OverflowPolicy::Block).unwrap();
    assert_eq!(json, "\"block\"");
    let decoded: OverflowPolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, OverflowPolicy::Block));
}

#[test]
fn overflow_policy_serde_drop_oldest() {
    let json = serde_json::to_string(&OverflowPolicy::DropOldest).unwrap();
    assert_eq!(json, "\"drop_oldest\"");
    let decoded: OverflowPolicy = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, OverflowPolicy::DropOldest));
}
