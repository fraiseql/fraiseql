//! Wire protocol messages for the realtime `WebSocket` endpoint.
//!
//! Defines the JSON message types exchanged between server and client
//! over the `/realtime/v1` `WebSocket` connection.

use serde::{Deserialize, Serialize};

/// Message sent by the client over the `WebSocket` connection.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ClientMessage {
    /// Heartbeat response from client.
    Pong,

    /// Subscribe to entity change events.
    Subscribe {
        /// Entity name to subscribe to (e.g., `"Post"`).
        entity: String,
        /// Event type filter: `"*"`, `"INSERT"`, `"UPDATE"`, or `"DELETE"`.
        #[serde(default = "default_event_filter")]
        event: String,
        /// Optional field filter in `field=op.value` format (e.g., `"author_id=eq.123"`).
        #[serde(default)]
        filter: Option<String>,
    },

    /// Unsubscribe from entity change events.
    Unsubscribe {
        /// Entity name to unsubscribe from.
        entity: String,
    },
}

/// Default event filter: all events.
fn default_event_filter() -> String {
    "*".to_owned()
}

/// Message sent by the server to the client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServerMessage {
    /// Sent after successful authentication and connection setup.
    Connected {
        /// Unique identifier for this connection.
        connection_id: String,
    },

    /// Periodic heartbeat from server.
    Ping,

    /// Authentication token has expired; connection will close.
    TokenExpired,

    /// Subscription confirmed.
    Subscribed {
        /// Entity that was subscribed to.
        entity: String,
    },

    /// Unsubscription confirmed.
    Unsubscribed {
        /// Entity that was unsubscribed from.
        entity: String,
    },

    /// Error message.
    Error {
        /// Human-readable error description.
        message: String,
    },
}

impl ServerMessage {
    /// Serialize this message to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if serialization fails (should not happen
    /// for well-formed `ServerMessage` variants).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
