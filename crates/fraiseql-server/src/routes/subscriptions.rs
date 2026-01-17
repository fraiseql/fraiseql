//! WebSocket subscription handler for graphql-ws protocol.
//!
//! Implements the graphql-ws (graphql-transport-ws) protocol for GraphQL subscriptions
//! over WebSocket connections.
//!
//! # Protocol
//!
//! Uses the modern graphql-ws protocol as specified at:
//! <https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md>
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_server::routes::subscriptions::{subscription_handler, SubscriptionState};
//!
//! let state = SubscriptionState::new(subscription_manager);
//!
//! let app = Router::new()
//!     .route("/ws", get(subscription_handler))
//!     .with_state(state);
//! ```

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use fraiseql_core::runtime::{
    protocol::{
        ClientMessage, ClientMessageType, CloseCode, GraphQLError, ServerMessage,
        SubscribePayload,
    },
    SubscriptionId, SubscriptionManager, SubscriptionPayload,
};
use futures::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Connection initialization timeout (5 seconds per graphql-ws spec).
const CONNECTION_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Ping interval for keepalive.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// State for subscription WebSocket handler.
#[derive(Clone)]
pub struct SubscriptionState {
    /// Subscription manager.
    pub manager: Arc<SubscriptionManager>,
}

impl SubscriptionState {
    /// Create new subscription state.
    pub fn new(manager: Arc<SubscriptionManager>) -> Self {
        Self { manager }
    }
}

/// WebSocket upgrade handler for subscriptions.
///
/// Upgrades HTTP connection to WebSocket and handles graphql-ws protocol.
pub async fn subscription_handler(
    ws: WebSocketUpgrade,
    State(state): State<SubscriptionState>,
) -> impl IntoResponse {
    ws.protocols(["graphql-transport-ws"])
        .on_upgrade(move |socket| handle_subscription_connection(socket, state))
}

/// Handle a WebSocket subscription connection.
async fn handle_subscription_connection(socket: WebSocket, state: SubscriptionState) {
    let connection_id = uuid::Uuid::new_v4().to_string();
    info!(connection_id = %connection_id, "WebSocket connection established");

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Wait for connection_init with timeout
    let init_result = tokio::time::timeout(CONNECTION_INIT_TIMEOUT, async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        if client_msg.parsed_type() == Some(ClientMessageType::ConnectionInit) {
                            return Some(client_msg);
                        }
                    }
                }
                Ok(Message::Close(_)) => return None,
                Err(e) => {
                    error!(error = %e, "WebSocket error during init");
                    return None;
                }
                _ => {}
            }
        }
        None
    })
    .await;

    // Handle init timeout or failure
    let _init_payload = match init_result {
        Ok(Some(msg)) => {
            // Send connection_ack
            let ack = ServerMessage::connection_ack(None);
            if let Ok(json) = ack.to_json() {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    error!(connection_id = %connection_id, "Failed to send connection_ack");
                    return;
                }
            }
            info!(connection_id = %connection_id, "Connection initialized");
            msg.payload
        }
        Ok(None) => {
            warn!(connection_id = %connection_id, "Connection closed during init");
            return;
        }
        Err(_) => {
            warn!(connection_id = %connection_id, "Connection init timeout");
            // Send close frame with timeout code
            let _ = sender
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: CloseCode::ConnectionInitTimeout.code(),
                    reason: CloseCode::ConnectionInitTimeout.reason().into(),
                })))
                .await;
            return;
        }
    };

    // Track active operations (operation_id -> subscription_id)
    let mut active_operations: HashMap<String, SubscriptionId> = HashMap::new();

    // Subscribe to event broadcast
    let mut event_receiver = state.manager.receiver();

    // Ping timer
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Main message loop
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(close_code) = handle_client_message(
                            &text,
                            &connection_id,
                            &state.manager,
                            &mut active_operations,
                            &mut sender,
                        ).await {
                            // Protocol error - close connection
                            let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: close_code.code(),
                                reason: close_code.reason().into(),
                            }))).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Pong received, connection is alive
                    }
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

            // Handle subscription events
            event = event_receiver.recv() => {
                match event {
                    Ok(payload) => {
                        // Find operation ID for this subscription
                        if let Some((op_id, _)) = active_operations
                            .iter()
                            .find(|(_, sub_id)| **sub_id == payload.subscription_id)
                        {
                            let msg = create_next_message(op_id, &payload);
                            if let Ok(json) = msg.to_json() {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    warn!(connection_id = %connection_id, "Failed to send event");
                                    break;
                                }
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

            // Send ping for keepalive
            _ = ping_interval.tick() => {
                let ping = ServerMessage::ping(None);
                if let Ok(json) = ping.to_json() {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        warn!(connection_id = %connection_id, "Failed to send ping");
                        break;
                    }
                }
            }
        }
    }

    // Cleanup: unsubscribe all operations for this connection
    state.manager.unsubscribe_connection(&connection_id);
    info!(connection_id = %connection_id, "WebSocket connection closed");
}

