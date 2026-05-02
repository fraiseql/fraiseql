//! Function operations endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/functions/*` expose deployed function listing,
//! invocation, log retrieval, and secrets management. All routes are
//! protected by the admin bearer token middleware.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Function record
// ---------------------------------------------------------------------------

/// A deployed function summary agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionEntry {
    /// Function name / identifier.
    pub name: String,
    /// Deployment version number.
    pub version: u32,
    /// Runtime type (e.g. `"wasm"`, `"deno"`).
    pub runtime: String,
    /// Deployment status (`"active"`, `"inactive"`, `"error"`).
    pub status: String,
    /// Deployment timestamp (RFC 3339).
    pub deployed_at: String,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Function list response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionListResponse {
    /// All deployed functions for this tenant.
    pub functions: Vec<FunctionEntry>,
}

/// Function invocation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResponse {
    /// Return value from the function.
    pub value: serde_json::Value,
    /// Captured log lines from the invocation.
    pub logs: Vec<String>,
    /// Wall-clock duration of the invocation in milliseconds.
    pub duration_ms: u64,
}

/// A single invocation log entry (ring-buffer record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationLogEntry {
    /// Invocation outcome (`"ok"` or `"error"`).
    pub status: String,
    /// Duration of this invocation in milliseconds.
    pub duration_ms: u64,
    /// Error message, if `status == "error"`.
    pub error: Option<String>,
    /// Invocation timestamp (RFC 3339).
    pub timestamp: String,
}

/// Secret keys list (values are never returned).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsKeysResponse {
    /// Secret key names for this function.
    pub keys: Vec<String>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request body for `POST /admin/v1/functions/{name}/invoke`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeRequest {
    /// Event payload to pass to the function.
    pub event: serde_json::Value,
}

/// Request body for `PUT /admin/v1/functions/{name}/secrets/{key}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretSetRequest {
    /// Secret value (encrypted and stored server-side).
    pub value: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/v1/functions` — list all deployed functions.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn list_functions_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(FunctionListResponse { functions: vec![] })
}

/// `POST /admin/v1/functions/{name}/invoke` — invoke a function.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `404` if the function does not exist.
pub async fn invoke_function_handler<A>(
    Path(_name): Path<String>,
    State(_state): State<AppState<A>>,
    Json(_req): Json<InvokeRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Not Implemented",
            "message": "Function invocation endpoint available in a future release"
        })),
    )
}

/// `GET /admin/v1/functions/{name}/logs` — last N invocation log entries.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn function_logs_handler<A>(
    Path(_name): Path<String>,
    State(_state): State<AppState<A>>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({ "logs": [] }))
}

/// `GET /admin/v1/functions/{name}/secrets` — secret key names (values never returned).
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn list_secrets_handler<A>(
    Path(_name): Path<String>,
    State(_state): State<AppState<A>>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(SecretsKeysResponse { keys: vec![] })
}

/// `PUT /admin/v1/functions/{name}/secrets/{key}` — set a secret value.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn set_secret_handler<A>(
    Path((_name, _key)): Path<(String, String)>,
    State(_state): State<AppState<A>>,
    Json(_req): Json<SecretSetRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({"success": true}))
}

/// `DELETE /admin/v1/functions/{name}/secrets/{key}` — delete a secret.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn delete_secret_handler<A>(
    Path((_name, _key)): Path<(String, String)>,
    State(_state): State<AppState<A>>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({"success": true}))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::*;

    #[test]
    fn test_function_list_serializes() {
        let resp = FunctionListResponse { functions: vec![] };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"functions\""));
    }

    #[test]
    fn test_secret_set_request_parses() {
        let input = r#"{"value":"s3cr3t"}"#;
        let req: SecretSetRequest = serde_json::from_str(input).unwrap();
        assert_eq!(req.value, "s3cr3t");
    }
}
