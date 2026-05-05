//! Mutation handlers (POST, PUT, PATCH, DELETE) and mutation execution helpers.

use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::Executor,
    schema::{DeleteResponse, TypeDefinition},
    security::SecurityContext,
};
use serde_json::json;

use super::{
    RestHandler,
    coercion::coerce_path_param_value,
    headers::{set_preference_applied, set_request_id},
    prefer::PreferHeader,
    response::{RestError, RestResponse},
};
use crate::routes::rest::{
    idempotency::{IdempotencyCheck, IdempotencyStore, StoredResponse},
    resource::{HttpMethod, RouteSource},
};

impl<'a, A: DatabaseAdapter> RestHandler<'a, A> {
    /// Set the idempotency store for POST mutation replay.
    #[must_use]
    pub const fn with_idempotency_store(mut self, store: &'a Arc<dyn IdempotencyStore>) -> Self {
        self.idempotency_store = Some(store);
        self
    }
}

impl<A: DatabaseAdapter + SupportsMutations> RestHandler<'_, A> {
    /// Handle a POST request (create mutation, bulk insert, or custom action).
    ///
    /// Array body on a collection route triggers bulk insert mode.
    /// `Prefer: resolution=merge-duplicates` triggers upsert mode.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found, body validation failure,
    /// or mutation execution error.
    pub async fn handle_post(
        &self,
        relative_path: &str,
        body: &serde_json::Value,
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        let resolved = self
            .route_table
            .resolve(relative_path, HttpMethod::Post)
            .ok_or_else(|| RestError::not_found("Route not found"))?;

        let mutation_name = match &resolved.route.source {
            RouteSource::Mutation { name } => name.as_str(),
            RouteSource::Query { .. } => {
                return Err(RestError::internal("POST route backed by query"));
            },
        };

        // Detect array body → bulk insert
        if let serde_json::Value::Array(items) = body {
            // Array body on a single-resource route (with :id) is not allowed
            if !resolved.path_params.is_empty() {
                return Err(RestError::bad_request(
                    "Array body not allowed on single-resource endpoint",
                ));
            }

            let prefer = PreferHeader::from_headers(headers);
            let bulk_handler = super::super::bulk::BulkHandler::new(
                self.executor,
                self.schema,
                self.config,
                self.route_table,
            );
            return bulk_handler
                .handle_bulk_insert(items, mutation_name, &prefer, headers, security_context)
                .await;
        }

        // Single POST (existing behavior)
        let variables = build_mutation_variables(&resolved.path_params, body);
        let variables_json = serde_json::Value::Object(variables);
        let vars_ref = Some(&variables_json);

        // Idempotency: check for Idempotency-Key header
        let idempotency_key =
            headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(String::from);

        if let (Some(ref key), Some(store)) = (&idempotency_key, self.idempotency_store) {
            let body_hash = super::super::idempotency::hash_body(body);
            match store.check(key, body_hash).await {
                IdempotencyCheck::Replay(stored) => {
                    return Ok(stored_response_to_rest(stored, headers));
                },
                IdempotencyCheck::Conflict => {
                    return Err(RestError {
                        status:  StatusCode::UNPROCESSABLE_ENTITY,
                        code:    "IDEMPOTENCY_CONFLICT",
                        message: "Idempotency-Key reused with different request body".to_string(),
                        details: None,
                    });
                },
                IdempotencyCheck::New => {
                    // Proceed with execution
                },
            }
        }

        // Check for upsert via Prefer header
        let prefer = PreferHeader::from_headers(headers);
        let effective_mutation = if let Some(ref resolution) = prefer.resolution {
            let mutation_def = self.schema.find_mutation(mutation_name);
            match resolution.as_str() {
                "merge-duplicates" | "ignore-duplicates" => {
                    match mutation_def.and_then(|md| md.upsert_function.as_deref()) {
                        Some(upsert_fn) => upsert_fn,
                        None => {
                            return Err(RestError::bad_request(
                                "Upsert not available — no compiler-generated upsert function exists",
                            ));
                        },
                    }
                },
                _ => mutation_name,
            }
        } else {
            mutation_name
        };

        let result =
            execute_mutation(self.executor, effective_mutation, vars_ref, security_context).await?;

        let mut response_headers = HeaderMap::new();
        set_request_id(headers, &mut response_headers);

        if let Some(ref resolution) = prefer.resolution {
            set_preference_applied(&mut response_headers, &[&format!("resolution={resolution}")]);
            response_headers.insert("x-rows-affected", HeaderValue::from_static("1"));
        }

        // Cache-Control: no-store for mutations
        super::super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::super::cache_control::CacheContext {
                is_get:      false,
                has_auth:    headers.get("authorization").is_some(),
                query_ttl:   None,
                default_ttl: self.config.default_cache_ttl,
                cdn_max_age: self.config.cdn_max_age,
            },
        );

        let status =
            StatusCode::from_u16(resolved.route.success_status).unwrap_or(StatusCode::CREATED);

        let rest_response = RestResponse {
            status,
            headers: response_headers,
            body: Some(result),
        };

        // Idempotency: store the response for replay
        if let (Some(key), Some(store)) = (idempotency_key, self.idempotency_store) {
            let body_hash = super::super::idempotency::hash_body(body);
            store
                .store(
                    key,
                    body_hash,
                    StoredResponse {
                        status:  rest_response.status.as_u16(),
                        headers: rest_response
                            .headers
                            .iter()
                            .map(|(k, v)| {
                                (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())
                            })
                            .collect(),
                        body:    rest_response.body.clone(),
                    },
                )
                .await;
        }

        Ok(rest_response)
    }

    /// Handle a PUT request (full update mutation).
    ///
    /// Validates that all writable fields are present in the request body.
    ///
    /// # Errors
    ///
    /// Returns `RestError::UnprocessableEntity` if required fields are missing.
    pub async fn handle_put(
        &self,
        relative_path: &str,
        body: &serde_json::Value,
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        let resolved = self
            .route_table
            .resolve(relative_path, HttpMethod::Put)
            .ok_or_else(|| RestError::not_found("Route not found"))?;

        let mutation_name = match &resolved.route.source {
            RouteSource::Mutation { name } => name.as_str(),
            RouteSource::Query { .. } => {
                return Err(RestError::internal("PUT route backed by query"));
            },
        };

        // Validate all writable fields are present
        let mutation_def = self.schema.find_mutation(mutation_name);
        if let Some(md) = mutation_def {
            let type_def = self.schema.find_type(&md.return_type);
            if let Some(td) = type_def {
                validate_put_body(body, td)?;
            }
        }

        let variables = build_mutation_variables(&resolved.path_params, body);
        let variables_json = serde_json::Value::Object(variables);
        let vars_ref = Some(&variables_json);

        let result =
            execute_mutation(self.executor, mutation_name, vars_ref, security_context).await?;

        let mut response_headers = HeaderMap::new();
        set_request_id(headers, &mut response_headers);

        // Cache-Control: no-store for mutations
        super::super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::super::cache_control::CacheContext {
                is_get:      false,
                has_auth:    headers.get("authorization").is_some(),
                query_ttl:   None,
                default_ttl: self.config.default_cache_ttl,
                cdn_max_age: self.config.cdn_max_age,
            },
        );

        Ok(RestResponse {
            status:  StatusCode::OK,
            headers: response_headers,
            body:    Some(result),
        })
    }

    /// Handle a PATCH request (partial update, bulk update, or sub-resource action).
    ///
    /// Collection-level PATCH (no `:id` in path, requires filter) triggers bulk
    /// update mode via the CQRS view-query-then-mutate pattern.
    ///
    /// Accepts `application/json` and `application/merge-patch+json`.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found or execution error.
    pub async fn handle_patch(
        &self,
        relative_path: &str,
        body: &serde_json::Value,
        query_params: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        // Validate Content-Type if present
        if let Some(ct) = headers.get("content-type") {
            if let Ok(ct_str) = ct.to_str() {
                let ct_lower = ct_str.to_lowercase();
                if !ct_lower.contains("application/json")
                    && !ct_lower.contains("application/merge-patch+json")
                {
                    return Err(RestError::bad_request(
                        "PATCH requires Content-Type: application/json or application/merge-patch+json",
                    ));
                }
            }
        }

        // Try single-resource PATCH first (with :id)
        let resolved = self.route_table.resolve(relative_path, HttpMethod::Patch);

        match resolved {
            Some(r) if !r.path_params.is_empty() => {
                // Single-resource PATCH (existing behavior)
                let mutation_name = match &r.route.source {
                    RouteSource::Mutation { name } => name.as_str(),
                    RouteSource::Query { .. } => {
                        return Err(RestError::internal("PATCH route backed by query"));
                    },
                };

                let variables = build_mutation_variables(&r.path_params, body);
                let variables_json = serde_json::Value::Object(variables);
                let vars_ref = Some(&variables_json);

                let result =
                    execute_mutation(self.executor, mutation_name, vars_ref, security_context)
                        .await?;

                let mut response_headers = HeaderMap::new();
                set_request_id(headers, &mut response_headers);

                super::super::cache_control::apply_cache_headers(
                    &mut response_headers,
                    &super::super::cache_control::CacheContext {
                        is_get:      false,
                        has_auth:    headers.get("authorization").is_some(),
                        query_ttl:   None,
                        default_ttl: self.config.default_cache_ttl,
                        cdn_max_age: self.config.cdn_max_age,
                    },
                );

                Ok(RestResponse {
                    status:  StatusCode::OK,
                    headers: response_headers,
                    body:    Some(result),
                })
            },
            _ => {
                // Collection-level PATCH → bulk update
                let bulk_handler = super::super::bulk::BulkHandler::new(
                    self.executor,
                    self.schema,
                    self.config,
                    self.route_table,
                );
                bulk_handler
                    .handle_bulk_update(
                        relative_path,
                        body,
                        query_params,
                        headers,
                        security_context,
                    )
                    .await
            },
        }
    }

    /// Handle a DELETE request.
    ///
    /// Single-resource DELETE (with `:id`): respects `Prefer: return=representation|minimal`
    /// and the configured [`DeleteResponse`] policy.
    ///
    /// Collection-level DELETE (no `:id`, requires filter): triggers bulk delete
    /// via the CQRS view-query-then-mutate pattern.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found or execution error.
    pub async fn handle_delete(
        &self,
        relative_path: &str,
        query_params: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        let resolved = self.route_table.resolve(relative_path, HttpMethod::Delete);

        match resolved {
            Some(r) if !r.path_params.is_empty() => {
                // Single-resource DELETE (existing behavior)
                let mutation_name = match &r.route.source {
                    RouteSource::Mutation { name } => name.as_str(),
                    RouteSource::Query { .. } => {
                        return Err(RestError::internal("DELETE route backed by query"));
                    },
                };

                let mut variables = serde_json::Map::new();
                for (key, value) in &r.path_params {
                    variables.insert(key.clone(), coerce_path_param_value(value));
                }
                let variables_json = serde_json::Value::Object(variables);
                let vars_ref = Some(&variables_json);

                let result =
                    execute_mutation(self.executor, mutation_name, vars_ref, security_context)
                        .await?;

                let prefer = PreferHeader::from_headers(headers);
                let mut response_headers = HeaderMap::new();
                set_request_id(headers, &mut response_headers);

                super::super::cache_control::apply_cache_headers(
                    &mut response_headers,
                    &super::super::cache_control::CacheContext {
                        is_get:      false,
                        has_auth:    headers.get("authorization").is_some(),
                        query_ttl:   None,
                        default_ttl: self.config.default_cache_ttl,
                        cdn_max_age: self.config.cdn_max_age,
                    },
                );

                let want_entity = if prefer.return_representation {
                    true
                } else if prefer.return_minimal {
                    false
                } else {
                    matches!(self.config.delete_response, DeleteResponse::Entity)
                };

                if want_entity {
                    let entity = extract_delete_entity(&result, mutation_name);

                    if let Some(entity_value) = entity {
                        if prefer.return_representation {
                            set_preference_applied(
                                &mut response_headers,
                                &["return=representation"],
                            );
                        }
                        Ok(RestResponse {
                            status:  StatusCode::OK,
                            headers: response_headers,
                            body:    Some(entity_value),
                        })
                    } else {
                        if prefer.return_representation {
                            set_preference_applied(&mut response_headers, &["return=minimal"]);
                            response_headers.insert(
                                "x-preference-fallback",
                                HeaderValue::from_static("entity-unavailable"),
                            );
                        }
                        Ok(RestResponse {
                            status:  StatusCode::NO_CONTENT,
                            headers: response_headers,
                            body:    None,
                        })
                    }
                } else {
                    if prefer.return_minimal {
                        set_preference_applied(&mut response_headers, &["return=minimal"]);
                    }
                    Ok(RestResponse {
                        status:  StatusCode::NO_CONTENT,
                        headers: response_headers,
                        body:    None,
                    })
                }
            },
            _ => {
                // Collection-level DELETE → bulk delete
                let bulk_handler = super::super::bulk::BulkHandler::new(
                    self.executor,
                    self.schema,
                    self.config,
                    self.route_table,
                );
                bulk_handler
                    .handle_bulk_delete(relative_path, query_params, headers, security_context)
                    .await
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Mutation helpers
// ---------------------------------------------------------------------------

/// Execute a mutation, routing through security context when available.
pub(super) async fn execute_mutation<A: DatabaseAdapter + SupportsMutations>(
    executor: &Executor<A>,
    mutation_name: &str,
    variables: Option<&serde_json::Value>,
    security_context: Option<&SecurityContext>,
) -> Result<serde_json::Value, RestError> {
    let result = if let Some(ctx) = security_context {
        executor
            .execute_mutation_with_security(
                mutation_name,
                variables.unwrap_or(&serde_json::json!({})),
                Some(ctx),
            )
            .await
    } else {
        executor.execute_mutation(mutation_name, variables, &HashMap::new()).await
    };
    result.map_err(RestError::from)
}

/// Build mutation variables from path params and request body.
pub(super) fn build_mutation_variables(
    path_params: &[(String, String)],
    body: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let mut variables = serde_json::Map::new();

    // Path params first (e.g., `id`)
    for (key, value) in path_params {
        variables.insert(key.clone(), coerce_path_param_value(value));
    }

    // Merge body fields
    if let serde_json::Value::Object(body_map) = body {
        for (key, value) in body_map {
            variables.insert(key.clone(), value.clone());
        }
    }

    variables
}

/// Validate that all writable fields are present in a PUT request body.
///
/// # Errors
///
/// Returns `RestError::UnprocessableEntity` with field-level details for each
/// missing field.
pub(super) fn validate_put_body(
    body: &serde_json::Value,
    type_def: &TypeDefinition,
) -> Result<(), RestError> {
    let serde_json::Value::Object(body_map) = body else {
        return Err(RestError::bad_request("PUT body must be a JSON object"));
    };

    let writable = type_def.writable_fields();
    let mut missing_fields = Vec::new();

    for field in &writable {
        let output_name = field.output_name();
        if !body_map.contains_key(output_name) {
            missing_fields.push(json!({
                "field": output_name,
                "message": format!("Required field '{}' is missing", output_name),
            }));
        }
    }

    if missing_fields.is_empty() {
        Ok(())
    } else {
        Err(RestError::unprocessable_entity(
            format!("PUT requires all writable fields; {} missing", missing_fields.len()),
            json!({ "missing_fields": missing_fields }),
        ))
    }
}

/// Extract entity from a DELETE mutation response.
///
/// Extracts `data.{mutation_name}.entity` from the executor result.
/// Returns `None` if entity is null or unavailable.
pub(super) fn extract_delete_entity(
    result: &serde_json::Value,
    mutation_name: &str,
) -> Option<serde_json::Value> {
    let mutation_result = result.get("data")?.get(mutation_name)?;

    // The executor flattens entity fields directly under `data.{mutation_name}`.
    // If an `entity` key exists, use it (raw mutation_response format).
    // Otherwise, treat the mutation result itself as the entity (executor output format).
    let entity = if mutation_result.get("entity").is_some() {
        // Raw format: extract nested entity (returns None if null)
        let e = mutation_result.get("entity")?;
        if e.is_null() {
            return None;
        }
        e
    } else if mutation_result.is_object() && !mutation_result.as_object()?.is_empty() {
        // Executor format: entity fields + __typename at top level
        mutation_result
    } else {
        return None;
    };

    // Strip internal __typename from the REST response
    let mut cleaned = entity.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        obj.remove("__typename");
    }

    if cleaned.is_null() || cleaned.as_object().is_some_and(serde_json::Map::is_empty) {
        None
    } else {
        Some(cleaned)
    }
}

/// Convert a [`StoredResponse`] from the idempotency store back to a [`RestResponse`].
pub(super) fn stored_response_to_rest(
    stored: StoredResponse,
    request_headers: &HeaderMap,
) -> RestResponse {
    let mut headers = HeaderMap::new();
    set_request_id(request_headers, &mut headers);

    for (key, value) in &stored.headers {
        if let (Ok(name), Ok(val)) = (
            axum::http::header::HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, val);
        }
    }

    // Mark as replayed
    headers.insert("idempotency-key", HeaderValue::from_static("replayed=true"));

    RestResponse {
        status: StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK),
        headers,
        body: stored.body,
    }
}