/// Handle a client message.
///
/// Returns `Ok(())` on success, or `Err(CloseCode)` if the connection should be closed.
async fn handle_client_message(
    text: &str,
    connection_id: &str,
    manager: &SubscriptionManager,
    active_operations: &mut HashMap<String, SubscriptionId>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), CloseCode> {
    let client_msg: ClientMessage = serde_json::from_str(text).map_err(|e| {
        warn!(error = %e, "Failed to parse client message");
        CloseCode::ProtocolError
    })?;

    match client_msg.parsed_type() {
        Some(ClientMessageType::Ping) => {
            // Respond with pong
            let pong = ServerMessage::pong(client_msg.payload);
            if let Ok(json) = pong.to_json() {
                let _ = sender.send(Message::Text(json.into())).await;
            }
        }

        Some(ClientMessageType::Pong) => {
            // Client responded to our ping, connection is alive
            debug!(connection_id = %connection_id, "Received pong");
        }

        Some(ClientMessageType::Subscribe) => {
            // Parse subscription payload first (borrows client_msg)
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

            // Extract subscription name from query
            // For now, we use a simple parser - in production, use proper GraphQL parsing
            let subscription_name = extract_subscription_name(&payload.query).ok_or_else(|| {
                let error = ServerMessage::error(
                    &op_id,
                    vec![GraphQLError::with_code(
                        "Could not parse subscription query",
                        "PARSE_ERROR",
                    )],
                );
                if let Ok(json) = error.to_json() {
                    let _ = futures::executor::block_on(sender.send(Message::Text(json.into())));
                }
                CloseCode::ProtocolError
            })?;

            // Subscribe
            let variables = serde_json::to_value(&payload.variables).unwrap_or_default();
            match manager.subscribe(&subscription_name, serde_json::json!({}), variables, connection_id)
            {
                Ok(sub_id) => {
                    active_operations.insert(op_id.clone(), sub_id);
                    info!(
                        connection_id = %connection_id,
                        operation_id = %op_id,
                        subscription = %subscription_name,
                        "Subscription started"
                    );
                }
                Err(e) => {
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(e.to_string(), "SUBSCRIPTION_ERROR")],
                    );
                    if let Ok(json) = error.to_json() {
                        let _ = sender.send(Message::Text(json.into())).await;
                    }
                }
            }
        }

        Some(ClientMessageType::Complete) => {
            let op_id = client_msg.id.ok_or_else(|| {
                warn!("Complete message missing operation ID");
                CloseCode::ProtocolError
            })?;

            // Unsubscribe
            if let Some(sub_id) = active_operations.remove(&op_id) {
                let _ = manager.unsubscribe(sub_id);
                info!(
                    connection_id = %connection_id,
                    operation_id = %op_id,
                    "Subscription completed"
                );
            }
        }

        Some(ClientMessageType::ConnectionInit) => {
            // Already initialized - too many init requests
            warn!(connection_id = %connection_id, "Duplicate connection_init");
            return Err(CloseCode::TooManyInitRequests);
        }

        None => {
            warn!(message_type = %client_msg.message_type, "Unknown message type");
            // Unknown message types are ignored per spec
        }
    }

    Ok(())
}

/// Create a "next" message for a subscription event.
fn create_next_message(operation_id: &str, payload: &SubscriptionPayload) -> ServerMessage {
    // Structure the data as GraphQL response format
    let data = serde_json::json!({
        payload.subscription_name.clone(): payload.data
    });
    ServerMessage::next(operation_id, data)
}

/// Extract subscription name from a GraphQL subscription query.
///
/// This is a simple parser for demonstration. In production, use proper GraphQL parsing.
fn extract_subscription_name(query: &str) -> Option<String> {
    // Look for pattern: subscription { subscriptionName
    // or: subscription OperationName { subscriptionName
    let query = query.trim();

    // Find "subscription" keyword
    let sub_idx = query.find("subscription")?;
    let after_sub = &query[sub_idx + "subscription".len()..];

    // Find opening brace
    let brace_idx = after_sub.find('{')?;
    let after_brace = after_sub[brace_idx + 1..].trim_start();

    // Extract first word (subscription field name)
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
        assert_eq!(
            extract_subscription_name(query),
            Some("orderCreated".to_string())
        );
    }

    #[test]
    fn test_extract_subscription_name_with_operation() {
        let query = "subscription OnOrderCreated { orderCreated { id amount } }";
        assert_eq!(
            extract_subscription_name(query),
            Some("orderCreated".to_string())
        );
    }

    #[test]
    fn test_extract_subscription_name_with_variables() {
        let query = "subscription ($userId: ID!) { userUpdated(userId: $userId) { id name } }";
        assert_eq!(
            extract_subscription_name(query),
            Some("userUpdated".to_string())
        );
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
        assert_eq!(
            extract_subscription_name(query),
            Some("orderCreated".to_string())
        );
    }

    #[test]
    fn test_extract_subscription_name_invalid() {
        assert_eq!(extract_subscription_name("query { users { id } }"), None);
        assert_eq!(extract_subscription_name("{ users { id } }"), None);
        assert_eq!(extract_subscription_name("subscription { }"), None);
    }
}
