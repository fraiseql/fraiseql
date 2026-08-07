#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
//! `WebSocket` E2E test for subscription delivery (C18).
//!
//! Exercises the full `WebSocket` subscription flow over a real TCP connection:
//!
//!   upgrade -> `connection_init` -> `connection_ack` -> subscribe
//!           -> event publication -> `next` frame delivery
//!
//! The test spins up a minimal axum server on an ephemeral port, connects via
//! `tokio-tungstenite`, and verifies the `graphql-transport-ws` protocol
//! state machine end-to-end.
//!
//! **Execution engine:** none (in-memory schema + subscription manager only)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral port)

use std::sync::Arc;

use fraiseql_core::{
    runtime::subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
    schema::{CompiledSchema, SubscriptionDefinition},
};
use fraiseql_server::routes::subscriptions::{SubscriptionState, subscription_handler};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};

/// Build a `CompiledSchema` that contains a single subscription definition.
fn schema_with_subscription(name: &str, return_type: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.subscriptions.push(SubscriptionDefinition::new(name, return_type));
    schema
}

/// Spawn an axum server with just the `/ws` subscription endpoint and return
/// its `ws://` URL.
async fn spawn_ws_server(state: SubscriptionState) -> String {
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("ws://{addr}/ws")
}

/// Helper: send a JSON text frame.
async fn send_json(
    ws: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Message,
    >,
    value: serde_json::Value,
) {
    let text = serde_json::to_string(&value).unwrap();
    ws.send(tungstenite::Message::Text(text.into())).await.unwrap();
}

/// Helper: receive the next text frame and parse as JSON, skipping keepalive
/// ping frames sent by the server.
async fn recv_json(
    ws: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for WebSocket message")
            .expect("stream ended unexpectedly")
            .expect("WebSocket error");

        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            // Skip server-initiated ping/pong keepalive frames at the
            // graphql-transport-ws level (these are JSON `{"type":"ping"}`
            // frames, distinct from WebSocket-level ping frames).
            if value.get("type").and_then(|t| t.as_str()) == Some("ping") {
                continue;
            }
            return value;
        }
        // Skip WebSocket-level ping/pong/binary frames
    }
}

/// Connect to the given `ws://` URL with the `graphql-transport-ws` sub-protocol.
async fn connect_ws(
    url: &str,
) -> (
    futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Message,
    >,
    futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let (ws_stream, _) = connect_async(url).await.expect("WebSocket connect failed");
    ws_stream.split()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full end-to-end: upgrade -> `connection_init` -> `connection_ack` -> subscribe
/// -> publish event -> verify `next` frame delivery.
#[tokio::test]
async fn ws_e2e_subscribe_and_receive_next_frame() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    // 1. connection_init -> connection_ack
    send_json(&mut sink, json!({"type": "connection_init"})).await;

    let ack = recv_json(&mut stream).await;
    assert_eq!(ack["type"], "connection_ack", "expected connection_ack, got {ack}");

    // 2. subscribe
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": {
                "query": "subscription { orderCreated { id status } }"
            }
        }),
    )
    .await;

    // Wait for the server to register the subscription (multi-hop TCP path).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    // 3. Publish an event through the manager.
    let event = SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "status": "pending"}),
    );
    let matched = manager.publish_event(event);
    assert_eq!(matched, 1, "event should match exactly one subscription");

    // 4. Receive the `next` frame.
    let next_frame = recv_json(&mut stream).await;
    assert_eq!(next_frame["type"], "next", "expected next frame, got {next_frame}");
    assert_eq!(next_frame["id"], "op_1");

    let payload = &next_frame["payload"];
    assert!(payload.get("data").is_some(), "next frame must contain data");
    let data = &payload["data"];
    // The handler wraps data under the subscription name key.
    assert_eq!(data["orderCreated"]["id"], "order_42");
    assert_eq!(data["orderCreated"]["status"], "pending");
}

