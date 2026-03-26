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

    // Give the server a moment to register the subscription.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Verify the subscription was registered.
    assert_eq!(manager.subscription_count(), 1, "subscription should be registered");

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

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(manager.subscription_count(), 1);

    // Complete (unsubscribe).
    send_json(&mut sink, json!({"type": "complete", "id": "op_1"})).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(manager.subscription_count(), 0, "subscription should be removed after complete");
}
