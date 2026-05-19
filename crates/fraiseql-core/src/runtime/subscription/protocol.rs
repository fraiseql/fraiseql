//! GraphQL over `WebSocket` subscription protocol types.
//!
//! Implements the `graphql-ws` protocol (v5+) message framing for
//! client-to-server and server-to-client subscription communication.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Client-to-server message types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientMessageType {
    /// Connection initialization.
    ConnectionInit,
    /// Ping (keepalive).
    Ping,
    /// Pong response.
    Pong,
    /// Subscribe to operation.
    Subscribe,
    /// Complete/unsubscribe from operation.
    Complete,
}

impl ClientMessageType {
    /// Parse message type from string.
    #[must_use]
    #[allow(clippy::should_implement_trait)] // Reason: returns Option<Self> (unknown types yield None), not a FromStr-compatible Result
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "connection_init" => Some(Self::ConnectionInit),
            "ping" => Some(Self::Ping),
            "pong" => Some(Self::Pong),
            "subscribe" => Some(Self::Subscribe),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }

    /// Get string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ConnectionInit => "connection_init",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Subscribe => "subscribe",
            Self::Complete => "complete",
        }
    }
}

/// Server-to-client message types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServerMessageType {
    /// Connection acknowledged.
    ConnectionAck,
    /// Ping (keepalive).
    Ping,
    /// Pong response.
    Pong,
    /// Subscription data.
    Next,
    /// Operation error.
    Error,
    /// Operation complete.
    Complete,
}

impl ServerMessageType {
    /// Get string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ConnectionAck => "connection_ack",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Next => "next",
            Self::Error => "error",
            Self::Complete => "complete",
        }
    }
}

/// Client message (from `WebSocket` client).
#[derive(Debug, Clone, Deserialize)]
pub struct ClientMessage {
    /// Message type.
    #[serde(rename = "type")]
    pub message_type: String,

    /// Operation ID (for subscribe/complete).
    #[serde(default)]
    pub id: Option<String>,

    /// Payload (connection params or subscription query).
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

impl ClientMessage {
    /// Parse the message type.
    #[must_use]
    pub fn parsed_type(&self) -> Option<ClientMessageType> {
        ClientMessageType::from_str(&self.message_type)
    }

    /// Extract connection parameters from `connection_init` payload.
    #[must_use]
    pub const fn connection_params(&self) -> Option<&serde_json::Value> {
        self.payload.as_ref()
    }

    /// Extract subscription query from subscribe payload.
    #[must_use]
    pub fn subscription_payload(&self) -> Option<SubscribePayload> {
        self.payload.as_ref().and_then(|p| serde_json::from_value(p.clone()).ok())
    }
}

/// Subscribe message payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubscribePayload {
    /// GraphQL query string.
    pub query: String,

    /// Optional operation name.
    #[serde(rename = "operationName")]
    #[serde(default)]
    pub operation_name: Option<String>,

    /// Query variables.
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Extensions (e.g., persisted query hash).
    #[serde(default)]
    pub extensions: HashMap<String, serde_json::Value>,
}

/// Server message (to `WebSocket` client).
#[derive(Debug, Clone, Serialize)]
pub struct ServerMessage {
    /// Message type.
    #[serde(rename = "type")]
    pub message_type: String,

    /// Operation ID (for next/error/complete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Payload (data, errors, or ack payload).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl ServerMessage {
    /// Create `connection_ack` message.
    #[must_use]
    pub fn connection_ack(payload: Option<serde_json::Value>) -> Self {
        Self {
            message_type: ServerMessageType::ConnectionAck.as_str().to_string(),
            id: None,
            payload,
        }
    }

    /// Create ping message.
    #[must_use]
    pub fn ping(payload: Option<serde_json::Value>) -> Self {
        Self {
            message_type: ServerMessageType::Ping.as_str().to_string(),
            id: None,
            payload,
        }
    }

    /// Create pong message.
    #[must_use]
    pub fn pong(payload: Option<serde_json::Value>) -> Self {
        Self {
            message_type: ServerMessageType::Pong.as_str().to_string(),
            id: None,
            payload,
        }
    }

    /// Create next (data) message.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // Reason: data is moved into serde_json::json! macro to construct the payload object
    pub fn next(id: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            message_type: ServerMessageType::Next.as_str().to_string(),
            id: Some(id.into()),
            payload: Some(serde_json::json!({ "data": data })),
        }
    }

    /// Create error message.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // Reason: errors is consumed by serde_json::to_value, which requires an owned value
    pub fn error(id: impl Into<String>, errors: Vec<GraphQLError>) -> Self {
        Self {
            message_type: ServerMessageType::Error.as_str().to_string(),
            id: Some(id.into()),
            payload: Some(serde_json::to_value(errors).unwrap_or_default()),
        }
    }

    /// Create complete message.
    #[must_use]
    pub fn complete(id: impl Into<String>) -> Self {
        Self {
            message_type: ServerMessageType::Complete.as_str().to_string(),
            id: Some(id.into()),
            payload: None,
        }
    }

    /// Serialize to JSON string.
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

pub use fraiseql_error::{GraphQLError, GraphQLErrorLocation as ErrorLocation};

/// Close codes for `WebSocket` connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseCode {
    /// Normal closure.
    Normal = 1000,
    /// Client violated protocol.
    ProtocolError = 1002,
    /// Internal server error.
    InternalError = 1011,
    /// Connection initialization timeout.
    ConnectionInitTimeout = 4408,
    /// Too many initialization requests.
    TooManyInitRequests = 4429,
    /// Subscriber already exists (duplicate ID).
    SubscriberAlreadyExists = 4409,
    /// Unauthorized.
    Unauthorized = 4401,
    /// Subscription not found (invalid ID on complete).
    SubscriptionNotFound = 4404,
}

impl CloseCode {
    /// Get the close code value.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Get the close reason message.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Normal => "Normal closure",
            Self::ProtocolError => "Protocol error",
            Self::InternalError => "Internal server error",
            Self::ConnectionInitTimeout => "Connection initialization timeout",
            Self::TooManyInitRequests => "Too many initialization requests",
            Self::SubscriberAlreadyExists => "Subscriber already exists",
            Self::Unauthorized => "Unauthorized",
            Self::SubscriptionNotFound => "Subscription not found",
        }
    }
}