/// #906: a spec-valid **aliased** subscription root field resolves the field
/// name and delivers under the alias.
///
/// An alias renames only the response key; the executed field is still
/// `orderCreated` (GraphQL spec § Response). Both halves are asserted here
/// because they fail independently: resolving the alias as the field name gets
/// `SubscriptionNotFound` and never delivers, while resolving the field name but
/// keying the payload by it delivers under a key the client did not ask for, so
/// a client reading `data.order` sees nothing arrive.
#[tokio::test]
async fn ws_e2e_aliased_root_field_delivers_under_the_alias() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": { "query": "subscription { order: orderCreated { id status } }" }
        }),
    )
    .await;

    // Half one: the field name is resolved, so the subscription is established
    // at all. Looking up the alias yields `SubscriptionNotFound` and the count
    // stays at zero.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "an aliased root field must resolve the FIELD name (`orderCreated`); resolving the \
             alias (`order`) finds no such subscription and delivery never starts"
        );
        tokio::task::yield_now().await;
    }

    let event = SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "status": "pending"}),
    );
    assert_eq!(manager.publish_event(event), 1, "event should match exactly one subscription");

    let next_frame = recv_json(&mut stream).await;
    assert_eq!(next_frame["type"], "next", "expected next frame, got {next_frame}");
    assert_eq!(next_frame["id"], "op_1");

    // Half two: the response is keyed by the ALIAS the client wrote.
    let data = &next_frame["payload"]["data"];
    assert_eq!(
        data["order"]["id"], "order_42",
        "the delivered payload must be keyed by the alias the client wrote, not by the \
         underlying field name — a client reading `data.order` sees nothing arrive: {next_frame}"
    );
    assert_eq!(data["order"]["status"], "pending", "{next_frame}");
    assert!(
        data.get("orderCreated").is_none(),
        "the field name must not appear as a response key when an alias was given: {next_frame}"
    );
}

/// #425 acceptance: a delivered `next` frame carries the Change-Spine envelope in
/// the graphql-transport-ws `extensions.changeSpine` slot, with the resolved
/// `data` untouched. Proves the envelope round-trips event → payload → client.
#[tokio::test]
async fn ws_e2e_next_frame_carries_change_spine_envelope() {
    use fraiseql_core::runtime::subscription::ChangeSpineEnvelope;

    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    // Publish an event stamped with the full Change-Spine envelope.
    let event = SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "status": "pending"}),
    )
    .with_change_spine(ChangeSpineEnvelope {
        actor_type: Some("ai_agent".to_string()),
        acting_for: Some("11111111-1111-1111-1111-111111111111".to_string()),
        schema_version: Some("v3".to_string()),
        duration_ms: Some(12),
        seq: Some(42),
        ..Default::default()
    });
    assert_eq!(manager.publish_event(event), 1, "event should match exactly one subscription");

    let next_frame = recv_json(&mut stream).await;
    assert_eq!(next_frame["type"], "next", "expected next frame, got {next_frame}");
    let payload = &next_frame["payload"];

    // Resolved data is unchanged (no regression).
    assert_eq!(payload["data"]["orderCreated"]["id"], "order_42");
    assert_eq!(payload["data"]["orderCreated"]["status"], "pending");

    // Envelope rides in extensions.changeSpine, camelCase, unset fields omitted.
    let cs = &payload["extensions"]["changeSpine"];
    assert_eq!(cs["actorType"], "ai_agent");
    assert_eq!(cs["actingFor"], "11111111-1111-1111-1111-111111111111");
    assert_eq!(cs["schemaVersion"], "v3");
    assert_eq!(cs["durationMs"], 12);
    assert_eq!(cs["seq"], 42);
    assert!(cs.get("tenantId").is_none(), "unset envelope fields are omitted");
}

/// Verify the `connection_init` -> `connection_ack` handshake in isolation.
#[tokio::test]
async fn ws_e2e_connection_init_ack_handshake() {
    let schema = Arc::new(CompiledSchema::new());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager);

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    // Send connection_init with optional payload.
    send_json(&mut sink, json!({"type": "connection_init", "payload": {"token": "test-jwt"}}))
        .await;

    let ack = recv_json(&mut stream).await;
    assert_eq!(ack["type"], "connection_ack");
}

/// Verify that subscribing to a non-existent subscription returns an error frame
/// (not a crash).
#[tokio::test]
async fn ws_e2e_subscribe_unknown_returns_error() {
    let schema = Arc::new(CompiledSchema::new()); // empty schema, no subscriptions
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager);

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    // Handshake.
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    let ack = recv_json(&mut stream).await;
    assert_eq!(ack["type"], "connection_ack");

    // Subscribe to something that does not exist.
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_bad",
            "payload": {
                "query": "subscription { nonExistent { id } }"
            }
        }),
    )
    .await;

    let error_frame = recv_json(&mut stream).await;
    assert_eq!(error_frame["type"], "error", "expected error frame, got {error_frame}");
    assert_eq!(error_frame["id"], "op_bad");
}

