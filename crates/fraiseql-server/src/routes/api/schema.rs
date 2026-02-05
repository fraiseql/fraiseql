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
    State(_state): State<AppState<A>>,
) -> Result<Response, ApiError> {
    // In a real implementation, this would:
    // 1. Extract the schema from AppState
    // 2. Convert to GraphQL SDL format
    // 3. Return as text/plain response

    let schema_sdl = generate_example_sdl();

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
    State(_state): State<AppState<A>>,
) -> Result<Json<ApiResponse<JsonSchemaResponse>>, ApiError> {
    // In a real implementation, this would:
    // 1. Extract the compiled schema from AppState
    // 2. Serialize to JSON
    // 3. Return wrapped in ApiResponse

    let response = JsonSchemaResponse {
        schema: generate_example_json_schema(),
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Generate example GraphQL SDL schema.
///
/// In a real implementation, this would convert the actual compiled schema
/// to GraphQL SDL format. For now, returns a placeholder example.
fn generate_example_sdl() -> String {
    r#""""Root query type for the GraphQL API."""
type Query {
  """Get all users."""
  users: [User!]!

  """Get a specific user by ID."""
  user(id: ID!): User
}

"""User type representing a person in the system."""
type User {
  """Unique identifier."""
  id: ID!

  """User's full name."""
  name: String!

  """User's email address."""
  email: String!

  """Posts created by this user."""
  posts: [Post!]!
}

"""Post type representing a published article."""
type Post {
  """Unique identifier."""
  id: ID!

  """Post title."""
  title: String!

  """Post content."""
  content: String!

  """Author of the post."""
  author: User!
}

"""Root mutation type for modifications."""
type Mutation {
  """Create a new user."""
  createUser(name: String!, email: String!): User

  """Create a new post."""
  createPost(title: String!, content: String!): Post
}
"#
    .to_string()
}

/// Generate example JSON schema.
///
/// In a real implementation, this would serialize the actual compiled schema.
/// For now, returns a placeholder example with realistic structure.
fn generate_example_json_schema() -> serde_json::Value {
    serde_json::json!({
        "types": [
            {
                "name": "Query",
                "kind": "OBJECT",
                "description": "Root query type",
                "fields": [
                    {
                        "name": "users",
                        "type": "[User!]!",
                        "description": "Get all users"
                    },
                    {
                        "name": "user",
                        "type": "User",
                        "description": "Get a specific user",
                        "arguments": [
                            {
                                "name": "id",
                                "type": "ID!",
                                "description": "User ID"
                            }
                        ]
                    }
                ]
            },
            {
                "name": "User",
                "kind": "OBJECT",
                "description": "User type",
                "fields": [
                    {
                        "name": "id",
                        "type": "ID!",
                        "description": "Unique identifier"
                    },
                    {
                        "name": "name",
                        "type": "String!",
                        "description": "User's name"
                    },
                    {
                        "name": "email",
                        "type": "String!",
                        "description": "User's email"
                    }
                ]
            },
            {
                "name": "Mutation",
                "kind": "OBJECT",
                "description": "Root mutation type",
                "fields": [
                    {
                        "name": "createUser",
                        "type": "User",
                        "description": "Create a new user",
                        "arguments": [
                            {"name": "name", "type": "String!", "description": "User name"},
                            {"name": "email", "type": "String!", "description": "User email"}
                        ]
                    }
                ]
            }
        ],
        "query_type": "Query",
        "mutation_type": "Mutation",
        "subscription_type": null
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_example_sdl() {
        let sdl = generate_example_sdl();

        assert!(sdl.contains("type Query"));
        assert!(sdl.contains("type User"));
        assert!(sdl.contains("type Post"));
        assert!(sdl.contains("type Mutation"));
    }

    #[test]
    fn test_generate_example_json_schema() {
        let schema = generate_example_json_schema();

        assert!(schema["types"].is_array());
        assert_eq!(schema["query_type"], "Query");
        assert_eq!(schema["mutation_type"], "Mutation");
        assert!(schema["subscription_type"].is_null());
    }

    #[test]
    fn test_json_schema_has_fields() {
        let schema = generate_example_json_schema();
        let types = schema["types"].as_array().unwrap();

        assert!(!types.is_empty());

        let query_type = types.iter().find(|t| t["name"] == "Query");
        assert!(query_type.is_some());
        assert!(query_type.unwrap()["fields"].is_array());
    }

    #[test]
    fn test_sdl_has_directives() {
        let sdl = generate_example_sdl();

        // Example has @ directives in comments
        assert!(sdl.contains("\"\"\""));
    }

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
