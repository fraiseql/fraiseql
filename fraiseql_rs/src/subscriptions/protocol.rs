//! GraphQL WebSocket Protocol (graphql-ws spec)
//!
//! Implements the GraphQL over WebSocket protocol.
//! See: <https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md>

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// GraphQL WebSocket message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GraphQLMessage {
    /// Client initiates connection
    ConnectionInit {
        /// Optional connection initialization payload
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
    },

    /// Server acknowledges connection
    ConnectionAck {
        /// Optional acknowledgment payload
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
    },

    /// Client sends subscription
    Subscribe {
        /// Subscription identifier
        id: String,
        /// Subscription request payload
        payload: SubscriptionPayload,
    },

    /// Server sends data
    Next {
        /// Subscription identifier
        id: String,
        /// Data payload
        payload: Value,
    },

    /// Server sends error
    Error {
        /// Subscription identifier
        id: String,
        /// Error messages
        payload: Vec<GraphQLError>,
    },

    /// Server completes subscription
    Complete {
        /// Subscription identifier
        id: String,
    },

    /// Client/Server ping
    Ping {
        /// Optional ping payload
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
    },

    /// Client/Server pong
    Pong {
        /// Optional pong payload
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
    },
}

/// Subscription message (internal representation)
#[derive(Debug, Clone)]
pub struct SubscriptionMessage {
    /// Subscription ID
    pub id: String,

    /// Query string
    pub query: String,

    /// Operation name
    pub operation_name: Option<String>,

    /// Variables
    pub variables: HashMap<String, Value>,

    /// Extensions
    pub extensions: Option<Value>,
}

/// Subscription payload from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPayload {
    /// GraphQL query
    pub query: String,

    /// Operation name
    #[serde(rename = "operationName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,

    /// Query variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,

    /// Extensions (e.g., APQ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// GraphQL error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Error message
    pub message: String,

    /// Error locations in query
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ErrorLocation>>,

    /// Path to error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,

    /// Additional error info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Error location in query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLocation {
    /// Line number
    pub line: u32,

    /// Column number
    pub column: u32,
}

/// Parsed subscription (after validation)
#[derive(Debug, Clone)]
pub struct ParsedSubscription {
    /// Subscription ID
    pub id: String,

    /// Subscription name
    pub name: String,

    /// Selected fields
    pub fields: Vec<String>,

    /// Filter conditions (if any)
    pub filter: Option<Value>,

    /// Variables
    pub variables: HashMap<String, Value>,
}

impl GraphQLMessage {
    /// Get message type name
    #[must_use] 
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::ConnectionInit { .. } => "connection_init",
            Self::ConnectionAck { .. } => "connection_ack",
            Self::Subscribe { .. } => "subscribe",
            Self::Next { .. } => "next",
            Self::Error { .. } => "error",
            Self::Complete { .. } => "complete",
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
        }
    }

    /// Check if this is a connection initialization message
    #[must_use] 
    pub const fn is_connection_init(&self) -> bool {
        matches!(self, Self::ConnectionInit { .. })
    }

    /// Check if this is a subscription message
    #[must_use] 
    pub const fn is_subscribe(&self) -> bool {
        matches!(self, Self::Subscribe { .. })
    }

    /// Create a connection init message
    #[must_use] 
    pub const fn connection_init(payload: Option<Value>) -> Self {
        Self::ConnectionInit { payload }
    }

    /// Create a connection ack message
    #[must_use] 
    pub const fn connection_ack(payload: Option<Value>) -> Self {
        Self::ConnectionAck { payload }
    }

    /// Create a next (data) message
    #[must_use] 
    pub const fn next(id: String, payload: Value) -> Self {
        Self::Next { id, payload }
    }

    /// Create an error message
    #[must_use] 
    pub fn error(id: String, message: String) -> Self {
        Self::Error {
            id,
            payload: vec![GraphQLError {
                message,
                locations: None,
                path: None,
                extensions: None,
            }],
        }
    }

    /// Create a complete message
    #[must_use] 
    pub const fn complete(id: String) -> Self {
        Self::Complete { id }
    }

    /// Create a ping message
    #[must_use] 
    pub const fn ping(payload: Option<Value>) -> Self {
        Self::Ping { payload }
    }

    /// Create a pong message
    #[must_use] 
    pub const fn pong(payload: Option<Value>) -> Self {
        Self::Pong { payload }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_connection_init_serialization() {
        let msg = GraphQLMessage::connection_init(Some(json!({"token": "xyz"})));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("connection_init"));
        assert!(json.contains("token"));
    }

    #[test]
    fn test_subscribe_serialization() {
        let msg = GraphQLMessage::Subscribe {
            id: "sub-1".to_string(),
            payload: SubscriptionPayload {
                query: "subscription { messageAdded { id } }".to_string(),
                operation_name: None,
                variables: None,
                extensions: None,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribe"));
        assert!(json.contains("sub-1"));
    }

    #[test]
    fn test_message_type_name() {
        assert_eq!(
            GraphQLMessage::connection_init(None).type_name(),
            "connection_init"
        );
        assert_eq!(GraphQLMessage::ping(None).type_name(), "ping");
        assert_eq!(GraphQLMessage::pong(None).type_name(), "pong");
    }

    #[test]
    fn test_message_type_checks() {
        let msg = GraphQLMessage::connection_init(None);
        assert!(msg.is_connection_init());
        assert!(!msg.is_subscribe());
    }

    #[test]
    fn test_error_message_creation() {
        let msg = GraphQLMessage::error("sub-1".to_string(), "Query error".to_string());
        match msg {
            GraphQLMessage::Error { id, payload } => {
                assert_eq!(id, "sub-1");
                assert_eq!(payload[0].message, "Query error");
            }
            _ => panic!("Expected error message"),
        }
    }
}
