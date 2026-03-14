//! OpenAPI 3.1.0 specification generator for compiled REST routes.
//!
//! Reads `CompiledSchema` REST annotations and produces a standards-compliant
//! OpenAPI document. The output can be embedded in `schema.compiled.json` at
//! compile time or generated dynamically at server startup.
//!
//! For the static, hand-written spec covering FraiseQL internal/admin endpoints,
//! see `fraiseql_server::routes::api::openapi`. Both specs coexist — this module
//! covers only user-defined REST routes derived from `@fraiseql.query` /
//! `@fraiseql.mutation` annotations.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::{rest::RestConfig, schema::CompiledSchema};
use crate::schema::{
    field_type::FieldType,
    graphql_type_defs::TypeDefinition,
};

/// Generate an OpenAPI 3.1.0 specification from compiled REST routes.
///
/// # Arguments
///
/// * `schema` — compiled schema (provides type info for component schemas)
/// * `config` — REST transport configuration (prefix, auth mode, metadata)
///
/// # Returns
///
/// OpenAPI 3.1.0 specification as a pretty-printed JSON string.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::CompiledSchema;
/// use fraiseql_core::schema::compiled::rest::RestConfig;
/// use fraiseql_core::schema::compiled::openapi_gen::generate_openapi_spec;
///
/// let schema = CompiledSchema::default();
/// let config = RestConfig::default();
/// let spec_json = generate_openapi_spec(&schema, &config);
/// let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();
/// assert_eq!(spec["openapi"], "3.1.0");
/// ```
#[must_use]
pub fn generate_openapi_spec(schema: &CompiledSchema, config: &RestConfig) -> String {
    let mut paths = serde_json::Map::new();

    // Build path items from REST-annotated queries
    for query_def in &schema.queries {
        let Some(ref rest) = query_def.rest else {
            continue;
        };

        let full_path = openapi_path(&config.prefix, &rest.path);
        let path_param_names: HashSet<&str> = rest.path_params().into_iter().collect();

        let operation = build_operation(
            &query_def.name,
            query_def.description.as_deref(),
            &query_def.arguments,
            &path_param_names,
            &rest.method,
            &query_def.return_type,
            query_def.returns_list,
            config,
        );

        let method = rest.method.to_lowercase();
        let path_entry = paths.entry(full_path).or_insert_with(|| json!({}));
        path_entry[method] = operation;
    }

    // Build path items from REST-annotated mutations
    for mutation_def in &schema.mutations {
        let Some(ref rest) = mutation_def.rest else {
            continue;
        };

        let full_path = openapi_path(&config.prefix, &rest.path);
        let path_param_names: HashSet<&str> = rest.path_params().into_iter().collect();

        let operation = build_operation(
            &mutation_def.name,
            mutation_def.description.as_deref(),
            &mutation_def.arguments,
            &path_param_names,
            &rest.method,
            &mutation_def.return_type,
            false, // mutations don't return lists
            config,
        );

        let method = rest.method.to_lowercase();
        let path_entry = paths.entry(full_path).or_insert_with(|| json!({}));
        path_entry[method] = operation;
    }

    // Build component schemas from referenced types
    let mut component_schemas = serde_json::Map::new();
    let referenced = collect_referenced_types(schema);
    for type_name in &referenced {
        if let Some(type_def) = schema.types.iter().find(|t| t.name.as_str() == type_name.as_str())
        {
            component_schemas.insert(type_name.clone(), type_to_json_schema(type_def));
        }
    }

    let mut spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": config.title.as_deref().unwrap_or("FraiseQL REST API"),
            "version": config.api_version.as_deref().unwrap_or("1.0.0"),
            "description": "Auto-generated REST API from FraiseQL schema"
        },
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(component_schemas)
        }
    });

    // Add security scheme when auth is configured
    if config.auth == "required" || config.auth == "optional" {
        spec["components"]["securitySchemes"] = json!({
            "BearerAuth": {
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT"
            }
        });
    }

    serde_json::to_string_pretty(&spec).expect("OpenAPI spec serialization cannot fail")
}

/// Convert a schema REST path to an OpenAPI path (curly braces are already correct).
///
/// e.g. `prefix="/rest/v1"`, `path="/users/{id}"` → `"/rest/v1/users/{id}"`
fn openapi_path(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = if path.starts_with('/') { path } else { &format!("/{path}") };
    format!("{prefix}{path}")
}

