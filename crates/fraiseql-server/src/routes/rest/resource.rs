//! REST resource derivation engine.
//!
//! Derives REST resources and routes from a [`CompiledSchema`] by grouping
//! operations by return type and mapping them to HTTP methods and paths.

use std::{collections::HashMap, fmt};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, DeleteResponse, FieldType, MutationDefinition,
    MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
};
use tracing::debug;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// HTTP method for a REST route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    /// HTTP GET.
    Get,
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
    /// HTTP PATCH.
    Patch,
    /// HTTP DELETE.
    Delete,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

/// Classification of an Update mutation's field coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCoverage {
    /// Mutation covers all writable fields — generates both PUT and PATCH.
    Full,
    /// Mutation covers only a subset — generates PATCH as a sub-resource action.
    Partial,
}

/// The kind of operation backing a REST route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteSource {
    /// Backed by a compiled query.
    Query {
        /// Query operation name.
        name: String,
    },
    /// Backed by a compiled mutation.
    Mutation {
        /// Mutation operation name.
        name: String,
    },
}

/// A single REST route derived from the compiled schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestRoute {
    /// HTTP method.
    pub method:          HttpMethod,
    /// Path relative to the REST base (e.g., `/users` or `/users/{id}`).
    pub path:            String,
    /// The operation backing this route.
    pub source:          RouteSource,
    /// For Update mutations, the coverage classification.
    pub update_coverage: Option<UpdateCoverage>,
    /// Expected successful HTTP status code.
    pub success_status:  u16,
}

/// A REST resource groups routes under a common base path derived from a
/// return type.
#[derive(Debug, Clone)]
pub struct RestResource {
    /// Resource base name (e.g., `users`).
    pub name:      String,
    /// GraphQL return type name (e.g., `User`).
    pub type_name: String,
    /// Name of the ID argument for single-resource routes (e.g., `id`).
    pub id_arg:    Option<String>,
    /// Routes for this resource.
    pub routes:    Vec<RestRoute>,
}

/// Complete route table derived from a compiled schema.
#[derive(Debug, Clone)]
pub struct RestRouteTable {
    /// REST base path (e.g., `/rest/v1`).
    pub base_path:   String,
    /// Resources keyed by resource name.
    pub resources:   Vec<RestResource>,
    /// Diagnostics emitted during derivation.
    pub diagnostics: Vec<Diagnostic>,
}

/// A diagnostic message from the derivation engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity level.
    pub level:   DiagnosticLevel,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Informational (e.g., fallback resource name derived from type).
    Info,
    /// Warning (e.g., CQRS naming violation).
    Warning,
    /// Error (e.g., route conflict).
    Error,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl RestRouteTable {
    /// Derive a route table from a compiled schema.
    ///
    /// Returns `Err` if route conflicts are detected that cannot be resolved.
    ///
    /// # Errors
    ///
    /// Returns an error string if two operations produce the same method+path
    /// combination and neither has a `rest_path` override.
    pub fn from_compiled_schema(schema: &CompiledSchema) -> Result<Self, String> {
        let config = schema.rest_config.clone().unwrap_or_default();
        let base_path = config.path.clone();

        // Group operations by return type.
        let mut query_groups: HashMap<&str, Vec<&QueryDefinition>> = HashMap::new();
        let mut mutation_groups: HashMap<&str, Vec<&MutationDefinition>> = HashMap::new();

        for q in &schema.queries {
            if should_skip_query(q) {
                debug!(query = %q.name, "skipping query (aggregate/window/scalar)");
                continue;
            }
            if is_filtered_out(&q.name, &config) {
                debug!(query = %q.name, "skipping query (include/exclude filter)");
                continue;
            }
            // Check return type has a TypeDefinition.
            if schema.find_type(&q.return_type).is_none() {
                debug!(query = %q.name, return_type = %q.return_type, "skipping query (no TypeDefinition)");
                continue;
            }
            query_groups.entry(q.return_type.as_str()).or_default().push(q);
        }

        for m in &schema.mutations {
            if is_filtered_out(&m.name, &config) {
                debug!(mutation = %m.name, "skipping mutation (include/exclude filter)");
                continue;
            }
            if schema.find_type(&m.return_type).is_none() {
                debug!(mutation = %m.name, return_type = %m.return_type, "skipping mutation (no TypeDefinition)");
                continue;
            }
            mutation_groups.entry(m.return_type.as_str()).or_default().push(m);
        }

        // Collect all return types.
        let mut all_types: Vec<&str> = query_groups.keys().copied().collect();
        for t in mutation_groups.keys() {
            if !all_types.contains(t) {
                all_types.push(t);
            }
        }
        all_types.sort_unstable();

        let mut resources = Vec::new();
        let mut diagnostics = Vec::new();

        for type_name in all_types {
            let type_def = match schema.find_type(type_name) {
                Some(td) => td,
                None => continue,
            };
            let queries = query_groups.get(type_name).map_or(&[][..], |v| v.as_slice());
            let mutations = mutation_groups.get(type_name).map_or(&[][..], |v| v.as_slice());

            let resource =
                derive_resource(type_name, type_def, queries, mutations, &config, &mut diagnostics);

            if let Some(r) = resource {
                resources.push(r);
            }
        }

        // Detect route conflicts.
        detect_conflicts(&resources, &mut diagnostics)?;

        let table = Self {
            base_path,
            resources,
            diagnostics,
        };

        Ok(table)
    }
}

