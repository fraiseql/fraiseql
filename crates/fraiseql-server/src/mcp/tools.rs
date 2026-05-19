//! Schema-to-MCP-tool converter.
//!
//! Converts FraiseQL `QueryDefinition` and `MutationDefinition` into MCP `Tool` objects.

use std::{borrow::Cow, sync::Arc};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, MutationDefinition, QueryDefinition,
};
use rmcp::model::{JsonObject, Tool};

use super::McpConfig;

/// Convert the compiled schema into a list of MCP tools.
#[must_use]
pub fn schema_to_tools(schema: &CompiledSchema, config: &McpConfig) -> Vec<Tool> {
    let mut tools = Vec::new();

    for query in &schema.queries {
        let display = schema.display_name(&query.name);
        if should_include(&display, config) {
            tools.push(query_to_tool(query, &display));
        }
    }

    for mutation in &schema.mutations {
        let display = schema.display_name(&mutation.name);
        if should_include(&display, config) {
            tools.push(mutation_to_tool(mutation, &display));
        }
    }

    tools
}

/// Check whether a given operation name should be included based on config filters.
#[must_use]
pub fn should_include(name: &str, config: &McpConfig) -> bool {
    if !config.include.is_empty() && !config.include.iter().any(|i| i == name) {
        return false;
    }
    if config.exclude.iter().any(|e| e == name) {
        return false;
    }
    true
}

/// Convert a query definition into an MCP tool.
fn query_to_tool(query: &QueryDefinition, display_name: &str) -> Tool {
    let description = query.description.clone().unwrap_or_else(|| format!("Query: {display_name}"));

    Tool::new(
        Cow::Owned(display_name.to_string()),
        Cow::Owned(description),
        Arc::new(arguments_to_json_schema(&query.arguments)),
    )
}

/// Convert a mutation definition into an MCP tool.
fn mutation_to_tool(mutation: &MutationDefinition, display_name: &str) -> Tool {
    let description = mutation
        .description
        .clone()
        .unwrap_or_else(|| format!("Mutation: {display_name}"));

    Tool::new(
        Cow::Owned(display_name.to_string()),
        Cow::Owned(description),
        Arc::new(arguments_to_json_schema(&mutation.arguments)),
    )
}

/// Convert argument definitions into a JSON Schema object for MCP tool input.
pub(crate) fn arguments_to_json_schema(arguments: &[ArgumentDefinition]) -> JsonObject {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for arg in arguments {
        let schema = field_type_to_json_schema(&arg.arg_type);
        let mut prop = serde_json::Map::new();

        if let serde_json::Value::Object(map) = schema {
            prop.extend(map);
        }

        if let Some(ref desc) = arg.description {
            prop.insert("description".to_string(), serde_json::Value::String(desc.clone()));
        }

        properties.insert(arg.name.clone(), serde_json::Value::Object(prop));

        if !arg.nullable && arg.default_value.is_none() {
            required.push(serde_json::Value::String(arg.name.clone()));
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), serde_json::Value::String("object".to_string()));
    schema.insert("properties".to_string(), serde_json::Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".to_string(), serde_json::Value::Array(required));
    }
    schema
}

/// Map a `FieldType` to a JSON Schema value.
pub(crate) fn field_type_to_json_schema(field_type: &FieldType) -> serde_json::Value {
    match field_type {
        FieldType::Int => serde_json::json!({ "type": "integer" }),
        FieldType::Float => serde_json::json!({ "type": "number" }),
        FieldType::Boolean => serde_json::json!({ "type": "boolean" }),
        FieldType::Json => serde_json::json!({ "type": "object" }),
        FieldType::Vector => serde_json::json!({ "type": "array", "items": { "type": "number" } }),
        FieldType::List(inner) => {
            serde_json::json!({ "type": "array", "items": field_type_to_json_schema(inner) })
        },
        // Reason: FieldType is #[non_exhaustive]; all other variants (including future ones) map to
        // string
        FieldType::String
        | FieldType::Id
        | FieldType::Uuid
        | FieldType::Decimal
        | FieldType::DateTime
        | FieldType::Date
        | FieldType::Time
        | FieldType::Scalar(_)
        | FieldType::Object(_)
        | FieldType::Enum(_)
        | FieldType::Input(_)
        | FieldType::Interface(_)
        | FieldType::Union(_)
        | _ => serde_json::json!({ "type": "string" }),
    }
}