/// Verify that sending `complete` cleanly removes the subscription.
#[tokio::test]
async fn ws_e2e_complete_unsubscribes() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    // Handshake.
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    let ack = recv_json(&mut stream).await;
    assert_eq!(ack["type"], "connection_ack");

    // Subscribe.
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": {
                "query": "subscription { orderCreated { id } }"
            }
        }),
    )
    .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    // Complete (unsubscribe).
    send_json(&mut sink, json!({"type": "complete", "id": "op_1"})).await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "subscription should be removed after complete"
        );
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// #422 operation-level authorization at subscribe-time
// ---------------------------------------------------------------------------

use fraiseql_core::{
    error::Result as FqlResult,
    security::{Authorizer, AuthzDecision, AuthzRequest},
};

struct DenyAll;
impl Authorizer for DenyAll {
    fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
        Ok(AuthzDecision::Deny {
            reason: "nope".into(),
        })
    }
}

struct AllowAll;
impl Authorizer for AllowAll {
    fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
        Ok(AuthzDecision::Allow)
    }
}

/// A configured authorizer that denies → the subscribe is rejected with an error
/// frame and the subscription is NOT registered.
#[tokio::test]
async fn ws_e2e_authorizer_deny_rejects_subscription() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone()).with_authorizer(Some(Arc::new(DenyAll)));

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_deny",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    let error_frame = recv_json(&mut stream).await;
    assert_eq!(
        error_frame["type"], "error",
        "deny must yield an error frame, got {error_frame}"
    );
    assert_eq!(error_frame["id"], "op_deny");

    // The subscription must NOT be registered.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(manager.subscription_count(), 0, "denied subscription must not register");
}

/// A configured authorizer that allows → the subscription registers normally.
#[tokio::test]
async fn ws_e2e_authorizer_allow_permits_subscription() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone()).with_authorizer(Some(Arc::new(AllowAll)));

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_allow",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "allowed subscription should register");
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// #786: graphql-transport-ws conformance around connection_init
// ---------------------------------------------------------------------------

/// Helper: wait for a `WebSocket` Close frame and return its close code.
async fn recv_close_code(
    ws: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> u16 {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for Close frame")
            // The server may drop the TCP stream right after (or instead of)
            // the Close frame; tungstenite surfaces that as an error/None.
            .expect("stream ended without a Close frame")
            .expect("stream errored without a Close frame");
        match msg {
            tungstenite::Message::Close(Some(frame)) => return frame.code.into(),
            tungstenite::Message::Close(None) => return 1005,
            _ => {},
        }
    }
}

fn plain_state() -> SubscriptionState {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    SubscriptionState::new(manager)
}

/// Before `connection_ack`, an undecodable message must close 4400 — not be
/// silently swallowed while the init timeout keeps running.
#[tokio::test]
async fn pre_ack_invalid_json_closes_4400() {
    let url = spawn_ws_server(plain_state()).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    sink.send(tungstenite::Message::Text("this is not json".into())).await.unwrap();

    assert_eq!(recv_close_code(&mut stream).await, 4400, "invalid JSON before init");
}

/// Before `connection_ack`, any valid message other than `connection_init`
/// must close 4401 (spec: Unauthorized) — not be silently discarded.
#[tokio::test]
async fn pre_ack_non_init_message_closes_4401() {
    let url = spawn_ws_server(plain_state()).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "1",
            "payload": { "query": "subscription { orderCreated { id } }" }
        }),
    )
    .await;

    assert_eq!(recv_close_code(&mut stream).await, 4401, "subscribe before init");
}

/// Legacy `connection_terminate` performs a graceful close — the connection
/// and its subscriptions must not stay alive.
#[tokio::test]
async fn connection_terminate_closes_gracefully() {
    let url = spawn_ws_server(plain_state()).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(&mut sink, json!({"type": "connection_terminate"})).await;

    assert_eq!(recv_close_code(&mut stream).await, 1000, "connection_terminate → normal close");
}

/// A malformed subscribe payload closes 4400 (Bad Request), not 1002.
#[tokio::test]
async fn malformed_subscribe_closes_4400() {
    let url = spawn_ws_server(plain_state()).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    // `subscribe` with no payload at all.
    send_json(&mut sink, json!({"type": "subscribe", "id": "1"})).await;

    assert_eq!(recv_close_code(&mut stream).await, 4400, "malformed subscribe payload");
}
