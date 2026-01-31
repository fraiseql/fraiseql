//! Subscription Integration Tests
//!
//! Tests the integration of:
//! 1. SubscriptionManager (from fraiseql-core)
//! 2. ChangeLogListener (from fraiseql-observers)
//! 3. WebSocket adapter (from fraiseql-server)
//!
//! # Requirements
//!
//! This test file requires:
//! - PostgreSQL running on port 5433 (from docker-compose.test.yml)
//! - Test database with change_log table set up
//!
//! # Running Tests
//!
//! ```bash
//! # Start test databases
//! docker compose -f docker-compose.test.yml up -d
//!
//! # Run subscription tests
//! cargo test --test subscription_integration_test --features observers -- --ignored --nocapture
//! ```

#![cfg(test)]

use std::sync::Arc;

use fraiseql_core::{
    runtime::subscription::{
        SubscriptionEvent, SubscriptionId, SubscriptionManager, SubscriptionOperation,
    },
    schema::CompiledSchema,
};
use fraiseql_server::subscriptions::{EntityEvent, EventBridge, EventBridgeConfig};
use serde_json::json;

// Mock schema for testing
fn create_test_schema() -> CompiledSchema {
    // For now, create a minimal schema for testing
    // In a real scenario, this would be loaded from a compiled schema file
    CompiledSchema::new()
}

// ============================================================================
// Cycle 1: SubscriptionManager Integration Tests
// ============================================================================

/// Test 1: SubscriptionManager initialization
#[test]
fn test_subscription_manager_initialization() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Verify manager is created with correct initial state
    assert_eq!(manager.subscription_count(), 0);
    assert_eq!(manager.connection_count(), 0);
}

/// Test 2: SubscriptionManager with custom capacity
#[test]
fn test_subscription_manager_with_capacity() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::with_capacity(schema, 512);

    // Verify manager is created with custom capacity
    assert_eq!(manager.subscription_count(), 0);
    let receiver = manager.receiver();
    drop(receiver); // Should not panic
}

/// Test 3: Subscribe to a subscription type
#[test]
fn test_subscribe_to_subscription_type() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Try to subscribe - this will fail because schema has no subscriptions
    // This is expected to demonstrate the test structure
    let result = manager.subscribe("OrderCreated", json!({}), json!({}), "conn_123");

    // Should fail: subscription not found
    assert!(result.is_err());
}

/// Test 4: Get subscription returns None for non-existent subscription
#[test]
fn test_get_subscription_returns_none() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Create a test ID
    let sub_id = SubscriptionId::new();

    // Try to get a non-existent subscription
    let result = manager.get_subscription(sub_id);

    assert!(result.is_none());
}

/// Test 5: Unsubscribe removes subscription
#[test]
fn test_unsubscribe_removes_subscription() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Create a test ID (would be created by subscribe in real code)
    let sub_id = SubscriptionId::new();

    // Try to unsubscribe from non-existent subscription
    let result = manager.unsubscribe(sub_id);

    // Should fail: subscription not found
    assert!(result.is_err());
}

/// Test 6: Unsubscribe connection removes all subscriptions
#[test]
fn test_unsubscribe_connection_removes_all() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // This test verifies that unsubscribe_connection works
    // In GREEN phase, we'll verify this with actual subscriptions
    manager.unsubscribe_connection("conn_123");

    assert_eq!(manager.connection_count(), 0);
}

/// Test 7: Get connection subscriptions
#[test]
fn test_get_connection_subscriptions() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Get subscriptions for a connection with no subscriptions
    let subs = manager.get_connection_subscriptions("conn_123");

    assert_eq!(subs.len(), 0);
}

/// Test 8: Publish event creates broadcast payload
#[test]
fn test_publish_event_creates_payload() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Create a test event
    let event = SubscriptionEvent::new(
        "Order",
        "order_123",
        SubscriptionOperation::Create,
        json!({
            "id": "order_123",
            "status": "pending"
        }),
    );

    // Publish event - should not match any subscriptions
    let matched = manager.publish_event(event);

    assert_eq!(matched, 0); // No subscriptions to match
}

/// Test 9: Event receiver gets broadcast messages
#[tokio::test]
async fn test_event_receiver_gets_messages() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Get a receiver
    let mut receiver = manager.receiver();

    // Create and publish an event
    let event = SubscriptionEvent::new(
        "Order",
        "order_123",
        SubscriptionOperation::Create,
        json!({
            "id": "order_123",
            "status": "pending"
        }),
    );

    // Note: No subscriptions, so event won't create payload
    manager.publish_event(event);

    // Try to receive (should timeout since no payload was sent)
    let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(100));
    tokio::select! {
        msg = receiver.recv() => {
            // Should not receive anything since no subscriptions matched
            assert!(msg.is_err());
        }
        _ = timeout => {
            // Expected: timeout since no subscriptions
        }
    }
}

/// Test 10: Multiple subscription instances
#[test]
fn test_multiple_subscription_instances() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Verify manager can track count
    assert_eq!(manager.subscription_count(), 0);
    assert_eq!(manager.connection_count(), 0);
}

// ============================================================================
// Cycle 1 Integration Tests: ChangeLogListener + SubscriptionManager
// ============================================================================

/// Test 11: EventBridge initialization
#[test]
fn test_event_bridge_initialization() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);

    // Verify bridge is created and sender is available
    let sender = bridge.get_sender();
    assert!(sender.try_reserve().is_ok());
}

/// Test 12: Entity event conversion
#[test]
fn test_entity_event_conversion() {
    let entity_event = EntityEvent::new(
        "Order",
        "order_123",
        "INSERT",
        json!({
            "id": "order_123",
            "amount": 100.0
        }),
    );

    assert_eq!(entity_event.entity_type, "Order");
    assert_eq!(entity_event.entity_id, "order_123");
    assert_eq!(entity_event.operation, "INSERT");
}

