#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::sync::Arc;

use uuid::Uuid;

use super::*;
use crate::schema::{CompiledSchema, SubscriptionDefinition};

fn create_test_schema() -> CompiledSchema {
    CompiledSchema {
        subscriptions: vec![
            SubscriptionDefinition::new("OrderCreated", "Order").with_topic("order_created"),
            SubscriptionDefinition::new("OrderUpdated", "Order").with_topic("order_updated"),
            SubscriptionDefinition::new("UserDeleted", "User").with_topic("user_deleted"),
        ],
        ..Default::default()
    }
}

#[test]
fn test_subscription_id() {
    let id1 = SubscriptionId::new();
    let id2 = SubscriptionId::new();
    assert_ne!(id1, id2);

    let uuid = Uuid::new_v4();
    let id3 = SubscriptionId::from_uuid(uuid);
    assert_eq!(id3.0, uuid);
}

#[test]
fn test_subscription_event_creation() {
    let event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "ord_123", "amount": 99.99}),
    );

    assert!(event.event_id.starts_with("evt_"));
    assert_eq!(event.entity_type, "Order");
    assert_eq!(event.entity_id, "ord_123");
    assert_eq!(event.operation, SubscriptionOperation::Create);
}

#[test]
fn test_subscription_manager_subscribe() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    let id = manager
        .subscribe(
            "OrderCreated",
            serde_json::json!({"user_id": "usr_123"}),
            serde_json::json!({}),
            "conn_1",
        )
        .unwrap();

    assert_eq!(manager.subscription_count(), 1);
    assert_eq!(manager.connection_count(), 1);

    let sub = manager.get_subscription(id).unwrap();
    assert_eq!(sub.subscription_name, "OrderCreated");
    assert_eq!(sub.connection_id, "conn_1");
}

#[test]
fn test_subscription_manager_subscribe_not_found() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    let result =
        manager.subscribe("NonExistent", serde_json::json!({}), serde_json::json!({}), "conn_1");

    assert!(matches!(result, Err(SubscriptionError::SubscriptionNotFound(_))));
}

#[test]
fn test_subscription_manager_unsubscribe() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    let id = manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    assert_eq!(manager.subscription_count(), 1);

    manager.unsubscribe(id).unwrap();

    assert_eq!(manager.subscription_count(), 0);
}

#[test]
fn test_subscription_manager_unsubscribe_connection() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Create multiple subscriptions for same connection
    manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    manager
        .subscribe("OrderUpdated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    assert_eq!(manager.subscription_count(), 2);

    manager.unsubscribe_connection("conn_1");

    assert_eq!(manager.subscription_count(), 0);
    assert_eq!(manager.connection_count(), 0);
}

#[test]
fn test_subscription_event_matching() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Subscribe to OrderCreated
    manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    // Create event should match
    let create_event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "ord_123"}),
    );

    let delivered = manager.publish_event(create_event);
    assert_eq!(delivered, 1);

    // Update event should not match (wrong operation)
    let update_event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Update,
        serde_json::json!({"id": "ord_123"}),
    );

    let delivered = manager.publish_event(update_event);
    assert_eq!(delivered, 0);
}

#[test]
fn test_subscription_event_wrong_entity() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Subscribe to OrderCreated
    manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    // User event should not match (wrong entity)
    let user_event = SubscriptionEvent::new(
        "User",
        "usr_123",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "usr_123"}),
    );

    let delivered = manager.publish_event(user_event);
    assert_eq!(delivered, 0);
}

#[test]
fn test_subscription_sequence_numbers() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    let mut receiver = manager.receiver();

    // Publish multiple events
    for i in 1..=3 {
        let event = SubscriptionEvent::new(
            "Order",
            format!("ord_{i}"),
            SubscriptionOperation::Create,
            serde_json::json!({"id": format!("ord_{}", i)}),
        );
        manager.publish_event(event);
    }

    // Check sequence numbers are monotonic
    let mut last_seq = 0;
    for _ in 0..3 {
        if let Ok(payload) = receiver.try_recv() {
            assert!(payload.event.sequence_number > last_seq);
            last_seq = payload.event.sequence_number;
        }
    }
}

// =========================================================================

// =========================================================================
// Transport Adapter Tests
// =========================================================================

