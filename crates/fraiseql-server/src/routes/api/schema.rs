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
/// Returns the schema in GraphQL Schema Definition Language (SDL) format,
/// generated from the actual compiled schema loaded by the server.
/// Includes type definitions, queries, mutations, subscriptions, and enums.
///
/// Response format: `text/plain` (not JSON wrapped)
pub async fn export_sdl_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Response, ApiError> {
    let schema = state.executor.schema();
    let sdl = generate_sdl_from_schema(schema);

    Ok((StatusCode::OK, sdl).into_response())
}

/// Export compiled schema as JSON.
///
/// Returns the full compiled schema in JSON format, serialized from the
/// actual loaded schema. Includes type information, field definitions,
/// queries, mutations, and metadata.
///
/// Response format: Standard JSON API response with data wrapper
pub async fn export_json_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<JsonSchemaResponse>>, ApiError> {
    let schema = state.executor.schema();

    let json_value = serde_json::to_value(schema).unwrap_or_else(|_| serde_json::json!({}));

    let response = JsonSchemaResponse { schema: json_value };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Generate GraphQL SDL from the compiled schema.
///
/// Uses the schema's stored SDL if available (from compilation), otherwise
/// builds an SDL representation from the type, query, mutation, enum, and
/// subscription definitions.
fn generate_sdl_from_schema(schema: &fraiseql_core::schema::CompiledSchema) -> String {
    // If the schema already has a raw SDL string, use it directly
    if let Some(ref sdl) = schema.schema_sdl {
        if !sdl.is_empty() {
            return sdl.clone();
        }
    }

    // Generate SDL from type definitions
    let mut sdl = String::new();

    // Enum definitions
    for enum_def in &schema.enums {
        if let Some(ref desc) = enum_def.description {
            sdl.push_str(&format!("\"\"\"{desc}\"\"\"\n"));
        }
        sdl.push_str(&format!("enum {} {{\n", enum_def.name));
        for value in &enum_def.values {
            if let Some(ref desc) = value.description {
                sdl.push_str(&format!("  \"\"\"{desc}\"\"\"\n"));
            }
            sdl.push_str(&format!("  {}\n", value.name));
        }
        sdl.push_str("}\n\n");
    }

    // Interface definitions
    for iface in &schema.interfaces {
        if let Some(ref desc) = iface.description {
            sdl.push_str(&format!("\"\"\"{desc}\"\"\"\n"));
        }
        sdl.push_str(&format!("interface {} {{\n", iface.name));
        for field in &iface.fields {
            sdl.push_str(&format!("  {}: {}\n", field.name, field.field_type));
        }
        sdl.push_str("}\n\n");
    }

    // Type definitions
    for type_def in &schema.types {
        if let Some(ref desc) = type_def.description {
            sdl.push_str(&format!("\"\"\"{desc}\"\"\"\n"));
        }
        sdl.push_str(&format!("type {}", type_def.name));
        if !type_def.implements.is_empty() {
            sdl.push_str(&format!(" implements {}", type_def.implements.join(" & ")));
        }
        sdl.push_str(" {\n");
        for field in &type_def.fields {
            if let Some(ref desc) = field.description {
                sdl.push_str(&format!("  \"\"\"{desc}\"\"\"\n"));
            }
            sdl.push_str(&format!("  {}: {}\n", field.name, field.field_type));
        }
        sdl.push_str("}\n\n");
    }

    // Input types
    for input in &schema.input_types {
        if let Some(ref desc) = input.description {
            sdl.push_str(&format!("\"\"\"{desc}\"\"\"\n"));
        }
        sdl.push_str(&format!("input {} {{\n", input.name));
        for field in &input.fields {
            sdl.push_str(&format!("  {}: {}\n", field.name, field.field_type));
        }
        sdl.push_str("}\n\n");
    }

    // Union definitions
    for union_def in &schema.unions {
        if let Some(ref desc) = union_def.description {
            sdl.push_str(&format!("\"\"\"{desc}\"\"\"\n"));
        }
        sdl.push_str(&format!("union {} = {}\n\n", union_def.name, union_def.member_types.join(" | ")));
    }

    // Query type
    if !schema.queries.is_empty() {
        sdl.push_str("type Query {\n");
        for query in &schema.queries {
            if let Some(ref desc) = query.description {
                sdl.push_str(&format!("  \"\"\"{desc}\"\"\"\n"));
            }
            let args = format_arguments(&query.arguments);
            let return_type = format_return_type(&query.return_type, query.returns_list, query.nullable);
            sdl.push_str(&format!("  {}{}: {}\n", query.name, args, return_type));
        }
        sdl.push_str("}\n\n");
    }

    // Mutation type
    if !schema.mutations.is_empty() {
        sdl.push_str("type Mutation {\n");
        for mutation in &schema.mutations {
            if let Some(ref desc) = mutation.description {
                sdl.push_str(&format!("  \"\"\"{desc}\"\"\"\n"));
            }
            let args = format_arguments(&mutation.arguments);
            sdl.push_str(&format!("  {}{}: {}\n", mutation.name, args, mutation.return_type));
        }
        sdl.push_str("}\n\n");
    }

    // Subscription type
    if !schema.subscriptions.is_empty() {
        sdl.push_str("type Subscription {\n");
        for sub in &schema.subscriptions {
            if let Some(ref desc) = sub.description {
                sdl.push_str(&format!("  \"\"\"{desc}\"\"\"\n"));
            }
            let args = format_arguments(&sub.arguments);
            sdl.push_str(&format!("  {}{}: {}\n", sub.name, args, sub.return_type));
        }
        sdl.push_str("}\n\n");
    }

    sdl
}

/// Format argument definitions for SDL output.
fn format_arguments(args: &[fraiseql_core::schema::ArgumentDefinition]) -> String {
    if args.is_empty() {
        return String::new();
    }

    let formatted: Vec<String> = args
        .iter()
        .map(|a| {
            let type_str = a.arg_type.to_string();
            if a.nullable {
                format!("{}: {}", a.name, type_str)
            } else {
                format!("{}: {}!", a.name, type_str)
            }
        })
        .collect();

    format!("({})", formatted.join(", "))
}

/// Format a return type with list and nullable wrapping.
fn format_return_type(type_name: &str, returns_list: bool, nullable: bool) -> String {
    let inner = if nullable {
        type_name.to_string()
    } else {
        format!("{type_name}!")
    };

    if returns_list {
        format!("[{inner}]!")
    } else {
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fraiseql_core::schema::{
        ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition,
        FieldType, QueryDefinition, TypeDefinition,
    };

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.types.push(
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition {
                    name:           "id".to_string(),
                    field_type:     FieldType::Scalar("ID".to_string()),
                    description:    Some("Unique identifier".to_string()),
                    nullable:       false,
                    default_value:  None,
                    vector_config:  None,
                    alias:          None,
                    deprecation:    None,
                    requires_scope: None,
                })
                .with_field(FieldDefinition {
                    name:           "name".to_string(),
                    field_type:     FieldType::Scalar("String".to_string()),
                    description:    None,
                    nullable:       false,
                    default_value:  None,
                    vector_config:  None,
                    alias:          None,
                    deprecation:    None,
                    requires_scope: None,
                }),
        );
        schema.queries.push(QueryDefinition::new("users", "User").returning_list());
        let mut user_query = QueryDefinition::new("user", "User");
        user_query.arguments.push(ArgumentDefinition::new(
            "id",
            FieldType::Scalar("ID".to_string()),
        ));
        schema.queries.push(user_query);
        schema
    }

    #[test]
    fn test_generate_sdl_includes_types() {
        let schema = test_schema();
        let sdl = generate_sdl_from_schema(&schema);

        assert!(sdl.contains("type User"));
        assert!(sdl.contains("id:"));
        assert!(sdl.contains("name:"));
    }

    #[test]
    fn test_generate_sdl_includes_queries() {
        let schema = test_schema();
        let sdl = generate_sdl_from_schema(&schema);

        assert!(sdl.contains("type Query"));
        assert!(sdl.contains("users:"));
        assert!(sdl.contains("user("));
    }

    #[test]
    fn test_generate_sdl_uses_stored_sdl_when_available() {
        let mut schema = CompiledSchema::new();
        schema.schema_sdl = Some("type Query { hello: String }".to_string());

        let sdl = generate_sdl_from_schema(&schema);
        assert_eq!(sdl, "type Query { hello: String }");
    }

    #[test]
    fn test_generate_sdl_with_enums() {
        let mut schema = CompiledSchema::new();
        schema.enums.push(
            EnumDefinition::new("Status")
                .with_value(EnumValueDefinition::new("ACTIVE"))
                .with_value(EnumValueDefinition::new("INACTIVE")),
        );

        let sdl = generate_sdl_from_schema(&schema);
        assert!(sdl.contains("enum Status"));
        assert!(sdl.contains("ACTIVE"));
        assert!(sdl.contains("INACTIVE"));
    }

    #[test]
    fn test_format_return_type_list() {
        assert_eq!(format_return_type("User", true, false), "[User!]!");
        assert_eq!(format_return_type("User", false, false), "User!");
        assert_eq!(format_return_type("User", false, true), "User");
        assert_eq!(format_return_type("User", true, true), "[User]!");
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
