//! Tests for the realtime `WebSocket` module (Phase 7, Cycles 1–5).

use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite};

use super::context_hash::security_context_hash;
use super::delivery::{EntityEvent, EventDeliveryPipeline, EventKindSerde, RlsEvaluator};
use super::observer::RealtimeBroadcastObserver;
use super::server::{
    RealtimeConfig, RealtimeServer, RealtimeState, TokenInfo, TokenValidator, ws_handler,
};

// ── Test token validator ────────────────────────────────────────────────

/// Test validator that accepts tokens starting with "valid-" and rejects others.
#[derive(Clone)]
struct TestValidator {
    /// Token expiration offset from now (in seconds). Negative = already expired.
    expires_in: i64,
}

impl TestValidator {
    const fn new() -> Self {
        Self { expires_in: 3600 }
    }

    const fn with_expires_in(expires_in: i64) -> Self {
        Self { expires_in }
    }
}

impl TokenValidator for TestValidator {
    async fn validate(&self, token: &str) -> Result<TokenInfo, String> {
        if token.starts_with("valid-") {
            let user_id = token.strip_prefix("valid-").unwrap_or("unknown").to_owned();
            Ok(TokenInfo {
                user_id: user_id.clone(),
                context_hash: {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    user_id.hash(&mut hasher);
                    hasher.finish()
                },
                expires_at: chrono::Utc::now().timestamp() + self.expires_in,
            })
        } else if token == "expired-token" {
            Err("token expired".to_owned())
        } else {
            Err("invalid token".to_owned())
        }
    }
}

// ── Test helpers ────────────────────────────────────────────────────────

/// Spawn a test server with the given config and validator, return the address.
async fn spawn_test_server(config: RealtimeConfig, validator: TestValidator) -> SocketAddr {
    let server = Arc::new(RealtimeServer::new(config));
    let state = RealtimeState {
        server,
        validator: Arc::new(validator),
    };

    let app = Router::new()
        .route("/realtime/v1", get(ws_handler::<TestValidator>))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    addr
}

/// Build a `WebSocket` URL for the given address with optional token.
fn ws_url(addr: SocketAddr, token: Option<&str>) -> String {
    match token {
        Some(t) => format!("ws://{addr}/realtime/v1?token={t}"),
        None => format!("ws://{addr}/realtime/v1"),
    }
}

/// Spawn a test server with known entities.
async fn spawn_test_server_with_entities(
    config: RealtimeConfig,
    validator: TestValidator,
    entities: HashSet<String>,
) -> SocketAddr {
    let server = Arc::new(RealtimeServer::with_entities(config, entities));
    let state = RealtimeState {
        server,
        validator: Arc::new(validator),
    };

    let app = Router::new()
        .route("/realtime/v1", get(ws_handler::<TestValidator>))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    addr
}

/// Parse a server message from a tungstenite message.
fn parse_server_msg(msg: &tungstenite::Message) -> serde_json::Value {
    match msg {
        tungstenite::Message::Text(text) => serde_json::from_str(text).unwrap(),
        other => panic!("Expected text message, got {other:?}"),
    }
}

/// Send a JSON message over the WebSocket.
async fn send_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    msg: serde_json::Value,
) {
    ws.send(tungstenite::Message::Text(msg.to_string().into()))
        .await
        .unwrap();
}

