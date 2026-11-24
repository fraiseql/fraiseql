//! The text a subscription event's time is delivered and stored as.
//!
//! The expected strings are chrono's for the same instants: what a webhook or Kafka
//! consumer read before the event's time became a `fraiseql_time::Timestamp`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::Timestamp;

use super::{DeliveredEvent, SubscriptionEvent, SubscriptionOperation};

/// 2025-11-20T12:00:00Z plus `nanos` nanoseconds.
fn at(nanos: i32) -> Timestamp {
    Timestamp::new(1_763_640_000, nanos).unwrap()
}

fn event_at(nanos: i32) -> SubscriptionEvent {
    let mut event =
        SubscriptionEvent::new("Order", "1", SubscriptionOperation::Create, serde_json::json!({}));
    event.timestamp = at(nanos);
    event
}

#[test]
fn a_delivered_event_carries_its_time_as_rfc3339_with_a_utc_offset() {
    for (nanos, text) in [
        (0, "2025-11-20T12:00:00+00:00"),
        (120_000_000, "2025-11-20T12:00:00.120+00:00"),
        (123_456_000, "2025-11-20T12:00:00.123456+00:00"),
        (123_456_789, "2025-11-20T12:00:00.123456789+00:00"),
    ] {
        let event = event_at(nanos);
        assert_eq!(DeliveredEvent::from_event(&event, "orders").timestamp, text);
    }
}

#[test]
fn an_event_serializes_its_time_with_z() {
    let value = serde_json::to_value(event_at(120_000_000)).unwrap();
    assert_eq!(value["timestamp"], "2025-11-20T12:00:00.120Z");
}
