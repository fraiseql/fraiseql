#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::doc_markdown)] // Reason: test comments reference protocol names without backticks
#![allow(missing_docs)] // Reason: test code does not require documentation
//! Integration tests for the federation subscription forwarder.
//!
//! Exercises `SubscriptionForwarder::forward()` against a mock subgraph
//! server that implements the `graphql-transport-ws` protocol.
//!
//! **Execution engine:** none (direct forwarder API)
//! **Infrastructure:** none (in-process mock WebSocket server)
//! **Parallelism:** safe (ephemeral ports)

use fraiseql_federation::subscription_forwarder::{
    ForwardError, ForwardedEvent, SubscriptionForwarder,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Set the env var that allows http:// URLs for local dev/test.
fn allow_insecure() {
    std::env::set_var("FRAISEQL_FEDERATION_ALLOW_INSECURE", "true");
}

// ── Mock Subgraph WebSocket Server ──────────────────────────────────────────

/// Configuration for a mock subgraph server.
#[derive(Clone)]
struct MockSubgraphConfig {
    /// Events to emit after receiving a subscribe message.
    events:        Vec<serde_json::Value>,
    /// Whether to send connection_ack on connection_init.
    send_ack:      bool,
    /// Delay before sending connection_ack (millis).
    ack_delay_ms:  u64,
    /// Whether to send `complete` after all events.
    send_complete: bool,
}

impl Default for MockSubgraphConfig {
    fn default() -> Self {
        Self {
            events:        vec![],
            send_ack:      true,
            ack_delay_ms:  0,
            send_complete: true,
        }
    }
}

/// Spawn a mock subgraph WebSocket server on an ephemeral port.
/// Returns the `ws://` URL and a shutdown sender.
async fn spawn_mock_subgraph(
    config: MockSubgraphConfig,
) -> (String, tokio::sync::oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                accept = listener.accept() => {
                    let (stream, _) = accept.unwrap();
                    let config = config.clone();
                    tokio::spawn(handle_mock_connection(stream, config));
                }
                _ = &mut shutdown_rx => break,
            }
        }
    });

    // The forwarder expects an http:// URL and converts it to ws://
    let url = format!("http://{addr}/graphql");
    (url, shutdown_tx)
}

