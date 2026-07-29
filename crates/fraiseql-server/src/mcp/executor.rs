//! MCP tool call executor.
//!
//! Bridges MCP tool calls to FraiseQL's GraphQL execution pipeline by building
//! minimal GraphQL queries from tool name + arguments and executing them via
//! the existing `Executor`.

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, McpConfig},
    security::SecurityContext,
};
use rmcp::model::{CallToolResult, Content};

use crate::config::error_sanitization::ErrorSanitizer;

/// Everything a tool call needs besides its own name and arguments.
///
/// `executor` is the executor the caller's **tenant** dispatched to, which is not
/// necessarily the one the session was constructed with; `schema` stays the
/// schema the tool list was advertised from, so an operation can only be reached
/// if it was advertised.
pub struct McpCallContext<'a, A: DatabaseAdapter> {
    /// The schema the advertised tool list was built from.
    pub schema:           &'a CompiledSchema,
    /// The executor this call must run on.
    pub executor:         &'a Executor<A>,
    /// The `[mcp]` configuration, including the tool allowlist.
    pub config:           &'a McpConfig,
    /// The validated caller, when the transport supplied one.
    pub security_context: Option<&'a SecurityContext>,
    /// Applied to every execution error before it reaches the MCP client.
    pub error_sanitizer:  &'a ErrorSanitizer,
}

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
    ctx: &McpCallContext<'_, A>,
) -> CallToolResult {
    let operation = match build_operation(tool_name, arguments, ctx.schema, ctx.config) {
        Ok(op) => op,
        Err(e) => return error_result(&e),
    };

    let variables = serde_json::Value::Object(operation.variables);

    // Route through the authenticated executor path when a security context is
    // present, mirroring the HTTP GraphQL handler. When it is absent, fail
    // closed if the schema enforces RLS or authentication is required — running
    // such a query through the unauthenticated path would bypass tenant
    // isolation and `@inject` JWT resolution.
    let exec_result = if let Some(security) = ctx.security_context {
        ctx.executor
            .execute_with_security(&operation.document, Some(&variables), security)
            .await
    } else {
        if ctx.config.require_auth || ctx.schema.has_rls_configured() {
            return error_result(
                "Authentication required: this MCP server enforces row-level security \
                 or requires authentication, but the request carried no validated \
                 security context. Provide a Bearer token over the HTTP transport, or \
                 disable require_auth and RLS for unauthenticated use.",
            );
        }
        ctx.executor.execute(&operation.document, Some(&variables)).await
    };

    match exec_result {
        Ok(result) => {
            let result_text = result.to_string();
            CallToolResult::success(vec![Content::text(result_text)])
        },
        // Sanitized exactly as `/graphql` sanitizes it: a raw `FraiseQLError::Database`
        // carries the driver message and SQLSTATE, internal relation names included,
        // and the caller here is an AI agent (#875, item 1).
        Err(e) => error_result(&super::handler::sanitize(ctx.error_sanitizer, &e)),
    }
}

/// A resolved MCP tool call, ready to hand to the executor.
pub(crate) struct McpOperation {
    /// The GraphQL document. Built **only** from schema-derived identifiers — the
    /// operation's advertised name and its declared argument names — so no
    /// caller-supplied text ever reaches the document.
    pub(crate) document:  String,
    /// The argument values, passed as GraphQL variables rather than spliced into
    /// the document as literals.
    pub(crate) variables: serde_json::Map<String, serde_json::Value>,
}