/// Read the next text message, with a timeout.
async fn next_msg(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timed out waiting for message")
        .expect("stream ended")
        .expect("WebSocket error");
    parse_server_msg(&msg)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_websocket_connect_with_valid_token() {
    let addr = spawn_test_server(RealtimeConfig::default(), TestValidator::new()).await;

    let (mut ws, _response) = connect_async(ws_url(addr, Some("valid-alice"))).await.unwrap();

    // First message should be "connected" with a connection_id
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");
    assert!(parsed["connection_id"].is_string());
    assert!(!parsed["connection_id"].as_str().unwrap().is_empty());

    ws.close(None).await.unwrap();
}

#[tokio::test]
async fn test_websocket_connect_without_token_returns_401() {
    let addr = spawn_test_server(RealtimeConfig::default(), TestValidator::new()).await;

    let result = connect_async(ws_url(addr, None)).await;
    assert!(result.is_err(), "Expected connection to be rejected");

    if let Err(tungstenite::Error::Http(response)) = result {
        assert_eq!(response.status(), 401);
    } else {
        panic!("Expected HTTP error with 401 status");
    }
}

#[tokio::test]
async fn test_websocket_connect_with_expired_token_returns_401() {
    let addr = spawn_test_server(RealtimeConfig::default(), TestValidator::new()).await;

    let result = connect_async(ws_url(addr, Some("expired-token"))).await;
    assert!(result.is_err(), "Expected connection to be rejected");

    if let Err(tungstenite::Error::Http(response)) = result {
        assert_eq!(response.status(), 401);
    } else {
        panic!("Expected HTTP error with 401 status");
    }
}

#[tokio::test]
async fn test_websocket_heartbeat_pong() {
    let config = RealtimeConfig {
        heartbeat_interval: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(10),
        ..RealtimeConfig::default()
    };
    let addr = spawn_test_server(config, TestValidator::new()).await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-bob"))).await.unwrap();

    // Read "connected" message
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");

    // Wait for a heartbeat ping from server
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "ping");

    // Respond with pong
    let pong = serde_json::json!({"type": "pong"}).to_string();
    ws.send(tungstenite::Message::Text(pong.into())).await.unwrap();

    // Should receive another ping (connection stays alive)
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "ping");

    ws.close(None).await.unwrap();
}

#[tokio::test]
async fn test_websocket_idle_timeout_disconnects() {
    let config = RealtimeConfig {
        heartbeat_interval: Duration::from_secs(60),
        idle_timeout: Duration::from_millis(200),
        ..RealtimeConfig::default()
    };
    let addr = spawn_test_server(config, TestValidator::new()).await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-carol"))).await.unwrap();

    // Read "connected" message
    let _connected = ws.next().await.unwrap().unwrap();

    // Wait for idle timeout — server should close the connection
    let msg = ws.next().await;
    match msg {
        Some(Ok(tungstenite::Message::Close(Some(frame)))) => {
            assert_eq!(
                frame.code,
                tungstenite::protocol::frame::coding::CloseCode::Normal
            );
        }
        // Connection closed without close frame — also acceptable
        None => {}
        other => {
            panic!("Expected close frame or connection drop, got {other:?}");
        }
    }
}

#[tokio::test]
async fn test_websocket_graceful_close() {
    let addr = spawn_test_server(RealtimeConfig::default(), TestValidator::new()).await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-dave"))).await.unwrap();

    // Read "connected" message
    let _connected = ws.next().await.unwrap().unwrap();

    // Client sends close
    ws.close(None).await.unwrap();

    // Should get close acknowledgement or connection drops cleanly.
    // tokio-tungstenite may return Close frame, None, or
    // `ResetWithoutClosingHandshake` if the server tears down quickly.
    let msg = ws.next().await;
    let is_clean_close = matches!(
        msg,
        None | Some(
            Ok(tungstenite::Message::Close(_))
                | Err(
                    tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake
                    ) | tungstenite::Error::ConnectionClosed
                )
        )
    );
    assert!(is_clean_close, "Expected close or None, got {msg:?}");
}

#[tokio::test]
async fn test_websocket_connection_limit_per_context() {
    let config = RealtimeConfig {
        max_connections_per_context: 2,
        ..RealtimeConfig::default()
    };
    let addr = spawn_test_server(config, TestValidator::new()).await;

    // Open 2 connections with same user (same context hash)
    let (mut ws1, _) = connect_async(ws_url(addr, Some("valid-eve"))).await.unwrap();
    let _connected1 = ws1.next().await.unwrap().unwrap();

    let (mut ws2, _) = connect_async(ws_url(addr, Some("valid-eve"))).await.unwrap();
    let _connected2 = ws2.next().await.unwrap().unwrap();

    // Third connection should be rejected (429 Too Many Requests)
    let result = connect_async(ws_url(addr, Some("valid-eve"))).await;
    assert!(result.is_err(), "Expected third connection to be rejected");

    if let Err(tungstenite::Error::Http(response)) = result {
        assert_eq!(response.status(), 429);
    } else {
        panic!("Expected HTTP 429 error");
    }

    // Different user should still work
    let (mut ws3, _) = connect_async(ws_url(addr, Some("valid-frank"))).await.unwrap();
    let msg = ws3.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");

    ws1.close(None).await.ok();
    ws2.close(None).await.ok();
    ws3.close(None).await.ok();
}

