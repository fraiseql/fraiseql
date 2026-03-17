//! GraphQL wire types.

use serde::{Deserialize, Serialize};

/// A GraphQL request body.
#[derive(Debug, Serialize)]
pub struct GraphQLRequest<'a> {
    /// The GraphQL query string.
    pub query: &'a str,
    /// Optional query variables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<&'a serde_json::Value>,
}

/// A GraphQL response body.
#[derive(Debug, Deserialize)]
pub struct GraphQLResponse<T> {
    /// The response data (present on success).
    pub data: Option<T>,
    /// GraphQL protocol errors (null or absent = success).
    pub errors: Option<Vec<GraphQLErrorEntry>>,
}

/// One entry in the GraphQL errors array.
#[derive(Debug, Deserialize, Clone)]
pub struct GraphQLErrorEntry {
    /// Human-readable error message.
    pub message: String,
    /// Optional location in the query.
    #[serde(default)]
    pub locations: Vec<GraphQLErrorLocation>,
    /// Optional path in the response.
    #[serde(default)]
    pub path: Vec<serde_json::Value>,
    /// Optional extensions map.
    pub extensions: Option<serde_json::Value>,
}

/// Line/column location in a GraphQL document.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct GraphQLErrorLocation {
    /// Line number (1-indexed).
    pub line: u32,
    /// Column number (1-indexed).
    pub column: u32,
}
