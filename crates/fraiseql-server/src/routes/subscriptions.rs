//! WebSocket subscription handler with protocol negotiation.
//!
//! Supports both the modern `graphql-transport-ws` protocol and the legacy
//! `graphql-ws` (Apollo subscriptions-transport-ws) protocol. Protocol
//! selection happens during the WebSocket upgrade via the `Sec-WebSocket-Protocol`
//! header.
//!
//! # Lifecycle Hooks
//!
//! Configurable callbacks are invoked at key points in the subscription
//! lifecycle: `on_connect`, `on_disconnect`, `on_subscribe`, `on_unsubscribe`.
//!
//! # Example
//!
//! ```no_run
//! // Requires: running server with initialized subscription manager.
//! use fraiseql_server::routes::subscriptions::{subscription_handler, SubscriptionState};
//!
//! let state = SubscriptionState::new(subscription_manager);
//!
//! let app = Router::new()
//!     .route("/ws", get(subscription_handler))
//!     .with_state(state);
//! ```

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::IntoResponse,
};
use fraiseql_core::runtime::{
    SubscriptionId, SubscriptionManager, SubscriptionPayload,
    protocol::{
        ClientMessage, ClientMessageType, CloseCode, GraphQLError, ServerMessage, SubscribePayload,
    },
};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::subscriptions::protocol::{ProtocolCodec, WsProtocol};
use crate::subscriptions::lifecycle::SubscriptionLifecycle;

// ── Subscription metrics (module-level atomics) ──────────────────────

static WS_CONNECTIONS_ACCEPTED: AtomicU64 = AtomicU64::new(0);
static WS_CONNECTIONS_REJECTED: AtomicU64 = AtomicU64::new(0);
static WS_SUBSCRIPTIONS_ACCEPTED: AtomicU64 = AtomicU64::new(0);
static WS_SUBSCRIPTIONS_REJECTED: AtomicU64 = AtomicU64::new(0);

/// Subscription metrics for Prometheus export.
#[must_use]
pub fn subscription_metrics() -> SubscriptionMetrics {
    SubscriptionMetrics {
        connections_accepted:  WS_CONNECTIONS_ACCEPTED.load(Ordering::Relaxed),
        connections_rejected:  WS_CONNECTIONS_REJECTED.load(Ordering::Relaxed),
        subscriptions_accepted: WS_SUBSCRIPTIONS_ACCEPTED.load(Ordering::Relaxed),
        subscriptions_rejected: WS_SUBSCRIPTIONS_REJECTED.load(Ordering::Relaxed),
    }
}

/// Reset all subscription counters to zero.
///
/// Call this at the start of each test that checks counter values to avoid
/// cross-test interference from the module-level statics.
#[cfg(test)]
pub fn reset_metrics_for_test() {
    WS_CONNECTIONS_ACCEPTED.store(0, Ordering::SeqCst);
    WS_CONNECTIONS_REJECTED.store(0, Ordering::SeqCst);
    WS_SUBSCRIPTIONS_ACCEPTED.store(0, Ordering::SeqCst);
    WS_SUBSCRIPTIONS_REJECTED.store(0, Ordering::SeqCst);
}

/// Snapshot of subscription counters.
pub struct SubscriptionMetrics {
    /// Total WebSocket connections accepted (after on_connect).
    pub connections_accepted: u64,
    /// Total WebSocket connections rejected by lifecycle hook.
    pub connections_rejected: u64,
    /// Total subscriptions accepted (after on_subscribe).
    pub subscriptions_accepted: u64,
    /// Total subscriptions rejected (by hook or limit).
    pub subscriptions_rejected: u64,
}

/// Connection initialization timeout (5 seconds per graphql-ws spec).
const CONNECTION_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Ping/keepalive interval.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// State for subscription WebSocket handler.
#[derive(Clone)]
pub struct SubscriptionState {
    /// Subscription manager.
    pub manager: Arc<SubscriptionManager>,
    /// Lifecycle hooks.
    pub lifecycle: Arc<dyn SubscriptionLifecycle>,
    /// Maximum subscriptions per connection (`None` = unlimited).
    pub max_subscriptions_per_connection: Option<u32>,
}

impl SubscriptionState {
    /// Create new subscription state.
    pub fn new(manager: Arc<SubscriptionManager>) -> Self {
        Self {
            manager,
            lifecycle: Arc::new(crate::subscriptions::lifecycle::NoopLifecycle),
            max_subscriptions_per_connection: None,
        }
    }

    /// Set lifecycle hooks.
    #[must_use]
    pub fn with_lifecycle(mut self, lifecycle: Arc<dyn SubscriptionLifecycle>) -> Self {
        self.lifecycle = lifecycle;
        self
    }

