//! Function operations endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/functions/*` expose deployed function listing,
//! invocation, log retrieval, and secrets management. All routes are
//! protected by the admin bearer token middleware.

use axum::{
    Json,
    extract::{Path, State},
    response::Response,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::{graphql::app_state::AppState, studio::not_implemented};

// ---------------------------------------------------------------------------
// Function record
// ---------------------------------------------------------------------------

/// A deployed function summary agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionEntry {
    /// Function name / identifier.
    pub name:        String,
    /// Deployment version number.
    pub version:     u32,
    /// Runtime type (e.g. `"wasm"`, `"deno"`).
    pub runtime:     String,
    /// Deployment status (`"active"`, `"inactive"`, `"error"`).
    pub status:      String,
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
    pub value:       serde_json::Value,
    /// Captured log lines from the invocation.
    pub logs:        Vec<String>,
    /// Wall-clock duration of the invocation in milliseconds.
    pub duration_ms: u64,
}

/// A single invocation log entry (ring-buffer record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationLogEntry {
    /// Invocation outcome (`"ok"` or `"error"`).
    pub status:      String,
    /// Duration of this invocation in milliseconds.
    pub duration_ms: u64,
    /// Error message, if `status == "error"`.
    pub error:       Option<String>,
    /// Invocation timestamp (RFC 3339).
    pub timestamp:   String,
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
/// Answers `501`: the function registry is not exposed here. It used to answer
/// `{"functions": []}`, which reads as "nothing is deployed" — false for every
/// deployment that declares functions.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `501` — see above.
pub async fn list_functions_handler<A>(State(_state): State<AppState<A>>) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.functions.list",
        "The deployed-function registry is not exposed through the admin API; an \
         empty list here would not mean no functions are deployed.",
    )
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
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.functions.invoke",
        "Ad-hoc function invocation is not exposed through the admin API. Use \
         `fraiseql functions invoke`.",
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
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.functions.logs",
        "No invocation-log ring buffer is exposed through the admin API; an empty \
         list here would not mean the function has never run.",
    )
}

/// `GET /admin/v1/functions/{name}/secrets` — secret key names (values never returned).
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn list_secrets_handler<A>(
    Path(_name): Path<String>,
    State(_state): State<AppState<A>>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.functions.secrets.list",
        "Function secrets are not managed through the admin API; an empty key list \
         here would not mean the function holds no secrets.",
    )
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
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // #749: this answered `{"success": true}` under a doc claiming the value was
    // "encrypted and stored server-side", storing nothing — so a credential rotation
    // reported success while the function kept using the leaked secret.
    not_implemented(
        "studio.functions.secrets.set",
        "Function secrets are not writable through the admin API; nothing was \
         stored. Manage them through the configured secrets backend.",
    )
}

/// `DELETE /admin/v1/functions/{name}/secrets/{key}` — delete a secret.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn delete_secret_handler<A>(
    Path((_name, _key)): Path<(String, String)>,
    State(_state): State<AppState<A>>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.functions.secrets.delete",
        "Function secrets are not deletable through the admin API; nothing was \
         removed. Manage them through the configured secrets backend.",
    )
}