#[tokio::test]
async fn test_websocket_token_expiry_disconnects() {
    let config = RealtimeConfig {
        heartbeat_interval: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(10),
        ..RealtimeConfig::default()
    };
    let validator = TestValidator::with_expires_in(1);
    let addr = spawn_test_server(config, validator).await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-grace"))).await.unwrap();

    // Read "connected" message
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");

    // Wait for token to expire + heartbeat check.
    // With 100ms heartbeat and 1s token life, should get a few pings then token_expired.
    let start = std::time::Instant::now();
    let mut got_token_expired = false;
    while start.elapsed() < Duration::from_secs(3) {
        match ws.next().await {
            Some(Ok(tungstenite::Message::Text(text))) => {
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
                if parsed["type"] == "token_expired" {
                    got_token_expired = true;
                    break;
                }
                assert_eq!(parsed["type"], "ping");
            }
            Some(Ok(tungstenite::Message::Close(Some(frame)))) => {
                assert_eq!(
                    frame.code,
                    tungstenite::protocol::frame::coding::CloseCode::from(4401)
                );
                got_token_expired = true;
                break;
            }
            Some(Ok(tungstenite::Message::Close(None))) | None => break,
            other => panic!("Unexpected message: {other:?}"),
        }
    }
    assert!(
        got_token_expired,
        "Expected token_expired message before close"
    );
}

/// Validator that lets the token through initially but marks it as expiring
/// this second (first heartbeat detects expiry).
#[derive(Clone)]
struct NearExpiryValidator;
impl TokenValidator for NearExpiryValidator {
    async fn validate(&self, token: &str) -> Result<TokenInfo, String> {
        if token.starts_with("valid-") {
            let user_id = token.strip_prefix("valid-").unwrap_or("unknown").to_owned();
            Ok(TokenInfo {
                user_id,
                context_hash: 42,
                expires_at: chrono::Utc::now().timestamp(),
            })
        } else {
            Err("invalid".to_owned())
        }
    }
}

#[tokio::test]
async fn test_websocket_token_revalidation_interval() {
    let config = RealtimeConfig {
        heartbeat_interval: Duration::from_millis(50),
        idle_timeout: Duration::from_secs(10),
        token_revalidation_interval: Duration::from_millis(50),
        ..RealtimeConfig::default()
    };

    let server = Arc::new(RealtimeServer::new(config));
    let state = RealtimeState {
        server,
        validator: Arc::new(NearExpiryValidator),
    };

    let app = Router::new()
        .route("/realtime/v1", get(ws_handler::<NearExpiryValidator>))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-henry"))).await.unwrap();

    // Read connected
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");

    // Within a few heartbeats, should get token_expired or close with 4401
    let mut got_expired = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
            Ok(Some(Ok(tungstenite::Message::Text(text)))) => {
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
                if parsed["type"] == "token_expired" {
                    got_expired = true;
                    break;
                }
            }
            Ok(Some(Ok(tungstenite::Message::Close(Some(frame))))) => {
                if frame.code
                    == tungstenite::protocol::frame::coding::CloseCode::from(4401)
                {
                    got_expired = true;
                }
                break;
            }
            Ok(None | Some(Ok(tungstenite::Message::Close(None)))) => break,
            _ => {}
        }
    }
    assert!(got_expired, "Expected token_expired from revalidation");
}

// ── Cycle 2: Subscription Protocol Tests ───────────────────────────────

fn test_entities() -> HashSet<String> {
    ["Post", "Comment"].iter().map(|s| (*s).to_owned()).collect()
}

