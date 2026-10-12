//! GraphQL request and response types.

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// GraphQL request payload (for POST requests).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    /// GraphQL query string (optional when using APQ with hash-only request).
    #[serde(default)]
    pub query: Option<String>,

    /// Query variables (optional).
    #[serde(default)]
    pub variables: Option<serde_json::Value>,

    /// Operation name (optional).
    #[serde(default)]
    pub operation_name: Option<String>,

    /// Protocol extensions (APQ, tracing, etc.).
    #[serde(default)]
    pub extensions: Option<serde_json::Value>,

    /// Trusted document identifier (GraphQL over HTTP spec).
    #[serde(default, rename = "documentId")]
    pub document_id: Option<String>,
}

/// GraphQL GET request parameters.
///
/// Per GraphQL over HTTP spec, GET requests encode parameters in the query string:
/// - `query`: the GraphQL query string; optional when `extensions` names a persisted query
/// - `variables`: Optional, JSON-encoded object
/// - `operationName`: Optional, name of the operation to execute
/// - `extensions`: Optional, JSON-encoded object — an APQ hash
///   (`{"persistedQuery":{"version":1,"sha256Hash":"…"}}`) lets a CDN cache the request by its URL
///   (#1564)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLGetParams {
    /// GraphQL query string; absent when `extensions` names a persisted query.
    #[serde(default)]
    pub query: Option<String>,

    /// Query variables as JSON-encoded string (optional).
    #[serde(default)]
    pub variables: Option<String>,

    /// Operation name (optional).
    #[serde(default)]
    pub operation_name: Option<String>,

    /// Protocol extensions as a JSON-encoded string (optional).
    #[serde(default)]
    pub extensions: Option<String>,
}

/// GraphQL response payload.
#[derive(Debug, Serialize)]
pub struct GraphQLResponse {
    /// Response data or errors.
    #[serde(flatten)]
    pub body: serde_json::Value,
}

impl IntoResponse for GraphQLResponse {
    fn into_response(self) -> Response {
        Json(self.body).into_response()
    }
}