#[test]
fn test_webhook_config_builder() {
    let config = WebhookConfig::new("https://api.example.com/webhooks")
        .with_secret("my-secret")
        .with_timeout(10_000)
        .with_max_retries(5)
        .with_retry_delay(500)
        .with_header("X-Custom-Header", "custom-value");

    assert_eq!(config.url, "https://api.example.com/webhooks");
    assert_eq!(config.secret, Some("my-secret".to_string()));
    assert_eq!(config.timeout_ms, 10_000);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.retry_delay_ms, 500);
    assert_eq!(config.headers.get("X-Custom-Header"), Some(&"custom-value".to_string()));
}

#[test]
fn test_webhook_config_defaults() {
    let config = WebhookConfig::new("https://api.example.com/webhooks");

    assert_eq!(config.url, "https://api.example.com/webhooks");
    assert!(config.secret.is_none());
    assert_eq!(config.timeout_ms, 30_000);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_delay_ms, 1000);
    assert!(config.headers.is_empty());
}

#[test]
fn test_webhook_payload_from_event() {
    let event = SubscriptionEvent {
        event_id:        "evt_123".to_string(),
        entity_type:     "Order".to_string(),
        entity_id:       "ord_456".to_string(),
        operation:       SubscriptionOperation::Create,
        data:            serde_json::json!({"id": "ord_456", "total": 99.99}),
        old_data:        None,
        timestamp:       chrono::Utc::now(),
        sequence_number: 42,
    };

    let payload = WebhookPayload::from_event(&event, "order_created");

    assert_eq!(payload.event_id, "evt_123");
    assert_eq!(payload.subscription_name, "order_created");
    assert_eq!(payload.entity_type, "Order");
    assert_eq!(payload.entity_id, "ord_456");
    assert_eq!(payload.operation, "Create");
    assert_eq!(payload.data["total"], 99.99);
    assert!(payload.old_data.is_none());
    assert_eq!(payload.sequence_number, 42);
}

#[test]
fn test_webhook_adapter_debug() {
    let config = WebhookConfig::new("https://api.example.com/webhooks").with_secret("secret-key");
    let adapter = WebhookAdapter::new(config);

    let debug = format!("{:?}", adapter);
    assert!(debug.contains("WebhookAdapter"));
    assert!(debug.contains("https://api.example.com/webhooks"));
    assert!(debug.contains("has_secret: true"));
}

#[test]
fn test_webhook_adapter_name() {
    let config = WebhookConfig::new("https://api.example.com/webhooks");
    let adapter = WebhookAdapter::new(config);

    assert_eq!(adapter.name(), "webhook");
}

#[test]
fn test_kafka_config_builder() {
    let config = KafkaConfig::new("localhost:9092", "events")
        .with_client_id("test-client")
        .with_acks("all")
        .with_timeout(5_000)
        .with_compression("gzip");

    assert_eq!(config.brokers, "localhost:9092");
    assert_eq!(config.default_topic, "events");
    assert_eq!(config.client_id, "test-client");
    assert_eq!(config.acks, "all");
    assert_eq!(config.timeout_ms, 5_000);
    assert_eq!(config.compression, Some("gzip".to_string()));
}

#[test]
fn test_kafka_config_defaults() {
    let config = KafkaConfig::new("localhost:9092", "events");

    assert_eq!(config.brokers, "localhost:9092");
    assert_eq!(config.default_topic, "events");
    assert_eq!(config.client_id, "fraiseql");
    assert_eq!(config.acks, "all"); // Default: wait for all replicas
    assert_eq!(config.timeout_ms, 30_000); // 30 seconds default
    assert!(config.compression.is_none());
}

#[test]
fn test_kafka_message_from_event() {
    let event = SubscriptionEvent {
        event_id:        "evt_789".to_string(),
        entity_type:     "User".to_string(),
        entity_id:       "usr_123".to_string(),
        operation:       SubscriptionOperation::Update,
        data:            serde_json::json!({"id": "usr_123", "name": "John"}),
        old_data:        Some(serde_json::json!({"id": "usr_123", "name": "Jane"})),
        timestamp:       chrono::Utc::now(),
        sequence_number: 100,
    };

    let message = KafkaMessage::from_event(&event, "user_updated");

    assert_eq!(message.event_id, "evt_789");
    assert_eq!(message.subscription_name, "user_updated");
    assert_eq!(message.entity_type, "User");
    assert_eq!(message.entity_id, "usr_123");
    assert_eq!(message.operation, "Update");
    assert_eq!(message.data["name"], "John");
    assert_eq!(message.old_data.as_ref().unwrap()["name"], "Jane");
    assert_eq!(message.sequence_number, 100);
}