impl fmt::Display for RestRouteTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "REST Route Table (base: {})", self.base_path)?;
        for resource in &self.resources {
            writeln!(f, "  Resource: {} (type: {})", resource.name, resource.type_name)?;
            for route in &resource.routes {
                writeln!(f, "    {} {}{}", route.method, self.base_path, route.path)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check if a query should be skipped (aggregate, window, or scalar return).
fn should_skip_query(q: &QueryDefinition) -> bool {
    q.name.ends_with("_aggregate") || q.name.ends_with("_window")
}

/// Check if an operation name is filtered out by include/exclude lists.
fn is_filtered_out(name: &str, config: &RestConfig) -> bool {
    if !config.include.is_empty() && !config.include.iter().any(|i| i == name) {
        return true;
    }
    config.exclude.iter().any(|e| e == name)
}

/// Derive the resource name from a list query name or type name.
fn derive_resource_name(
    type_name: &str,
    queries: &[&QueryDefinition],
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    // Prefer the list query name as resource name.
    if let Some(list_q) = queries.iter().find(|q| q.returns_list) {
        return list_q.name.clone();
    }

    // Fall back: strip CQRS prefix from sql_source if available, then pluralize.
    if let Some(q) = queries.first() {
        if let Some(ref sql) = q.sql_source {
            let stripped = strip_cqrs_prefix(sql);
            if !stripped.is_empty() {
                return simple_pluralize(stripped);
            }
        }
    }

    // Last resort: lowercase type name + simple pluralize.
    let base = type_name_to_snake(type_name);
    let name = simple_pluralize(&base);
    diagnostics.push(Diagnostic {
        level:   DiagnosticLevel::Info,
        message: format!(
            "No list query for type '{type_name}'; derived resource name '{name}' from type name"
        ),
    });
    name
}

/// Strip CQRS prefixes (`v_`, `tv_`, `tb_`) from a SQL identifier.
fn strip_cqrs_prefix(name: &str) -> &str {
    name.strip_prefix("v_")
        .or_else(|| name.strip_prefix("tv_"))
        .or_else(|| name.strip_prefix("tb_"))
        .unwrap_or(name)
}

/// Convert `PascalCase` type name to `snake_case`.
fn type_name_to_snake(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Very simple English pluralization (covers common cases).
fn simple_pluralize(word: &str) -> String {
    // Already-plural heuristic: words ending in "ics", "ies", "es" (preceded by
    // consonant cluster), or trailing "s" after a vowel+consonant are kept as-is.
    if word.ends_with("ics") || word.ends_with("ies") {
        return word.to_string();
    }
    if word.ends_with("ss") || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        format!("{word}es")
    } else if word.ends_with('s') {
        // Words ending in single 's' (like "address", "status") — assume already plural-ish.
        format!("{word}es")
    } else if word.ends_with('y')
        && !word.ends_with("ey")
        && !word.ends_with("ay")
        && !word.ends_with("oy")
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// Detect the ID argument for single-resource routes.
///
/// Prefers `id: UUID/ID/Int` argument. Falls back to `pk_*` arguments.
fn detect_id_arg(
    type_def: &TypeDefinition,
    mutations: &[&MutationDefinition],
    queries: &[&QueryDefinition],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    // Check mutation/query arguments for `id` by name.
    let all_args: Vec<&ArgumentDefinition> = mutations
        .iter()
        .flat_map(|m| &m.arguments)
        .chain(queries.iter().filter(|q| !q.returns_list).flat_map(|q| &q.arguments))
        .collect();

    // Prefer `id` argument of type ID, UUID, Int, or String.
    if let Some(arg) = all_args.iter().find(|a| a.name == "id" && is_id_like_type(&a.arg_type)) {
        return Some(arg.name.clone());
    }

    // Fall back to pk_* argument.
    if let Some(arg) = all_args
        .iter()
        .find(|a| a.name.starts_with("pk_") && is_id_like_type(&a.arg_type))
    {
        let type_name = type_def.name.as_str();
        diagnostics.push(Diagnostic {
            level:   DiagnosticLevel::Info,
            message: format!(
                "No `id` field found on '{type_name}'; using `{}` as path parameter",
                arg.name
            ),
        });
        return Some(arg.name.clone());
    }

    // Check the type's fields as a last resort.
    if type_def.find_field("id").is_some() {
        return Some("id".to_string());
    }
    if let Some(pk) = type_def.fields.iter().find(|f| f.name.as_str().starts_with("pk_")) {
        let type_name = type_def.name.as_str();
        diagnostics.push(Diagnostic {
            level:   DiagnosticLevel::Info,
            message: format!(
                "No `id` field found on '{type_name}'; using `{}` as path parameter",
                pk.name
            ),
        });
        return Some(pk.name.to_string());
    }

    None
}

/// Check if a `FieldType` is suitable as an ID parameter.
const fn is_id_like_type(ft: &FieldType) -> bool {
    matches!(ft, FieldType::Id | FieldType::Uuid | FieldType::Int | FieldType::String)
}

/// Derive a single resource from grouped operations.
fn derive_resource(
    type_name: &str,
    type_def: &TypeDefinition,
    queries: &[&QueryDefinition],
    mutations: &[&MutationDefinition],
    config: &RestConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<RestResource> {
    let resource_name = derive_resource_name(type_name, queries, diagnostics);

    // CQRS validation on queries.
    for q in queries {
        if let Some(ref sql) = q.sql_source {
            validate_cqrs_query(sql, &q.name, diagnostics);
        }
    }

    // CQRS validation on mutations.
    for m in mutations {
        validate_cqrs_mutation(&m.operation, &m.name, diagnostics);
    }

    // CQRS field type validation.
    validate_field_types(type_def, diagnostics);

    let id_arg = detect_id_arg(type_def, mutations, queries, diagnostics);
    let mut routes = Vec::new();

    // --- Query routes ---
    for q in queries {
        if let Some(ref override_path) = q.rest_path {
            let method =
                q.rest_method.as_deref().and_then(parse_http_method).unwrap_or(HttpMethod::Get);
            routes.push(RestRoute {
                method,
                path: override_path.clone(),
                source: RouteSource::Query {
                    name: q.name.clone(),
                },
                update_coverage: None,
                success_status: 200,
            });
            continue;
        }

        if q.returns_list {
            routes.push(RestRoute {
                method:          HttpMethod::Get,
                path:            format!("/{resource_name}"),
                source:          RouteSource::Query {
                    name: q.name.clone(),
                },
                update_coverage: None,
                success_status:  200,
            });
        } else if let Some(ref id) = id_arg {
            routes.push(RestRoute {
                method:          HttpMethod::Get,
                path:            format!("/{resource_name}/{{{id}}}"),
                source:          RouteSource::Query {
                    name: q.name.clone(),
                },
                update_coverage: None,
                success_status:  200,
            });
        }
    }

    // --- Mutation routes ---
    let writable_fields = type_def.writable_fields();
    let writable_names: Vec<&str> = writable_fields.iter().map(|f| f.name.as_str()).collect();

    for m in mutations {
        if let Some(ref override_path) = m.rest_path {
            let method =
                m.rest_method.as_deref().and_then(parse_http_method).unwrap_or(HttpMethod::Post);
            routes.push(RestRoute {
                method,
                path: override_path.clone(),
                source: RouteSource::Mutation {
                    name: m.name.clone(),
                },
                update_coverage: None,
                success_status: 200,
            });
            continue;
        }

        derive_mutation_routes(
            m,
            type_name,
            &resource_name,
            id_arg.as_ref(),
            &writable_names,
            config,
            &mut routes,
            diagnostics,
        );
    }

    if routes.is_empty() {
        return None;
    }

    Some(RestResource {
        name: resource_name,
        type_name: type_name.to_string(),
        id_arg,
        routes,
    })
}

/// Derive routes from a single mutation.
#[allow(clippy::too_many_arguments)] // Reason: internal helper, all params are needed
fn derive_mutation_routes(
    m: &MutationDefinition,
    type_name: &str,
    resource_name: &str,
    id_arg: Option<&String>,
    writable_names: &[&str],
    config: &RestConfig,
    routes: &mut Vec<RestRoute>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &m.operation {
        MutationOperation::Insert { .. } => {
            routes.push(RestRoute {
                method:          HttpMethod::Post,
                path:            format!("/{resource_name}"),
                source:          RouteSource::Mutation {
                    name: m.name.clone(),
                },
                update_coverage: None,
                success_status:  201,
            });
        },
        MutationOperation::Update { .. } => {
            let coverage = classify_update_coverage(m, writable_names);
            diagnostics.push(Diagnostic {
                level:   DiagnosticLevel::Info,
                message: format!(
                    "Mutation '{}' classified as {:?} coverage update for type '{type_name}'",
                    m.name, coverage
                ),
            });

            match coverage {
                UpdateCoverage::Full => {
                    if let Some(id) = id_arg {
                        routes.push(RestRoute {
                            method:          HttpMethod::Put,
                            path:            format!("/{resource_name}/{{{id}}}"),
                            source:          RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Full),
                            success_status:  200,
                        });
                        routes.push(RestRoute {
                            method:          HttpMethod::Patch,
                            path:            format!("/{resource_name}/{{{id}}}"),
                            source:          RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Full),
                            success_status:  200,
                        });
                    }
                },
                UpdateCoverage::Partial => {
                    let action = derive_action_name(&m.name, type_name);
                    if let Some(id) = id_arg {
                        routes.push(RestRoute {
                            method:          HttpMethod::Patch,
                            path:            format!("/{resource_name}/{{{id}}}/{action}"),
                            source:          RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Partial),
                            success_status:  200,
                        });
                    }
                },
            }
        },
        MutationOperation::Delete { .. } => {
            let status = match config.delete_response {
                DeleteResponse::NoContent => 204,
                DeleteResponse::Entity => 200,
                // non_exhaustive future variants default to 204.
                _ => 204,
            };
            if let Some(id) = id_arg {
                routes.push(RestRoute {
                    method:          HttpMethod::Delete,
                    path:            format!("/{resource_name}/{{{id}}}"),
                    source:          RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status:  status,
                });
            }
        },
        MutationOperation::Custom => {
            let action = derive_action_name(&m.name, type_name);
            if let Some(id) = id_arg {
                routes.push(RestRoute {
                    method:          HttpMethod::Post,
                    path:            format!("/{resource_name}/{{{id}}}/{action}"),
                    source:          RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status:  200,
                });
            } else {
                routes.push(RestRoute {
                    method:          HttpMethod::Post,
                    path:            format!("/{resource_name}/{action}"),
                    source:          RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status:  200,
                });
            }
        },
    }
}

/// Classify whether an Update mutation covers all writable fields.
fn classify_update_coverage(m: &MutationDefinition, writable_names: &[&str]) -> UpdateCoverage {
    // Mutation args (excluding the ID argument) vs writable fields.
    let mutation_arg_names: Vec<&str> = m
        .arguments
        .iter()
        .filter(|a| !is_id_like_arg(a))
        .map(|a| a.name.as_str())
        .collect();

    // Full coverage: mutation args cover ALL writable fields.
    let covers_all = writable_names.iter().all(|wf| mutation_arg_names.contains(wf));

    if covers_all {
        UpdateCoverage::Full
    } else {
        UpdateCoverage::Partial
    }
}

/// Check if an argument looks like an ID parameter (for exclusion from coverage check).
fn is_id_like_arg(arg: &ArgumentDefinition) -> bool {
    (arg.name == "id" || arg.name.starts_with("pk_")) && is_id_like_type(&arg.arg_type)
}

/// Strip the type-name prefix from a mutation name and kebab-case the remainder.
///
/// `archiveUser` on type `User` → `archive`
/// `updateUserEmail` on type `User` → `update-email`
fn derive_action_name(mutation_name: &str, type_name: &str) -> String {
    // Find the type name (case-insensitive) within the mutation name and remove it.
    // e.g., "archiveUser" → find "User" at pos 7 → "archive"
    // e.g., "updateUserEmail" → find "User" at pos 6 → "updateEmail" → "update-email"
    let lower_mutation = mutation_name.to_ascii_lowercase();
    let lower_type = type_name.to_ascii_lowercase();

    let without_type = if let Some(pos) = lower_mutation.find(&lower_type) {
        let before = &mutation_name[..pos];
        let after = &mutation_name[pos + type_name.len()..];
        format!("{before}{after}")
    } else {
        mutation_name.to_string()
    };

    camel_to_kebab(&without_type)
}

/// Convert a `camelCase` or `PascalCase` string to `kebab-case`.
fn camel_to_kebab(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(ch.to_ascii_lowercase());
    }
    // Trim leading dash if first char was uppercase.
    if result.starts_with('-') {
        result.remove(0);
    }
    result
}

