#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use crate::schema::observer_types::RetryConfig;
use crate::schema::security_config::SecurityConfig;

use super::query::QueryDefinition;
use super::schema::CompiledSchema;
use super::super::observer_types::ObserverDefinition;

#[test]
fn test_compiled_schema_with_observers() {
    let json = r#"{
        "types": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "queries": [],
        "mutations": [],
        "subscriptions": [],
        "observers": [
            {
                "name": "onHighValueOrder",
                "entity": "Order",
                "event": "INSERT",
                "condition": "total > 1000",
                "actions": [
                    {
                        "type": "webhook",
                        "url": "https://api.example.com/webhook"
                    }
                ],
                "retry": {
                    "max_attempts": 3,
                    "backoff_strategy": "exponential",
                    "initial_delay_ms": 1000,
                    "max_delay_ms": 60000
                }
            }
        ]
    }"#;

    let schema = CompiledSchema::from_json(json).unwrap();

    assert!(schema.has_observers());
    assert_eq!(schema.observer_count(), 1);

    let observer = schema.find_observer("onHighValueOrder").unwrap();
    assert_eq!(observer.entity, "Order");
    assert_eq!(observer.event, "INSERT");
    assert_eq!(observer.condition, Some("total > 1000".to_string()));
    assert_eq!(observer.actions.len(), 1);
    assert_eq!(observer.retry.max_attempts, 3);
    assert!(observer.retry.is_exponential());
}

#[test]
fn test_compiled_schema_backward_compatible() {
    // Schema without observers field should still load
    let json = r#"{
        "types": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "queries": [],
        "mutations": [],
        "subscriptions": []
    }"#;

    let schema = CompiledSchema::from_json(json).unwrap();
    assert!(!schema.has_observers());
    assert_eq!(schema.observer_count(), 0);
}

#[test]
fn test_find_observers_for_entity() {
    let schema = CompiledSchema {
        observers: vec![
            ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
            ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
            ObserverDefinition::new("onUserInsert", "User", "INSERT"),
        ],
        ..Default::default()
    };

    let order_observers = schema.find_observers_for_entity("Order");
    assert_eq!(order_observers.len(), 2);

    let user_observers = schema.find_observers_for_entity("User");
    assert_eq!(user_observers.len(), 1);
}

#[test]
fn test_find_observers_for_event() {
    let schema = CompiledSchema {
        observers: vec![
            ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
            ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
            ObserverDefinition::new("onUserInsert", "User", "INSERT"),
        ],
        ..Default::default()
    };

    let insert_observers = schema.find_observers_for_event("INSERT");
    assert_eq!(insert_observers.len(), 2);

    let update_observers = schema.find_observers_for_event("UPDATE");
    assert_eq!(update_observers.len(), 1);
}

#[test]
fn test_observer_definition_builder() {
    let observer = ObserverDefinition::new("test", "Order", "INSERT")
        .with_condition("total > 1000")
        .with_action(serde_json::json!({"type": "webhook", "url": "https://example.com"}))
        .with_retry(RetryConfig::exponential(5, 1000, 60000));

    assert_eq!(observer.name, "test");
    assert_eq!(observer.entity, "Order");
    assert_eq!(observer.event, "INSERT");
    assert!(observer.has_condition());
    assert_eq!(observer.action_count(), 1);
    assert_eq!(observer.retry.max_attempts, 5);
}

#[test]
fn test_retry_config_types() {
    let exponential = RetryConfig::exponential(3, 1000, 60000);
    assert!(exponential.is_exponential());
    assert!(!exponential.is_linear());
    assert!(!exponential.is_fixed());

    let linear = RetryConfig::linear(3, 1000, 60000);
    assert!(!linear.is_exponential());
    assert!(linear.is_linear());
    assert!(!linear.is_fixed());

    let fixed = RetryConfig::fixed(3, 5000);
    assert!(!fixed.is_exponential());
    assert!(!fixed.is_linear());
    assert!(fixed.is_fixed());
    assert_eq!(fixed.initial_delay_ms, 5000);
    assert_eq!(fixed.max_delay_ms, 5000);
}

// =========================================================================
// content_hash tests
// =========================================================================

#[test]
fn test_content_hash_stable() {
    let schema = CompiledSchema::default();
    assert_eq!(schema.content_hash(), schema.content_hash(), "Same schema must produce same hash");
}

#[test]
fn test_content_hash_length() {
    let hash = CompiledSchema::default().content_hash();
    assert_eq!(hash.len(), 32, "Hash must be 32 hex chars (16 bytes)");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "Hash must be valid hex");
}

#[test]
fn test_content_hash_changes_on_field_rename() {
    let mut schema_a = CompiledSchema::default();
    schema_a.queries.push(QueryDefinition::new("users", "User").with_sql_source("v_user"));

    let mut schema_b = CompiledSchema::default();
    schema_b.queries.push(QueryDefinition::new("users", "User").with_sql_source("v_account")); // different view

    assert_ne!(
        schema_a.content_hash(),
        schema_b.content_hash(),
        "Schemas with different view names must produce different hashes"
    );
}

// =========================================================================
// has_rls_configured tests
// =========================================================================

#[test]
fn test_has_rls_configured_no_security() {
    let schema = CompiledSchema::default();
    assert!(!schema.has_rls_configured(), "Schema with no security section must return false");
}

#[test]
fn test_has_rls_configured_with_empty_policies() {
    let mut sec = SecurityConfig::default();
    sec.additional.insert("policies".to_string(), serde_json::json!([]));
    let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
    assert!(!schema.has_rls_configured(), "Empty policies array must return false");
}

#[test]
fn test_has_rls_configured_with_policies() {
    let mut sec = SecurityConfig::default();
    sec.additional.insert(
        "policies".to_string(),
        serde_json::json!([{"name": "tenant_isolation", "condition": "tenant_id = $1"}]),
    );
    let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
    assert!(schema.has_rls_configured(), "Non-empty policies array must return true");
}

#[test]
fn test_has_rls_configured_no_policies_key() {
    let mut sec = SecurityConfig::default();
    sec.additional.insert("rate_limiting".to_string(), serde_json::json!({"enabled": true}));
    let schema = CompiledSchema { security: Some(sec), ..CompiledSchema::default() };
    assert!(!schema.has_rls_configured(), "Security without policies key must return false");
}