#[test]
fn test_kafka_message_key() {
    let event = SubscriptionEvent {
        event_id:        "evt_1".to_string(),
        entity_type:     "Order".to_string(),
        entity_id:       "ord_partition_key".to_string(),
        operation:       SubscriptionOperation::Create,
        data:            serde_json::json!({}),
        old_data:        None,
        timestamp:       chrono::Utc::now(),
        sequence_number: 1,
    };

    let message = KafkaMessage::from_event(&event, "test_sub");

    // Key should be entity_id for consistent partitioning
    assert_eq!(message.key(), "ord_partition_key");
}

#[test]
fn test_kafka_adapter_name() {
    let config = KafkaConfig::new("localhost:9092", "events");
    let adapter = KafkaAdapter::new(config).unwrap();

    assert_eq!(adapter.name(), "kafka");
}

#[test]
fn test_transport_manager_new() {
    let manager = TransportManager::new();
    assert!(manager.is_empty());
    assert_eq!(manager.adapter_count(), 0);
}

#[test]
fn test_transport_manager_add_adapter() {
    let mut manager = TransportManager::new();

    let webhook = WebhookAdapter::new(WebhookConfig::new("https://api.example.com/webhooks"));
    manager.add_adapter(Box::new(webhook));

    assert!(!manager.is_empty());
    assert_eq!(manager.adapter_count(), 1);
}

#[test]
fn test_transport_manager_debug() {
    let mut manager = TransportManager::new();
    let webhook = WebhookAdapter::new(WebhookConfig::new("https://api.example.com/webhooks"));
    manager.add_adapter(Box::new(webhook));

    let debug = format!("{:?}", manager);
    assert!(debug.contains("TransportManager"));
    assert!(debug.contains("adapter_count: 1"));
}

#[test]
fn test_delivery_result_all_succeeded() {
    let result = DeliveryResult {
        successful: 3,
        failed:     0,
        errors:     vec![],
    };

    assert!(result.all_succeeded());
    assert!(result.any_succeeded());
}

#[test]
fn test_delivery_result_partial_failure() {
    let result = DeliveryResult {
        successful: 2,
        failed:     1,
        errors:     vec![("webhook".to_string(), "Connection refused".to_string())],
    };

    assert!(!result.all_succeeded());
    assert!(result.any_succeeded());
}

#[test]
fn test_delivery_result_all_failed() {
    let result = DeliveryResult {
        successful: 0,
        failed:     2,
        errors:     vec![
            ("webhook".to_string(), "Connection refused".to_string()),
            ("kafka".to_string(), "Broker unavailable".to_string()),
        ],
    };

    assert!(!result.all_succeeded());
    assert!(!result.any_succeeded());
}

// =========================================================================
// Filter Evaluation Tests
// =========================================================================

#[test]
fn test_get_json_pointer_value_simple() {
    use super::manager::get_json_pointer_value;

    let data = serde_json::json!({"id": "123", "name": "Test"});

    assert_eq!(get_json_pointer_value(&data, "/id"), Some(&serde_json::json!("123")));
    assert_eq!(get_json_pointer_value(&data, "/name"), Some(&serde_json::json!("Test")));
    assert_eq!(get_json_pointer_value(&data, "/missing"), None);
}

#[test]
fn test_get_json_pointer_value_nested() {
    use super::manager::get_json_pointer_value;

    let data = serde_json::json!({
        "user": {
            "profile": {
                "name": "Alice"
            }
        }
    });

    assert_eq!(
        get_json_pointer_value(&data, "/user/profile/name"),
        Some(&serde_json::json!("Alice"))
    );
}

#[test]
fn test_get_json_pointer_value_dot_notation() {
    use super::manager::get_json_pointer_value;

    let data = serde_json::json!({"user": {"name": "Bob"}});

    // Dot notation should be converted to JSON pointer
    assert_eq!(get_json_pointer_value(&data, "user.name"), Some(&serde_json::json!("Bob")));
}

#[test]
fn test_filter_condition_eq() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    assert!(evaluate_filter_condition(
        Some(&serde_json::json!("active")),
        FilterOperator::Eq,
        &serde_json::json!("active")
    ));

    assert!(!evaluate_filter_condition(
        Some(&serde_json::json!("active")),
        FilterOperator::Eq,
        &serde_json::json!("inactive")
    ));
}