/// Build a single OpenAPI operation object for a query or mutation.
#[allow(clippy::too_many_arguments)] // Reason: all parameters are genuinely distinct inputs
fn build_operation(
    operation_id: &str,
    description: Option<&str>,
    arguments: &[crate::schema::ArgumentDefinition],
    path_param_names: &HashSet<&str>,
    method: &str,
    return_type: &str,
    returns_list: bool,
    config: &RestConfig,
) -> Value {
    let method_upper = method.to_uppercase();

    // Parameters: path params + query params (for GET/DELETE)
    let mut parameters: Vec<Value> = Vec::new();

    for arg in arguments {
        if path_param_names.contains(arg.name.as_str()) {
            parameters.push(json!({
                "name": arg.name,
                "in": "path",
                "required": true,
                "schema": field_type_to_json_schema(&arg.arg_type)
            }));
        } else if matches!(method_upper.as_str(), "GET" | "DELETE" | "HEAD") {
            parameters.push(json!({
                "name": arg.name,
                "in": "query",
                "required": !arg.nullable,
                "schema": field_type_to_json_schema(&arg.arg_type)
            }));
        }
    }

    // Response schema
    let item_schema = if is_scalar_type(return_type) {
        scalar_name_to_json_schema(return_type)
    } else {
        json!({"$ref": format!("#/components/schemas/{return_type}")})
    };

    let response_schema = if returns_list {
        json!({"type": "array", "items": item_schema})
    } else {
        item_schema
    };

    let mut operation = json!({
        "operationId": operation_id,
        "parameters": parameters,
        "responses": {
            "200": {
                "description": "Success",
                "content": {
                    "application/json": {
                        "schema": response_schema
                    }
                }
            },
            "404": {"description": "Not found"},
            "500": {"description": "Internal server error"}
        }
    });

    if let Some(desc) = description {
        operation["description"] = json!(desc);
    }

    // Security for required auth
    if config.auth == "required" {
        operation["security"] = json!([{"BearerAuth": []}]);
    } else if config.auth == "optional" {
        operation["security"] = json!([{"BearerAuth": []}, {}]);
    }

    // Request body for methods that carry a body
    if matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH") {
        let body_args: Vec<_> =
            arguments.iter().filter(|a| !path_param_names.contains(a.name.as_str())).collect();

        if !body_args.is_empty() {
            let mut properties = serde_json::Map::new();
            let mut required: Vec<Value> = Vec::new();

            for arg in &body_args {
                properties
                    .insert(arg.name.clone(), field_type_to_json_schema(&arg.arg_type));
                if !arg.nullable {
                    required.push(json!(arg.name.clone()));
                }
            }

            let mut body_schema = json!({"type": "object", "properties": Value::Object(properties)});
            if !required.is_empty() {
                body_schema["required"] = Value::Array(required);
            }

            operation["requestBody"] = json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": body_schema
                    }
                }
            });
        }
    }

    operation
}

/// Collect all type names referenced (directly or transitively) by REST-annotated operations.
fn collect_referenced_types(schema: &CompiledSchema) -> HashSet<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut to_visit: Vec<String> = Vec::new();

    // Seed with direct return types of REST operations
    for query in &schema.queries {
        if query.rest.is_some() && !is_scalar_type(&query.return_type) {
            to_visit.push(query.return_type.clone());
        }
    }
    for mutation in &schema.mutations {
        if mutation.rest.is_some() && !is_scalar_type(&mutation.return_type) {
            to_visit.push(mutation.return_type.clone());
        }
    }

    // BFS over nested object types
    while let Some(type_name) = to_visit.pop() {
        if visited.contains(&type_name) {
            continue;
        }
        visited.insert(type_name.clone());

        // Find nested object types referenced by this type's fields
        if let Some(type_def) =
            schema.types.iter().find(|t| t.name.as_str() == type_name.as_str())
        {
            for field in &type_def.fields {
                let nested = nested_object_type(&field.field_type);
                if let Some(name) = nested {
                    if !visited.contains(name) {
                        to_visit.push(name.to_string());
                    }
                }
            }
        }
    }

    visited
}

/// Extract the object type name from a field type, if it references a named object type.
fn nested_object_type(ft: &FieldType) -> Option<&str> {
    match ft {
        FieldType::Object(name) => Some(name.as_str()),
        FieldType::List(inner) => nested_object_type(inner),
        _ => None,
    }
}

