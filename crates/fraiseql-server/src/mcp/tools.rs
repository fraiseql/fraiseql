//! Schema-to-MCP-tool converter.
//!
//! Converts FraiseQL `QueryDefinition` and `MutationDefinition` into MCP `Tool` objects.

use std::borrow::Cow;
use std::sync::Arc;

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, MutationDefinition, QueryDefinition,
};
use rmcp::model::{JsonObject, Tool};

use super::McpConfig;

/// Convert the compiled schema into a list of MCP tools.
pub fn schema_to_tools(schema: &CompiledSchema, config: &McpConfig) -> Vec<Tool> {
    let mut tools = Vec::new();

    for query in &schema.queries {
        if should_include(&query.name, config) {
            tools.push(query_to_tool(query));
        }
    }

    for mutation in &schema.mutations {
        if should_include(&mutation.name, config) {
            tools.push(mutation_to_tool(mutation));
        }
    }

    tools
}

/// Check whether a given operation name should be included based on config filters.
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
fn query_to_tool(query: &QueryDefinition) -> Tool {
    let description = query
        .description
        .clone()
        .unwrap_or_else(|| format!("Query: {}", query.name));

    Tool {
        name: Cow::Owned(query.name.clone()),
        title: None,
        description: Some(Cow::Owned(description)),
        input_schema: Arc::new(arguments_to_json_schema(&query.arguments)),
        annotations: None,
        output_schema: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

/// Convert a mutation definition into an MCP tool.
fn mutation_to_tool(mutation: &MutationDefinition) -> Tool {
    let description = mutation
        .description
        .clone()
        .unwrap_or_else(|| format!("Mutation: {}", mutation.name));

    Tool {
        name: Cow::Owned(mutation.name.clone()),
        title: None,
        description: Some(Cow::Owned(description)),
        input_schema: Arc::new(arguments_to_json_schema(&mutation.arguments)),
        annotations: None,
        output_schema: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

/// Convert argument definitions into a JSON Schema object for MCP tool input.
fn arguments_to_json_schema(arguments: &[ArgumentDefinition]) -> JsonObject {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for arg in arguments {
        let schema = field_type_to_json_schema(&arg.arg_type);
        let mut prop = serde_json::Map::new();

        if let serde_json::Value::Object(map) = schema {
            prop.extend(map);
        }

        if let Some(ref desc) = arg.description {
            prop.insert(
                "description".to_string(),
                serde_json::Value::String(desc.clone()),
            );
        }

        properties.insert(arg.name.clone(), serde_json::Value::Object(prop));

        if !arg.nullable && arg.default_value.is_none() {
            required.push(serde_json::Value::String(arg.name.clone()));
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert(
        "type".to_string(),
        serde_json::Value::String("object".to_string()),
    );
    schema.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !required.is_empty() {
        schema.insert("required".to_string(), serde_json::Value::Array(required));
    }
    schema
}

/// Map a `FieldType` to a JSON Schema value.
fn field_type_to_json_schema(field_type: &FieldType) -> serde_json::Value {
    match field_type {
        FieldType::String | FieldType::Id | FieldType::Uuid | FieldType::Decimal => {
            serde_json::json!({ "type": "string" })
        }
        FieldType::Int => serde_json::json!({ "type": "integer" }),
        FieldType::Float => serde_json::json!({ "type": "number" }),
        FieldType::Boolean => serde_json::json!({ "type": "boolean" }),
        FieldType::DateTime | FieldType::Date | FieldType::Time => {
            serde_json::json!({ "type": "string" })
        }
        FieldType::Json => serde_json::json!({ "type": "object" }),
        FieldType::Vector => serde_json::json!({ "type": "array", "items": { "type": "number" } }),
        FieldType::Scalar(_) => serde_json::json!({ "type": "string" }),
        FieldType::List(inner) => {
            serde_json::json!({ "type": "array", "items": field_type_to_json_schema(inner) })
        }
        FieldType::Object(_)
        | FieldType::Enum(_)
        | FieldType::Input(_)
        | FieldType::Interface(_)
        | FieldType::Union(_) => serde_json::json!({ "type": "string" }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn make_config(include: Vec<String>, exclude: Vec<String>) -> McpConfig {
        McpConfig {
            enabled: true,
            transport: "http".to_string(),
            path: "/mcp".to_string(),
            require_auth: true,
            include,
            exclude,
        }
    }

    #[test]
    fn test_should_include_all_when_empty() {
        let config = make_config(vec![], vec![]);
        assert!(should_include("users", &config));
        assert!(should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_whitelist() {
        let config = make_config(vec!["users".to_string()], vec![]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_blacklist() {
        let config = make_config(vec![], vec!["createUser".to_string()]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_field_type_to_json_schema() {
        let schema = field_type_to_json_schema(&FieldType::String);
        assert_eq!(schema, serde_json::json!({ "type": "string" }));

        let schema = field_type_to_json_schema(&FieldType::Int);
        assert_eq!(schema, serde_json::json!({ "type": "integer" }));

        let schema = field_type_to_json_schema(&FieldType::Boolean);
        assert_eq!(schema, serde_json::json!({ "type": "boolean" }));

        let schema = field_type_to_json_schema(&FieldType::List(Box::new(FieldType::Int)));
        assert_eq!(
            schema,
            serde_json::json!({ "type": "array", "items": { "type": "integer" } })
        );
    }

    #[test]
    fn test_arguments_to_json_schema() {
        let args = vec![
            ArgumentDefinition::new("id", FieldType::Id),
            ArgumentDefinition::optional("name", FieldType::String),
        ];

        let schema = arguments_to_json_schema(&args);
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("id"));
        assert!(props.contains_key("name"));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "id");
    }
}
