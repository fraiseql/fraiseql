//! Arrow Flight exchange protocol for bidirectional streaming.
//!
//! This module defines the protocol for bi-directional streaming via `do_exchange()`,
//! enabling request/response correlation and supporting multiple operation types
//! in a single stream.

use serde::{Deserialize, Serialize};

/// Maximum byte size accepted for a `do_exchange` message payload.
///
/// Exchange messages carry GraphQL queries and variables in `app_metadata`.
/// 256 `KiB` matches the Flight ticket cap and prevents deeply-nested JSON from
/// exhausting parser memory in `serde_json::from_slice`.
const MAX_EXCHANGE_MESSAGE_BYTES: usize = 256 * 1024; // 256 KiB

/// Exchange message wrapper for `do_exchange()` streaming.
///
/// Encapsulates requests, responses, and control messages with correlation IDs
/// to match requests to responses in a bidirectional stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExchangeMessage {
    /// Client request with correlation ID for tracking.
    ///
    /// The `correlation_id` is used to match this request to its response(s).
    /// Multiple requests can be in-flight simultaneously.
    Request {
        /// Unique identifier for this request (UUID recommended)
        correlation_id: String,
        /// The operation to perform
        request_type:   RequestType,
    },

    /// Server response with correlation ID.
    ///
    /// Response is correlated to request via `correlation_id`.
    Response {
        /// Matches the `correlation_id` from the original request
        correlation_id: String,
        /// Result of the operation (error or Arrow-encoded data)
        result:         Result<Vec<u8>, String>,
    },

    /// Stream completion signal.
    ///
    /// Indicates the sender has no more messages to send.
    /// Receiving this does NOT mean the stream is closed, just that this
    /// direction of the stream is done.
    Complete {
        /// For informational purposes (often empty or `correlation_id` of last message)
        correlation_id: String,
    },
}

/// Request types for exchange protocol.
///
/// Each request type has different semantics and response format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RequestType {
    /// Execute a GraphQL query.
    ///
    /// Returns Arrow-encoded result of the query in the Response.result field.
    /// Result is serialized as a `RecordBatch` in Arrow IPC format.
    Query {
        /// GraphQL query string
        query:     String,
        /// Optional GraphQL variables as JSON
        variables: Option<serde_json::Value>,
    },

    /// Upload a batch of data (similar to `do_put` but in exchange context).
    ///
    /// Inserts data into the specified table. The batch field contains
    /// a pre-encoded Arrow `RecordBatch` in IPC format.
    ///
    /// Response contains operation status and affected row count.
    Upload {
        /// Target table name
        table: String,
        /// Serialized Arrow `RecordBatch` (IPC format)
        batch: Vec<u8>,
    },

    /// Subscribe to entity change events (FUTURE).
    ///
    /// Currently unimplemented. When implemented, will stream change events
    /// matching the specified `entity_type` and optional filter.
    Subscribe {
        /// Entity type to subscribe to (e.g., "Order", "User")
        entity_type: String,
        /// Optional filter predicate (format TBD)
        filter:      Option<String>,
    },
}

