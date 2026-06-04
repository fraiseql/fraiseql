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
    security::SecurityContext,
};
use rmcp::model::{CallToolResult, Content};

/// Execute an MCP tool call by building and running a GraphQL query.
///
/// When `security_context` is `Some`, the call is routed through the
/// authenticated executor path ([`Executor::execute_with_security`]) so RLS
/// `WHERE` clauses, session variables, and `@inject` JWT parameters are applied
/// exactly as they are for the HTTP GraphQL endpoint.
///
/// When `security_context` is `None`, the call is **refused** (fail-closed) if
/// the compiled schema has an RLS policy configured or `require_auth` is set —
/// running such a query without a security context would bypass tenant
/// isolation. Non-RLS schemas with `require_auth = false` continue to run
/// unauthenticated (development convenience).
pub async fn call_tool<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
    executor: &Arc<Executor<A>>,
    security_context: Option<&SecurityContext>,
    require_auth: bool,
) -> CallToolResult {
    let is_mutation = schema.mutations.iter().any(|m| m.name == tool_name);

    let graphql_query = match build_graphql_query(tool_name, arguments, schema, is_mutation) {
        Ok(q) => q,
        Err(e) => return error_result(&e),
    };

    let variables = arguments.map(|args| serde_json::Value::Object(args.clone()));

    // Route through the authenticated executor path when a security context is
    // present, mirroring the HTTP GraphQL handler. When it is absent, fail
    // closed if the schema enforces RLS or authentication is required — running
    // such a query through the unauthenticated path would bypass tenant
    // isolation and `@inject` JWT resolution.
    let exec_result = if let Some(ctx) = security_context {
        executor.execute_with_security(&graphql_query, variables.as_ref(), ctx).await
    } else {
        if require_auth || schema.has_rls_configured() {
            return error_result(
                "Authentication required: this MCP server enforces row-level security \
                 or requires authentication, but the request carried no validated \
                 security context. Provide a Bearer token over the HTTP transport, or \
                 disable require_auth and RLS for unauthenticated use.",
            );
        }
        executor.execute(&graphql_query, variables.as_ref()).await
    };

    match exec_result {
        Ok(result) => {
            let result_text = result.to_string();
            CallToolResult::success(vec![Content::text(result_text)])
        },
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
pub(crate) fn is_valid_graphql_name(name: &str) -> bool {
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
pub(crate) fn graphql_value(value: &serde_json::Value) -> String {
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
#[must_use]
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
pub(crate) fn is_scalar_field_type(field_type: &FieldType) -> bool {
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

pub(super) fn error_result(message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message.to_string())])
}
