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
    schema::{
        CompiledSchema, FieldDefinition, FieldType, SecurityConfig, SubscriptionDefinition,
        TypeDefinition,
    },
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
/// A state whose subscriptions are planned (ruling AA 4) by an executor over `schema` — the
/// schema the manager serves. The executor reads nothing here: plans are applied to the
/// published after-images.
fn planned_state(manager: Arc<SubscriptionManager>, schema: CompiledSchema) -> SubscriptionState {
    let executor = Arc::new(fraiseql_core::runtime::Executor::new(
        schema,
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    ));
    SubscriptionState::new(manager)
        .with_live_executor(Some(Arc::new(move || Arc::clone(&executor))))
}

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
    // Ruling AA 5: the durable `seq` is a server-wide position and stays server-side.
    assert!(cs.get("seq").is_none(), "the Change-Spine seq never reaches a subscriber: {cs}");
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

/// The subscription reaches the authorizer with the type it delivers (`target_type`),
/// read from the schema serving now — so a rule written for reading `Order` holds for
/// `orderCreated` as for `orders`.
#[tokio::test]
async fn ws_e2e_authorizer_sees_the_subscriptions_type() {
    struct DenyOrders;
    impl Authorizer for DenyOrders {
        fn authorize(&self, req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            Ok(if req.target_type == Some("Order") {
                AuthzDecision::Deny {
                    reason: "no orders".into(),
                }
            } else {
                AuthzDecision::Allow
            })
        }
    }
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema.clone()));
    let live: fraiseql_server::routes::subscriptions::LiveSchema = Arc::new(move || schema.clone());
    let state = SubscriptionState::new(manager.clone())
        .with_authorizer(Some(Arc::new(DenyOrders)))
        .with_live_schema(Some(live));

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_typed",
            "payload": { "query": "subscription { orderCreated { id } }" }
        }),
    )
    .await;

    let frame = recv_json(&mut stream).await;
    assert_eq!(frame["type"], "error", "a rule on Order must deny orderCreated: {frame}");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(manager.subscription_count(), 0);
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

/// A subscription is a read of its type, delivered by push: the read gates a query of the
/// same type meets apply to it. Here an anonymous subscriber selects `{ id }` of an `Order`
/// whose `secret` requires a scope (with a `security` section, so the query path enforces
/// it). A query would neither select nor serve `secret`; the `next` frame must not carry it.
#[tokio::test]
async fn ws_e2e_a_subscription_does_not_deliver_a_scoped_field_to_an_anonymous_subscriber() {
    let mut schema = schema_with_subscription("orderCreated", "Order");
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("secret", FieldType::String).with_requires_scope("read:secret"),
    ];
    schema.types.push(order);
    schema.security = Some(SecurityConfig::default());
    schema.build_indexes();
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema.clone())));
    let url = spawn_ws_server(planned_state(manager.clone(), schema)).await;
    let (mut sink, mut stream) = connect_ws(&url).await;

    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": { "query": "subscription { orderCreated { id } }" }
        }),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    let event = SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "secret": "s3cr3t"}),
    );
    assert_eq!(manager.publish_event(event), 1, "event should match exactly one subscription");

    let next_frame = recv_json(&mut stream).await;
    assert!(
        !next_frame.to_string().contains("s3cr3t"),
        "a field the subscriber neither selected nor may read was delivered: {next_frame}"
    );
}

// ── AA 4: a subscription is planned at subscribe time from the client's selection ──

/// `Order { id, status, secret }`, `secret` requiring `read:secret` (Reject, the default),
/// with a `security` section granting nobody the scope; `orderCreated` may filter by
/// `secret` (a declared argument), so only the read gate can refuse that filter.
fn gated_order_manager() -> (Arc<SubscriptionManager>, CompiledSchema) {
    let mut schema = schema_with_subscription("orderCreated", "Order");
    let sub = schema.subscriptions.iter_mut().find(|s| s.name == "orderCreated").unwrap();
    sub.arguments
        .push(fraiseql_core::schema::ArgumentDefinition::optional("secret", FieldType::String));
    sub.filter_fields = vec!["secret".to_string()];
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("status", FieldType::String),
        FieldDefinition::nullable("secret", FieldType::String).with_requires_scope("read:secret"),
    ];
    schema.types.push(order);
    schema.security = Some(SecurityConfig::default());
    schema.build_indexes();
    (Arc::new(SubscriptionManager::new(Arc::new(schema.clone()))), schema)
}