impl ExchangeMessage {
    /// Serialize message to JSON bytes for transmission in `FlightData.app_metadata`.
    ///
    /// # Errors
    ///
    /// Returns error if JSON serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("Failed to serialize exchange message: {}", e))
    }

    /// Deserialize message from JSON bytes received in `FlightData.app_metadata`.
    ///
    /// # Errors
    ///
    /// Returns error if the bytes exceed `MAX_EXCHANGE_MESSAGE_BYTES`, JSON
    /// deserialization fails, or the format is invalid.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_EXCHANGE_MESSAGE_BYTES {
            return Err(format!(
                "Exchange message too large ({} bytes, max {MAX_EXCHANGE_MESSAGE_BYTES})",
                bytes.len()
            ));
        }
        serde_json::from_slice(bytes)
            .map_err(|e| format!("Failed to deserialize exchange message: {}", e))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    #[test]
    fn test_query_request_serialization() {
        let msg = ExchangeMessage::Request {
            correlation_id: "req-1".to_string(),
            request_type:   RequestType::Query {
                query:     "{ orders { id total } }".to_string(),
                variables: None,
            },
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Request {
                correlation_id,
                request_type: RequestType::Query { query, variables },
            } => {
                assert_eq!(correlation_id, "req-1");
                assert_eq!(query, "{ orders { id total } }");
                assert!(variables.is_none());
            },
            _ => panic!("Expected Query request"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let msg = ExchangeMessage::Response {
            correlation_id: "req-1".to_string(),
            result:         Ok(vec![1, 2, 3, 4]),
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Response {
                correlation_id,
                result,
            } => {
                assert_eq!(correlation_id, "req-1");
                assert_eq!(result, Ok(vec![1, 2, 3, 4]));
            },
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_error_response_serialization() {
        let msg = ExchangeMessage::Response {
            correlation_id: "req-1".to_string(),
            result:         Err("Database error".to_string()),
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Response {
                correlation_id,
                result,
            } => {
                assert_eq!(correlation_id, "req-1");
                assert_eq!(result, Err("Database error".to_string()));
            },
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_complete_serialization() {
        let msg = ExchangeMessage::Complete {
            correlation_id: "stream-complete".to_string(),
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Complete { correlation_id } => {
                assert_eq!(correlation_id, "stream-complete");
            },
            _ => panic!("Expected Complete"),
        }
    }

    #[test]
    fn test_upload_request_serialization() {
        let batch_data = vec![1, 2, 3, 4, 5];
        let msg = ExchangeMessage::Request {
            correlation_id: "upload-1".to_string(),
            request_type:   RequestType::Upload {
                table: "orders".to_string(),
                batch: batch_data.clone(),
            },
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Request {
                correlation_id,
                request_type: RequestType::Upload { table, batch },
            } => {
                assert_eq!(correlation_id, "upload-1");
                assert_eq!(table, "orders");
                assert_eq!(batch, batch_data);
            },
            _ => panic!("Expected Upload request"),
        }
    }

    #[test]
    fn test_query_with_variables_serialization() {
        let variables = serde_json::json!({
            "customerId": 123,
            "status": "pending"
        });

        let msg = ExchangeMessage::Request {
            correlation_id: "query-with-vars".to_string(),
            request_type: RequestType::Query {
                query: "query($customerId: ID!, $status: String) { orders(customerId: $customerId, status: $status) { id } }"
                    .to_string(),
                variables: Some(variables.clone()),
            },
        };

        let bytes = msg.to_json_bytes().expect("Failed to serialize");
        let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

        match deserialized {
            ExchangeMessage::Request {
                correlation_id,
                request_type:
                    RequestType::Query {
                        query,
                        variables: Some(vars),
                    },
            } => {
                assert_eq!(correlation_id, "query-with-vars");
                assert!(query.contains("customerId"));
                assert_eq!(vars, variables);
            },
            _ => panic!("Expected Query request with variables"),
        }
    }

    #[test]
    fn test_exchange_message_at_size_limit_is_rejected_as_json_not_size() {
        // Exactly at MAX_EXCHANGE_MESSAGE_BYTES: size check passes, JSON parse fails.
        let bytes = vec![b'{'; MAX_EXCHANGE_MESSAGE_BYTES];
        let result = ExchangeMessage::from_json_bytes(&bytes);
        assert!(result.is_err(), "expected Err, got: {result:?}");
        let msg = result.unwrap_err();
        assert!(!msg.contains("too large"), "Should fail JSON parsing, not size limit: {msg}");
    }

    #[test]
    fn test_exchange_message_exceeding_size_limit_is_rejected() {
        let oversized = vec![b'x'; MAX_EXCHANGE_MESSAGE_BYTES + 1];
        let result = ExchangeMessage::from_json_bytes(&oversized);
        assert!(result.is_err(), "expected Err, got: {result:?}");
        let msg = result.unwrap_err();
        assert!(msg.contains("too large"), "Expected size-limit error, got: {msg}");
    }
}
