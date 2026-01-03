//! WebSocket handler for GraphQL subscriptions
//!
//! This module implements WebSocket support for GraphQL subscriptions using the
//! graphql-ws protocol. In Commit 3, we establish the basic connection handling
//! and message routing. Full subscription execution is deferred to Commit 4.
//!
//! # Protocol Overview
//!
//! The GraphQL WebSocket Protocol defines these message types:
//! - `connection_init`: Client initiates the connection
//! - `connection_ack`: Server acknowledges the connection
//! - `subscribe`: Client sends a subscription request
//! - `next`: Server sends subscription data
//! - `error`: Server sends error information
//! - `complete`: Subscription is complete
//! - `ping`/`pong`: Keep-alive messages
//!
//! # Current Implementation (Commit 3)
//!
//! For this commit, we implement:
//! - WebSocket upgrade handling
//! - Connection acceptance with simple echo
//! - Message reception
//! - Basic connection cleanup
//!
//! Full subscription logic (parsing, validation, execution) will be
//! implemented in Commit 4 with integration to the subscription executor.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures_util::stream::StreamExt;
use futures_util::sink::SinkExt;

/// Handles WebSocket upgrade for GraphQL subscriptions
///
/// This handler processes the WebSocket upgrade request and initiates
/// the connection protocol. It will be called when a client connects
/// to the `/graphql/subscriptions` endpoint.
///
/// Currently implements a simple echo server that demonstrates:
/// - Accepting WebSocket upgrades
/// - Handling incoming messages
/// - Proper connection cleanup
///
/// In Commit 4, this will be extended to:
/// - Validate connection_init messages
/// - Parse GraphQL subscription requests
/// - Execute subscriptions through the pipeline
/// - Send subscription data to the client
///
/// # Arguments
///
/// * `ws` - WebSocket upgrade handler from Axum
///
/// # Returns
///
/// A response that upgrades the HTTP connection to WebSocket
pub async fn websocket_handler(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// Handles the WebSocket socket connection
///
/// This function manages the basic WebSocket lifecycle:
/// 1. Accept the connection
/// 2. Split the socket into sender and receiver
/// 3. Process incoming messages
/// 4. Clean up on disconnect
///
/// # Arguments
///
/// * `socket` - The WebSocket socket from the upgrade
async fn handle_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

    eprintln!("WebSocket connection established");

    // Message handling loop
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                eprintln!("Received text message: {}", text);

                // For Commit 3: Simple echo
                // In Commit 4: Parse as GraphQL message and route accordingly
                if let Err(e) = sender.send(Message::Text(text)).await {
                    eprintln!("Error sending response: {}", e);
                    break;
                }
            }

            Ok(Message::Close(close_frame)) => {
                eprintln!(
                    "WebSocket close received: {:?}",
                    close_frame.map(|cf| (cf.code, cf.reason))
                );
                break;
            }

            Ok(Message::Ping(data)) => {
                // Respond to ping with pong
                if let Err(e) = sender.send(Message::Pong(data)).await {
                    eprintln!("Error sending pong: {}", e);
                    break;
                }
            }

            Ok(Message::Pong(_)) => {
                // Pong received, connection is alive
                eprintln!("Pong received, connection alive");
            }

            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }

            _ => {
                // Other message types (binary, etc.) are not supported yet
                eprintln!("Unsupported message type received");
            }
        }
    }

    eprintln!("WebSocket connection closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_message_types() {
        // Verify message construction works
        let text_msg = Message::Text("test".to_string());
        assert!(matches!(text_msg, Message::Text(_)));

        let ping = Message::Ping(vec![1, 2, 3]);
        assert!(matches!(ping, Message::Ping(_)));

        let pong = Message::Pong(vec![1, 2, 3]);
        assert!(matches!(pong, Message::Pong(_)));
    }

    #[test]
    fn test_close_frame_creation() {
        // Test creating a close frame
        use axum::extract::ws::CloseFrame;

        let close = CloseFrame {
            code: 1000,
            reason: "Normal closure".into(),
        };

        let close_msg = Message::Close(Some(close));
        assert!(matches!(close_msg, Message::Close(_)));
    }

    #[test]
    fn test_graphql_message_json_structure() {
        // Test that GraphQL protocol messages can be represented as JSON
        let connection_init = serde_json::json!({
            "type": "connection_init",
            "payload": {}
        });

        assert_eq!(connection_init["type"], "connection_init");

        let subscribe = serde_json::json!({
            "id": "sub-1",
            "type": "subscribe",
            "payload": {
                "query": "subscription { event { id } }",
                "variables": {}
            }
        });

        assert_eq!(subscribe["id"], "sub-1");
        assert_eq!(subscribe["type"], "subscribe");
        assert_eq!(
            subscribe["payload"]["query"],
            "subscription { event { id } }"
        );
    }

    #[test]
    fn test_next_message_structure() {
        // Test data message structure
        let next_msg = serde_json::json!({
            "id": "sub-1",
            "type": "next",
            "payload": {
                "data": {
                    "event": {
                        "id": "evt-123",
                        "text": "Hello"
                    }
                }
            }
        });

        assert_eq!(next_msg["type"], "next");
        assert_eq!(next_msg["payload"]["data"]["event"]["id"], "evt-123");
    }

    #[test]
    fn test_error_message_structure() {
        // Test error message structure
        let error_msg = serde_json::json!({
            "id": "sub-1",
            "type": "error",
            "payload": [
                {
                    "message": "Invalid query",
                    "extensions": {
                        "code": "GRAPHQL_PARSE_FAILED"
                    }
                }
            ]
        });

        assert_eq!(error_msg["type"], "error");
        assert_eq!(error_msg["payload"][0]["message"], "Invalid query");
    }

    #[test]
    fn test_complete_message_structure() {
        // Test complete message structure
        let complete_msg = serde_json::json!({
            "id": "sub-1",
            "type": "complete"
        });

        assert_eq!(complete_msg["type"], "complete");
        assert_eq!(complete_msg["id"], "sub-1");
    }

    #[test]
    fn test_ping_pong_cycle() {
        // Test ping/pong message creation
        let ping_data = vec![1, 2, 3, 4, 5];
        let ping = Message::Ping(ping_data.clone());

        // In a real connection, the server responds with pong
        let pong = Message::Pong(ping_data);

        assert!(matches!(ping, Message::Ping(_)));
        assert!(matches!(pong, Message::Pong(_)));
    }

    #[test]
    fn test_message_content_preservation() {
        // Ensure message content is preserved
        let original_text = "complex message with {\"json\": \"data\"}";
        let msg = Message::Text(original_text.to_string());

        if let Message::Text(received) = msg {
            assert_eq!(received, original_text);
        } else {
            panic!("Expected text message");
        }
    }
}