#[tokio::test]
async fn test_subscribe_to_entity() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-alice"))).await.unwrap();
    let connected = next_msg(&mut ws).await;
    assert_eq!(connected["type"], "connected");

    // Subscribe to Post
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post", "event": "*"}))
        .await;

    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Post");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_subscribe_with_event_filter() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-bob"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe with INSERT-only filter
    send_json(
        &mut ws,
        serde_json::json!({"type": "subscribe", "entity": "Post", "event": "INSERT"}),
    )
    .await;

    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Post");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_subscribe_with_field_filter() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-carol"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe with field filter
    send_json(
        &mut ws,
        serde_json::json!({"type": "subscribe", "entity": "Post", "filter": "author_id=eq.123"}),
    )
    .await;

    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Post");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_unsubscribe() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-dave"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe then unsubscribe
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");

    send_json(&mut ws, serde_json::json!({"type": "unsubscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "unsubscribed");
    assert_eq!(reply["entity"], "Post");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_subscribe_to_nonexistent_entity_returns_error() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-eve"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe to unknown entity
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Foo"})).await;

    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "error");
    assert!(
        reply["message"].as_str().unwrap().contains("unknown entity"),
        "Expected 'unknown entity' error, got: {}",
        reply["message"]
    );

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_subscribe_exceeds_fan_out_limit() {
    let config = RealtimeConfig {
        max_subscriptions_per_entity: 2,
        max_connections_per_context: 100,
        ..RealtimeConfig::default()
    };
    let addr =
        spawn_test_server_with_entities(config, TestValidator::new(), test_entities()).await;

    // Connect 3 different users and subscribe each to Post
    let (mut ws1, _) = connect_async(ws_url(addr, Some("valid-user1"))).await.unwrap();
    let _ = next_msg(&mut ws1).await;
    send_json(&mut ws1, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws1).await;
    assert_eq!(reply["type"], "subscribed");

    let (mut ws2, _) = connect_async(ws_url(addr, Some("valid-user2"))).await.unwrap();
    let _ = next_msg(&mut ws2).await;
    send_json(&mut ws2, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws2).await;
    assert_eq!(reply["type"], "subscribed");

    // Third subscription should hit the limit
    let (mut ws3, _) = connect_async(ws_url(addr, Some("valid-user3"))).await.unwrap();
    let _ = next_msg(&mut ws3).await;
    send_json(&mut ws3, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws3).await;
    assert_eq!(reply["type"], "error");
    assert!(
        reply["message"].as_str().unwrap().contains("limit"),
        "Expected fan-out limit error, got: {}",
        reply["message"]
    );

    ws1.close(None).await.ok();
    ws2.close(None).await.ok();
    ws3.close(None).await.ok();
}

#[tokio::test]
async fn test_multiple_subscriptions_same_client() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-frank"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe to Post
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Post");

    // Subscribe to Comment
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Comment"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Comment");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_duplicate_subscribe_is_idempotent() {
    let addr = spawn_test_server_with_entities(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-grace"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe to Post twice
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    // Second subscribe should also succeed (idempotent, no error)
    assert_eq!(reply["type"], "subscribed");
    assert_eq!(reply["entity"], "Post");

    ws.close(None).await.ok();
}

// ── Cycle 3: Event Delivery with RLS Tests ─────────────────────────────

/// RLS evaluator that allows all access.
struct AllowAllRls;
impl RlsEvaluator for AllowAllRls {
    async fn can_access(&self, _context_hash: u64, _entity: &str, _row: &serde_json::Value) -> bool {
        true
    }
}

/// RLS evaluator that denies all access.
struct DenyAllRls;
impl RlsEvaluator for DenyAllRls {
    async fn can_access(&self, _context_hash: u64, _entity: &str, _row: &serde_json::Value) -> bool {
        false
    }
}

/// RLS evaluator that tracks how many times it's called.
struct CountingRls {
    call_count: std::sync::atomic::AtomicUsize,
}
impl CountingRls {
    fn new() -> Self {
        Self {
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    fn count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}
impl RlsEvaluator for CountingRls {
    async fn can_access(&self, _context_hash: u64, _entity: &str, _row: &serde_json::Value) -> bool {
        self.call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        true
    }
}

/// Helper to spawn a server + delivery pipeline and return (addr, event_tx).
async fn spawn_server_with_delivery<R: RlsEvaluator>(
    config: RealtimeConfig,
    validator: TestValidator,
    entities: HashSet<String>,
    rls: Arc<R>,
) -> (SocketAddr, mpsc::Sender<EntityEvent>) {
    let server = Arc::new(RealtimeServer::with_entities(config, entities));
    let (event_tx, event_rx) = mpsc::channel(1000);

    // Spawn the delivery pipeline
    let pipeline = EventDeliveryPipeline::new(
        server.subscriptions.clone(),
        server.connections.clone(),
        rls,
        event_rx,
    );
    tokio::spawn(pipeline.run());

    let state = RealtimeState {
        server,
        validator: Arc::new(validator),
    };

    let app = Router::new()
        .route("/realtime/v1", get(ws_handler::<TestValidator>))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, event_tx)
}

fn make_post_event(event_kind: EventKindSerde, author_id: i64) -> EntityEvent {
    EntityEvent {
        entity: "Post".to_owned(),
        event_kind,
        new: Some(serde_json::json!({"id": 1, "author_id": author_id, "title": "Hello"})),
        old: None,
        timestamp: "2026-04-28T12:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn test_event_delivered_to_subscribed_client() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-alice"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Subscribe to Post
    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let reply = next_msg(&mut ws).await;
    assert_eq!(reply["type"], "subscribed");

    // Send an event through the pipeline
    event_tx.send(make_post_event(EventKindSerde::Insert, 42)).await.unwrap();

    // Client should receive the change event
    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["entity"], "Post");
    assert_eq!(msg["event"], "INSERT");
    assert_eq!(msg["new"]["author_id"], 42);

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_not_delivered_to_unsubscribed_client() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    // Connect but do NOT subscribe
    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-bob"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    // Send an event
    event_tx.send(make_post_event(EventKindSerde::Insert, 42)).await.unwrap();

    // Give delivery time to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Client should NOT receive anything (use timeout to verify)
    let result = tokio::time::timeout(Duration::from_millis(100), ws.next()).await;
    assert!(result.is_err(), "Expected timeout (no message), got a message");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_rls_filters_unauthorized() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(DenyAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-carol"))).await.unwrap();
    let _ = next_msg(&mut ws).await;

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Send event — RLS denies all
    event_tx.send(make_post_event(EventKindSerde::Insert, 42)).await.unwrap();

    // Should NOT receive the event (silently dropped by RLS)
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = tokio::time::timeout(Duration::from_millis(100), ws.next()).await;
    assert!(result.is_err(), "Expected no message (RLS denied), got a message");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_rls_allows_authorized() {
    // Use AllowAllRls — client should receive
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-dave"))).await.unwrap();
    let _ = next_msg(&mut ws).await;

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    event_tx.send(make_post_event(EventKindSerde::Update, 99)).await.unwrap();

    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["event"], "UPDATE");
    assert_eq!(msg["new"]["author_id"], 99);

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_rls_grouping_by_context_hash() {
    // The TestValidator hashes user_id, so same user = same hash.
    // Different users = different hashes = separate RLS evaluations.
    let counting_rls = Arc::new(CountingRls::new());
    let config = RealtimeConfig {
        max_connections_per_context: 100,
        ..RealtimeConfig::default()
    };
    let (addr, event_tx) = spawn_server_with_delivery(
        config,
        TestValidator::new(),
        test_entities(),
        counting_rls.clone(),
    )
    .await;

    // Connect 3 clients with same user (same context hash)
    let mut same_user_ws = Vec::new();
    for i in 0..3 {
        let (mut ws, _) = connect_async(ws_url(addr, Some("valid-sameuser"))).await.unwrap();
        let _ = next_msg(&mut ws).await;
        send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
        let reply = next_msg(&mut ws).await;
        assert_eq!(reply["type"], "subscribed", "client {i} failed to subscribe");
        same_user_ws.push(ws);
    }

    // Connect 2 clients with different users (different hashes)
    let (mut ws_diff1, _) = connect_async(ws_url(addr, Some("valid-other1"))).await.unwrap();
    let _ = next_msg(&mut ws_diff1).await;
    send_json(&mut ws_diff1, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws_diff1).await;

    let (mut ws_diff2, _) = connect_async(ws_url(addr, Some("valid-other2"))).await.unwrap();
    let _ = next_msg(&mut ws_diff2).await;
    send_json(&mut ws_diff2, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws_diff2).await;

    // Send event
    event_tx.send(make_post_event(EventKindSerde::Insert, 1)).await.unwrap();

    // Wait for delivery
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should have 3 RLS evaluations (3 distinct context hashes: sameuser, other1, other2)
    // NOT 5 (one per connection)
    let rls_calls = counting_rls.count();
    assert_eq!(
        rls_calls, 3,
        "Expected 3 RLS evaluations (one per distinct context hash), got {rls_calls}"
    );

    // All 5 clients should have received the event
    for ws in &mut same_user_ws {
        let msg = next_msg(ws).await;
        assert_eq!(msg["type"], "change");
    }
    let msg = next_msg(&mut ws_diff1).await;
    assert_eq!(msg["type"], "change");
    let msg = next_msg(&mut ws_diff2).await;
    assert_eq!(msg["type"], "change");

    for ws in &mut same_user_ws {
        ws.close(None).await.ok();
    }
    ws_diff1.close(None).await.ok();
    ws_diff2.close(None).await.ok();
}

#[tokio::test]
async fn test_event_field_filter_applied() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-eve"))).await.unwrap();
    let _ = next_msg(&mut ws).await;

    // Subscribe with field filter: only author_id=123
    send_json(
        &mut ws,
        serde_json::json!({"type": "subscribe", "entity": "Post", "filter": "author_id=eq.123"}),
    )
    .await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Send event with author_id=456 — should NOT be delivered
    event_tx.send(make_post_event(EventKindSerde::Insert, 456)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = tokio::time::timeout(Duration::from_millis(100), ws.next()).await;
    assert!(result.is_err(), "Expected no message for author_id=456");

    // Send event with author_id=123 — SHOULD be delivered
    event_tx.send(make_post_event(EventKindSerde::Insert, 123)).await.unwrap();
    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["new"]["author_id"], 123);

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_type_filter_applied() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-frank"))).await.unwrap();
    let _ = next_msg(&mut ws).await;

    // Subscribe to INSERT only
    send_json(
        &mut ws,
        serde_json::json!({"type": "subscribe", "entity": "Post", "event": "INSERT"}),
    )
    .await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Send UPDATE event — should NOT be delivered
    event_tx.send(make_post_event(EventKindSerde::Update, 42)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = tokio::time::timeout(Duration::from_millis(100), ws.next()).await;
    assert!(result.is_err(), "Expected no message for UPDATE event");

    // Send INSERT event — SHOULD be delivered
    event_tx.send(make_post_event(EventKindSerde::Insert, 42)).await.unwrap();
    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["event"], "INSERT");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_event_payload_format() {
    let (addr, event_tx) = spawn_server_with_delivery(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-grace"))).await.unwrap();
    let _ = next_msg(&mut ws).await;

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Send a DELETE event with old data
    let event = EntityEvent {
        entity: "Post".to_owned(),
        event_kind: EventKindSerde::Delete,
        new: None,
        old: Some(serde_json::json!({"id": 7, "title": "Deleted post"})),
        timestamp: "2026-04-28T15:30:00Z".to_owned(),
    };
    event_tx.send(event).await.unwrap();

    let msg = next_msg(&mut ws).await;
    // Verify full payload format
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["entity"], "Post");
    assert_eq!(msg["event"], "DELETE");
    assert!(msg["new"].is_null());
    assert_eq!(msg["old"]["id"], 7);
    assert_eq!(msg["old"]["title"], "Deleted post");
    assert_eq!(msg["timestamp"], "2026-04-28T15:30:00Z");

    ws.close(None).await.ok();
}

// ── Cycle 4: RealtimeBroadcastObserver Tests ────────────────────────────

#[tokio::test]
async fn test_observer_receives_mutation_event() {
    let (observer, mut event_rx) = RealtimeBroadcastObserver::new(100);
    let event = make_post_event(EventKindSerde::Insert, 1);

    observer.on_mutation_complete(event.clone());

    let received = event_rx.recv().await.unwrap();
    assert_eq!(received.entity, event.entity);
    assert_eq!(received.event_kind, event.event_kind);
}

#[tokio::test]
async fn test_observer_enqueues_event_to_delivery_pipeline() {
    let (observer, mut event_rx) = RealtimeBroadcastObserver::new(100);

    for i in 0..5_i64 {
        observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, i));
    }

    let mut received = Vec::new();
    for _ in 0..5 {
        received.push(event_rx.try_recv().unwrap());
    }
    assert_eq!(received.len(), 5);
}

#[tokio::test]
async fn test_observer_returns_immediately() {
    // Capacity 100 — try_send should complete in microseconds, not milliseconds.
    let (observer, _rx) = RealtimeBroadcastObserver::new(100);
    let event = make_post_event(EventKindSerde::Insert, 1);

    let start = std::time::Instant::now();
    observer.on_mutation_complete(event);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1,
        "on_mutation_complete took {}µs — expected <1ms",
        elapsed.as_micros()
    );
}

#[tokio::test]
async fn test_observer_channel_full_drops_event() {
    // Capacity 1: one event fills the channel; subsequent sends are dropped.
    let (observer, _rx) = RealtimeBroadcastObserver::new(1);

    observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, 1));
    assert_eq!(observer.events_dropped_total(), 0, "first send must succeed");

    observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, 2));
    assert_eq!(observer.events_dropped_total(), 1, "second send must be dropped");

    observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, 3));
    assert_eq!(observer.events_dropped_total(), 2, "third send must be dropped");
}