/// Validate CQRS naming: queries should read from `v_*` or `tv_*`.
fn validate_cqrs_query(sql_source: &str, query_name: &str, diagnostics: &mut Vec<Diagnostic>) {
    if sql_source.starts_with("tb_") {
        diagnostics.push(Diagnostic {
            level:   DiagnosticLevel::Warning,
            message: format!(
                "Query '{query_name}' reads from write table '{sql_source}' \
                 — expected `v_` or `tv_` prefix. This may indicate a CQRS violation."
            ),
        });
    }
}

/// Validate CQRS naming: mutations should write to `tb_*`.
fn validate_cqrs_mutation(
    op: &MutationOperation,
    mutation_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let table = match op {
        MutationOperation::Insert { table }
        | MutationOperation::Update { table }
        | MutationOperation::Delete { table } => table.as_str(),
        MutationOperation::Custom => return,
    };

    if table.starts_with("v_") || table.starts_with("tv_") {
        diagnostics.push(Diagnostic {
            level:   DiagnosticLevel::Warning,
            message: format!(
                "Mutation '{mutation_name}' writes to view '{table}' — expected `tb_` prefix"
            ),
        });
    }
}

/// Validate pk_*/fk_*/id field types.
fn validate_field_types(type_def: &TypeDefinition, diagnostics: &mut Vec<Diagnostic>) {
    for field in &type_def.fields {
        let name: &str = field.name.as_str();
        if name.starts_with("pk_") || name.starts_with("fk_") {
            if !matches!(field.field_type, FieldType::Int | FieldType::Id) {
                diagnostics.push(Diagnostic {
                    level:   DiagnosticLevel::Warning,
                    message: format!(
                        "pk_/fk_ field '{name}' is {:?}, expected Int or BigInt",
                        field.field_type
                    ),
                });
            }
        } else if name == "id" && matches!(field.field_type, FieldType::Int) {
            diagnostics.push(Diagnostic {
                level:   DiagnosticLevel::Warning,
                message: format!(
                    "id field on '{}' is Int, expected UUID or ID",
                    type_def.name.as_str()
                ),
            });
        }
    }
}