/// Subscribe `query` (with `variables`) as anonymous, publish one `Order` after-image, and
/// return the first frame for the operation after the subscribe.
async fn first_frame_after_publish(
    (manager, schema): (Arc<SubscriptionManager>, CompiledSchema),
    query: &str,
    variables: serde_json::Value,
) -> serde_json::Value {
    let url = spawn_ws_server(planned_state(manager.clone(), schema)).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack", "handshake");
    send_json(
        &mut sink,
        json!({"type": "subscribe", "id": "op_1",
               "payload": {"query": query, "variables": variables}}),
    )
    .await;
    // Either the subscription registers, or the server answers the subscribe at once.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
    while manager.subscription_count() == 0 && tokio::time::Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    manager.publish_event(SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "status": "open", "secret": "s3cr3t"}),
    ));
    recv_json(&mut stream).await
}

/// A subscription serves its selection, as a query does: `status` is readable, but not
/// selected.
#[tokio::test]
async fn ws_e2e_a_subscription_serves_only_its_selection() {
    let frame = first_frame_after_publish(
        gated_order_manager(),
        "subscription { orderCreated { id } }",
        json!({}),
    )
    .await;
    assert_eq!(frame["type"], "next", "{frame}");
    assert_eq!(frame["payload"]["data"]["orderCreated"], json!({"id": "order_42"}), "{frame}");
}

/// Selecting a `Reject` field the subscriber may not read refuses the subscription, as it
/// refuses a query.
#[tokio::test]
async fn ws_e2e_selecting_a_rejected_field_refuses_the_subscription() {
    let frame = first_frame_after_publish(
        gated_order_manager(),
        "subscription { orderCreated { id secret } }",
        json!({}),
    )
    .await;
    assert_eq!(frame["type"], "error", "the subscription must be refused: {frame}");
    assert!(!frame.to_string().contains("s3cr3t"), "{frame}");
}

/// Filtering by a field the subscriber may not read is refused (ruling AA 3): which events
/// arrive would answer a question about its value.
#[tokio::test]
async fn ws_e2e_filtering_by_a_field_the_subscriber_may_not_read_is_refused() {
    let frame = first_frame_after_publish(
        gated_order_manager(),
        "subscription($secret: String) { orderCreated(secret: $secret) { id } }",
        json!({"secret": "s3cr3t"}),
    )
    .await;
    assert_eq!(frame["type"], "error", "the filter must be refused: {frame}");
}

