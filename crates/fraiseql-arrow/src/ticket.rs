//! Flight ticket encoding and decoding.
//!
//! Flight tickets are opaque bytes that identify what data to fetch.
//! We use JSON encoding for human readability during development.

use crate::error::{ArrowFlightError, Result};
use serde::{Deserialize, Serialize};

/// Flight ticket identifying what data to fetch.
///
/// Tickets are serialized as JSON for human readability during development.
/// In production, a more compact format (protobuf, msgpack) could be used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum FlightTicket {
    /// GraphQL query result.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "GraphQLQuery",
    ///   "query": "{ users { id name } }",
    ///   "variables": null
    /// }
    /// ```
    GraphQLQuery {
        /// GraphQL query string
        query: String,
        /// Optional query variables
        variables: Option<serde_json::Value>,
    },

    /// Observer events stream.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "ObserverEvents",
    ///   "entity_type": "Order",
    ///   "start_date": "2026-01-01",
    ///   "limit": 10000
    /// }
    /// ```
    ObserverEvents {
        /// Entity type to filter (e.g., "Order", "User")
        entity_type: String,
        /// Start date filter (ISO 8601 format)
        start_date: Option<String>,
        /// End date filter (ISO 8601 format)
        end_date: Option<String>,
        /// Maximum number of events to return
        limit: Option<usize>,
    },

    /// Bulk data export.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "BulkExport",
    ///   "table": "users",
    ///   "limit": 1000000
    /// }
    /// ```
    BulkExport {
        /// Table name to export
        table: String,
        /// Optional filter condition
        filter: Option<String>,
        /// Maximum number of rows
        limit: Option<usize>,
    },
}

impl FlightTicket {
    /// Encode ticket as bytes for Flight protocol.
    ///
    /// # Errors
    ///
    /// Returns `Err` if JSON serialization fails.
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Decode ticket from bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the bytes are not valid JSON or don't match the ticket schema.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            ArrowFlightError::InvalidTicket(format!("Failed to parse ticket: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_query_ticket_roundtrip() {
        let ticket = FlightTicket::GraphQLQuery {
            query: "{ users { id } }".to_string(),
            variables: None,
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        assert_eq!(ticket, decoded);
    }

    #[test]
    fn test_graphql_query_with_variables() {
        let ticket = FlightTicket::GraphQLQuery {
            query: "query($id: ID!) { user(id: $id) { name } }".to_string(),
            variables: Some(serde_json::json!({"id": "123"})),
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        match decoded {
            FlightTicket::GraphQLQuery { query, variables } => {
                assert_eq!(query, "query($id: ID!) { user(id: $id) { name } }");
                assert_eq!(variables, Some(serde_json::json!({"id": "123"})));
            }
            _ => panic!("Wrong ticket type"),
        }
    }

    #[test]
    fn test_observer_events_ticket_roundtrip() {
        let ticket = FlightTicket::ObserverEvents {
            entity_type: "Order".to_string(),
            start_date: Some("2026-01-01".to_string()),
            end_date: Some("2026-01-31".to_string()),
            limit: Some(10_000),
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        match decoded {
            FlightTicket::ObserverEvents {
                entity_type,
                limit,
                ..
            } => {
                assert_eq!(entity_type, "Order");
                assert_eq!(limit, Some(10_000));
            }
            _ => panic!("Wrong ticket type"),
        }
    }

    #[test]
    fn test_bulk_export_ticket() {
        let ticket = FlightTicket::BulkExport {
            table: "users".to_string(),
            filter: Some("active = true".to_string()),
            limit: Some(1_000_000),
        };

        let bytes = ticket.encode().unwrap();
        let decoded = FlightTicket::decode(&bytes).unwrap();

        assert_eq!(ticket, decoded);
    }

    #[test]
    fn test_invalid_ticket_returns_error() {
        let invalid_json = b"not valid json";
        let result = FlightTicket::decode(invalid_json);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ArrowFlightError::InvalidTicket(_)
        ));
    }
}
