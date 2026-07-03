#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::{BTreeMap, HashMap};

use fraiseql_functions::{IngestSource, PushSource, RawDelivery, Source, Transport};
use sqlx::PgPool;

use super::{WebhookInboundState, WebhookSource, webhook_router};

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
}

fn timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-03T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

mod router_construction {
    //! Router-construction test — axum validates path-capture syntax inside
    //! `Router::route`, so a stale `:param` literal panics here at build time
    //! rather than at first server boot (issue #316 class).

    use super::{HashMap, WebhookInboundState, lazy_pool, webhook_router};

    #[tokio::test]
    async fn webhook_router_constructs() {
        let state = WebhookInboundState::new(lazy_pool(), &HashMap::new(), |_| None);
        let _ = webhook_router(state);
    }
}

#[test]
fn webhook_source_declares_push_transport() {
    let source = WebhookSource::new("stripe");
    assert_eq!(
        source.source(),
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        }
    );
    assert_eq!(source.transport(), Transport::Push);
}

#[test]
fn webhook_source_normalizes_delivery_and_carries_payload() {
    let source = WebhookSource::new("stripe");
    let payload = serde_json::json!({ "id": "evt_1", "type": "charge.succeeded" });
    let mut headers = BTreeMap::new();
    headers.insert("webhook-id".to_string(), "evt_1".to_string());
    let raw = RawDelivery {
        event_id:    "evt_1",
        event_type:  "charge.succeeded",
        payload:     &payload,
        headers:     &headers,
        received_at: timestamp(),
    };

    let message = source.normalize(&raw).unwrap();

    assert_eq!(
        message.source,
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        }
    );
    assert_eq!(message.idempotency_key, "evt_1");
    assert_eq!(message.subject.as_deref(), Some("charge.succeeded"));
    assert_eq!(message.payload.as_ref(), Some(&payload));
    assert_eq!(message.headers.get("webhook-id").map(String::as_str), Some("evt_1"));
    assert_eq!(message.trigger_type(), "after:ingest:webhook:stripe");
}

#[test]
fn webhook_source_rejects_delivery_without_event_id() {
    let source = WebhookSource::new("stripe");
    let payload = serde_json::json!({});
    let headers = BTreeMap::new();
    let raw = RawDelivery {
        event_id:    "",
        event_type:  "x",
        payload:     &payload,
        headers:     &headers,
        received_at: timestamp(),
    };
    assert!(source.normalize(&raw).is_err());
}