/// A policy reload re-plans every live subscription (ruling AC 7): the executor serving
/// after the reload decides what the next event is served as. Here `note` gains a scope no
/// role grants; an event after the reload must carry it masked.
#[tokio::test]
async fn ws_e2e_a_policy_reload_replans_a_live_subscription() {
    let order = |gated: bool| {
        let mut schema = schema_with_subscription("orderCreated", "Order");
        let mut note = FieldDefinition::nullable("note", FieldType::String);
        if gated {
            note = note
                .with_requires_scope("read:note")
                .with_on_deny(fraiseql_core::schema::FieldDenyPolicy::Mask);
            schema.security = Some(SecurityConfig::default());
        }
        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields = vec![FieldDefinition::new("id", FieldType::Id), note];
        schema.types.push(order);
        schema.build_indexes();
        schema
    };
    let executor_for = |schema: CompiledSchema| {
        Arc::new(fraiseql_core::runtime::Executor::new(
            schema,
            Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
        ))
    };
    let serving = Arc::new(std::sync::Mutex::new(executor_for(order(false))));
    let live = Arc::clone(&serving);
    let manager = Arc::new(SubscriptionManager::new(Arc::new(order(false))));
    let (reload_tx, reload_rx) = tokio::sync::watch::channel(0_u64);
    let state = SubscriptionState::new(manager.clone())
        .with_live_executor(Some(Arc::new(move || Arc::clone(&live.lock().unwrap()))))
        .with_policy_reload(Some(reload_rx));
    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack", "handshake");
    send_json(
        &mut sink,
        json!({"type": "subscribe", "id": "op_1",
               "payload": {"query": "subscription { orderCreated { id note } }"}}),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    // The reload: the serving executor now masks `note`.
    *serving.lock().unwrap() = executor_for(order(true));
    reload_tx.send(1).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    manager.publish_event(SubscriptionEvent::new(
        "Order",
        "o1",
        SubscriptionOperation::Create,
        json!({"id": "o1", "note": "n"}),
    ));
    let frame = recv_json(&mut stream).await;
    assert_eq!(
        frame["payload"]["data"]["orderCreated"],
        json!({"id": "o1", "note": null}),
        "the plan in force is the one the reload derived: {frame}"
    );
}

/// A reload whose plan refuses the subscription ends it, fail-closed, with an error frame:
/// here `note` becomes a `Reject` field the subscriber may not read.
#[tokio::test]
async fn ws_e2e_a_reload_that_refuses_the_plan_ends_the_subscription() {
    let order = |gated: bool| {
        let mut schema = schema_with_subscription("orderCreated", "Order");
        let mut note = FieldDefinition::nullable("note", FieldType::String);
        if gated {
            note = note
                .with_requires_scope("read:note")
                .with_on_deny(fraiseql_core::schema::FieldDenyPolicy::Reject);
            schema.security = Some(SecurityConfig::default());
        }
        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields = vec![FieldDefinition::new("id", FieldType::Id), note];
        schema.types.push(order);
        schema.build_indexes();
        schema
    };
    let executor_for = |schema: CompiledSchema| {
        Arc::new(fraiseql_core::runtime::Executor::new(
            schema,
            Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
        ))
    };
    let serving = Arc::new(std::sync::Mutex::new(executor_for(order(false))));
    let live = Arc::clone(&serving);
    let manager = Arc::new(SubscriptionManager::new(Arc::new(order(false))));
    let (reload_tx, reload_rx) = tokio::sync::watch::channel(0_u64);
    let state = SubscriptionState::new(manager.clone())
        .with_live_executor(Some(Arc::new(move || Arc::clone(&live.lock().unwrap()))))
        .with_policy_reload(Some(reload_rx));
    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack", "handshake");
    send_json(
        &mut sink,
        json!({"type": "subscribe", "id": "op_1",
               "payload": {"query": "subscription { orderCreated { id note } }"}}),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }

    // The reload: `note` is now a Reject field the subscriber may not read.
    *serving.lock().unwrap() = executor_for(order(true));
    reload_tx.send(1).unwrap();
    let frame = recv_json(&mut stream).await;
    assert_eq!(frame["type"], "error", "the reload's refusal ends the operation: {frame}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(tokio::time::Instant::now() < deadline, "the refused subscription is removed");
        tokio::task::yield_now().await;
    }
}

// ── Ruling AE 1: every /ws subscribe answers to this server's gates ──
//
// `SubscriptionState::with_remote_subscription_fields` let an embedder map a subscription
// name to a remote subgraph. /ws then forwarded the subscribe before the tenant check, the
// suspended-tenant check and the #596 row policy, without a plan, and without the
// subscriber's principal; a forward that failed answered nothing at all. The subgraph URL
// below passes the SSRF guard and never resolves (RFC 6761 `.invalid`), so each test sees
// only what this server answered.

/// A subgraph URL the SSRF guard accepts and no resolver answers.
const NEVER_RESOLVES: &str = "https://subgraph.invalid/graphql";

/// Route `name` to [`NEVER_RESOLVES`], as an embedder federating that field would.
fn routed_away(state: SubscriptionState, name: &str) -> SubscriptionState {
    state.with_remote_subscription_fields(std::collections::HashMap::from([(
        name.to_string(),
        NEVER_RESOLVES.to_string(),
    )]))
}

/// Connect (with `headers`), hand-shake, send one subscribe, and return this server's
/// answer to it: the first frame within two seconds, or `None`.
async fn answer_to_subscribe(
    state: SubscriptionState,
    headers: &[(&'static str, &'static str)],
    query: &str,
    variables: serde_json::Value,
) -> Option<serde_json::Value> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let url = spawn_ws_server(state).await;
    let mut request = url.into_client_request().unwrap();
    for (name, value) in headers {
        request
            .headers_mut()
            .insert(*name, tungstenite::http::HeaderValue::from_static(value));
    }
    let (ws, _) = connect_async(request).await.expect("WebSocket connect failed");
    let (mut sink, mut stream) = ws.split();
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack", "handshake");
    send_json(
        &mut sink,
        json!({"type": "subscribe", "id": "op_1",
               "payload": {"query": query, "variables": variables}}),
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(2), recv_json(&mut stream))
        .await
        .ok()
}

