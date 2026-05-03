//! Helper functions for REST resource derivation.
//!
//! Contains utility functions for resource name derivation, validation,
//! and REST route classification.

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, DeleteResponse, FieldType, MutationDefinition,
    MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
};

use super::{Diagnostic, DiagnosticLevel, RestRoute, RestResource, UpdateCoverage, RouteSource, HttpMethod};

        return true;
    }
    config.exclude.iter().any(|e| e == name)
}

/// Derive the resource name from a list query name or type name.
pub(super) fn derive_resource_name(
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
pub(super) fn strip_cqrs_prefix(name: &str) -> &str {
    name.strip_prefix("v_")
        .or_else(|| name.strip_prefix("tv_"))
        .or_else(|| name.strip_prefix("tb_"))
        .unwrap_or(name)
}

/// Convert `PascalCase` type name to `snake_case`.
pub(super) fn type_name_to_snake(name: &str) -> String {
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
pub(super) fn simple_pluralize(word: &str) -> String {
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
pub(super) fn detect_id_arg(
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
pub(super) fn derive_resource(
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
pub(super) fn derive_mutation_routes(
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
        // Reason: MutationOperation is #[non_exhaustive]; skip unknown variants.
        _ => {},
    }
}

/// Classify whether an Update mutation covers all writable fields.
pub(super) fn classify_update_coverage(m: &MutationDefinition, writable_names: &[&str]) -> UpdateCoverage {
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
pub(super) fn is_id_like_arg(arg: &ArgumentDefinition) -> bool {
    (arg.name == "id" || arg.name.starts_with("pk_")) && is_id_like_type(&arg.arg_type)
}

/// Strip the type-name prefix from a mutation name and kebab-case the remainder.
///
/// `archiveUser` on type `User` → `archive`
/// `updateUserEmail` on type `User` → `update-email`
pub(super) fn derive_action_name(mutation_name: &str, type_name: &str) -> String {
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
pub(super) fn camel_to_kebab(s: &str) -> String {
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
pub(super) fn validate_cqrs_query(sql_source: &str, query_name: &str, diagnostics: &mut Vec<Diagnostic>) {
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
pub(super) fn validate_cqrs_mutation(
    op: &MutationOperation,
    mutation_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let table = match op {
        MutationOperation::Insert { table }
        | MutationOperation::Update { table }
        | MutationOperation::Delete { table } => table.as_str(),
        // Reason: MutationOperation is #[non_exhaustive]; Custom and unknown variants are skipped.
        _ => return,
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
pub(super) fn validate_field_types(type_def: &TypeDefinition, diagnostics: &mut Vec<Diagnostic>) {
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
pub(super) fn detect_conflicts(
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
pub(super) fn parse_http_method(s: &str) -> Option<HttpMethod> {
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