/// Spawn a server + delivery pipeline wired through a `RealtimeBroadcastObserver`.
///
/// Returns `(addr, observer)` — call `observer.on_mutation_complete(event)` to
/// inject events end-to-end.
async fn spawn_server_with_observer<R: RlsEvaluator>(
    config: RealtimeConfig,
    validator: TestValidator,
    entities: HashSet<String>,
    rls: Arc<R>,
) -> (SocketAddr, RealtimeBroadcastObserver) {
    let server = Arc::new(RealtimeServer::with_entities(config.clone(), entities));
    let (observer, event_rx) = RealtimeBroadcastObserver::new(config.event_channel_capacity);

    let pipeline = EventDeliveryPipeline::new(
        server.subscriptions.clone(),
        server.connections.clone(),
        rls,
        event_rx,
    );
    tokio::spawn(pipeline.run());

    let state = RealtimeState {
        server,
        validator: Arc::new(validator),
    };

    let app = Router::new()
        .route("/realtime/v1", get(ws_handler::<TestValidator>))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, observer)
}

#[tokio::test]
async fn test_observer_slow_client_disconnected() {
    // Use a tiny per-connection channel (capacity 3) and a low kick threshold (3 drops)
    // so we can trigger the slow-consumer disconnect without sending hundreds of events.
    let config = RealtimeConfig {
        max_consecutive_drops: 3,
        connection_event_capacity: 3,
        max_connections_per_context: 100,
        ..RealtimeConfig::default()
    };
    let (addr, observer) = spawn_server_with_observer(
        config,
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-slow"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Read the first event so the server confirms delivery works.
    observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, 1));
    let _ = next_msg(&mut ws).await;

    // Stop reading. Send capacity(3) + threshold(3) = 6 events to fill the
    // per-connection channel and then trigger 3 consecutive drops → close signal 4002.
    for i in 2..=8_i64 {
        observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, i));
    }

    // Give the delivery pipeline and connection handler time to process.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Drain the WebSocket until we see close code 4002.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut got_close_4002 = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(tungstenite::Message::Close(Some(frame))))) => {
                if frame.code == tungstenite::protocol::frame::coding::CloseCode::from(4002) {
                    got_close_4002 = true;
                }
                break;
            }
            Ok(Some(Ok(_))) => {} // flush buffered change events
            Ok(None) | Ok(Some(Err(_))) => break,
            Err(_) => break, // outer timeout
        }
    }
    assert!(got_close_4002, "Expected close frame with code 4002 for slow consumer");
}