/// Resolve an MCP tool call into a GraphQL operation and its variables.
///
/// For the advertised tool `users` with args `{ limit: 10 }` and return type
/// `User` with scalar fields `[id, name, email]`, produces the document
///
/// ```graphql
/// query ($limit: Int) { users(limit: $limit) { id name email } }
/// ```
///
/// and the variables `{ "limit": 10 }`.
///
/// # Security
///
/// Argument *values* are never rendered into the document. The previous
/// implementation built the whole document by string interpolation, validating
/// only top-level argument names; the keys of a nested object value were spliced
/// in raw, so a caller could close the argument list and append root fields of
/// their choosing — reaching operations `[mcp] include`/`exclude` was configured
/// to withhold, with field selections the tool's own projection would never emit
/// (#808). Values now travel as variables, and the only caller-controlled input
/// that reaches the document is an argument *name*, which must match one the
/// resolved operation declares.
///
/// # Errors
///
/// Returns the message to hand back to the MCP client when the tool is not
/// advertised, or when an argument is not one the operation declares.
pub(crate) fn build_operation(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
    config: &McpConfig,
) -> Result<McpOperation, String> {
    // Resolve against the advertised set, so `include`/`exclude`/`read_only` are
    // enforced where the call executes and not only where the tool list is built.
    let operation = super::tools::resolve_tool(tool_name, schema, config)
        .ok_or_else(|| format!("Unknown tool: {tool_name}"))?;

    let declared = operation.arguments();
    let supplied = arguments.filter(|args| !args.is_empty());

    let mut variable_defs = Vec::new();
    let mut call_args = Vec::new();
    let mut variables = serde_json::Map::new();

    if let Some(args) = supplied {
        for (name, value) in args {
            let Some(arg_def) = declared.iter().find(|a| &a.name == name) else {
                return Err(format!(
                    "Unknown argument '{name}' for tool '{tool_name}'. Accepted arguments: {}.",
                    accepted_argument_list(&declared)
                ));
            };
            // Belt and braces: a compiled schema should never declare an argument
            // whose name is not a GraphQL identifier, but this name is about to be
            // written into the document as a variable name.
            if !is_valid_graphql_name(name) {
                return Err(format!(
                    "Invalid argument name: '{name}'. Only [_A-Za-z][_0-9A-Za-z]* is allowed."
                ));
            }
            variable_defs.push(format!("${name}: {}", graphql_type_name(&arg_def.arg_type)));
            call_args.push(format!("{name}: ${name}"));
            variables.insert(name.clone(), value.clone());
        }
    }

    let var_defs_str = if variable_defs.is_empty() {
        String::new()
    } else {
        format!("({})", variable_defs.join(", "))
    };
    let args_str = if call_args.is_empty() {
        String::new()
    } else {
        format!("({})", call_args.join(", "))
    };

    let fields = scalar_fields_for_type(operation.return_type(), schema);
    let fields_str = if fields.is_empty() {
        // Scalar return type — no field selection needed
        String::new()
    } else {
        format!(" {{ {} }}", fields.join(" "))
    };

    let op_type = if operation.is_mutation() {
        "mutation"
    } else {
        "query"
    };

    Ok(McpOperation {
        document: format!("{op_type} {var_defs_str} {{ {tool_name}{args_str}{fields_str} }}"),
        variables,
    })
}

/// Render the accepted-argument list for an unknown-argument error.
fn accepted_argument_list(declared: &[fraiseql_core::schema::ArgumentDefinition]) -> String {
    if declared.is_empty() {
        "none".to_string()
    } else {
        declared.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// Render a [`FieldType`] as the GraphQL type name used in a variable definition.
///
/// Variables are always declared nullable: MCP arguments are optional at the
/// transport level (the client may omit any of them), and a `!` here would only
/// describe a constraint this layer does not enforce.
fn graphql_type_name(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Int => "Int".to_string(),
        FieldType::Float => "Float".to_string(),
        FieldType::Boolean => "Boolean".to_string(),
        FieldType::Id => "ID".to_string(),
        FieldType::DateTime => "DateTime".to_string(),
        FieldType::Date => "Date".to_string(),
        FieldType::Time => "Time".to_string(),
        FieldType::Uuid => "UUID".to_string(),
        FieldType::Decimal => "Decimal".to_string(),
        // JSON and Vector are both exposed as JSON, matching introspection.
        FieldType::Json | FieldType::Vector => "JSON".to_string(),
        FieldType::Scalar(name)
        | FieldType::Object(name)
        | FieldType::Enum(name)
        | FieldType::Input(name)
        | FieldType::Interface(name)
        | FieldType::Union(name) => name.clone(),
        FieldType::List(inner) => format!("[{}]", graphql_type_name(inner)),
        // Reason: FieldType is #[non_exhaustive]; a future variant carries no known
        // GraphQL name, and `String` is the same fallback `field_type_to_json_schema`
        // uses for the advertised input schema.
        _ => "String".to_string(),
    }
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