async fn handle_mock_connection(stream: tokio::net::TcpStream, config: MockSubgraphConfig) {
    let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Wait for connection_init
    while let Some(Ok(msg)) = read.next().await {
        if let WsMessage::Text(text) = msg {
            let val: serde_json::Value = serde_json::from_str(&text).unwrap();
            if val.get("type").and_then(|t| t.as_str()) == Some("connection_init") {
                break;
            }
        }
    }

    if !config.send_ack {
        // Don't send ack — let the forwarder timeout
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        return;
    }

    if config.ack_delay_ms > 0 {
        tokio::time::sleep(tokio::time::Duration::from_millis(config.ack_delay_ms)).await;
    }

    // Send connection_ack
    let ack = json!({"type": "connection_ack"});
    write
        .send(WsMessage::Text(serde_json::to_string(&ack).unwrap().into()))
        .await
        .unwrap();

    // Wait for subscribe message
    let mut operation_id = String::new();
    while let Some(Ok(msg)) = read.next().await {
        if let WsMessage::Text(text) = msg {
            let val: serde_json::Value = serde_json::from_str(&text).unwrap();
            if val.get("type").and_then(|t| t.as_str()) == Some("subscribe") {
                operation_id =
                    val.get("id").and_then(|id| id.as_str()).unwrap_or("unknown").to_string();
                break;
            }
        }
    }

    // Emit configured events
    for event in &config.events {
        let msg_json = serde_json::to_string(event).unwrap();
        if write.send(WsMessage::Text(msg_json.into())).await.is_err() {
            return;
        }
        // Small delay between events for realism
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    // Send complete if configured
    if config.send_complete {
        let complete = json!({"type": "complete", "id": operation_id});
        let _ = write
            .send(WsMessage::Text(serde_json::to_string(&complete).unwrap().into()))
            .await;
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_forwarder_receives_next_events() {
    allow_insecure();
    let config = MockSubgraphConfig {
        events: vec![
            json!({"type": "next", "id": "op_1", "payload": {"data": {"postCreated": {"id": "1", "body": "hello"}}}}),
            json!({"type": "next", "id": "op_1", "payload": {"data": {"postCreated": {"id": "2", "body": "world"}}}}),
        ],
        ..Default::default()
    };

    let (url, _shutdown) = spawn_mock_subgraph(config).await;
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, mut rx) = mpsc::channel(16);
    forwarder
        .forward("op_1", "subscription { postCreated { id body } }", json!({}), tx)
        .await
        .unwrap();

    // Should receive 2 next events + 1 complete
    let event1 = rx.recv().await.unwrap();
    assert!(matches!(&event1, ForwardedEvent::Next(v) if v["data"]["postCreated"]["id"] == "1"));

    let event2 = rx.recv().await.unwrap();
    assert!(matches!(&event2, ForwardedEvent::Next(v) if v["data"]["postCreated"]["id"] == "2"));

    let event3 = rx.recv().await.unwrap();
    assert!(matches!(event3, ForwardedEvent::Complete));
}

#[tokio::test]
async fn test_forwarder_receives_error_event() {
    allow_insecure();
    let config = MockSubgraphConfig {
        events: vec![json!({
            "type": "error",
            "id": "op_1",
            "payload": [{"message": "subgraph error: field not found"}]
        })],
        send_complete: false,
        ..Default::default()
    };

    let (url, _shutdown) = spawn_mock_subgraph(config).await;
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, mut rx) = mpsc::channel(16);
    forwarder
        .forward("op_1", "subscription { bad { id } }", json!({}), tx)
        .await
        .unwrap();

    let event = rx.recv().await.unwrap();
    assert!(
        matches!(&event, ForwardedEvent::Error(v) if v[0]["message"].as_str().unwrap().contains("subgraph error"))
    );
}

#[tokio::test]
async fn test_forwarder_handles_complete_without_events() {
    allow_insecure();
    let config = MockSubgraphConfig {
        events: vec![],
        send_complete: true,
        ..Default::default()
    };

    let (url, _shutdown) = spawn_mock_subgraph(config).await;
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, mut rx) = mpsc::channel(16);
    forwarder
        .forward("op_1", "subscription { noop { id } }", json!({}), tx)
        .await
        .unwrap();

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, ForwardedEvent::Complete));
}

#[tokio::test]
async fn test_forwarder_init_timeout_when_no_ack() {
    allow_insecure();
    let config = MockSubgraphConfig {
        send_ack: false,
        ..Default::default()
    };

    let (url, _shutdown) = spawn_mock_subgraph(config).await;
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, _rx) = mpsc::channel(16);
    let result = forwarder.forward("op_1", "subscription { x { id } }", json!({}), tx).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(&err, ForwardError::InitFailed(msg) if msg.contains("timeout")),
        "expected InitFailed with timeout, got: {err}"
    );
}

#[tokio::test]
async fn test_forwarder_connection_refused() {
    allow_insecure();
    // Bind a port then immediately drop the listener — connection will be refused
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let url = format!("http://{addr}/graphql");
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, _rx) = mpsc::channel(16);
    let result = forwarder.forward("op_1", "subscription { x { id } }", json!({}), tx).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ForwardError::ConnectionFailed(_)));
}

#[tokio::test]
async fn test_forwarder_passes_variables() {
    allow_insecure();
    // The mock doesn't validate variables, but we verify the forwarder doesn't
    // reject them and completes successfully.
    let config = MockSubgraphConfig {
        events: vec![json!({
            "type": "next",
            "id": "op_vars",
            "payload": {"data": {"userUpdated": {"name": "Alice"}}}
        })],
        ..Default::default()
    };

    let (url, _shutdown) = spawn_mock_subgraph(config).await;
    let forwarder = SubscriptionForwarder::new(&url).unwrap();

    let (tx, mut rx) = mpsc::channel(16);
    forwarder
        .forward(
            "op_vars",
            "subscription ($userId: ID!) { userUpdated(userId: $userId) { name } }",
            json!({"userId": "user_42"}),
            tx,
        )
        .await
        .unwrap();

    let event = rx.recv().await.unwrap();
    assert!(
        matches!(&event, ForwardedEvent::Next(v) if v["data"]["userUpdated"]["name"] == "Alice")
    );
}