    /// Set maximum subscriptions per connection.
    #[must_use]
    pub const fn with_max_subscriptions(mut self, max: Option<u32>) -> Self {
        self.max_subscriptions_per_connection = max;
        self
    }
}

/// WebSocket upgrade handler for subscriptions.
///
/// Negotiates the WebSocket sub-protocol from the `Sec-WebSocket-Protocol`
/// header. Supports `graphql-transport-ws` (modern) and `graphql-ws` (legacy).
/// Defaults to `graphql-transport-ws` when no header is present.
/// Returns `400 Bad Request` for unrecognised protocols.
pub async fn subscription_handler(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<SubscriptionState>,
) -> impl IntoResponse {
    let protocol_header = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok());

    let protocol = match protocol_header {
        None => WsProtocol::GraphqlTransportWs,
        Some(header) => match WsProtocol::from_header(Some(header)) {
            Some(p) => p,
            None => {
                warn!(header = %header, "Unknown WebSocket sub-protocol requested");
                return axum::http::StatusCode::BAD_REQUEST.into_response();
            }
        },
    };

    ws.protocols([protocol.as_str()])
        .on_upgrade(move |socket| handle_subscription_connection(socket, state, protocol))
        .into_response()
}

/// Handle a WebSocket subscription connection.
async fn handle_subscription_connection(
    socket: WebSocket,
    state: SubscriptionState,
    protocol: WsProtocol,
) {
    let connection_id = uuid::Uuid::new_v4().to_string();
    let codec = ProtocolCodec::new(protocol);
    info!(
        connection_id = %connection_id,
        protocol = %protocol.as_str(),
        "WebSocket connection established"
    );

    let (mut sender, mut receiver) = socket.split();

    // Wait for connection_init with timeout
    let init_result = tokio::time::timeout(CONNECTION_INIT_TIMEOUT, async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(client_msg) = codec.decode(&text) {
                        if client_msg.parsed_type() == Some(ClientMessageType::ConnectionInit) {
                            return Some(client_msg);
                        }
                    }
                },
                Ok(Message::Close(_)) => return None,
                Err(e) => {
                    error!(error = %e, "WebSocket error during init");
                    return None;
                },
                _ => {},
            }
        }
        None
    })
    .await;

    // Handle init timeout or failure
    let _init_payload = match init_result {
        Ok(Some(msg)) => {
            // Call lifecycle on_connect hook
            let params = msg.payload.clone().unwrap_or(serde_json::json!({}));
            if let Err(reason) = state.lifecycle.on_connect(&params, &connection_id).await {
                warn!(
                    connection_id = %connection_id,
                    reason = %reason,
                    "Lifecycle on_connect rejected connection"
                );
                WS_CONNECTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                // Best-effort: connection is already being terminated.
                let _ = sender
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code:   4400,
                        reason: reason.into(),
                    })))
                    .await;
                return;
            }

            // Send connection_ack
            let ack = ServerMessage::connection_ack(None);
            if let Err(send_err) = send_server_message(&codec, &mut sender, ack).await {
                error!(connection_id = %connection_id, error = %send_err, "Failed to send connection_ack");
                return;
            }
            WS_CONNECTIONS_ACCEPTED.fetch_add(1, Ordering::Relaxed);
            info!(connection_id = %connection_id, "Connection initialized");
            msg.payload
        },
        Ok(None) => {
            warn!(connection_id = %connection_id, "Connection closed during init");
            return;
        },
        Err(_) => {
            warn!(connection_id = %connection_id, "Connection init timeout");
            // Best-effort: connection is already being terminated.
            let _ = sender
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code:   CloseCode::ConnectionInitTimeout.code(),
                    reason: CloseCode::ConnectionInitTimeout.reason().into(),
                })))
                .await;
            return;
        },
    };

    // Track active operations (operation_id -> subscription_id)
    let mut active_operations: HashMap<String, SubscriptionId> = HashMap::new();

    // Subscribe to event broadcast
    let mut event_receiver = state.manager.receiver();

    // Ping/keepalive timer
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // A44 — Token expiry re-check on long-lived subscriptions.
    //
    // JWTs validated at ConnectionInit may expire while the WebSocket is open.
    // The check below should be added when the auth layer surfaces expiry data:
    //
    //   1. At ConnectionInit, extract the `exp` claim from the JWT and store it:
    //      `let token_expires_at: Option<std::time::Instant> = extract_exp(&init_payload);`
    //
    //   2. In the select! loop (before processing each client message or broadcast event),
    //      check expiry:
    //      ```rust,ignore
    //      if token_expires_at.is_some_and(|exp| std::time::Instant::now() >= exp) {
    //          warn!(connection_id = %connection_id, "Token expired; closing WebSocket");
    //          let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
    //              code: CloseCode::Unauthorized.code(),
    //              reason: "Token expired".into(),
    //          }))).await;
    //          break;
    //      }
    //      ```
    //
    // This requires the lifecycle `on_connect` hook or the JWT middleware to return
    // the expiry time, which is not yet threaded through `SubscriptionState`.
    // Tracked as A44 in the remediation plan.

    // Main message loop
    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(close_code) = handle_client_message(
                            &text,
                            &connection_id,
                            &state,
                            &codec,
                            &mut active_operations,
                            &mut sender,
                        ).await {
                            // Best-effort: connection is already being closed.
                            let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: close_code.code(),
                                reason: close_code.reason().into(),
                            }))).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Best-effort: if the connection is already dead the pong will fail.
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) => {
                        info!(connection_id = %connection_id, "Client closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(connection_id = %connection_id, error = %e, "WebSocket error");
                        break;
                    }
                    None => {
                        info!(connection_id = %connection_id, "WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            event = event_receiver.recv() => {
                match event {
                    Ok(payload) => {
                        if let Some((op_id, _)) = active_operations
                            .iter()
                            .find(|(_, sub_id)| **sub_id == payload.subscription_id)
                        {
                            let msg = create_next_message(op_id, &payload);
                            if send_server_message(&codec, &mut sender, msg).await.is_err() {
                                warn!(connection_id = %connection_id, "Failed to send event");
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(connection_id = %connection_id, lagged = n, "Event receiver lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        error!(connection_id = %connection_id, "Event channel closed");
                        break;
                    }
                }
            }

            _ = ping_interval.tick() => {
                let msg = ServerMessage::ping(None);
                if send_server_message(&codec, &mut sender, msg).await.is_err() {
                    warn!(connection_id = %connection_id, "Failed to send ping/keepalive");
                    break;
                }
            }
        }
    }

    // Cleanup
    state.manager.unsubscribe_connection(&connection_id);
    state.lifecycle.on_disconnect(&connection_id).await;
    info!(connection_id = %connection_id, "WebSocket connection closed");
}

/// Handle a client message.
///
/// Returns `Ok(())` on success, or `Err(CloseCode)` if the connection should be closed.
async fn handle_client_message(
    text: &str,
    connection_id: &str,
    state: &SubscriptionState,
    codec: &ProtocolCodec,
    active_operations: &mut HashMap<String, SubscriptionId>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), CloseCode> {
    let client_msg: ClientMessage = codec.decode(text).map_err(|e| {
        warn!(error = %e, "Failed to parse client message");
        CloseCode::ProtocolError
    })?;

    match client_msg.parsed_type() {
        Some(ClientMessageType::Ping) => {
            let pong = ServerMessage::pong(client_msg.payload);
            // Best-effort: if the connection is already dead the pong will fail.
            let _ = send_server_message(codec, sender, pong).await;
        },

        Some(ClientMessageType::Pong) => {
            debug!(connection_id = %connection_id, "Received pong");
        },

        Some(ClientMessageType::Subscribe) => {
            let payload: SubscribePayload = client_msg.subscription_payload().ok_or_else(|| {
                warn!("Invalid subscribe payload");
                CloseCode::ProtocolError
            })?;

            let op_id = client_msg.id.ok_or_else(|| {
                warn!("Subscribe message missing operation ID");
                CloseCode::ProtocolError
            })?;

            // Check for duplicate operation ID
            if active_operations.contains_key(&op_id) {
                warn!(operation_id = %op_id, "Duplicate operation ID");
                return Err(CloseCode::SubscriberAlreadyExists);
            }

            // Enforce per-connection subscription limit
            if let Some(max) = state.max_subscriptions_per_connection {
                if active_operations.len() >= max as usize {
                    warn!(
                        connection_id = %connection_id,
                        active = active_operations.len(),
                        max = max,
                        "Subscription limit reached"
                    );
                    WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(
                            format!("Maximum subscriptions per connection ({max}) reached"),
                            "SUBSCRIPTION_LIMIT_REACHED",
                        )],
                    );
                    if let Err(e) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %e, "Could not send subscription limit error to client");
                    }
                    return Ok(());
                }
            }

            // Extract subscription name from query
            let subscription_name = match extract_subscription_name(&payload.query) {
                Some(name) => name,
                None => {
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(
                            "Could not parse subscription query",
                            "PARSE_ERROR",
                        )],
                    );
                    if let Err(e) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %e, "Could not send parse error to client");
                    }
                    return Ok(());
                }
            };

            // Call lifecycle on_subscribe hook
            // HashMap<String, Value> serialization is infallible; the error path cannot occur.
            let variables_value = serde_json::to_value(&payload.variables)
                .expect("HashMap<String, serde_json::Value> serialization is infallible");
            if let Err(reason) = state
                .lifecycle
                .on_subscribe(&subscription_name, &variables_value, connection_id)
                .await
            {
                warn!(
                    connection_id = %connection_id,
                    subscription = %subscription_name,
                    reason = %reason,
                    "Lifecycle on_subscribe rejected subscription"
                );
                WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                let error = ServerMessage::error(
                    &op_id,
                    vec![GraphQLError::with_code(reason, "SUBSCRIPTION_REJECTED")],
                );
                if let Err(e) = send_server_message(codec, sender, error).await {
                    debug!(connection_id = %connection_id, error = %e, "Could not send subscription rejection to client");
                }
                return Ok(());
            }

            // Subscribe
            match state.manager.subscribe(
                &subscription_name,
                serde_json::json!({}),
                variables_value,
                connection_id,
            ) {
                Ok(sub_id) => {
                    active_operations.insert(op_id.clone(), sub_id);
                    WS_SUBSCRIPTIONS_ACCEPTED.fetch_add(1, Ordering::Relaxed);
                    info!(
                        connection_id = %connection_id,
                        operation_id = %op_id,
                        subscription = %subscription_name,
                        "Subscription started"
                    );
                },
                Err(e) => {
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(e.to_string(), "SUBSCRIPTION_ERROR")],
                    );
                    if let Err(send_err) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %send_err, "Could not send subscription error to client");
                    }
                },
            }
        },

        Some(ClientMessageType::Complete) => {
            let op_id = client_msg.id.ok_or_else(|| {
                warn!("Complete message missing operation ID");
                CloseCode::ProtocolError
            })?;

            if let Some(sub_id) = active_operations.remove(&op_id) {
                if let Err(e) = state.manager.unsubscribe(sub_id) {
                    warn!(connection_id = %connection_id, operation_id = %op_id, error = %e, "Failed to unsubscribe; subscription may be leaked");
                }
                state.lifecycle.on_unsubscribe(&op_id, connection_id).await;
                info!(
                    connection_id = %connection_id,
                    operation_id = %op_id,
                    "Subscription completed"
                );
            }
        },

        Some(ClientMessageType::ConnectionInit) => {
            warn!(connection_id = %connection_id, "Duplicate connection_init");
            return Err(CloseCode::TooManyInitRequests);
        },

        None => {
            warn!(message_type = %client_msg.message_type, "Unknown message type");
        },
    }

    Ok(())
}

