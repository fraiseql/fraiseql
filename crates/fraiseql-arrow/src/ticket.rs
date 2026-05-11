//! Flight ticket encoding and decoding.
//!
//! Flight tickets are opaque bytes that identify what data to fetch.
//! We use JSON encoding for human readability during development.

use serde::{Deserialize, Serialize};

use crate::error::{ArrowFlightError, Result};

/// Maximum byte size accepted for a Flight ticket payload.
///
/// A well-formed ticket (query string + variables JSON) is at most a few `KiB`.
/// 256 `KiB` is a generous cap that still prevents a client from sending a
/// pathologically deep JSON structure that exhausts the parser's stack/heap.
pub(crate) const MAX_FLIGHT_TICKET_BYTES: usize = 256 * 1024; // 256 KiB

/// Flight ticket identifying what data to fetch.
///
/// Tickets are serialized as JSON for human readability during development.
/// In production, a more compact format (protobuf, msgpack) could be used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
#[non_exhaustive]
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
        query:     String,
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
        start_date:  Option<String>,
        /// End date filter (ISO 8601 format)
        end_date:    Option<String>,
        /// Maximum number of events to return
        limit:       Option<usize>,
    },

    /// Optimized pre-compiled Arrow view.
    ///
    /// Uses compiler-generated `va_*` views for maximum performance.
    /// Pre-compiled Arrow schemas eliminate runtime type inference.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "OptimizedView",
    ///   "view": "va_orders",
    ///   "filter": "created_at > '2026-01-01'",
    ///   "limit": 100000
    /// }
    /// ```
    OptimizedView {
        /// View name (e.g., "`va_orders`", "`va_users`")
        view:     String,
        /// Optional WHERE clause filter
        filter:   Option<String>,
        /// Optional ORDER BY clause
        order_by: Option<String>,
        /// Maximum number of rows
        limit:    Option<usize>,
        /// Offset for pagination
        offset:   Option<usize>,
    },

    /// Bulk data export.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "BulkExport",
    ///   "table": "users",
    ///   "format": "parquet",
    ///   "limit": 1000000
    /// }
    /// ```
    BulkExport {
        /// Table name to export
        table:  String,
        /// Optional filter condition
        filter: Option<String>,
        /// Maximum number of rows
        limit:  Option<usize>,
        /// Export format: "parquet", "csv", or "json" (default: "parquet")
        format: Option<String>,
    },

    /// Batched queries for efficient bulk operations.
    ///
    /// Execute multiple SQL queries in a single request and receive
    /// all results as a combined Arrow stream. Improves throughput by
    /// 20-30% compared to sequential requests.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "BatchedQueries",
    ///   "queries": [
    ///     "SELECT * FROM ta_users LIMIT 100",
    ///     "SELECT * FROM ta_orders WHERE created_at > NOW() - INTERVAL '7 days' LIMIT 100"
    ///   ]
    /// }
    /// ```
    BatchedQueries {
        /// List of SQL queries to execute
        queries: Vec<String>,
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
    /// Returns `Err` if the bytes exceed `MAX_FLIGHT_TICKET_BYTES`, are not valid
    /// JSON, or don't match the ticket schema.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_FLIGHT_TICKET_BYTES {
            return Err(ArrowFlightError::InvalidTicket(format!(
                "Ticket too large ({} bytes, max {MAX_FLIGHT_TICKET_BYTES})",
                bytes.len()
            )));
        }
        serde_json::from_slice(bytes)
            .map_err(|e| ArrowFlightError::InvalidTicket(format!("Failed to parse ticket: {e}")))
    }
}

#[cfg(test)]
mod tests;

