//! Schema export API endpoints.
//!
//! Provides endpoints for:
//! - Exporting compiled schema as GraphQL SDL
//! - Exporting schema as JSON

use axum::{
    extract::State,
    Json,
    response::{Response, IntoResponse},
    http::StatusCode,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use crate::routes::{api::types::{ApiResponse, ApiError}, graphql::AppState};

/// Response containing GraphQL SDL schema.
#[derive(Debug, Serialize)]
pub struct GraphQLSchemaResponse {
    pub schema: String,
}

/// Response containing JSON schema.
#[derive(Debug, Serialize)]
pub struct JsonSchemaResponse {
    pub schema: serde_json::Value,
}

/// Export compiled schema as GraphQL SDL.
pub async fn export_sdl_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
) -> Result<Response, ApiError> {
    // Placeholder implementation
    let schema = "type Query { hello: String }";

    Ok((StatusCode::OK, schema).into_response())
}

/// Export compiled schema as JSON.
pub async fn export_json_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
) -> Result<Json<ApiResponse<JsonSchemaResponse>>, ApiError> {
    // Placeholder implementation
    let response = JsonSchemaResponse {
        schema: serde_json::json!({}),
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}
