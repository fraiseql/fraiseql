#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for the observer transport pipeline.
//!
//! Tests the full event routing pipeline using `InMemoryTransport`:
//! - Publish → subscribe round-trip
//! - `EventFilter` field construction
//! - `EventMatcher` routing to observer definitions
//! - `ConditionParser` evaluation on routed events
//!
//! **Execution engine:** none (in-memory only)
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::{collections::HashMap, sync::Arc};

use fraiseql_observers::{
    condition::ConditionParser,
    config::{ActionConfig, FailurePolicy, ObserverDefinition, RetryConfig},
    event::{EntityEvent, EventKind},
    matcher::EventMatcher,
    transport::{EventFilter, EventTransport, HealthStatus, InMemoryTransport, TransportType},
};
use futures::StreamExt;
use serde_json::json;
use uuid::Uuid;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_observer(event_type: &str, entity: &str, condition: Option<&str>) -> ObserverDefinition {
    ObserverDefinition {
        event_type: event_type.to_string(),
        entity:     entity.to_string(),
        condition:  condition.map(str::to_string),
        actions:    vec![ActionConfig::Webhook {
            url:           Some("https://example.com/hook".to_string()),
            url_env:       None,
            headers:       HashMap::default(),
            body_template: Some("{}".to_string()),
        }],
        retry:      RetryConfig::default(),
        on_failure: FailurePolicy::Log,
    }
}

fn order_created(total: u64) -> EntityEvent {
    EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({ "total": total }),
    )
}

fn order_updated(total: u64) -> EntityEvent {
    EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({ "total": total }),
    )
}

// ── Transport type and health ─────────────────────────────────────

/// `InMemoryTransport` reports `TransportType::InMemory`.
#[test]
fn transport_type_is_in_memory() {
    let t = InMemoryTransport::new();
    assert_eq!(t.transport_type(), TransportType::InMemory);
}

/// `InMemoryTransport` is always healthy (no external dependencies).
#[tokio::test]
async fn transport_health_is_healthy() {
    let t = InMemoryTransport::new();
    let health = t.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.message.is_some(), "healthy message should be present");
}

// ── EventFilter construction ──────────────────────────────────────

/// `EventFilter::default()` has all fields as `None`.
#[test]
fn event_filter_default_fields_are_none() {
    let f = EventFilter::default();
    assert!(f.entity_type.is_none());
    assert!(f.operation.is_none());
    assert!(f.tenant_id.is_none());
}

/// `EventFilter` fields can be set individually.
#[test]
fn event_filter_fields_set_correctly() {
    let f = EventFilter {
        entity_type: Some("Order".to_string()),
        operation:   Some("INSERT".to_string()),
        tenant_id:   Some("tenant-abc".to_string()),
    };
    assert_eq!(f.entity_type.as_deref(), Some("Order"));
    assert_eq!(f.operation.as_deref(), Some("INSERT"));
    assert_eq!(f.tenant_id.as_deref(), Some("tenant-abc"));
}

// ── Publish/subscribe pipeline ────────────────────────────────────

/// A single event published to `InMemoryTransport` is received by the subscriber.
#[tokio::test]
async fn single_event_round_trip() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    let event = order_created(200);
    let id = event.id;
    transport.publish(event).await.unwrap();

    let received = stream.next().await.unwrap().unwrap();
    assert_eq!(received.id, id);
    assert_eq!(received.entity_type, "Order");
    assert_eq!(received.data["total"], 200);
}

/// Multiple events arrive in publish order.
#[tokio::test]
async fn multiple_events_preserve_order() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    let totals = [10u64, 20, 30, 40, 50];
    for total in totals {
        transport.publish(order_created(total)).await.unwrap();
    }

    for expected in totals {
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received.data["total"], expected);
    }
}

