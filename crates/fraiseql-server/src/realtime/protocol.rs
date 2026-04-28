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
