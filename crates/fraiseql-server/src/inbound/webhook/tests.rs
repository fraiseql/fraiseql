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

mod after_ingest_bridge {
    //! #594: the after:ingest `fraiseql_query` bridge factory is threaded onto the
    //! webhook inbound state so an after:ingest function can write back under its
    //! `run_as` ceiling. The `run_as`→bridge mechanism itself is proven in
    //! `routes::after_mutation::tests::query_bridge_wiring` (same `spawn_dispatch`
    //! path); this proves the state carries + would pass the factory rather than the
    //! pre-#594 `None`.
    use std::{future::Future, pin::Pin, sync::Arc};

    use fraiseql_functions::host::live::QueryExecutor;
    use serde_json::Value;

    use super::{HashMap, WebhookInboundState, lazy_pool};
    use crate::routes::after_mutation::QueryExecutorFactory;

    struct MockExec;
    impl QueryExecutor for MockExec {
        fn execute_query(
            &self,
            _query: &str,
            _variables: Option<&Value>,
        ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Value>> + Send + '_>> {
            Box::pin(async { Ok(Value::Null) })
        }
    }

    fn factory() -> QueryExecutorFactory {
        Arc::new(|_identity| Arc::new(MockExec) as Arc<dyn QueryExecutor>)
    }

    #[tokio::test] // `connect_lazy` needs a Tokio context (it spawns the pool's keeper).
    async fn without_a_factory_the_bridge_is_unwired() {
        let state = WebhookInboundState::new(lazy_pool(), &HashMap::new(), |_| None);
        assert!(state.query_executor_factory().is_none());
    }

    #[tokio::test]
    async fn with_a_factory_the_state_carries_the_after_ingest_bridge() {
        let state = WebhookInboundState::new(lazy_pool(), &HashMap::new(), |_| None)
            .with_query_executor_factory(factory());
        assert!(
            state.query_executor_factory().is_some(),
            "#594: the webhook state must carry the query bridge so after:ingest can write back"
        );
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

// ── #751: dedup keys must derive from signed material only ──────────────

mod signed_dedup_key {
    //! Every supported provider signs the body only — no verifier covers request
    //! headers. Keying the replay defence on `webhook-id` / `x-github-delivery`
    //! therefore let anyone replaying one captured signed delivery mint a fresh
    //! idempotency key per attempt, re-firing `after:ingest` each time.

    use std::collections::BTreeMap;

    use super::super::{extract_event_id, extract_event_type};

    fn headers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    // Header-independence is a *type-level* property here: `extract_event_id` no
    // longer takes headers at all, so no assertion in this module can vary them.
    // The behavioural proof that a replay under a fresh `x-github-delivery` now
    // deduplicates end-to-end lives in the live-PG regression test
    // `tests/webhook_replay_header_dedup_pg.rs`, which drives the real handler.

    #[test]
    fn an_identical_body_always_yields_an_identical_key() {
        // Without a payload `id`, the key must still be a pure function of the signed
        // body — that is what collapses a replay onto one idempotency claim.
        let body = br#"{"action":"opened","number":1}"#;
        let payload = serde_json::from_slice(body).unwrap();

        assert_eq!(extract_event_id(&payload, body), extract_event_id(&payload, body));
    }

    #[test]
    fn signed_payload_id_keys_the_delivery_when_present() {
        let body = br#"{"id":"evt_1","type":"charge.succeeded"}"#;
        let payload = serde_json::from_slice(body).unwrap();

        assert_eq!(
            extract_event_id(&payload, body),
            "evt_1",
            "a signed top-level id is the natural key and must be used verbatim"
        );
    }

    #[test]
    fn distinct_bodies_get_distinct_keys() {
        let a = br#"{"action":"opened","number":1}"#;
        let b = br#"{"action":"opened","number":2}"#;
        let pa = serde_json::from_slice(a).unwrap();
        let pb = serde_json::from_slice(b).unwrap();

        assert_ne!(
            extract_event_id(&pa, a),
            extract_event_id(&pb, b),
            "genuinely different deliveries must not collapse into one key"
        );
    }

    #[test]
    fn body_hash_key_is_sha256_not_the_unstable_default_hasher() {
        // The key is persisted, so it has to survive a toolchain bump.
        // SHA-256("{}") = 44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a
        let body = b"{}";
        let payload = serde_json::from_slice(body).unwrap();

        assert_eq!(
            extract_event_id(&payload, body),
            "body:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    #[test]
    fn signed_payload_type_beats_an_injected_header() {
        let payload = serde_json::json!({ "type": "charge.succeeded" });
        let h = headers(&[("x-github-event", "push")]);

        assert_eq!(
            extract_event_type(&payload, &h),
            "charge.succeeded",
            "an unsigned header must not relabel a delivery whose signed body states its type"
        );
    }

    #[test]
    fn github_event_header_still_used_when_the_body_carries_no_type() {
        // GitHub bodies have no `type`; the header is the only source there is.
        let payload = serde_json::json!({ "action": "opened" });
        let h = headers(&[("x-github-event", "pull_request")]);

        assert_eq!(extract_event_type(&payload, &h), "pull_request");
    }
}