/// Detect conflicting routes (same method+path from different operations).
fn detect_conflicts(
    resources: &[RestResource],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    let mut seen: HashMap<(HttpMethod, String), &str> = HashMap::new();

    for resource in resources {
        for route in &resource.routes {
            let key = (route.method, route.path.clone());
            if let Some(prev_op) = seen.get(&key) {
                let current_op = match &route.source {
                    RouteSource::Query { name } | RouteSource::Mutation { name } => name.as_str(),
                };
                let err = format!(
                    "Route conflict: {} {} is claimed by both '{}' and '{}'. \
                     Use `rest_path` override to resolve.",
                    route.method, route.path, prev_op, current_op
                );
                diagnostics.push(Diagnostic {
                    level:   DiagnosticLevel::Error,
                    message: err.clone(),
                });
                return Err(err);
            }
            let op_name = match &route.source {
                RouteSource::Query { name } | RouteSource::Mutation { name } => name.as_str(),
            };
            seen.insert(key, op_name);
        }
    }

    Ok(())
}

/// Parse an HTTP method string to `HttpMethod`.
fn parse_http_method(s: &str) -> Option<HttpMethod> {
    match s.to_ascii_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "PATCH" => Some(HttpMethod::Patch),
        "DELETE" => Some(HttpMethod::Delete),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use fraiseql_core::schema::{FieldDefinition, FieldEncryptionConfig, FieldType};

    use super::*;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn user_type_def() -> TypeDefinition {
        let mut auto = FieldDefinition::new("created_at", FieldType::DateTime);
        auto.auto_generated = true;
        let mut computed = FieldDefinition::new("full_name", FieldType::String);
        computed.computed = true;

        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Uuid))
            .with_field(FieldDefinition::new("pk_user", FieldType::Int))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(auto)
            .with_field(computed)
            .with_field(FieldDefinition::new("ssn", FieldType::String).with_encryption(
                FieldEncryptionConfig {
                    key_reference: "keys/ssn".to_string(),
                    algorithm:     "AES-256-GCM".to_string(),
                },
            ))
    }

    fn list_query(name: &str, return_type: &str) -> QueryDefinition {
        QueryDefinition::new(name, return_type).returning_list()
    }

    fn single_query(name: &str, return_type: &str) -> QueryDefinition {
        let mut q = QueryDefinition::new(name, return_type);
        q.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
        q
    }

    fn insert_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, return_type);
        m.operation = MutationOperation::Insert {
            table: table.to_string(),
        };
        m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
        m.arguments.push(ArgumentDefinition::new("name", FieldType::String));
        m
    }

    fn full_update_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, return_type);
        m.operation = MutationOperation::Update {
            table: table.to_string(),
        };
        m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
        // All writable fields of user_type_def: email, name.
        m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
        m.arguments.push(ArgumentDefinition::new("name", FieldType::String));
        m
    }

    fn partial_update_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, return_type);
        m.operation = MutationOperation::Update {
            table: table.to_string(),
        };
        m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
        // Only email — partial coverage.
        m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
        m
    }

    fn delete_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, return_type);
        m.operation = MutationOperation::Delete {
            table: table.to_string(),
        };
        m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
        m
    }

    fn custom_mutation(name: &str, return_type: &str) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, return_type);
        m.operation = MutationOperation::Custom;
        m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
        m
    }

    fn schema_with_rest_config(config: Option<RestConfig>) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.rest_config = config;
        schema
    }

    // -----------------------------------------------------------------------
    // Full resource derivation
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_crud_resource() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User").with_sql_source("v_user"));
        schema.queries.push(single_query("user", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
        schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
        schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert_eq!(table.resources.len(), 1);
        let r = &table.resources[0];
        assert_eq!(r.name, "users");
        assert_eq!(r.type_name, "User");
        assert_eq!(r.id_arg.as_deref(), Some("id"));

        let methods: Vec<_> = r.routes.iter().map(|rt| (rt.method, rt.path.as_str())).collect();
        assert!(methods.contains(&(HttpMethod::Get, "/users")));
        assert!(methods.contains(&(HttpMethod::Get, "/users/{id}")));
        assert!(methods.contains(&(HttpMethod::Post, "/users")));
        assert!(methods.contains(&(HttpMethod::Put, "/users/{id}")));
        assert!(methods.contains(&(HttpMethod::Patch, "/users/{id}")));
        assert!(methods.contains(&(HttpMethod::Delete, "/users/{id}")));
    }

    #[test]
    fn test_full_coverage_update_generates_put_and_patch() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
        schema.queries.push(single_query("user", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        let update_routes: Vec<_> = r
            .routes
            .iter()
            .filter(|rt| rt.update_coverage == Some(UpdateCoverage::Full))
            .collect();
        assert_eq!(update_routes.len(), 2);
        assert!(update_routes.iter().any(|rt| rt.method == HttpMethod::Put));
        assert!(update_routes.iter().any(|rt| rt.method == HttpMethod::Patch));
    }

    #[test]
    fn test_partial_coverage_update_generates_patch_action() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema
            .mutations
            .push(partial_update_mutation("updateUserEmail", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        let patch_route = r.routes.iter().find(|rt| rt.method == HttpMethod::Patch).unwrap();
        assert_eq!(patch_route.path, "/users/{id}/update-email");
        assert_eq!(patch_route.update_coverage, Some(UpdateCoverage::Partial));
    }

    #[test]
    fn test_custom_mutation_post_action() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(custom_mutation("archiveUser", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        let custom = r.routes.iter().find(|rt| rt.method == HttpMethod::Post).unwrap();
        assert_eq!(custom.path, "/users/{id}/archive");
        assert_eq!(custom.success_status, 200);
    }

    #[test]
    fn test_no_list_query_derives_name_from_type() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        // Only a single query, no list query.
        schema.queries.push(single_query("user", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        assert_eq!(r.name, "users");
        assert!(table.diagnostics.iter().any(|d| d.message.contains("No list query")));
    }

    #[test]
    fn test_rest_path_override_on_query() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        let mut q = list_query("users", "User");
        q.rest_path = Some("/custom/users".to_string());
        schema.queries.push(q);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        let route = r.routes.iter().find(|rt| rt.path == "/custom/users").unwrap();
        assert_eq!(route.method, HttpMethod::Get);
    }

    #[test]
    fn test_rest_path_override_on_mutation() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        let mut m = insert_mutation("createUser", "User", "tb_user");
        m.rest_path = Some("/custom/create".to_string());
        m.rest_method = Some("PUT".to_string());
        schema.mutations.push(m);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        let route = r.routes.iter().find(|rt| rt.path == "/custom/create").unwrap();
        assert_eq!(route.method, HttpMethod::Put);
    }

    // -----------------------------------------------------------------------
    // Route conflict detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_route_conflict_detected() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        // Two full-coverage updates → conflict on PUT /users/{id}.
        schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
        let mut m2 = full_update_mutation("updateUser2", "User", "tb_user");
        m2.name = "updateUser2".to_string();
        schema.mutations.push(m2);

        let result = RestRouteTable::from_compiled_schema(&schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Route conflict"));
    }

    // -----------------------------------------------------------------------
    // Exclusion rules
    // -----------------------------------------------------------------------

    #[test]
    fn test_scalar_return_type_excluded() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        // No TypeDefinition for "Int" — query returning Int is excluded.
        let q = QueryDefinition::new("totalCount", "Int").returning_list();
        schema.queries.push(q);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.resources.is_empty());
    }

    #[test]
    fn test_aggregate_query_excluded() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(TypeDefinition::new("UserAggregate", "v_user_aggregate"));
        schema.queries.push(list_query("users_aggregate", "UserAggregate"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.resources.is_empty());
    }

    #[test]
    fn test_window_query_excluded() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(TypeDefinition::new("SalesWindow", "tv_sales_window"));
        schema.queries.push(list_query("sales_window", "SalesWindow"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.resources.is_empty());
    }

    // -----------------------------------------------------------------------
    // Include/exclude filters
    // -----------------------------------------------------------------------

    #[test]
    fn test_exclude_filter() {
        let config = RestConfig {
            exclude: vec!["deleteUser".to_string()],
            ..RestConfig::default()
        };
        let mut schema = schema_with_rest_config(Some(config));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
        schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        assert!(!r.routes.iter().any(|rt| rt.method == HttpMethod::Delete));
    }

    #[test]
    fn test_include_filter() {
        let config = RestConfig {
            include: vec!["users".to_string(), "createUser".to_string()],
            ..RestConfig::default()
        };
        let mut schema = schema_with_rest_config(Some(config));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
        schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        assert!(r.routes.iter().any(|rt| rt.method == HttpMethod::Get));
        assert!(r.routes.iter().any(|rt| rt.method == HttpMethod::Post));
        assert!(!r.routes.iter().any(|rt| rt.method == HttpMethod::Delete));
    }

    // -----------------------------------------------------------------------
    // CQRS validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_cqrs_query_from_view_no_warning() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User").with_sql_source("v_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(
            !table
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("CQRS"))
        );
    }

    #[test]
    fn test_cqrs_query_from_table_warns() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User").with_sql_source("tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.diagnostics.iter().any(|d| {
            d.level == DiagnosticLevel::Warning && d.message.contains("reads from write table")
        }));
    }

    #[test]
    fn test_cqrs_query_from_table_view_no_warning() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("Analytics", "tv_analytics");
        schema.types.push(td);
        schema
            .queries
            .push(list_query("analytics", "Analytics").with_sql_source("tv_analytics"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(
            !table
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("CQRS"))
        );
    }

    #[test]
    fn test_cqrs_mutation_to_view_warns() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "v_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.diagnostics.iter().any(|d| {
            d.level == DiagnosticLevel::Warning && d.message.contains("writes to view")
        }));
    }

    #[test]
    fn test_cqrs_mutation_to_table_no_warning() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(
            !table
                .diagnostics
                .iter()
                .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("writes to"))
        );
    }

    // -----------------------------------------------------------------------
    // PK field type validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_pk_field_varchar_warns() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("pk_user", FieldType::String));
        schema.types.push(td);
        schema.queries.push(list_query("users", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.diagnostics.iter().any(|d| {
            d.level == DiagnosticLevel::Warning && d.message.contains("pk_/fk_ field 'pk_user'")
        }));
    }

    #[test]
    fn test_id_field_bigint_warns() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Int))
            .with_field(FieldDefinition::new("email", FieldType::String));
        schema.types.push(td);
        schema.queries.push(list_query("users", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert!(table.diagnostics.iter().any(|d| {
            d.level == DiagnosticLevel::Warning && d.message.contains("id field on 'User' is Int")
        }));
    }

    // -----------------------------------------------------------------------
    // ID parameter detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_pk_fallback_when_no_id() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("pk_user", FieldType::Int))
            .with_field(FieldDefinition::new("email", FieldType::String));
        schema.types.push(td);
        let mut m = MutationDefinition::new("updateUser", "User");
        m.operation = MutationOperation::Update {
            table: "tb_user".to_string(),
        };
        m.arguments.push(ArgumentDefinition::new("pk_user", FieldType::Int));
        m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
        schema.mutations.push(m);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        assert_eq!(r.id_arg.as_deref(), Some("pk_user"));
        assert!(table.diagnostics.iter().any(|d| d.message.contains("using `pk_user`")));
    }

    // -----------------------------------------------------------------------
    // Resource name derivation from CQRS
    // -----------------------------------------------------------------------

    #[test]
    fn test_resource_name_from_view() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Uuid));
        schema.types.push(td);
        // No list query, but single query with sql_source.
        let q = QueryDefinition::new("user", "User").with_sql_source("v_user");
        schema.queries.push(q);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        assert_eq!(r.name, "users");
    }

    #[test]
    fn test_resource_name_from_table_view() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        let td = TypeDefinition::new("Analytics", "tv_analytics")
            .with_field(FieldDefinition::new("id", FieldType::Uuid));
        schema.types.push(td);
        let q = QueryDefinition::new("analytics_item", "Analytics").with_sql_source("tv_analytics");
        schema.queries.push(q);

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let r = &table.resources[0];
        // Falls back since no list query; derives from sql_source.
        assert_eq!(r.name, "analytics");
    }

    // -----------------------------------------------------------------------
    // Action naming
    // -----------------------------------------------------------------------

    #[test]
    fn test_action_name_archive_user() {
        assert_eq!(derive_action_name("archiveUser", "User"), "archive");
    }

    #[test]
    fn test_action_name_update_user_email() {
        assert_eq!(derive_action_name("updateUserEmail", "User"), "update-email");
    }

    #[test]
    fn test_action_name_no_prefix_match() {
        assert_eq!(derive_action_name("doSomething", "User"), "do-something");
    }

    // -----------------------------------------------------------------------
    // DeleteResponse config
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete_response_no_content() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let del = table.resources[0]
            .routes
            .iter()
            .find(|r| r.method == HttpMethod::Delete)
            .unwrap();
        assert_eq!(del.success_status, 204);
    }

    #[test]
    fn test_delete_response_entity() {
        let config = RestConfig {
            delete_response: DeleteResponse::Entity,
            ..RestConfig::default()
        };
        let mut schema = schema_with_rest_config(Some(config));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let del = table.resources[0]
            .routes
            .iter()
            .find(|r| r.method == HttpMethod::Delete)
            .unwrap();
        assert_eq!(del.success_status, 200);
    }

    // -----------------------------------------------------------------------
    // Display trait
    // -----------------------------------------------------------------------

    #[test]
    fn test_route_table_display() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let display = format!("{table}");
        assert!(display.contains("REST Route Table"));
        assert!(display.contains("/rest/v1"));
        assert!(display.contains("GET"));
    }

    // -----------------------------------------------------------------------
    // Helper unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_simple_pluralize() {
        assert_eq!(simple_pluralize("user"), "users");
        assert_eq!(simple_pluralize("bus"), "buses");
        assert_eq!(simple_pluralize("box"), "boxes");
        assert_eq!(simple_pluralize("church"), "churches");
        assert_eq!(simple_pluralize("dish"), "dishes");
        assert_eq!(simple_pluralize("category"), "categories");
        assert_eq!(simple_pluralize("key"), "keys");
        assert_eq!(simple_pluralize("analytics"), "analytics");
    }

    #[test]
    fn test_camel_to_kebab() {
        assert_eq!(camel_to_kebab("updateEmail"), "update-email");
        assert_eq!(camel_to_kebab("archive"), "archive");
        assert_eq!(camel_to_kebab("UpdateEmail"), "update-email");
        assert_eq!(camel_to_kebab(""), "");
    }

    #[test]
    fn test_type_name_to_snake() {
        assert_eq!(type_name_to_snake("User"), "user");
        assert_eq!(type_name_to_snake("BlogPost"), "blog_post");
        assert_eq!(type_name_to_snake("HTTPResponse"), "h_t_t_p_response");
    }

    #[test]
    fn test_strip_cqrs_prefix() {
        assert_eq!(strip_cqrs_prefix("v_user"), "user");
        assert_eq!(strip_cqrs_prefix("tv_analytics"), "analytics");
        assert_eq!(strip_cqrs_prefix("tb_user"), "user");
        assert_eq!(strip_cqrs_prefix("user"), "user");
    }

    #[test]
    fn test_is_filtered_out() {
        let config = RestConfig {
            include: vec!["users".to_string()],
            ..RestConfig::default()
        };
        assert!(!is_filtered_out("users", &config));
        assert!(is_filtered_out("posts", &config));

        let config2 = RestConfig {
            exclude: vec!["deleteUser".to_string()],
            ..RestConfig::default()
        };
        assert!(!is_filtered_out("createUser", &config2));
        assert!(is_filtered_out("deleteUser", &config2));
    }

    #[test]
    fn test_no_rest_config_uses_defaults() {
        let mut schema = CompiledSchema::new();
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        assert_eq!(table.base_path, "/rest/v1");
        assert_eq!(table.resources.len(), 1);
    }

    #[test]
    fn test_insert_mutation_returns_201() {
        let mut schema = schema_with_rest_config(Some(RestConfig::default()));
        schema.types.push(user_type_def());
        schema.queries.push(list_query("users", "User"));
        schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));

        let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
        let create =
            table.resources[0].routes.iter().find(|r| r.method == HttpMethod::Post).unwrap();
        assert_eq!(create.success_status, 201);
    }
}