#[test]
fn test_filter_condition_ne() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    assert!(evaluate_filter_condition(
        Some(&serde_json::json!("active")),
        FilterOperator::Ne,
        &serde_json::json!("inactive")
    ));

    assert!(!evaluate_filter_condition(
        Some(&serde_json::json!("active")),
        FilterOperator::Ne,
        &serde_json::json!("active")
    ));
}

#[test]
fn test_filter_condition_numeric_comparisons() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    // Greater than
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!(100)),
        FilterOperator::Gt,
        &serde_json::json!(50)
    ));
    assert!(!evaluate_filter_condition(
        Some(&serde_json::json!(50)),
        FilterOperator::Gt,
        &serde_json::json!(100)
    ));

    // Greater than or equal
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!(100)),
        FilterOperator::Gte,
        &serde_json::json!(100)
    ));

    // Less than
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!(50)),
        FilterOperator::Lt,
        &serde_json::json!(100)
    ));

    // Less than or equal
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!(100)),
        FilterOperator::Lte,
        &serde_json::json!(100)
    ));
}

#[test]
fn test_filter_condition_string_comparisons() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    // Contains
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!("hello world")),
        FilterOperator::Contains,
        &serde_json::json!("world")
    ));

    // StartsWith
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!("hello world")),
        FilterOperator::StartsWith,
        &serde_json::json!("hello")
    ));

    // EndsWith
    assert!(evaluate_filter_condition(
        Some(&serde_json::json!("hello world")),
        FilterOperator::EndsWith,
        &serde_json::json!("world")
    ));
}

#[test]
fn test_filter_condition_array_contains() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    assert!(evaluate_filter_condition(
        Some(&serde_json::json!(["a", "b", "c"])),
        FilterOperator::Contains,
        &serde_json::json!("b")
    ));

    assert!(!evaluate_filter_condition(
        Some(&serde_json::json!(["a", "b", "c"])),
        FilterOperator::Contains,
        &serde_json::json!("d")
    ));
}

#[test]
fn test_filter_condition_null_handling() {
    use super::manager::evaluate_filter_condition;
    use crate::schema::FilterOperator;

    // Missing value equals null
    assert!(evaluate_filter_condition(None, FilterOperator::Eq, &serde_json::Value::Null));

    // Missing value does not equal non-null
    assert!(!evaluate_filter_condition(
        None,
        FilterOperator::Eq,
        &serde_json::json!("value")
    ));
}

#[test]
fn test_subscription_filter_matching() {
    use std::collections::HashMap;

    use crate::schema::{FilterOperator, StaticFilterCondition, SubscriptionFilter};

    let mut argument_paths = HashMap::new();
    argument_paths.insert("orderId".to_string(), "/id".to_string());

    let filter = SubscriptionFilter {
        argument_paths,
        static_filters: vec![StaticFilterCondition {
            path:     "/status".to_string(),
            operator: FilterOperator::Eq,
            value:    serde_json::json!("active"),
        }],
    };

    let schema = Arc::new(CompiledSchema {
        subscriptions: vec![
            SubscriptionDefinition::new("OrderUpdated", "Order")
                .with_topic("order_updated")
                .with_filter(filter),
        ],
        ..Default::default()
    });

    let manager = SubscriptionManager::new(schema);

    // Subscribe with a specific orderId
    manager
        .subscribe(
            "OrderUpdated",
            serde_json::json!({}),
            serde_json::json!({"orderId": "ord_123"}),
            "conn_1",
        )
        .unwrap();

    // Event matching the filter
    let matching_event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Update,
        serde_json::json!({"id": "ord_123", "status": "active"}),
    );
    assert_eq!(manager.publish_event(matching_event), 1);

    // Event with wrong orderId
    let wrong_id_event = SubscriptionEvent::new(
        "Order",
        "ord_456",
        SubscriptionOperation::Update,
        serde_json::json!({"id": "ord_456", "status": "active"}),
    );
    assert_eq!(manager.publish_event(wrong_id_event), 0);

    // Event with wrong status
    let wrong_status_event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Update,
        serde_json::json!({"id": "ord_123", "status": "inactive"}),
    );
    assert_eq!(manager.publish_event(wrong_status_event), 0);
}