#[tokio::test]
async fn test_observer_slow_client_counter_resets() {
    // A client that reads every event never accumulates drops → never kicked.
    let config = RealtimeConfig {
        max_consecutive_drops: 5,
        connection_event_capacity: 10,
        max_connections_per_context: 100,
        ..RealtimeConfig::default()
    };
    let (addr, observer) = spawn_server_with_observer(
        config,
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-reader"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Client reads 10 events one-by-one — the drop counter resets on every
    // successful delivery, so the client is never kicked.
    for i in 0..10_i64 {
        observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, i));
        let msg = next_msg(&mut ws).await;
        assert_eq!(msg["type"], "change");
    }

    // Verify the client is still connected by receiving one final event.
    observer.on_mutation_complete(make_post_event(EventKindSerde::Insert, 99));
    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["new"]["author_id"], 99);

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_observer_end_to_end() {
    // Full integration: subscribe → observer.on_mutation_complete → pipeline → client receives.
    let (addr, observer) = spawn_server_with_observer(
        RealtimeConfig::default(),
        TestValidator::new(),
        test_entities(),
        Arc::new(AllowAllRls),
    )
    .await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-e2e"))).await.unwrap();
    let _ = next_msg(&mut ws).await; // connected

    send_json(&mut ws, serde_json::json!({"type": "subscribe", "entity": "Post"})).await;
    let _ = next_msg(&mut ws).await; // subscribed

    // Inject an event via the observer (the same path that a real mutation would use).
    observer.on_mutation_complete(EntityEvent {
        entity: "Post".to_owned(),
        event_kind: EventKindSerde::Insert,
        new: Some(serde_json::json!({"id": 42, "title": "Hello realtime", "author_id": 7})),
        old: None,
        timestamp: "2026-04-29T00:00:00Z".to_owned(),
    });

    let msg = next_msg(&mut ws).await;
    assert_eq!(msg["type"], "change");
    assert_eq!(msg["entity"], "Post");
    assert_eq!(msg["event"], "INSERT");
    assert_eq!(msg["new"]["id"], 42);
    assert_eq!(msg["new"]["title"], "Hello realtime");
    assert!(msg["old"].is_null());
    assert_eq!(msg["timestamp"], "2026-04-29T00:00:00Z");

    ws.close(None).await.ok();
}