/// A name this server does not define is refused, wherever an embedder routed it.
#[tokio::test]
#[ignore = "AE 1 reproduction: /ws forwards a routed name and answers nothing"]
async fn ws_ae1_a_subscription_this_server_does_not_define_is_refused() {
    let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::new())));
    let state = routed_away(SubscriptionState::new(manager), "postCreated");
    let answer =
        answer_to_subscribe(state, &[], "subscription { postCreated { id body } }", json!({}))
            .await;
    let frame = answer.expect("the subscribe must be answered, not forwarded");
    assert_eq!(frame["type"], "error", "{frame}");
}

/// A subscription is planned as a read of its type, wherever an embedder routed its name:
/// selecting a `Reject` field the subscriber may not read is refused.
#[tokio::test]
#[ignore = "AE 1 reproduction: a routed name is never planned"]
async fn ws_ae1_a_routed_subscription_is_still_planned() {
    let (manager, schema) = gated_order_manager();
    let state = routed_away(planned_state(manager, schema), "orderCreated");
    let answer =
        answer_to_subscribe(state, &[], "subscription { orderCreated { id secret } }", json!({}))
            .await;
    let frame = answer.expect("the plan must refuse the subscribe");
    assert_eq!(frame["type"], "error", "{frame}");
    let code = &frame["payload"][0]["extensions"]["code"];
    assert!(
        code == "FORBIDDEN" || code == "SUBSCRIPTION_REFUSED",
        "the plan's refusal: {frame}"
    );
}

/// A client-supplied tenant that contradicts the server-resolved one is refused, wherever
/// an embedder routed the name.
#[tokio::test]
#[ignore = "AE 1 reproduction: a routed name skips the tenant check"]
async fn ws_ae1_a_routed_subscription_still_answers_the_tenant_check() {
    let schema = Arc::new(schema_with_subscription("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = routed_away(SubscriptionState::new(manager), "orderCreated");
    let answer = answer_to_subscribe(
        state,
        &[("x-tenant-id", "tenant_a")],
        "subscription { orderCreated { id } }",
        json!({"tenant_id": "tenant_b"}),
    )
    .await;
    let frame = answer.expect("the tenant check must refuse the subscribe");
    assert_eq!(frame["type"], "error", "{frame}");
    assert_eq!(frame["payload"][0]["extensions"]["code"], "TENANT_MISMATCH", "{frame}");
}

/// A subscription whose type declares a row policy (#596) is refused to a subscriber whose
/// identity the policy cannot resolve, wherever an embedder routed the name.
#[tokio::test]
#[ignore = "AE 1 reproduction: a routed name skips the #596 row policy"]
async fn ws_ae1_a_routed_subscription_still_answers_the_row_policy() {
    let mut schema = CompiledSchema::new();
    schema
        .types
        .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
            fraiseql_core::schema::SubscriptionPolicy {
                owner_path:     "$.owner_id".to_string(),
                identity_field: "user_id".to_string(),
                bypass_roles:   vec![],
            },
        ));
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    let policies =
        Arc::new(fraiseql_server::routes::subscriptions::build_subscription_policies(&schema));
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let state = routed_away(
        SubscriptionState::new(manager).with_subscription_policies(policies),
        "orderCreated",
    );
    let answer =
        answer_to_subscribe(state, &[], "subscription { orderCreated { id } }", json!({})).await;
    let frame = answer.expect("the row policy must refuse the subscribe");
    assert_eq!(frame["type"], "error", "{frame}");
    assert_eq!(frame["payload"][0]["extensions"]["code"], "SUBSCRIPTION_REFUSED", "{frame}");
}
