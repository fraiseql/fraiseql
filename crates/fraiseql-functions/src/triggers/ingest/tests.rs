#![allow(clippy::panic)] // Reason: test code, panics acceptable

use std::collections::BTreeMap;

use super::{
    Attachment, InboundMessage, IngestSource, IngestTrigger, PushSource, RawDelivery, RoutingRule,
    Source, StorageRef, Transport, parse_recipient, resolve_routing,
};

/// A fake push source that normalizes a Stripe-style delivery into an
/// `InboundMessage` — the minimal adapter used to drive the primitive.
struct FakeStripeSource;

impl Source for FakeStripeSource {
    fn source(&self) -> IngestSource {
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        }
    }

    fn transport(&self) -> Transport {
        Transport::Push
    }
}

impl PushSource for FakeStripeSource {
    fn normalize(&self, delivery: &RawDelivery<'_>) -> Result<InboundMessage, super::IngestError> {
        if delivery.event_id.is_empty() {
            return Err(super::IngestError::new("missing event id"));
        }
        let mut message =
            InboundMessage::new(self.source(), delivery.event_id, delivery.received_at);
        message.subject = Some(delivery.event_type.to_string());
        message.body_text =
            delivery.payload.get("summary").and_then(|v| v.as_str()).map(String::from);
        Ok(message)
    }
}

fn timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-03T12:00:00Z")
        .expect("valid timestamp")
        .with_timezone(&chrono::Utc)
}

#[test]
fn push_source_normalizes_delivery_into_inbound_message() {
    let source = FakeStripeSource;
    let payload = serde_json::json!({ "summary": "charge succeeded" });
    let headers = BTreeMap::new();
    let delivery = RawDelivery {
        event_id:    "evt_123",
        event_type:  "payment_intent.succeeded",
        payload:     &payload,
        headers:     &headers,
        received_at: timestamp(),
    };

    let message = source.normalize(&delivery).expect("normalizes");

    assert_eq!(
        message.source,
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        }
    );
    assert_eq!(message.idempotency_key, "evt_123");
    assert_eq!(message.subject.as_deref(), Some("payment_intent.succeeded"));
    assert_eq!(message.body_text.as_deref(), Some("charge succeeded"));
    assert_eq!(message.received_at, timestamp());
}

#[test]
fn normalize_fails_loud_on_missing_event_id() {
    let source = FakeStripeSource;
    let payload = serde_json::json!({});
    let headers = BTreeMap::new();
    let delivery = RawDelivery {
        event_id:    "",
        event_type:  "x",
        payload:     &payload,
        headers:     &headers,
        received_at: timestamp(),
    };

    let error = source.normalize(&delivery).expect_err("empty event id is rejected");
    assert!(error.message.contains("event id"));
}

#[test]
fn source_key_is_the_after_ingest_discriminant() {
    assert_eq!(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        }
        .as_key(),
        "webhook:stripe"
    );
    assert_eq!(IngestSource::Email.as_key(), "email");
}

#[test]
fn ingest_trigger_matches_by_source() {
    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "evt_1",
        timestamp(),
    );

    // A source-specific trigger fires only for its source.
    let stripe = IngestTrigger {
        function_name: "onStripe".to_string(),
        source:        Some(IngestSource::Webhook {
            provider: "stripe".to_string(),
        }),
    };
    assert!(stripe.matches(&message));

    let email = IngestTrigger {
        function_name: "onEmail".to_string(),
        source:        Some(IngestSource::Email),
    };
    assert!(!email.matches(&message));

    // A source-agnostic trigger fires for every source.
    let any = IngestTrigger {
        function_name: "onAny".to_string(),
        source:        None,
    };
    assert!(any.matches(&message));
}

#[test]
fn build_payload_carries_the_normalized_message() {
    let mut message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "evt_1",
        timestamp(),
    );
    message.subject = Some("hello".to_string());
    message.attachments.push(Attachment {
        storage:      StorageRef {
            bucket: "inbound".to_string(),
            key:    "evt_1/att_0".to_string(),
        },
        content_type: "application/pdf".to_string(),
        filename:     "invoice.pdf".to_string(),
    });

    let trigger = IngestTrigger {
        function_name: "onStripe".to_string(),
        source:        None,
    };
    let payload = trigger.build_payload(&message);

    assert_eq!(payload.trigger_type, "after:ingest:webhook:stripe");
    assert_eq!(payload.entity, "webhook:stripe");
    assert_eq!(payload.event_kind, "ingest");
    assert_eq!(payload.timestamp, timestamp());
    // The event data round-trips back to the same message.
    let roundtrip: InboundMessage =
        serde_json::from_value(payload.data).expect("payload data is the message");
    assert_eq!(roundtrip, message);
}

#[test]
fn parse_recipient_splits_base_and_plus_tag() {
    let tagged = parse_recipient("support+ticket-42@example.com").expect("valid address");
    assert_eq!(tagged.base, "support@example.com");
    assert_eq!(tagged.tag.as_deref(), Some("ticket-42"));

    let plain = parse_recipient("support@example.com").expect("valid address");
    assert_eq!(plain.base, "support@example.com");
    assert_eq!(plain.tag, None);

    // Not an address.
    assert_eq!(parse_recipient("not-an-address"), None);
    assert_eq!(parse_recipient("@example.com"), None);
}

#[test]
fn plus_tagged_address_routes_to_the_right_entity() {
    let mut message = InboundMessage::new(IngestSource::Email, "msg-1", timestamp());
    message.to.push("support+ticket-42@example.com".to_string());

    let rules = [RoutingRule {
        address:     "support@example.com".to_string(),
        entity_type: "Ticket".to_string(),
    }];

    let routing = resolve_routing(&message, &rules);
    assert_eq!(routing.entity_type.as_deref(), Some("Ticket"));
    assert_eq!(routing.entity_id.as_deref(), Some("ticket-42"));
}

#[test]
fn dedicated_address_routes_without_a_tag() {
    let mut message = InboundMessage::new(IngestSource::Email, "msg-2", timestamp());
    // Case-insensitive match, and the rule fires even with no plus-tag.
    message.to.push("Invoices@Example.com".to_string());

    let rules = [RoutingRule {
        address:     "invoices@example.com".to_string(),
        entity_type: "Invoice".to_string(),
    }];

    let routing = resolve_routing(&message, &rules);
    assert_eq!(routing.entity_type.as_deref(), Some("Invoice"));
    assert_eq!(routing.entity_id, None);
}

#[test]
fn no_matching_address_yields_empty_routing() {
    let mut message = InboundMessage::new(IngestSource::Email, "msg-3", timestamp());
    message.to.push("random@elsewhere.com".to_string());

    let rules = [RoutingRule {
        address:     "support@example.com".to_string(),
        entity_type: "Ticket".to_string(),
    }];

    assert_eq!(resolve_routing(&message, &rules), super::InboundRouting::default());
}