// ── Cycle 5: Security Context Hashing & Connection State ───────────────

/// Build a `SecurityContextHashInput` for testing.
fn make_ctx<'a>(
    user_id: &'a str,
    roles: &'a [&'a str],
    tenant_id: Option<&'a str>,
    scopes: &'a [&'a str],
) -> super::context_hash::SecurityContextHashInput<'a> {
    super::context_hash::SecurityContextHashInput {
        user_id,
        roles,
        tenant_id,
        scopes,
    }
}

#[test]
fn test_security_context_hash_stable() {
    let ctx = make_ctx("user-1", &["admin", "editor"], Some("tenant-A"), &["read:post"]);
    let h1 = security_context_hash(&ctx);
    let h2 = security_context_hash(&ctx);
    assert_eq!(h1, h2, "same context must produce same hash");
}

#[test]
fn test_security_context_hash_differs_on_role_change() {
    let ctx_a = make_ctx("user-1", &["admin"], None, &[]);
    let ctx_b = make_ctx("user-1", &["editor"], None, &[]);
    assert_ne!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "different roles must produce different hashes"
    );
}

#[test]
fn test_security_context_hash_ignores_role_order() {
    let ctx_a = make_ctx("user-1", &["admin", "editor"], None, &[]);
    let ctx_b = make_ctx("user-1", &["editor", "admin"], None, &[]);
    assert_eq!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "role order must not affect hash"
    );
}

