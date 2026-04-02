//! Canonical GraphQL protocol error types.
//!
//! Implements the [GraphQL specification](https://spec.graphql.org/October2021/#sec-Errors)
//! error format used across the FraiseQL workspace — `WebSocket` subscription protocol
//! (graphql-ws v5+), HTTP response bodies, and federation subgraph communication.
//!
//! # Structure
//!
//! ```json
//! {
//!   "message": "Cannot query field 'id' on type 'User'.",
//!   "locations": [{ "line": 3, "column": 5 }],
//!   "path": ["user", "friends", 0, "name"],
//!   "extensions": { "code": "FIELD_NOT_FOUND" }
//! }
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Location in a GraphQL document (source text) where an error occurred.
///
/// Line and column are 1-indexed as required by the GraphQL specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQLErrorLocation {
    /// Line number in the query document (1-indexed).
    pub line: u32,
    /// Column number in the query document (1-indexed).
    pub column: u32,
}

/// A GraphQL protocol-level error as defined in the
/// [GraphQL specification §7.1.2](https://spec.graphql.org/October2021/#sec-Errors).
///
/// This is the canonical wire format used by:
/// - `WebSocket` subscription protocol (`graphql-ws` v5+) `error` messages
/// - HTTP GraphQL response `errors` array
/// - Federation subgraph HTTP responses
///
/// Crates that need HTTP-layer concerns (error codes, status codes, sanitization)
/// build on top of this type rather than replacing it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Human-readable error message.
    pub message: String,

    /// Locations in the query document where the error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<GraphQLErrorLocation>>,

    /// Path into the response data where the error occurred.
    ///
    /// Elements are either string field names or integer list indices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<serde_json::Value>>,

    /// Arbitrary extension data (error codes, categories, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

impl GraphQLError {
    /// Create a simple error with only a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            locations: None,
            path: None,
            extensions: None,
        }
    }

    /// Create an error with a string extension code.
    ///
    /// This is the conventional way to attach a machine-readable error code:
    ///
    /// ```rust
    /// use fraiseql_error::GraphQLError;
    ///
    /// let err = GraphQLError::with_code("Subscription not found", "SUBSCRIPTION_NOT_FOUND");
    /// ```
    #[must_use]
    pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        let mut extensions = HashMap::new();
        extensions.insert("code".to_string(), serde_json::json!(code.into()));
        Self {
            message: message.into(),
            locations: None,
            path: None,
            extensions: Some(extensions),
        }
    }

    /// Add a source location to this error.
    #[must_use]
    pub fn with_location(mut self, line: u32, column: u32) -> Self {
        let loc = GraphQLErrorLocation { line, column };
        self.locations.get_or_insert_with(Vec::new).push(loc);
        self
    }

    /// Add a response path to this error.
    #[must_use]
    pub fn with_path(mut self, path: Vec<serde_json::Value>) -> Self {
        self.path = Some(path);
        self
    }
}