#[test]
fn test_subscription_field_projection() {
    let schema = Arc::new(CompiledSchema {
        subscriptions: vec![
            SubscriptionDefinition::new("OrderCreated", "Order")
                .with_topic("order_created")
                .with_fields(vec!["id".to_string(), "total".to_string()]),
        ],
        ..Default::default()
    });

    let manager = SubscriptionManager::new(schema);

    manager
        .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
        .unwrap();

    let mut receiver = manager.receiver();

    let event = SubscriptionEvent::new(
        "Order",
        "ord_123",
        SubscriptionOperation::Create,
        serde_json::json!({
            "id": "ord_123",
            "total": 99.99,
            "secret_field": "should_not_appear",
            "customer": "John"
        }),
    );

    manager.publish_event(event);

    if let Ok(payload) = receiver.try_recv() {
        // Only projected fields should be present
        assert_eq!(payload.data.get("id"), Some(&serde_json::json!("ord_123")));
        assert_eq!(payload.data.get("total"), Some(&serde_json::json!(99.99)));
        assert!(payload.data.get("secret_field").is_none());
        assert!(payload.data.get("customer").is_none());
    }
}

// =========================================================================
// filter_fields Expansion Tests
// =========================================================================

#[test]
fn test_filter_fields_auto_generates_argument_paths() {
    let mut def = SubscriptionDefinition::new("OrderCreated", "Order")
        .with_topic("order_created");
    def.filter_fields = vec!["user_id".to_string(), "tenant_id".to_string()];

    let schema = Arc::new(CompiledSchema {
        subscriptions: vec![def],
        ..Default::default()
    });

    let manager = SubscriptionManager::new(schema);

    // Subscribe with filter_fields variables
    let id = manager
        .subscribe(
            "OrderCreated",
            serde_json::json!({}),
            serde_json::json!({"user_id": "usr_1", "tenant_id": "t_1"}),
            "conn_1",
        )
        .unwrap();

    // The definition should now have argument_paths auto-generated
    let sub = manager.get_subscription(id).unwrap();
    let filter = sub.definition.filter.as_ref().expect("filter should exist");
    assert_eq!(filter.argument_paths.get("user_id"), Some(&"/user_id".to_string()));
    assert_eq!(filter.argument_paths.get("tenant_id"), Some(&"/tenant_id".to_string()));
}

#[test]
fn test_filter_fields_does_not_overwrite_explicit_argument_paths() {
    use std::collections::HashMap;
    use crate::schema::SubscriptionFilter;

    let mut argument_paths = HashMap::new();
    argument_paths.insert("user_id".to_string(), "/author/id".to_string());

    let mut def = SubscriptionDefinition::new("OrderCreated", "Order")
        .with_topic("order_created")
        .with_filter(SubscriptionFilter {
            argument_paths,
            static_filters: Vec::new(),
        });
    // filter_fields includes user_id (already in argument_paths) and tenant_id (new)
    def.filter_fields = vec!["user_id".to_string(), "tenant_id".to_string()];

    let schema = Arc::new(CompiledSchema {
        subscriptions: vec![def],
        ..Default::default()
    });

    let manager = SubscriptionManager::new(schema);

    let id = manager
        .subscribe(
            "OrderCreated",
            serde_json::json!({}),
            serde_json::json!({"user_id": "usr_1", "tenant_id": "t_1"}),
            "conn_1",
        )
        .unwrap();

    let sub = manager.get_subscription(id).unwrap();
    let filter = sub.definition.filter.as_ref().unwrap();
    // Explicit path should be preserved, not overwritten
    assert_eq!(filter.argument_paths.get("user_id"), Some(&"/author/id".to_string()));
    // New field should be auto-generated
    assert_eq!(filter.argument_paths.get("tenant_id"), Some(&"/tenant_id".to_string()));
}

#[test]
fn test_filter_fields_filtering_events() {
    let mut def = SubscriptionDefinition::new("OrderCreated", "Order")
        .with_topic("order_created");
    def.filter_fields = vec!["user_id".to_string()];

    let schema = Arc::new(CompiledSchema {
        subscriptions: vec![def],
        ..Default::default()
    });

    let manager = SubscriptionManager::new(schema);

    // Subscribe with user_id filter
    manager
        .subscribe(
            "OrderCreated",
            serde_json::json!({}),
            serde_json::json!({"user_id": "usr_1"}),
            "conn_1",
        )
        .unwrap();

    // Matching event
    let matching = SubscriptionEvent::new(
        "Order",
        "ord_1",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "ord_1", "user_id": "usr_1"}),
    );
    assert_eq!(manager.publish_event(matching), 1);

    // Non-matching event (different user_id)
    let non_matching = SubscriptionEvent::new(
        "Order",
        "ord_2",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "ord_2", "user_id": "usr_2"}),
    );
    assert_eq!(manager.publish_event(non_matching), 0);
}
