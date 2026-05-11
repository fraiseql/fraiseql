use chrono::Utc;
use uuid::Uuid;

use super::*;
use crate::HistoricalEvent;

#[test]
fn test_create_subscription() {
    let manager = SubscriptionManager::new();

    let rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    assert_eq!(manager.subscription_count(), 1);
    drop(rx);
}

#[test]
fn test_unsubscribe() {
    let manager = SubscriptionManager::new();

    let rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    assert_eq!(manager.subscription_count(), 1);

    manager.unsubscribe("sub-1");
    assert_eq!(manager.subscription_count(), 0);

    drop(rx);
}

#[test]
fn test_unsubscribe_nonexistent() {
    let manager = SubscriptionManager::new();

    let result = manager.unsubscribe("nonexistent");
    assert!(!result);
    assert_eq!(manager.subscription_count(), 0);
}

#[test]
fn test_broadcast_event_filters_by_entity_type() {
    let manager = SubscriptionManager::new();

    let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    let mut rx2 = manager.subscribe("sub-2".to_string(), "User".to_string(), None);

    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100}),
        user_id:     None,
        tenant_id:   None,
        timestamp:   Utc::now(),
    };

    manager.broadcast_event(&event);

    // rx1 should receive the event
    let received = rx1.try_recv().ok();
    assert!(received.is_some());

    // rx2 should not receive it
    let received = rx2.try_recv().ok();
    assert!(received.is_none());
}

#[test]
fn test_multiple_subscriptions_same_type() {
    let manager = SubscriptionManager::new();

    let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
    let mut rx2 = manager.subscribe("sub-2".to_string(), "Order".to_string(), None);

    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100}),
        user_id:     None,
        tenant_id:   None,
        timestamp:   Utc::now(),
    };

    manager.broadcast_event(&event);

    // Both should receive the event
    assert!(rx1.try_recv().ok().is_some());
    assert!(rx2.try_recv().ok().is_some());
}

#[test]
fn test_matches_filter_no_filter() {
    let event = make_event(serde_json::json!({"status": "shipped"}));
    assert!(SubscriptionManager::matches_filter(&event, &None));
}

#[test]
fn test_matches_filter_equality() {
    let event = make_event(serde_json::json!({"status": "shipped", "region": "us-east"}));
    assert!(SubscriptionManager::matches_filter(
        &event,
        &Some("status = 'shipped'".to_string())
    ));
    assert!(!SubscriptionManager::matches_filter(
        &event,
        &Some("status = 'pending'".to_string())
    ));
}

#[test]
fn test_matches_filter_inequality() {
    let event = make_event(serde_json::json!({"status": "shipped"}));
    assert!(SubscriptionManager::matches_filter(
        &event,
        &Some("status != 'pending'".to_string())
    ));
    assert!(!SubscriptionManager::matches_filter(
        &event,
        &Some("status != 'shipped'".to_string())
    ));
}

#[test]
fn test_matches_filter_missing_field() {
    let event = make_event(serde_json::json!({"status": "shipped"}));
    assert!(!SubscriptionManager::matches_filter(
        &event,
        &Some("missing_field = 'value'".to_string())
    ));
}

#[test]
fn test_matches_filter_unparseable() {
    let event = make_event(serde_json::json!({"status": "shipped"}));
    assert!(!SubscriptionManager::matches_filter(
        &event,
        &Some("garbage filter".to_string())
    ));
}

#[test]
fn test_matches_filter_empty_string() {
    let event = make_event(serde_json::json!({"status": "shipped"}));
    assert!(SubscriptionManager::matches_filter(&event, &Some(String::new())));
}

#[test]
fn test_simulate_event_broadcasts() {
    let manager = SubscriptionManager::new();
    let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    let event = make_event(serde_json::json!({"total": 100}));
    manager.simulate_event(event);

    assert!(rx.try_recv().ok().is_some());
}

#[test]
fn test_broadcast_with_filter() {
    let manager = SubscriptionManager::new();
    let mut rx_match = manager.subscribe(
        "sub-1".to_string(),
        "Order".to_string(),
        Some("status = 'shipped'".to_string()),
    );
    let mut rx_no_match = manager.subscribe(
        "sub-2".to_string(),
        "Order".to_string(),
        Some("status = 'pending'".to_string()),
    );

    let event = make_event(serde_json::json!({"status": "shipped"}));
    manager.broadcast_event(&event);

    assert!(rx_match.try_recv().ok().is_some());
    assert!(rx_no_match.try_recv().ok().is_none());
}

fn make_event(data: serde_json::Value) -> HistoricalEvent {
    HistoricalEvent {
        id: Uuid::new_v4(),
        event_type: "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id: Uuid::new_v4(),
        data,
        user_id: None,
        tenant_id: None,
        timestamp: Utc::now(),
    }
}

// ─── bounded-channel tests (S34) ─────────────────────────────────────────

#[test]
fn test_subscription_uses_bounded_channel() {
    // with_subscription_buffer(2): only 2 events can queue; a 3rd is dropped
    let manager = SubscriptionManager::new().with_subscription_buffer(2);
    let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    let e1 = make_event(serde_json::json!({"n": 1}));
    let e2 = make_event(serde_json::json!({"n": 2}));
    let e3 = make_event(serde_json::json!({"n": 3}));

    // First two fit in the buffer; third overflows silently (logged as warn)
    manager.broadcast_event(&e1);
    manager.broadcast_event(&e2);
    manager.broadcast_event(&e3); // dropped — buffer full

    // Drain: expect exactly 2 events (the first two)
    assert!(rx.try_recv().is_ok(), "first event should be in buffer");
    assert!(rx.try_recv().is_ok(), "second event should be in buffer");
    assert!(
        rx.try_recv().is_err(),
        "third event must be dropped when buffer is full (bounded channel)"
    );
}

#[test]
fn test_subscription_buffer_default_allows_256_events() {
    let manager = SubscriptionManager::new();
    assert_eq!(manager.per_subscription_buffer, super::DEFAULT_SUBSCRIPTION_BUFFER);
    assert_eq!(manager.per_subscription_buffer, 256);
}

#[test]
fn test_with_subscription_buffer_overrides_default() {
    let manager = SubscriptionManager::new().with_subscription_buffer(64);
    assert_eq!(manager.per_subscription_buffer, 64);
}

#[test]
fn test_subscription_channel_is_not_unbounded() {
    // Verify that we cannot queue more than the configured capacity.
    // capacity=1: only one event fits; the second is dropped.
    let manager = SubscriptionManager::new().with_subscription_buffer(1);
    let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

    manager.broadcast_event(&make_event(serde_json::json!({"n": 1})));
    manager.broadcast_event(&make_event(serde_json::json!({"n": 2}))); // must be dropped

    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_err(), "channel must be bounded to capacity=1");
}
