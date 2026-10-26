//! Schema export API endpoints.
//!
//! Provides endpoints for:
//! - Exporting compiled schema as GraphQL SDL (Schema Definition Language)
//! - Exporting schema as JSON for programmatic access
//!
//! Both describe the schema as the caller may see it, as GraphQL introspection does: a
//! `requires_role` operation or type is left out for a caller without the role. The
//! caller is the one the authentication layer admitted, so without
//! `schema_export_require_auth` every caller is anonymous and is answered the operations
//! no role gates. Its identity is resolved as every transport resolves it, since
//! enrichment can add roles and refuses a subject it does not know.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fraiseql_core::security::SecurityContext;
use serde::Serialize;

use crate::{
    extractors::OptionalSecurityContext,
    routes::{
        api::types::{ApiError, ApiResponse},
        graphql::AppState,
    },
};

/// Response containing GraphQL SDL schema.
#[derive(Debug, Serialize)]
pub struct GraphQLSchemaResponse {
    /// GraphQL Schema Definition Language (SDL) representation
    pub schema: String,
}

/// Response containing JSON-formatted schema.
#[derive(Debug, Serialize)]
pub struct JsonSchemaResponse {
    /// Compiled schema as JSON object
    pub schema: serde_json::Value,
}

/// The caller with its identity resolved, or the refusal every transport answers a
/// denied subject (403) or a resolver outage (503) with.
#[cfg_attr(not(feature = "auth"), allow(clippy::unused_async))] // Reason: resolution only exists with `auth`
async fn resolved_caller(
    state: &AppState,
    caller: Option<SecurityContext>,
) -> Result<Option<SecurityContext>, ApiError> {
    #[cfg(feature = "auth")]
    {
        let mut caller = caller;
        match crate::identity::resolve_request_identity(&state.identity, caller.as_mut()).await {
            crate::identity::EnrichmentOutcome::Proceed => Ok(caller),
            crate::identity::EnrichmentOutcome::Denied => {
                Err(ApiError::new(crate::identity::EnrichmentOutcome::DENIED_MESSAGE, "FORBIDDEN"))
            },
            crate::identity::EnrichmentOutcome::Unavailable => Err(ApiError::new(
                crate::identity::EnrichmentOutcome::UNAVAILABLE_MESSAGE,
                "SERVICE_UNAVAILABLE",
            )),
        }
    }
    #[cfg(not(feature = "auth"))]
    {
        let _ = state;
        Ok(caller)
    }
}

/// Export compiled schema as GraphQL SDL.
///
/// Returns the schema in GraphQL Schema Definition Language (SDL) format.
/// This is human-readable and suitable for documentation, tools, and introspection.
///
/// Response format: `text/plain` (not JSON wrapped)
///
/// # Errors
///
/// Returns `ApiError` when the caller's identity is denied or cannot be resolved.
pub async fn export_sdl_handler(
    State(state): State<AppState>,
    OptionalSecurityContext(caller): OptionalSecurityContext,
) -> Result<Response, ApiError> {
    let caller = resolved_caller(&state, caller).await?;
    let schema_sdl = state.executor().schema_for(caller.as_ref()).raw_schema();
    Ok((StatusCode::OK, schema_sdl).into_response())
}

/// Export compiled schema as JSON.
///
/// Returns the full compiled schema in JSON format.
/// This includes type information, field definitions, and metadata.
/// Useful for programmatic access and tooling.
///
/// Response format: Standard JSON API response with data wrapper
///
/// # Errors
///
/// Returns `ApiError` when the caller's identity is denied or cannot be resolved, or with
/// an internal error if the schema cannot be serialized to JSON.
pub async fn export_json_handler(
    State(state): State<AppState>,
    OptionalSecurityContext(caller): OptionalSecurityContext,
) -> Result<Json<ApiResponse<JsonSchemaResponse>>, ApiError> {
    let caller = resolved_caller(&state, caller).await?;
    let schema_json = serde_json::to_value(state.executor().schema_for(caller.as_ref()))
        .map_err(|e| ApiError::internal_error(format!("Failed to serialize schema: {e}")))?;

    let response = JsonSchemaResponse {
        schema: schema_json,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}