/// Send a server message through the codec, handling protocol translation.
async fn send_server_message(
    codec: &ProtocolCodec,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: ServerMessage,
) -> Result<(), String> {
    match codec.encode(msg) {
        Ok(Some(json)) => {
            sender
                .send(Message::Text(json.into()))
                .await
                .map_err(|e| e.to_string())
        }
        Ok(None) => Ok(()), // Message suppressed by codec (e.g. pong in legacy mode)
        Err(e) => Err(e.to_string()),
    }
}

/// Create a "next" message for a subscription event.
fn create_next_message(operation_id: &str, payload: &SubscriptionPayload) -> ServerMessage {
    let data = serde_json::json!({
        payload.subscription_name.clone(): payload.data
    });
    ServerMessage::next(operation_id, data)
}

/// Extract subscription name from a GraphQL subscription query.
fn extract_subscription_name(query: &str) -> Option<String> {
    let query = query.trim();

    let sub_idx = query.find("subscription")?;
    let after_sub = &query[sub_idx + "subscription".len()..];

    let brace_idx = after_sub.find('{')?;
    let after_brace = after_sub[brace_idx + 1..].trim_start();

    let name_end = after_brace
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_brace.len());

    if name_end == 0 {
        return None;
    }

    Some(after_brace[..name_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_subscription_name_simple() {
        let query = "subscription { orderCreated { id } }";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_with_operation() {
        let query = "subscription OnOrderCreated { orderCreated { id amount } }";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_with_variables() {
        let query = "subscription ($userId: ID!) { userUpdated(userId: $userId) { id name } }";
        assert_eq!(extract_subscription_name(query), Some("userUpdated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_whitespace() {
        let query = r"
            subscription {
                orderCreated {
                    id
                }
            }
        ";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_invalid() {
        assert_eq!(extract_subscription_name("query { users { id } }"), None);
        assert_eq!(extract_subscription_name("{ users { id } }"), None);
        assert_eq!(extract_subscription_name("subscription { }"), None);
    }
}
