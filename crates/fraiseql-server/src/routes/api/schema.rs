//! Schema export API endpoints.
//!
//! Provides endpoints for:
//! - Exporting compiled schema as GraphQL SDL (Schema Definition Language)
//! - Exporting schema as JSON for programmatic access

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;

use crate::routes::{
    api::types::{ApiError, ApiResponse},
    graphql::AppState,
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

/// Export compiled schema as GraphQL SDL.
///
/// Returns the schema in GraphQL Schema Definition Language (SDL) format.
/// This is human-readable and suitable for documentation, tools, and introspection.
///
/// Response format: `text/plain` (not JSON wrapped)
pub async fn export_sdl_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Response, ApiError> {
    let schema_sdl = state.executor.schema().raw_schema();
    Ok((StatusCode::OK, schema_sdl).into_response())
}

/// Export compiled schema as JSON.
///
/// Returns the full compiled schema in JSON format.
/// This includes type information, field definitions, and metadata.
/// Useful for programmatic access and tooling.
///
/// Response format: Standard JSON API response with data wrapper
pub async fn export_json_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<JsonSchemaResponse>>, ApiError> {
    let schema_json = serde_json::to_value(state.executor.schema()).map_err(|e| {
        ApiError::internal_error(format!("Failed to serialize schema: {e}"))
    })?;

    let response = JsonSchemaResponse {
        schema: schema_json,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_response_creation() {
        let response = GraphQLSchemaResponse {
            schema: "type Query { hello: String }".to_string(),
        };

        assert_eq!(response.schema, "type Query { hello: String }");
    }

    #[test]
    fn test_json_response_creation() {
        let response = JsonSchemaResponse {
            schema: serde_json::json!({"types": []}),
        };

        assert!(response.schema.is_object());
    }
}