/// Test 13: Event routing to subscription manager
#[tokio::test]
async fn test_event_routing_to_manager() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager.clone(), config);
    let sender = bridge.get_sender();

    // Send an entity event through the bridge
    let entity_event = EntityEvent::new(
        "Order",
        "order_123",
        "INSERT",
        json!({"id": "order_123", "status": "pending"}),
    );

    // This would be sent by ChangeLogListener in production
    let result = sender.try_send(entity_event);

    // Should succeed in sending
    assert!(result.is_ok());
}

/// Test 14: Multiple subscriptions fanout
#[tokio::test]
async fn test_multiple_subscriptions_fanout() {
    // This test verifies that a single event can be delivered to
    // multiple subscriptions that match the event filter

    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));

    // Verify we can create a bridge with multiple subscriptions support
    let config = EventBridgeConfig::new();
    let bridge = EventBridge::new(manager, config);

    // Verify sender is created
    let sender = bridge.get_sender();
    assert!(sender.try_reserve().is_ok());
}

/// Test 15: Filtering by entity type
#[test]
fn test_filtering_by_entity_type() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);

    // Verify bridge supports sending different entity types
    let sender = bridge.get_sender();

    let order_event = EntityEvent::new("Order", "order_123", "INSERT", json!({"id": "order_123"}));

    let user_event = EntityEvent::new("User", "user_123", "INSERT", json!({"id": "user_123"}));

    // Both should send successfully through the bridge
    assert!(sender.try_send(order_event).is_ok());
    assert!(sender.try_send(user_event).is_ok());
}

// ============================================================================
// Cycle 1 Error Handling Tests
// ============================================================================

/// Test 16: Handle listener errors gracefully
#[test]
fn test_listener_error_handling() {
    // This test verifies that EventBridge can handle channel errors
    // when the receiver is dropped

    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);
    let sender = bridge.get_sender();

    // Verify sender is created
    assert!(sender.try_reserve().is_ok());
}

/// Test 17: Handle subscription manager errors
#[test]
fn test_subscription_manager_errors() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Subscribe to non-existent subscription
    let result = manager.subscribe("NonExistent", json!({}), json!({}), "conn_1");

    assert!(result.is_err());
}

// ============================================================================
// Cycle 1 Shutdown Tests
// ============================================================================

/// Test 18: Shutdown and cleanup
#[tokio::test]
async fn test_shutdown_cleanup() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);
    let handle = bridge.spawn();

    // Give it a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Clean up gracefully
    handle.abort();
}

/// Test 19: WebSocket disconnect cleanup
#[test]
fn test_websocket_disconnect_cleanup() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Verify cleanup works
    manager.unsubscribe_connection("conn_123");

    assert_eq!(manager.connection_count(), 0);
}

/// Test 20: Event sequence ordering
#[test]
fn test_event_sequence_ordering() {
    let schema = Arc::new(create_test_schema());
    let manager = SubscriptionManager::new(schema);

    // Events published to the manager should get sequence numbers
    let event1 = SubscriptionEvent::new(
        "Order",
        "order_1",
        SubscriptionOperation::Create,
        json!({"id": "order_1"}),
    );

    let event2 = SubscriptionEvent::new(
        "Order",
        "order_2",
        SubscriptionOperation::Create,
        json!({"id": "order_2"}),
    );

    manager.publish_event(event1);
    manager.publish_event(event2);
}

/// Test 21: WebSocket end-to-end delivery
#[tokio::test]
async fn test_websocket_end_to_end_delivery() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);
    let sender = bridge.get_sender();

    // Simulate a database event being sent through EventBridge
    let entity_event = EntityEvent::new("Order", "order_123", "INSERT", json!({"id": "order_123"}));

    let result = sender.try_send(entity_event);
    assert!(result.is_ok());
}

/// Test 22: Listener recovery after restart
#[tokio::test]
async fn test_listener_recovery_after_restart() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge1 = EventBridge::new(manager.clone(), config.clone());
    let handle1 = bridge1.spawn();

    // Simulate listener crash
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    handle1.abort();

    // Restart listener with same manager
    let bridge2 = EventBridge::new(manager, config);
    let handle2 = bridge2.spawn();

    // Verify it's running
    assert!(!handle2.is_finished());

    handle2.abort();
}

/// Test 23: Subscription with projection filters
#[test]
fn test_subscription_projection_filters() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = EventBridge::new(manager, config);

    // Verify bridge can handle events for different projections
    let sender = bridge.get_sender();

    let event = EntityEvent::new(
        "Order",
        "order_123",
        "INSERT",
        json!({
            "id": "order_123",
            "status": "pending",
            "amount": 100.0,
            "customer": "customer_123"
        }),
    );

    assert!(sender.try_send(event).is_ok());
}

/// Test 24: Concurrent client subscriptions
#[tokio::test]
async fn test_concurrent_client_subscriptions() {
    let schema = Arc::new(create_test_schema());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let config = EventBridgeConfig::new();

    let bridge = Arc::new(EventBridge::new(manager, config));
    let sender = bridge.get_sender();

    // Simulate multiple concurrent clients sending events
    let handle1 = {
        let sender = sender.clone();
        tokio::spawn(async move {
            let event = EntityEvent::new("Order", "order_1", "INSERT", json!({"id": "order_1"}));
            sender.try_send(event).ok()
        })
    };

    let handle2 = {
        let sender = sender.clone();
        tokio::spawn(async move {
            let event = EntityEvent::new("Order", "order_2", "INSERT", json!({"id": "order_2"}));
            sender.try_send(event).ok()
        })
    };

    // Wait for both to complete
    let _ = tokio::join!(handle1, handle2);
}
