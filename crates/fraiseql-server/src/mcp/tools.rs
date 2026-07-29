//! Schema-to-MCP-tool converter.
//!
//! Converts FraiseQL `QueryDefinition` and `MutationDefinition` into MCP `Tool` objects.

use std::{borrow::Cow, sync::Arc};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, MutationDefinition, QueryDefinition,
};
use rmcp::model::{JsonObject, Tool};

use super::McpConfig;

/// The schema operation an advertised MCP tool dispatches to.
///
/// Carries the operation itself rather than its name, so a caller that resolved a
/// tool cannot then look the operation up a second time by a different identifier
/// — the drift that made every tool call fail under `naming_convention =
/// "camelCase"` (#857).
#[derive(Debug, Clone, Copy)]
pub enum ExposedOperation<'a> {
    /// A read operation, executed as `query { … }`.
    Query(&'a QueryDefinition),
    /// A write operation, executed as `mutation { … }`.
    Mutation(&'a MutationDefinition),
}

impl ExposedOperation<'_> {
    /// The operation's return type name.
    #[must_use]
    pub const fn return_type(&self) -> &str {
        match self {
            Self::Query(q) => q.return_type.as_str(),
            Self::Mutation(m) => m.return_type.as_str(),
        }
    }

    /// Whether the operation is executed as a mutation.
    #[must_use]
    pub const fn is_mutation(&self) -> bool {
        matches!(self, Self::Mutation(_))
    }

    /// Every argument a caller may supply, in the shape the GraphQL surface
    /// accepts it.
    ///
    /// For queries this includes the auto-wired `where`/`orderBy`/`limit`/`offset`
    /// parameters enabled by
    /// [`auto_params`](fraiseql_core::schema::QueryDefinition::auto_params), which
    /// the runtime reads straight off the argument map and which are therefore not
    /// present in `arguments`.
    #[must_use]
    pub fn arguments(&self) -> Vec<ArgumentDefinition> {
        match self {
            Self::Query(q) => q.graphql_arguments(),
            Self::Mutation(m) => m.arguments.clone(),
        }
    }
}

/// Every operation the `[mcp]` configuration exposes, paired with the name it is
/// advertised under.
///
/// This is the single source of truth for "what is reachable over MCP".
/// [`schema_to_tools`] renders it into the advertised tool list and
/// [`resolve_tool`] resolves an incoming `tools/call` against it, so the
/// advertisement and the execution can never disagree — neither about the
/// identifier (#857) nor about the `include`/`exclude`/`read_only` allowlist
/// (#808).
#[must_use]
pub fn exposed_operations<'a>(
    schema: &'a CompiledSchema,
    config: &McpConfig,
) -> Vec<(String, ExposedOperation<'a>)> {
    let mut exposed = Vec::new();

    for query in &schema.queries {
        let display = schema.display_name(&query.name);
        if should_include(&display, config) {
            exposed.push((display, ExposedOperation::Query(query)));
        }
    }

    // Read-only exposure (fail-closed): no mutation is ever reachable when
    // `read_only` is set — this wins over `include` and guarantees a mutation added
    // to the schema later is not silently exposed to AI callers.
    if config.read_only {
        // Surface a self-contradiction loudly: `read_only` + an `include` naming a
        // mutation means that mutation is deliberately NOT exposed despite the
        // include. `read_only` wins (fail-closed).
        let contradicts: Vec<&String> = config
            .include
            .iter()
            .filter(|name| schema.mutations.iter().any(|m| &schema.display_name(&m.name) == *name))
            .collect();
        if !contradicts.is_empty() {
            tracing::warn!(
                mutations = ?contradicts,
                "[mcp] read_only = true overrides `include` listing these mutations — they are \
                 NOT exposed as tools (read_only wins, fail-closed)"
            );
        }
    } else {
        for mutation in &schema.mutations {
            let display = schema.display_name(&mutation.name);
            if should_include(&display, config) {
                exposed.push((display, ExposedOperation::Mutation(mutation)));
            }
        }
    }

    exposed
}

/// Resolve an incoming tool name against the exposed set.
///
/// Returns `None` for a name that is not advertised — whether because no such
/// operation exists, because `include`/`exclude` withholds it, or because
/// `read_only` withholds every mutation. The three are deliberately
/// indistinguishable to the caller: an error that separated "does not exist" from
/// "exists but is forbidden" would be an existence oracle for exactly the
/// operations the allowlist is configured to hide.
#[must_use]
pub fn resolve_tool<'a>(
    tool_name: &str,
    schema: &'a CompiledSchema,
    config: &McpConfig,
) -> Option<ExposedOperation<'a>> {
    exposed_operations(schema, config)
        .into_iter()
        .find(|(name, _)| name == tool_name)
        .map(|(_, op)| op)
}

/// Convert the compiled schema into a list of MCP tools.
#[must_use]
pub fn schema_to_tools(schema: &CompiledSchema, config: &McpConfig) -> Vec<Tool> {
    exposed_operations(schema, config)
        .into_iter()
        .map(|(display, op)| match op {
            ExposedOperation::Query(q) => query_to_tool(q, &display),
            ExposedOperation::Mutation(m) => mutation_to_tool(m, &display),
        })
        .collect()
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
///
/// The advertised input schema is built from
/// [`ExposedOperation::arguments`], the same list the executor validates an
/// incoming call against — so `where`/`orderBy`/`limit`/`offset` are advertised
/// exactly when `auto_params` makes them acceptable, and an argument the tool
/// advertises is always an argument the tool accepts.
fn query_to_tool(query: &QueryDefinition, display_name: &str) -> Tool {
    let description = query.description.clone().unwrap_or_else(|| format!("Query: {display_name}"));

    Tool::new(
        Cow::Owned(display_name.to_string()),
        Cow::Owned(description),
        Arc::new(arguments_to_json_schema(&ExposedOperation::Query(query).arguments())),
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
