//! REST router builder.
//!
//! Iterates over compiled schema queries and mutations that carry `rest`
//! annotations and mounts the corresponding axum routes.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use fraiseql_core::{db::traits::DatabaseAdapter, schema::CompiledSchema};
use serde_json::Value;
use tracing::{debug, error, info};

use super::translator::{RestOutcome, build_graphql_request, classify_response};
use crate::routes::graphql::app_state::AppState;

/// Shared state for REST route handlers.
#[derive(Clone)]
struct RestRouteState<A: DatabaseAdapter> {
    app_state:      AppState<A>,
    operation:      String, // "query" or "mutation"
    operation_name: String,
    arguments:      Arc<Vec<fraiseql_core::schema::ArgumentDefinition>>,
    return_fields:  Arc<Vec<String>>,
    /// Whether the operation returns a list (affects 404 vs 200 semantics for null data).
    returns_list:   bool,
}

/// Build the REST router from the compiled schema.
///
/// Returns `None` when no queries or mutations have REST annotations.
pub fn build_rest_router<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    schema: &CompiledSchema,
    app_state: &AppState<A>,
) -> Option<Router> {
    let mut router: Router = Router::new();
    let mut route_count = 0u32;

    // Mount query routes
    for query_def in &schema.queries {
        let Some(ref rest) = query_def.rest else {
            continue;
        };

        // Derive return field names from the schema type (scalar fields only)
        let return_fields = derive_return_fields(schema, &query_def.return_type);

        let state = RestRouteState {
            app_state:      app_state.clone(),
            operation:      "query".to_string(),
            operation_name: query_def.name.clone(),
            arguments:      Arc::new(query_def.arguments.clone()),
            return_fields:  Arc::new(return_fields),
            returns_list:   query_def.returns_list,
        };

        let axum_path = to_axum_path(&rest.path);
        let method = rest.method.to_ascii_uppercase();

        router = match method.as_str() {
            "GET" => router.route(&axum_path, axum::routing::get(rest_handler::<A>).with_state(state)),
            "POST" => router.route(&axum_path, axum::routing::post(rest_handler::<A>).with_state(state)),
            "PUT" => router.route(&axum_path, axum::routing::put(rest_handler::<A>).with_state(state)),
            "PATCH" => router.route(&axum_path, axum::routing::patch(rest_handler::<A>).with_state(state)),
            "DELETE" => router.route(&axum_path, axum::routing::delete(rest_handler::<A>).with_state(state)),
            other => {
                error!(method = %other, path = %rest.path, "Unsupported REST method in schema — skipping route");
                continue;
            },
        };

        info!(method = %method, path = %rest.path, query = %query_def.name, "REST route mounted");
        route_count += 1;
    }

    // Mount mutation routes
    for mutation_def in &schema.mutations {
        let Some(ref rest) = mutation_def.rest else {
            continue;
        };

        let return_fields = derive_return_fields(schema, &mutation_def.return_type);

        let state = RestRouteState {
            app_state:      app_state.clone(),
            operation:      "mutation".to_string(),
            operation_name: mutation_def.name.clone(),
            arguments:      Arc::new(mutation_def.arguments.clone()),
            return_fields:  Arc::new(return_fields),
            returns_list:   false, // mutations don't return lists
        };

        let axum_path = to_axum_path(&rest.path);
        let method = rest.method.to_ascii_uppercase();

        router = match method.as_str() {
            "GET" => router.route(&axum_path, axum::routing::get(rest_handler::<A>).with_state(state)),
            "POST" => router.route(&axum_path, axum::routing::post(rest_handler::<A>).with_state(state)),
            "PUT" => router.route(&axum_path, axum::routing::put(rest_handler::<A>).with_state(state)),
            "PATCH" => router.route(&axum_path, axum::routing::patch(rest_handler::<A>).with_state(state)),
            "DELETE" => router.route(&axum_path, axum::routing::delete(rest_handler::<A>).with_state(state)),
            other => {
                error!(method = %other, path = %rest.path, "Unsupported REST method in schema — skipping route");
                continue;
            },
        };

        info!(method = %method, path = %rest.path, mutation = %mutation_def.name, "REST route mounted");
        route_count += 1;
    }

    if route_count == 0 {
        return None;
    }

    // Optionally serve an OpenAPI spec for all REST routes
    if let Some(ref rest_config) = schema.rest_config {
        if rest_config.openapi_enabled {
            // Use pre-generated spec if available, otherwise generate dynamically
            let spec = schema.rest_openapi_spec.clone().unwrap_or_else(|| {
                fraiseql_core::schema::openapi_gen::generate_openapi_spec(
                    schema,
                    rest_config,
                )
            });

            let openapi_path = rest_config.openapi_path.clone();
            router = router.route(
                &openapi_path,
                axum::routing::get(move || {
                    let spec = spec.clone();
                    async move {
                        (
                            StatusCode::OK,
                            [(header::CONTENT_TYPE, "application/json")],
                            spec,
                        )
                    }
                }),
            );

            info!(path = %rest_config.openapi_path, "OpenAPI spec endpoint mounted");
        }
    }

    Some(router)
}