/// Events of different `EventKind` variants are all received.
#[tokio::test]
async fn mixed_event_kinds_all_received() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    let events = vec![
        EntityEvent::new(EventKind::Created, "User".to_string(), Uuid::new_v4(), json!({})),
        EntityEvent::new(EventKind::Updated, "User".to_string(), Uuid::new_v4(), json!({})),
        EntityEvent::new(EventKind::Deleted, "User".to_string(), Uuid::new_v4(), json!({})),
    ];
    let ids: Vec<_> = events.iter().map(|e| e.id).collect();
    for e in events {
        transport.publish(e).await.unwrap();
    }

    for id in ids {
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received.id, id);
    }
}

// ── EventMatcher routing ──────────────────────────────────────────

/// An empty `EventMatcher` returns no matches for any event.
#[test]
fn empty_matcher_returns_no_matches() {
    let matcher = EventMatcher::new();
    let event = order_created(100);
    let matches = matcher.find_matches(&event);
    assert!(matches.is_empty());
}

/// An observer registered for "INSERT/Order" matches an `EventKind::Created` Order event.
#[test]
fn matcher_routes_insert_to_correct_observer() {
    let mut observers = HashMap::new();
    let obs = make_observer("INSERT", "Order", None);
    observers.insert("on_order_created".to_string(), obs);

    let matcher = EventMatcher::build(observers).unwrap();
    let event = order_created(50);
    let matches = matcher.find_matches(&event);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].entity, "Order");
    assert_eq!(matches[0].event_type, "INSERT");
}

/// An observer for "UPDATE/Order" does NOT match a Created (INSERT) event.
#[test]
fn matcher_does_not_route_wrong_event_type() {
    let mut observers = HashMap::new();
    observers.insert(
        "on_order_updated".to_string(),
        make_observer("UPDATE", "Order", None),
    );

    let matcher = EventMatcher::build(observers).unwrap();
    let event = order_created(50); // INSERT, not UPDATE
    let matches = matcher.find_matches(&event);
    assert!(matches.is_empty(), "INSERT event must not match UPDATE observer");
}

/// Multiple observers for different entities are routed independently.
#[test]
fn matcher_routes_multiple_entities_independently() {
    let mut observers = HashMap::new();
    observers.insert("obs1".to_string(), make_observer("INSERT", "Order", None));
    observers.insert("obs2".to_string(), make_observer("INSERT", "User", None));

    let matcher = EventMatcher::build(observers).unwrap();

    let order_event = order_created(10);
    let order_matches = matcher.find_matches(&order_event);
    assert_eq!(order_matches.len(), 1);
    assert_eq!(order_matches[0].entity, "Order");

    let user_event = EntityEvent::new(
        EventKind::Created,
        "User".to_string(),
        Uuid::new_v4(),
        json!({}),
    );
    let user_matches = matcher.find_matches(&user_event);
    assert_eq!(user_matches.len(), 1);
    assert_eq!(user_matches[0].entity, "User");
}

/// `observer_count()` reflects all registered observers.
#[test]
fn matcher_count_reflects_registered_observers() {
    let mut observers = HashMap::new();
    observers.insert("a".to_string(), make_observer("INSERT", "Order", None));
    observers.insert("b".to_string(), make_observer("UPDATE", "Order", None));
    observers.insert("c".to_string(), make_observer("DELETE", "User", None));

    let matcher = EventMatcher::build(observers).unwrap();
    assert_eq!(matcher.observer_count(), 3);
}

// ── ConditionParser evaluation ────────────────────────────────────

/// Condition `total > 100` passes for an event with `total: 200`.
#[test]
fn condition_greater_than_passes() {
    let parser = ConditionParser {};
    let event = order_created(200);
    let result = parser.parse_and_evaluate("total > 100", &event).unwrap();
    assert!(result, "total=200 must satisfy total > 100");
}