#[test]
fn test_security_context_hash_ignores_scope_order() {
    let ctx_a = make_ctx("user-1", &[], None, &["read:post", "write:post"]);
    let ctx_b = make_ctx("user-1", &[], None, &["write:post", "read:post"]);
    assert_eq!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "scope order must not affect hash"
    );
}

#[test]
fn test_security_context_hash_differs_on_user_id() {
    let ctx_a = make_ctx("user-1", &["admin"], None, &[]);
    let ctx_b = make_ctx("user-2", &["admin"], None, &[]);
    assert_ne!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "different user IDs must produce different hashes"
    );
}

#[test]
fn test_security_context_hash_differs_on_tenant() {
    let ctx_a = make_ctx("user-1", &[], Some("tenant-A"), &[]);
    let ctx_b = make_ctx("user-1", &[], Some("tenant-B"), &[]);
    assert_ne!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "different tenant IDs must produce different hashes"
    );
}

#[test]
fn test_security_context_hash_tenant_none_vs_some() {
    let ctx_a = make_ctx("user-1", &[], None, &[]);
    let ctx_b = make_ctx("user-1", &[], Some("tenant-A"), &[]);
    assert_ne!(
        security_context_hash(&ctx_a),
        security_context_hash(&ctx_b),
        "absent vs present tenant must produce different hashes"
    );
}

#[test]
fn test_connection_state_stores_context_hash() {
    use super::connections::ConnectionState;
    let state = ConnectionState::new(
        "conn-xyz".to_owned(),
        "user-1".to_owned(),
        0xdeadbeef,
        9_999_999_999,
    );
    assert_eq!(state.context_hash, 0xdeadbeef);
    assert_eq!(state.user_id, "user-1");
    assert_eq!(state.connection_id, "conn-xyz");
}

#[tokio::test]
async fn test_connection_state_cleanup_on_disconnect() {
    // Connect a client, then disconnect and verify the connection limit slot is freed.
    let addr = spawn_test_server(RealtimeConfig::default(), TestValidator::new()).await;

    let (mut ws, _) = connect_async(ws_url(addr, Some("valid-cleanup"))).await.unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    let parsed = parse_server_msg(&msg);
    assert_eq!(parsed["type"], "connected");

    ws.close(None).await.ok();

    // Give the server time to process the disconnect
    tokio::time::sleep(Duration::from_millis(100)).await;

    // A new connection with the same user must succeed (slot freed)
    let result = connect_async(ws_url(addr, Some("valid-cleanup"))).await;
    assert!(result.is_ok(), "reconnect should succeed after disconnect cleanup");
    if let Ok((mut ws2, _)) = result {
        ws2.close(None).await.ok();
    }
}
