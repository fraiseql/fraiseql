//! GraphQL over WebSocket subscription protocol types.
//!
//! Implements the `graphql-ws` protocol (v5+) message framing for
//! client-to-server and server-to-client subscription communication.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Client-to-server message types.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub fn as_str(&self) -> &'static str {
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
    pub fn as_str(&self) -> &'static str {
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

/// Client message (from WebSocket client).
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

    /// Extract connection parameters from connection_init payload.
    #[must_use]
    pub fn connection_params(&self) -> Option<&serde_json::Value> {
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

/// Server message (to WebSocket client).
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
    /// Create connection_ack message.
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
    pub fn next(id: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            message_type: ServerMessageType::Next.as_str().to_string(),
            id:           Some(id.into()),
            payload:      Some(serde_json::json!({ "data": data })),
        }
    }

    /// Create error message.
    #[must_use]
    pub fn error(id: impl Into<String>, errors: Vec<GraphQLError>) -> Self {
        Self {
            message_type: ServerMessageType::Error.as_str().to_string(),
            id:           Some(id.into()),
            payload:      Some(serde_json::to_value(errors).unwrap_or_default()),
        }
    }

    /// Create complete message.
    #[must_use]
    pub fn complete(id: impl Into<String>) -> Self {
        Self {
            message_type: ServerMessageType::Complete.as_str().to_string(),
            id:           Some(id.into()),
            payload:      None,
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

/// GraphQL error format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Error message.
    pub message: String,

    /// Error locations in query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ErrorLocation>>,

    /// Error path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<serde_json::Value>>,

    /// Extensions (error codes, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

impl GraphQLError {
    /// Create a simple error message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message:    message.into(),
            locations:  None,
            path:       None,
            extensions: None,
        }
    }

    /// Create an error with code extension.
    #[must_use]
    pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        let mut extensions = HashMap::new();
        extensions.insert("code".to_string(), serde_json::json!(code.into()));

        Self {
            message:    message.into(),
            locations:  None,
            path:       None,
            extensions: Some(extensions),
        }
    }
}

/// Error location in query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLocation {
    /// Line number (1-indexed).
    pub line:   u32,
    /// Column number (1-indexed).
    pub column: u32,
}

/// Close codes for WebSocket connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseCode {
    /// Normal closure.
    Normal               = 1000,
    /// Client violated protocol.
    ProtocolError        = 1002,
    /// Internal server error.
    InternalError        = 1011,
    /// Connection initialization timeout.
    ConnectionInitTimeout = 4408,
    /// Too many initialization requests.
    TooManyInitRequests  = 4429,
    /// Subscriber already exists (duplicate ID).
    SubscriberAlreadyExists = 4409,
    /// Unauthorized.
    Unauthorized         = 4401,
    /// Subscription not found (invalid ID on complete).
    SubscriptionNotFound = 4404,
}

impl CloseCode {
    /// Get the close code value.
    #[must_use]
    pub fn code(self) -> u16 {
        self as u16
    }

    /// Get the close reason message.
    #[must_use]
    pub fn reason(self) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_type_parsing() {
        assert_eq!(
            ClientMessageType::from_str("connection_init"),
            Some(ClientMessageType::ConnectionInit)
        );
        assert_eq!(ClientMessageType::from_str("subscribe"), Some(ClientMessageType::Subscribe));
        assert_eq!(ClientMessageType::from_str("invalid"), None);
    }

    #[test]
    fn test_server_message_connection_ack() {
        let msg = ServerMessage::connection_ack(None);
        assert_eq!(msg.message_type, "connection_ack");
        assert!(msg.id.is_none());

        let json = msg.to_json().unwrap();
        assert!(json.contains("connection_ack"));
    }

    #[test]
    fn test_server_message_next() {
        let data = serde_json::json!({"orderCreated": {"id": "ord_123"}});
        let msg = ServerMessage::next("op_1", data);

        assert_eq!(msg.message_type, "next");
        assert_eq!(msg.id, Some("op_1".to_string()));

        let json = msg.to_json().unwrap();
        assert!(json.contains("next"));
        assert!(json.contains("op_1"));
        assert!(json.contains("orderCreated"));
    }

    #[test]
    fn test_server_message_error() {
        let errors = vec![GraphQLError::with_code(
            "Subscription not found",
            "SUBSCRIPTION_NOT_FOUND",
        )];
        let msg = ServerMessage::error("op_1", errors);

        assert_eq!(msg.message_type, "error");
        let json = msg.to_json().unwrap();
        assert!(json.contains("Subscription not found"));
    }

    #[test]
    fn test_server_message_complete() {
        let msg = ServerMessage::complete("op_1");

        assert_eq!(msg.message_type, "complete");
        assert_eq!(msg.id, Some("op_1".to_string()));
        assert!(msg.payload.is_none());
    }

    #[test]
    fn test_client_message_parsing() {
        let json = r#"{
            "type": "subscribe",
            "id": "op_1",
            "payload": {
                "query": "subscription { orderCreated { id } }"
            }
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.parsed_type(), Some(ClientMessageType::Subscribe));
        assert_eq!(msg.id, Some("op_1".to_string()));

        let payload = msg.subscription_payload().unwrap();
        assert!(payload.query.contains("orderCreated"));
    }

    #[test]
    fn test_close_codes() {
        assert_eq!(CloseCode::Normal.code(), 1000);
        assert_eq!(CloseCode::Unauthorized.code(), 4401);
        assert_eq!(CloseCode::SubscriberAlreadyExists.code(), 4409);
    }

    #[test]
    fn test_graphql_error() {
        let error = GraphQLError::with_code("Test error", "TEST_ERROR");
        assert_eq!(error.message, "Test error");
        assert!(error.extensions.is_some());

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("TEST_ERROR"));
    }
}