/// Condition `total > 100` fails for an event with `total: 50`.
#[test]
fn condition_greater_than_fails() {
    let parser = ConditionParser {};
    let event = order_created(50);
    let result = parser.parse_and_evaluate("total > 100", &event).unwrap();
    assert!(!result, "total=50 must not satisfy total > 100");
}

/// Condition with AND: `total > 10 && total < 1000` passes for `total: 200`.
#[test]
fn condition_logical_and_passes() {
    let parser = ConditionParser {};
    let event = order_created(200);
    let result = parser
        .parse_and_evaluate("total > 10 && total < 1000", &event)
        .unwrap();
    assert!(result, "total=200 satisfies both total > 10 and total < 1000");
}

/// Condition with OR: `total < 10 || total > 100` fails for `total: 50`.
#[test]
fn condition_logical_or_fails_when_neither_branch_matches() {
    let parser = ConditionParser {};
    let event = order_created(50);
    let result = parser
        .parse_and_evaluate("total < 10 || total > 100", &event)
        .unwrap();
    assert!(!result, "total=50 satisfies neither total < 10 nor total > 100");
}

// ── End-to-end transport + matcher + condition ────────────────────

/// Events published via transport that pass condition evaluation are matched by observer.
#[tokio::test]
async fn transport_matcher_condition_pipeline() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    // Set up matcher with a conditional observer
    let mut observers = HashMap::new();
    observers.insert(
        "high_value_orders".to_string(),
        make_observer("INSERT", "Order", Some("total > 500")),
    );
    let matcher = EventMatcher::build(observers).unwrap();
    let parser = ConditionParser {};

    // Publish two events: one that matches condition, one that doesn't
    transport.publish(order_created(100)).await.unwrap(); // below threshold
    transport.publish(order_created(1000)).await.unwrap(); // above threshold

    let low_event = stream.next().await.unwrap().unwrap();
    let high_event = stream.next().await.unwrap().unwrap();

    // Both events match the INSERT/Order observer...
    let low_matches = matcher.find_matches(&low_event);
    let high_matches = matcher.find_matches(&high_event);
    assert_eq!(low_matches.len(), 1);
    assert_eq!(high_matches.len(), 1);

    // ...but only the high-value event satisfies the condition
    let low_condition = low_matches[0].condition.as_deref().unwrap_or("true");
    let high_condition = high_matches[0].condition.as_deref().unwrap_or("true");

    assert!(!parser.parse_and_evaluate(low_condition, &low_event).unwrap());
    assert!(parser.parse_and_evaluate(high_condition, &high_event).unwrap());
}

/// Events of the wrong entity type are correctly rejected by the matcher.
#[tokio::test]
async fn transport_only_order_events_match_order_observer() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    let mut observers = HashMap::new();
    observers.insert("order_obs".to_string(), make_observer("INSERT", "Order", None));
    let matcher = EventMatcher::build(observers).unwrap();

    // Publish Order and User events
    transport.publish(order_created(10)).await.unwrap();
    transport
        .publish(EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({}),
        ))
        .await
        .unwrap();

    let order_event = stream.next().await.unwrap().unwrap();
    let user_event = stream.next().await.unwrap().unwrap();

    assert_eq!(matcher.find_matches(&order_event).len(), 1);
    assert!(matcher.find_matches(&user_event).is_empty());
}

/// An `UPDATE` event does not match an observer watching for `INSERT` events.
#[tokio::test]
async fn transport_update_event_does_not_match_insert_observer() {
    let transport = Arc::new(InMemoryTransport::new());
    let mut stream = transport.subscribe(EventFilter::default()).await.unwrap();

    let mut observers = HashMap::new();
    observers.insert("ins_obs".to_string(), make_observer("INSERT", "Order", None));
    let matcher = EventMatcher::build(observers).unwrap();

    transport.publish(order_updated(10)).await.unwrap();
    let received = stream.next().await.unwrap().unwrap();
    assert!(
        matcher.find_matches(&received).is_empty(),
        "UPDATE event must not match INSERT observer"
    );
}
