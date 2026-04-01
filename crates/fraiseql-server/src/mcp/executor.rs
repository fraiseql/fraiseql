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
            let mut pairs = Vec::with_capacity(args.len());
            for (k, v) in args {
                // Validate argument name: must be a GraphQL identifier [_A-Za-z][_0-9A-Za-z]*
                // to prevent injection via malformed argument names.
                if !is_valid_graphql_name(k) {
                    return Err(format!(
                        "Invalid argument name: '{k}'. Only [_A-Za-z][_0-9A-Za-z]* is allowed."
                    ));
                }
                pairs.push(format!("{k}: {}", graphql_value(v)));
            }
            format!("({})", pairs.join(", "))
        }
    } else {
        String::new()
    };

    // Find the return type and build field selection
    let return_type = if is_mutation {
        schema.mutations.iter().find(|m| m.name == name).map(|m| m.return_type.as_str())
    } else {
        schema.queries.iter().find(|q| q.name == name).map(|q| q.return_type.as_str())
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
        },
        None => return Err(format!("Unknown operation: {name}")),
    };

    Ok(format!("{op_type} {{ {name}{args_str}{fields_str} }}"))
}

/// Validate that `name` is a legal GraphQL name: `[_A-Za-z][_0-9A-Za-z]*`.
fn is_valid_graphql_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        },
        _ => false,
    }
}

/// Escape a string for safe embedding in a GraphQL string literal.
///
/// Escapes `\`, `"`, and common control characters per the GraphQL spec.
fn escape_graphql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Convert a JSON value to its GraphQL literal representation.
///
/// String values are escaped to prevent GraphQL injection.
fn graphql_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_graphql_string(s)),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(graphql_value).collect();
            format!("[{}]", items.join(", "))
        },
        serde_json::Value::Object(obj) => {
            let pairs: Vec<String> =
                obj.iter().map(|(k, v)| format!("{k}: {}", graphql_value(v))).collect();
            format!("{{{}}}", pairs.join(", "))
        },
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
        .map(|f| f.name.to_string())
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
        // Reason: FieldType is #[non_exhaustive]; future variants also default to non-scalar
        FieldType::Object(_)
        | FieldType::Input(_)
        | FieldType::Interface(_)
        | FieldType::Union(_)
        | _ => false,
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
    fn test_graphql_value_string_escapes_quotes() {
        let v = serde_json::Value::String("say \"hi\"".to_string());
        assert_eq!(graphql_value(&v), r#""say \"hi\"""#);
    }

    #[test]
    fn test_graphql_value_string_escapes_backslash() {
        let v = serde_json::Value::String(r"a\b".to_string());
        assert_eq!(graphql_value(&v), r#""a\\b""#);
    }

    #[test]
    fn test_graphql_value_string_escapes_newline() {
        let v = serde_json::Value::String("line1\nline2".to_string());
        assert_eq!(graphql_value(&v), "\"line1\\nline2\"");
    }

    #[test]
    fn test_is_valid_graphql_name() {
        assert!(is_valid_graphql_name("limit"));
        assert!(is_valid_graphql_name("_private"));
        assert!(is_valid_graphql_name("field1"));
        assert!(!is_valid_graphql_name(""));
        assert!(!is_valid_graphql_name("1abc"));
        assert!(!is_valid_graphql_name("has space"));
        assert!(!is_valid_graphql_name("inject: bad"));
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
