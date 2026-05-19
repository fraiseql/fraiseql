//! REST resource derivation: ID detection, route classification, and resource building.

use fraiseql_core::schema::{
    ArgumentDefinition, DeleteResponse, FieldType, MutationDefinition, MutationOperation,
    QueryDefinition, RestConfig, TypeDefinition,
};

use super::{
    Diagnostic, DiagnosticLevel, HttpMethod, RestResource, RestRoute, RouteSource, UpdateCoverage,
    naming::{derive_action_name, derive_resource_name},
    validation::{validate_cqrs_mutation, validate_cqrs_query, validate_field_types},
};

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
            level: DiagnosticLevel::Info,
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
            level: DiagnosticLevel::Info,
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

/// Check if an argument looks like an ID parameter (for exclusion from coverage check).
pub(super) fn is_id_like_arg(arg: &ArgumentDefinition) -> bool {
    (arg.name == "id" || arg.name.starts_with("pk_")) && is_id_like_type(&arg.arg_type)
}

/// Classify whether an Update mutation covers all writable fields.
pub(super) fn classify_update_coverage(
    m: &MutationDefinition,
    writable_names: &[&str],
) -> UpdateCoverage {
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
                method: HttpMethod::Post,
                path: format!("/{resource_name}"),
                source: RouteSource::Mutation {
                    name: m.name.clone(),
                },
                update_coverage: None,
                success_status: 201,
            });
        },
        MutationOperation::Update { .. } => {
            let coverage = classify_update_coverage(m, writable_names);
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Info,
                message: format!(
                    "Mutation '{}' classified as {:?} coverage update for type '{type_name}'",
                    m.name, coverage
                ),
            });

            match coverage {
                UpdateCoverage::Full => {
                    if let Some(id) = id_arg {
                        routes.push(RestRoute {
                            method: HttpMethod::Put,
                            path: format!("/{resource_name}/{{{id}}}"),
                            source: RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Full),
                            success_status: 200,
                        });
                        routes.push(RestRoute {
                            method: HttpMethod::Patch,
                            path: format!("/{resource_name}/{{{id}}}"),
                            source: RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Full),
                            success_status: 200,
                        });
                    }
                },
                UpdateCoverage::Partial => {
                    let action = derive_action_name(&m.name, type_name);
                    if let Some(id) = id_arg {
                        routes.push(RestRoute {
                            method: HttpMethod::Patch,
                            path: format!("/{resource_name}/{{{id}}}/{action}"),
                            source: RouteSource::Mutation {
                                name: m.name.clone(),
                            },
                            update_coverage: Some(UpdateCoverage::Partial),
                            success_status: 200,
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
                    method: HttpMethod::Delete,
                    path: format!("/{resource_name}/{{{id}}}"),
                    source: RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status: status,
                });
            }
        },
        MutationOperation::Custom => {
            let action = derive_action_name(&m.name, type_name);
            if let Some(id) = id_arg {
                routes.push(RestRoute {
                    method: HttpMethod::Post,
                    path: format!("/{resource_name}/{{{id}}}/{action}"),
                    source: RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status: 200,
                });
            } else {
                routes.push(RestRoute {
                    method: HttpMethod::Post,
                    path: format!("/{resource_name}/{action}"),
                    source: RouteSource::Mutation {
                        name: m.name.clone(),
                    },
                    update_coverage: None,
                    success_status: 200,
                });
            }
        },
        // Reason: MutationOperation is #[non_exhaustive]; skip unknown variants.
        _ => {},
    }
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
                method: HttpMethod::Get,
                path: format!("/{resource_name}"),
                source: RouteSource::Query {
                    name: q.name.clone(),
                },
                update_coverage: None,
                success_status: 200,
            });
        } else if let Some(ref id) = id_arg {
            routes.push(RestRoute {
                method: HttpMethod::Get,
                path: format!("/{resource_name}/{{{id}}}"),
                source: RouteSource::Query {
                    name: q.name.clone(),
                },
                update_coverage: None,
                success_status: 200,
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
