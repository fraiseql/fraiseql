//! Public type definitions for FraiseQL API

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request for executing a GraphQL query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// GraphQL query string
    pub query: String,
    /// GraphQL variables as JSON values
    pub variables: HashMap<String, serde_json::Value>,
    /// Operation name (optional, required if multiple operations in query)
    pub operation_name: Option<String>,
}

/// Request for executing a GraphQL mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationRequest {
    /// GraphQL mutation string
    pub mutation: String,
    /// GraphQL variables as JSON values
    pub variables: HashMap<String, serde_json::Value>,
}

/// GraphQL response from query or mutation execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLResponse {
    /// Query/mutation result data (None if errors occurred)
    pub data: Option<serde_json::Value>,
    /// GraphQL errors (if any)
    pub errors: Option<Vec<GraphQLError>>,
    /// Extensions (metadata, timing, etc.)
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

/// GraphQL error as per GraphQL spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Error message
    pub message: String,
    /// Source location in query where error occurred
    pub locations: Option<Vec<SourceLocation>>,
    /// Path to the field that caused the error
    pub path: Option<Vec<PathElement>>,
}

/// Source location in a GraphQL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub column: u32,
}

/// Path element (field name or array index)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathElement {
    /// Field name
    Field(String),
    /// Array index
    Index(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_request() {
        let req = QueryRequest {
            query: "{ users { id } }".to_string(),
            variables: HashMap::new(),
            operation_name: None,
        };
        assert_eq!(req.query, "{ users { id } }");
    }

    #[test]
    fn test_graphql_response() {
        let resp = GraphQLResponse {
            data: Some(serde_json::json!({"users": []})),
            errors: None,
            extensions: None,
        };
        assert!(resp.data.is_some());
        assert!(resp.errors.is_none());
    }

    #[test]
    fn test_graphql_error() {
        let err = GraphQLError {
            message: "test error".to_string(),
            locations: None,
            path: None,
        };
        assert_eq!(err.message, "test error");
    }
}
