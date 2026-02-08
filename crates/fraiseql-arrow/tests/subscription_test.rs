//! Tests for real-time event subscription (do_exchange)

use chrono::Utc;
use fraiseql_arrow::{HistoricalEvent, SubscriptionManager};
use uuid::Uuid;

#[tokio::test]
async fn test_subscribe_and_broadcast() {
    let manager = SubscriptionManager::new();

    // Create subscription
    let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    assert_eq!(manager.subscription_count(), 1);

    // Create and broadcast event
    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100.50}),
        user_id:     Some("user123".to_string()),
        tenant_id:   Some("tenant1".to_string()),
        timestamp:   Utc::now(),
    };

    manager.broadcast_event(&event);

    // Subscriber should receive the event
    let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await;

    assert!(received.is_ok());
    assert!(received.unwrap().is_some());
}

#[tokio::test]
async fn test_multiple_subscribers_same_type() {
    let manager = SubscriptionManager::new();

    let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    let mut rx2 = manager.subscribe("sub-2".to_string(), "Order".to_string(), None);

    assert_eq!(manager.subscription_count(), 2);

    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "UPDATE".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 200.00}),
        user_id:     None,
        tenant_id:   None,
        timestamp:   Utc::now(),
    };

    manager.broadcast_event(&event);

    // Both subscribers should receive the event
    let received1 = tokio::time::timeout(std::time::Duration::from_secs(1), rx1.recv()).await;

    let received2 = tokio::time::timeout(std::time::Duration::from_secs(1), rx2.recv()).await;

    assert!(received1.is_ok() && received1.unwrap().is_some());
    assert!(received2.is_ok() && received2.unwrap().is_some());
}

#[tokio::test]
async fn test_type_filtering() {
    let manager = SubscriptionManager::new();

    let mut order_rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    let mut user_rx = manager.subscribe("sub-2".to_string(), "User".to_string(), None);

    assert_eq!(manager.subscription_count(), 2);

    // Broadcast Order event
    let order_event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100}),
        user_id:     None,
        tenant_id:   None,
        timestamp:   Utc::now(),
    };

    manager.broadcast_event(&order_event);

    // Order subscriber receives, User subscriber doesn't
    let order_received =
        tokio::time::timeout(std::time::Duration::from_millis(100), order_rx.recv()).await;

    let user_received =
        tokio::time::timeout(std::time::Duration::from_millis(100), user_rx.recv()).await;

    assert!(order_received.is_ok() && order_received.unwrap().is_some());
    assert!(user_received.is_err() || user_received.unwrap().is_none());
}

#[tokio::test]
async fn test_unsubscribe() {
    let manager = SubscriptionManager::new();

    let rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    assert_eq!(manager.subscription_count(), 1);

    // Unsubscribe
    let removed = manager.unsubscribe("sub-1");
    assert!(removed);
    assert_eq!(manager.subscription_count(), 0);

    drop(rx);
}

#[tokio::test]
async fn test_sequential_events() {
    let manager = SubscriptionManager::new();
    let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    // Send multiple events
    for i in 0..5 {
        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"order_num": i}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        manager.broadcast_event(&event);
    }

    // Receiver should get all events
    let mut count = 0;
    loop {
        let received = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;

        match received {
            Ok(Some(_)) => count += 1,
            _ => break,
        }
    }

    assert_eq!(count, 5);
}

#[test]
fn test_subscription_manager_default() {
    let manager = SubscriptionManager::default();
    assert_eq!(manager.subscription_count(), 0);
}

#[test]
fn test_event_serialization() {
    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "UPDATE".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100.50, "status": "shipped"}),
        user_id:     Some("user123".to_string()),
        tenant_id:   Some("tenant1".to_string()),
        timestamp:   Utc::now(),
    };

    // Should serialize to JSON without errors
    let json = serde_json::to_string(&event).expect("should serialize");
    assert!(json.contains("\"event_type\":\"UPDATE\""));
    assert!(json.contains("\"entity_type\":\"Order\""));
    assert!(json.contains("\"total\":100.5"));

    // Should deserialize back
    let deserialized: HistoricalEvent = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(deserialized.event_type, "UPDATE");
    assert_eq!(deserialized.entity_type, "Order");
}