/// Convert a schema REST path (e.g. `/users/{id}`) to an axum path (e.g. `/users/:id`).
fn to_axum_path(schema_path: &str) -> String {
    // Replace `{param}` with `:param`
    let mut result = String::with_capacity(schema_path.len());
    let mut chars = schema_path.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            result.push(':');
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                result.push(inner);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Derive the list of scalar field names for a return type.
///
/// Falls back to `["__typename"]` if the type is not found in the schema.
fn derive_return_fields(schema: &CompiledSchema, type_name: &str) -> Vec<String> {
    if let Some(type_def) = schema.types.iter().find(|t| t.name == type_name) {
        type_def
            .fields
            .iter()
            .map(|f| f.name.to_string())
            .collect()
    } else {
        vec!["__typename".to_string()]
    }
}

/// Generic REST route handler.
async fn rest_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<RestRouteState<A>>,
    path_params: Option<Path<HashMap<String, String>>>,
    Query(query_params): Query<HashMap<String, String>>,
    body: Option<axum::extract::Json<Value>>,
) -> Response {
    let path_map = path_params.map(|p| p.0).unwrap_or_default();
    let body_val = body.map(|b| b.0);

    debug!(
        operation = %state.operation_name,
        path_params = ?path_map,
        "REST request received"
    );

    let translated = build_graphql_request(
        &state.operation,
        &state.operation_name,
        &state.arguments,
        &state.return_fields,
        &path_map,
        &query_params,
        body_val.as_ref(),
    );

    let exec_result = if let Some(vars) = &translated.variables {
        state
            .app_state
            .executor
            .execute(&translated.query, Some(vars))
            .await
    } else {
        state
            .app_state
            .executor
            .execute(&translated.query, None)
            .await
    };

    match exec_result {
        Ok(json_str) => {
            match classify_response(&json_str, &state.operation_name, state.returns_list) {
                RestOutcome::Ok(data) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string()),
                )
                    .into_response(),

                RestOutcome::Partial { data, errors } => {
                    let body = serde_json::json!({"data": data, "errors": errors, "_partial": true});
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        serde_json::to_string(&body)
                            .unwrap_or_else(|_| r#"{"_partial":true}"#.to_string()),
                    )
                        .into_response()
                },

                RestOutcome::Failure { status, body } => {
                    let sc = StatusCode::from_u16(status)
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                    error!(
                        status = %status,
                        operation = %state.operation_name,
                        "REST operation failed"
                    );
                    (
                        sc,
                        [(header::CONTENT_TYPE, "application/json")],
                        serde_json::to_string(&body).unwrap_or_else(|_| "[]".to_string()),
                    )
                        .into_response()
                },

                RestOutcome::NotFound => {
                    let body = serde_json::json!({
                        "error": "Not found",
                        "operation": &state.operation_name
                    });
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string()),
                    )
                        .into_response()
                },
            }
        },
        Err(e) => {
            error!(error = %e, operation = %state.operation_name, "REST execution failed");
            let body = serde_json::json!({"error": e.to_string()});
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&body).unwrap_or_else(|_| r#"{"error":"internal"}"#.to_string()),
            )
                .into_response()
        },
    }
}