/// Convert a `TypeDefinition` to a JSON Schema object.
#[must_use]
pub fn type_to_json_schema(type_def: &TypeDefinition) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    for field in &type_def.fields {
        let mut field_schema = field_type_to_json_schema(&field.field_type);

        if let Some(desc) = &field.description {
            field_schema["description"] = json!(desc);
        }

        if !field.nullable {
            required.push(json!(field.name.as_str()));
        }

        properties.insert(field.name.to_string(), field_schema);
    }

    let mut schema = json!({
        "type": "object",
        "properties": Value::Object(properties)
    });

    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }

    if let Some(desc) = &type_def.description {
        schema["description"] = json!(desc);
    }

    schema
}

/// Map a `FieldType` to a JSON Schema value.
fn field_type_to_json_schema(ft: &FieldType) -> Value {
    match ft {
        FieldType::String => json!({"type": "string"}),
        FieldType::Int => json!({"type": "integer"}),
        FieldType::Float => json!({"type": "number"}),
        FieldType::Boolean => json!({"type": "boolean"}),
        FieldType::Id => json!({"type": "string", "format": "uuid"}),
        FieldType::DateTime => json!({"type": "string", "format": "date-time"}),
        FieldType::Date => json!({"type": "string", "format": "date"}),
        FieldType::Time => json!({"type": "string", "format": "time"}),
        FieldType::Json => json!({}), // any JSON value
        FieldType::Uuid => json!({"type": "string", "format": "uuid"}),
        FieldType::Decimal => json!({"type": "string", "format": "decimal"}),
        FieldType::Vector => json!({"type": "array", "items": {"type": "number"}}),
        FieldType::Scalar(name) => json!({"type": "string", "description": format!("Custom scalar: {name}")}),
        FieldType::List(inner) => json!({"type": "array", "items": field_type_to_json_schema(inner)}),
        FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name) => {
            json!({"$ref": format!("#/components/schemas/{name}")})
        },
        FieldType::Enum(name) => json!({"$ref": format!("#/components/schemas/{name}")}),
        FieldType::Input(name) => json!({"$ref": format!("#/components/schemas/{name}")}),
    }
}

/// Check whether a type name is a built-in scalar (should not appear in components/schemas).
fn is_scalar_type(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Int"
            | "Float"
            | "Boolean"
            | "ID"
            | "DateTime"
            | "Date"
            | "Time"
            | "JSON"
            | "UUID"
            | "Decimal"
    )
}

