//! Tests for the realtime `WebSocket` module (Phase 7, Cycles 1–2).

use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};

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
