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

// ── #1045: the route must not hand-roll its error body past the sanitizer ──

mod error_body_sanitization {
    //! `impl IntoResponse for FraiseQLError` (`fraiseql-error/src/http.rs`) exists to
    //! collapse internal error text — it renders `Authentication` as a flat
    //! `"Authentication failed"` and `Database` as `"A database error occurred"`,
    //! under a comment reading *"database/config/storage/internal/observer details
    //! must not leak to clients"*.
    //!
    //! The webhook route bypassed it: `json_status` hand-rolled
    //! `json!({"error": mapped.to_string()})`, so an **unauthenticated** caller was
    //! handed the `Display` chain instead — the verifier's internal reason on a 401,
    //! and, via `WebhookError::Database` → `"inbound spine: claim: {error}"`, raw
    //! PostgreSQL error text on a 5xx.

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt as _;

    use super::{HashMap, WebhookInboundState, lazy_pool, webhook_router};
    use crate::config::WebhookRouteConfig;

    /// A router with one generic HMAC route whose signing secret resolves.
    ///
    /// `hmac-sha256` needs no timestamp and no `public_url`, and a forged signature
    /// is refused by `verify_signature` **before** any database work, so the lazy
    /// pool is never connected.
    fn router_with_one_route() -> Router {
        let mut routes = HashMap::new();
        routes.insert(
            "hooks".to_string(),
            WebhookRouteConfig {
                secret_env: "TEST_WEBHOOK_SECRET".to_string(),
                provider:   "hmac-sha256".to_string(),
                path:       None,
                public_url: None,
            },
        );
        let state =
            WebhookInboundState::new(lazy_pool(), &routes, |_| Some("s3cret-value".to_string()));
        webhook_router(state)
    }

    #[tokio::test]
    async fn a_forged_signature_is_refused_without_echoing_the_internal_reason() {
        let request = Request::builder()
            .method("POST")
            .uri("/webhooks/hooks")
            .header("X-Signature", "deadbeef")
            .body(Body::from(r#"{"id":"evt_1"}"#))
            .unwrap();

        let response = router_with_one_route().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "a forged signature is the sender's fault, so the status stays 401"
        );

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();

        // Positive first: an unsanitized body would satisfy every `!contains` below
        // too, so absence-only assertions cannot pin this on their own.
        assert_eq!(
            body["error_description"], "Authentication failed",
            "the sanitizer's flat description is what an unauthenticated caller may see; \
             got {text}"
        );
        assert_eq!(
            body["error"], "authentication_error",
            "the route must render through FraiseQLError's IntoResponse, not its own shape; \
             got {text}"
        );
        assert!(
            !text.contains("signature mismatch"),
            "the verifier's internal reason must not reach the sender; got {text}"
        );
    }
}

// ── #1045: who is at fault decides the status, and `Crypto` cannot decide it ──

mod key_material_is_not_the_senders_fault {
    //! `SignatureError::KeyMaterial` was raised for **two** unrelated things: server-side
    //! key material that failed to parse, and sender-supplied signature bytes that
    //! failed to parse. Both collapsed into `SignatureInvalid` → 401.
    //!
    //! A 401 for a server-side misconfiguration blames the sender for the operator's
    //! mistake, and providers that disable an endpoint after sustained auth failures
    //! drop every event in the window. But the naive repair — routing `Crypto` to 5xx
    //! at the pipeline — is worse: `discord.rs:97/100` and `sendgrid.rs:102/105` parse
    //! **sender** bytes, so it would hand any anonymous caller an on-demand 5xx.
    //!
    //! Hence the split at the producer, and hence the second test here: it is the
    //! guard that the fix did not overshoot.

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt as _;

    use super::{HashMap, WebhookInboundState, lazy_pool, webhook_router};
    use crate::config::WebhookRouteConfig;

    /// RFC 8032 test vector 1's public key — a genuinely valid Ed25519 point, so a
    /// route carrying it reaches the *signature* parse rather than failing on the key.
    const VALID_ED25519_PUBLIC_KEY: &str =
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    fn discord_router(configured_key: &'static str) -> Router {
        let mut routes = HashMap::new();
        routes.insert(
            "discord".to_string(),
            WebhookRouteConfig {
                secret_env: "TEST_DISCORD_KEY".to_string(),
                provider:   "discord".to_string(),
                path:       None,
                public_url: None,
            },
        );
        let state =
            WebhookInboundState::new(lazy_pool(), &routes, |_| Some(configured_key.to_string()));
        webhook_router(state)
    }

    /// Discord checks freshness before it parses either key or signature, so both
    /// tests need a timestamp inside the tolerance window to reach the parse at all.
    fn request(signature: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/webhooks/discord")
            .header("X-Signature-Ed25519", signature)
            .header("X-Signature-Timestamp", chrono::Utc::now().timestamp().to_string())
            .body(Body::from(r#"{"id":"evt_1","type":1}"#))
            .unwrap()
    }

    #[tokio::test]
    async fn a_misconfigured_signing_key_is_the_servers_fault_not_a_401() {
        // The operator pasted something that is not hex into the route's key env.
        let response = discord_router("not-hex!").oneshot(request("aabb")).await.unwrap();

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "unparseable configured key material is a server-side misconfiguration; \
             answering 401 blames the sender and invites the provider to disable the endpoint"
        );

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(
            !text.contains("hex") && !text.contains("public key"),
            "the key-parsing detail belongs in the operator's log, not the response; got {text}"
        );
    }

    #[tokio::test]
    async fn a_malformed_sender_signature_is_still_a_401() {
        // Guard against overshooting: this Crypto comes from the *sender's* header, so
        // routing the whole variant to 5xx would let any anonymous caller mint one.
        let response =
            discord_router(VALID_ED25519_PUBLIC_KEY).oneshot(request("zz")).await.unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "an unparseable signature header is the sender's fault and must stay 401, \
             or an anonymous caller can produce a 5xx on demand"
        );
    }
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