/// Map a scalar type name to a JSON Schema snippet (used for scalar return types).
fn scalar_name_to_json_schema(name: &str) -> Value {
    match name {
        "Int" => json!({"type": "integer"}),
        "Float" => json!({"type": "number"}),
        "Boolean" => json!({"type": "boolean"}),
        "ID" | "UUID" => json!({"type": "string", "format": "uuid"}),
        "DateTime" => json!({"type": "string", "format": "date-time"}),
        "Date" => json!({"type": "string", "format": "date"}),
        "Time" => json!({"type": "string", "format": "time"}),
        "JSON" => json!({}),
        _ => json!({"type": "string"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        compiled::{
            mutation::MutationDefinition,
            query::QueryDefinition,
            rest::{RestConfig, RestRoute},
        },
        field_type::{FieldDefinition, FieldType},
        graphql_type_defs::TypeDefinition,
    };

    fn user_type() -> TypeDefinition {
        TypeDefinition {
            name:                "User".into(),
            sql_source:          "v_user".into(),
            jsonb_column:        "data".to_string(),
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                FieldDefinition::new("email", FieldType::String),
            ],
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
        }
    }

    fn address_type() -> TypeDefinition {
        TypeDefinition {
            name:                "Address".into(),
            sql_source:          "v_address".into(),
            jsonb_column:        "data".to_string(),
            fields:              vec![
                FieldDefinition::new("street", FieldType::String),
                FieldDefinition::new("city", FieldType::String),
            ],
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
        }
    }

    fn schema_with_get_user() -> CompiledSchema {
        let mut q = QueryDefinition::new("get_user", "User");
        q.arguments.push(crate::schema::ArgumentDefinition::new("id", FieldType::Id));
        q.rest = Some(RestRoute { path: "/users/{id}".to_string(), method: "GET".to_string() });
        CompiledSchema { queries: vec![q], types: vec![user_type()], ..Default::default() }
    }

    fn schema_with_create_user() -> CompiledSchema {
        let mut m = MutationDefinition::new("create_user", "User");
        m.arguments.push(crate::schema::ArgumentDefinition::new("name", FieldType::String));
        m.rest = Some(RestRoute { path: "/users".to_string(), method: "POST".to_string() });
        CompiledSchema { mutations: vec![m], types: vec![user_type()], ..Default::default() }
    }

    fn schema_with_list_users() -> CompiledSchema {
        let mut q = QueryDefinition::new("list_users", "User");
        q.returns_list = true;
        q.rest = Some(RestRoute { path: "/users".to_string(), method: "GET".to_string() });
        CompiledSchema { queries: vec![q], types: vec![user_type()], ..Default::default() }
    }

    fn schema_with_nested_type() -> CompiledSchema {
        let mut user_with_address = TypeDefinition {
            name:                "UserWithAddress".into(),
            sql_source:          "v_user_with_address".into(),
            jsonb_column:        "data".to_string(),
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("address", FieldType::Object("Address".to_string())),
            ],
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
        };
        user_with_address.fields[1].nullable = true;

        let mut q = QueryDefinition::new("get_user_with_address", "UserWithAddress");
        q.arguments.push(crate::schema::ArgumentDefinition::new("id", FieldType::Id));
        q.rest =
            Some(RestRoute { path: "/users/{id}".to_string(), method: "GET".to_string() });

        CompiledSchema {
            queries: vec![q],
            types:   vec![user_with_address, address_type()],
            ..Default::default()
        }
    }

    #[test]
    fn test_generates_valid_openapi_31() {
        let schema = schema_with_get_user();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();
        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(spec["info"]["title"], "FraiseQL REST API");
    }

    #[test]
    fn test_path_params_appear_in_parameters() {
        let schema = schema_with_get_user();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        let params = &spec["paths"]["/rest/users/{id}"]["get"]["parameters"];
        let id_param = params.as_array().unwrap().iter().find(|p| p["name"] == "id").unwrap();
        assert_eq!(id_param["in"], "path");
        assert_eq!(id_param["required"], true);
    }

    #[test]
    fn test_mutation_has_request_body() {
        let schema = schema_with_create_user();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        let body = &spec["paths"]["/rest/users"]["post"]["requestBody"];
        assert!(body.is_object());
        assert_eq!(body["required"], true);
        let props = &body["content"]["application/json"]["schema"]["properties"];
        assert!(props["name"].is_object());
    }

    #[test]
    fn test_return_type_referenced_in_components() {
        let schema = schema_with_get_user();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        assert!(spec["components"]["schemas"]["User"].is_object());
    }

    #[test]
    fn test_nested_types_included() {
        let schema = schema_with_nested_type();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        assert!(spec["components"]["schemas"]["UserWithAddress"].is_object());
        assert!(spec["components"]["schemas"]["Address"].is_object());
    }

    #[test]
    fn test_list_return_produces_array_schema() {
        let schema = schema_with_list_users();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        let response_schema =
            &spec["paths"]["/rest/users"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(response_schema["type"], "array");
        assert!(response_schema["items"]["$ref"].as_str().unwrap().contains("User"));
    }

    #[test]
    fn test_security_added_when_auth_required() {
        let schema = schema_with_get_user();
        let config = RestConfig {
            auth: "required".to_string(),
            ..Default::default()
        };
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        let security = &spec["paths"]["/rest/users/{id}"]["get"]["security"];
        assert!(security.as_array().is_some_and(|a| !a.is_empty()));
        assert!(spec["components"]["securitySchemes"]["BearerAuth"].is_object());
    }

    #[test]
    fn test_no_security_when_auth_none() {
        let schema = schema_with_get_user();
        let config = RestConfig::default(); // auth = "none"
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        let security = &spec["paths"]["/rest/users/{id}"]["get"]["security"];
        assert!(security.is_null());
        assert!(spec["components"]["securitySchemes"].is_null());
    }

    #[test]
    fn test_empty_schema_produces_empty_paths() {
        let schema = CompiledSchema::default();
        let config = RestConfig::default();
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        assert_eq!(spec["paths"], json!({}));
    }

    #[test]
    fn test_custom_title_and_version() {
        let schema = schema_with_get_user();
        let config = RestConfig {
            title:       Some("My API".to_string()),
            api_version: Some("2.0.0".to_string()),
            ..Default::default()
        };
        let spec_json = generate_openapi_spec(&schema, &config);
        let spec: Value = serde_json::from_str(&spec_json).unwrap();

        assert_eq!(spec["info"]["title"], "My API");
        assert_eq!(spec["info"]["version"], "2.0.0");
    }
}
