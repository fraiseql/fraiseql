//! MCP tool call executor.
//!
//! Bridges MCP tool calls to FraiseQL's GraphQL execution pipeline by building
//! minimal GraphQL queries from tool name + arguments and executing them via
//! the existing `Executor`.

use std::sync::Arc;

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use rmcp::model::{CallToolResult, Content};

/// Execute an MCP tool call by building and running a GraphQL query.
pub async fn call_tool<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
    executor: &Arc<Executor<A>>,
) -> CallToolResult {
    let is_mutation = schema.mutations.iter().any(|m| m.name == tool_name);

    let graphql_query = match build_graphql_query(tool_name, arguments, schema, is_mutation) {
        Ok(q) => q,
        Err(e) => return error_result(&e),
    };

    let variables = arguments.map(|args| serde_json::Value::Object(args.clone()));

    match executor.execute(&graphql_query, variables.as_ref()).await {
        Ok(result) => CallToolResult::success(vec![Content::text(result)]),
        Err(e) => error_result(&e.to_string()),
    }
}

/// Build a GraphQL query string from an MCP tool call.
///
/// For a query named `users` with args `{ limit: 10 }` and return type `User`
/// with fields `[id, name, email]`, produces:
///
/// ```graphql
/// query { users(limit: 10) { id name email } }
/// ```
fn build_graphql_query(
    name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
    is_mutation: bool,
) -> Result<String, String> {
    let op_type = if is_mutation { "mutation" } else { "query" };

    // Build argument string
    let args_str = if let Some(args) = arguments {
        if args.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = args
                .iter()
                .map(|(k, v)| format!("{k}: {}", graphql_value(v)))
                .collect();
            format!("({})", pairs.join(", "))
        }
    } else {
        String::new()
    };

    // Find the return type and build field selection
    let return_type = if is_mutation {
        schema
            .mutations
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.return_type.as_str())
    } else {
        schema
            .queries
            .iter()
            .find(|q| q.name == name)
            .map(|q| q.return_type.as_str())
    };

    let fields_str = match return_type {
        Some(type_name) => {
            let fields = scalar_fields_for_type(type_name, schema);
            if fields.is_empty() {
                // Scalar return type — no field selection needed
                String::new()
            } else {
                format!(" {{ {} }}", fields.join(" "))
            }
        }
        None => return Err(format!("Unknown operation: {name}")),
    };

    Ok(format!("{op_type} {{ {name}{args_str}{fields_str} }}"))
}

/// Convert a JSON value to its GraphQL literal representation.
fn graphql_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(graphql_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(obj) => {
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{k}: {}", graphql_value(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
    }
}

/// Get all scalar field names for a given type.
///
/// Walks the `TypeDefinition.fields` and returns names of fields whose type
/// is a scalar (not `Object`, not `List(Object)`).
pub fn scalar_fields_for_type(type_name: &str, schema: &CompiledSchema) -> Vec<String> {
    let Some(type_def) = schema.types.iter().find(|t| t.name == type_name) else {
        return vec![];
    };

    type_def
        .fields
        .iter()
        .filter(|f| is_scalar_field_type(&f.field_type))
        .map(|f| f.name.clone())
        .collect()
}

/// Check whether a field type is a scalar (not requiring sub-selection).
fn is_scalar_field_type(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::String
        | FieldType::Int
        | FieldType::Float
        | FieldType::Boolean
        | FieldType::Id
        | FieldType::DateTime
        | FieldType::Date
        | FieldType::Time
        | FieldType::Json
        | FieldType::Uuid
        | FieldType::Decimal
        | FieldType::Vector
        | FieldType::Scalar(_)
        | FieldType::Enum(_) => true,
        FieldType::List(inner) => is_scalar_field_type(inner),
        FieldType::Object(_)
        | FieldType::Input(_)
        | FieldType::Interface(_)
        | FieldType::Union(_) => false,
    }
}

fn error_result(message: &str) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(message.to_string())],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_value_string() {
        let v = serde_json::Value::String("hello".to_string());
        assert_eq!(graphql_value(&v), "\"hello\"");
    }

    #[test]
    fn test_graphql_value_number() {
        let v = serde_json::json!(42);
        assert_eq!(graphql_value(&v), "42");
    }

    #[test]
    fn test_graphql_value_bool() {
        let v = serde_json::Value::Bool(true);
        assert_eq!(graphql_value(&v), "true");
    }

    #[test]
    fn test_graphql_value_array() {
        let v = serde_json::json!([1, 2, 3]);
        assert_eq!(graphql_value(&v), "[1, 2, 3]");
    }

    #[test]
    fn test_is_scalar_field_type() {
        assert!(is_scalar_field_type(&FieldType::String));
        assert!(is_scalar_field_type(&FieldType::Int));
        assert!(is_scalar_field_type(&FieldType::List(Box::new(FieldType::Int))));
        assert!(!is_scalar_field_type(&FieldType::Object("User".to_string())));
    }
}
