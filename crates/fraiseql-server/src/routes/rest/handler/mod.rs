//! REST request handler — direct execution without GraphQL parsing.
//!
//! Receives HTTP requests, resolves routes from [`RestRouteTable`], extracts
//! parameters via [`RestParamExtractor`], builds a [`QueryMatch`] or mutation
//! call, and executes directly via the [`Executor`] APIs.

pub mod helpers;
pub mod prefer;
pub mod response;

// Re-export public types from submodules for external use
pub use prefer::{CountPreference, HandlingPreference, PreferHeader};
pub use response::{RestError, RestResponse};

use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::{Executor, QueryMatch},
    schema::{CompiledSchema, DeleteResponse, RestConfig, TypeDefinition},
    security::SecurityContext,
};
use serde_json::json;

use super::{
    idempotency::{IdempotencyCheck, IdempotencyStore, StoredResponse},
    params::{PaginationParams, RestFieldSpec, RestParamExtractor},
    resource::{HttpMethod, RestResource, RestRoute, RestRouteTable, RouteSource},
};
use helpers::{
    build_fts_where_clause, build_mutation_variables, build_query_response, coerce_path_param_value,
    execute_mutation, extract_delete_entity, extract_relay_page_info, set_preference_applied,
    set_request_id, stored_response_to_rest, validate_put_body,
};

// ---------------------------------------------------------------------------
// Route resolution
// ---------------------------------------------------------------------------

/// Resolved route from a request path and method.
#[derive(Debug)]
pub struct ResolvedRoute<'a> {
    /// The matched REST resource.
    pub resource:    &'a RestResource,
    /// The matched REST route.
    pub route:       &'a RestRoute,
    /// Path parameters extracted from the URL (e.g., `[("id", "123")]`).
    pub path_params: Vec<(String, String)>,
}

impl RestRouteTable {
    /// Resolve a request path and HTTP method to a route.
    ///
    /// `request_path` should be the path relative to the REST base path,
    /// e.g., `/users/123` when base is `/rest/v1`.
    ///
    /// # Errors
    ///
    /// Returns `None` if no route matches the path+method combination.
    #[must_use]
    pub fn resolve(&self, relative_path: &str, method: HttpMethod) -> Option<ResolvedRoute<'_>> {
        let segments: Vec<&str> = relative_path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for resource in &self.resources {
            for route in &resource.routes {
                if route.method != method {
                    continue;
                }

                if let Some(path_params) = match_route_path(&route.path, &segments) {
                    return Some(ResolvedRoute {
                        resource,
                        route,
                        path_params,
                    });
                }
            }
        }

        None
    }
}

/// Match a route path pattern against URL segments.
///
/// Route paths use `{param}` syntax for path parameters.
/// Returns extracted path params on match, or `None`.
fn match_route_path(route_path: &str, segments: &[&str]) -> Option<Vec<(String, String)>> {
    let pattern_segments: Vec<&str> = route_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if pattern_segments.len() != segments.len() {
        return None;
    }

    let mut path_params = Vec::new();
    for (pattern, actual) in pattern_segments.iter().zip(segments.iter()) {
        if pattern.starts_with('{') && pattern.ends_with('}') {
            let param_name = &pattern[1..pattern.len() - 1];
            path_params.push((param_name.to_string(), (*actual).to_string()));
        } else if *pattern != *actual {
            return None;
        }
    }

    Some(path_params)
}

// ---------------------------------------------------------------------------
// REST Handler
// ---------------------------------------------------------------------------

/// Pre-resolved GET query context, ready for execution.
///
/// Produced by [`RestHandler::resolve_get_query`] and consumed by both
/// `handle_get` (JSON envelope) and NDJSON streaming.
pub struct ResolvedGetQuery {
    /// Name of the matched query.
    pub query_name:  String,
    /// Pre-built query match with field selection and arguments.
    pub query_match: QueryMatch,
    /// Variables for relay pagination.
    pub variables:   serde_json::Value,
    /// Extracted request parameters (pagination, embeddings, etc.).
    pub params:      super::params::ExtractedParams,
}

/// REST request handler — translates HTTP requests to direct executor calls.
///
/// This handler does NOT construct GraphQL strings. It builds typed
/// [`QueryMatch`] or mutation calls and executes them directly.
pub struct RestHandler<'a, A: DatabaseAdapter> {
    executor:          &'a Arc<Executor<A>>,
    schema:            &'a CompiledSchema,
    config:            &'a RestConfig,
    route_table:       &'a RestRouteTable,
    idempotency_store: Option<&'a Arc<dyn IdempotencyStore>>,
}

impl<'a, A: DatabaseAdapter> RestHandler<'a, A> {
    /// Create a new REST handler.
    #[must_use]
    pub const fn new(
        executor: &'a Arc<Executor<A>>,
        schema: &'a CompiledSchema,
        config: &'a RestConfig,
        route_table: &'a RestRouteTable,
    ) -> Self {
        Self {
            executor,
            schema,
            config,
            route_table,
            idempotency_store: None,
        }
    }

    /// Access the underlying executor.
    #[must_use]
    pub const fn executor(&self) -> &Arc<Executor<A>> {
        self.executor
    }

    /// Access the REST configuration.
    #[must_use]
    pub const fn config(&self) -> &RestConfig {
        self.config
    }

    /// Set the idempotency store for POST mutation replay.
    #[must_use]
    pub const fn with_idempotency_store(mut self, store: &'a Arc<dyn IdempotencyStore>) -> Self {
        self.idempotency_store = Some(store);
        self
    }

    /// Resolve a GET request path into a prepared query match and extracted params.
    ///
    /// Shared by `handle_get` (JSON envelope) and NDJSON streaming. Performs
    /// route resolution, role checking, parameter extraction, and builds the
    /// `QueryMatch` + variables.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found, role check failure, or
    /// parameter extraction error.
    pub fn resolve_get_query(
        &self,
        relative_path: &str,
        query_pairs: &[(&str, &str)],
        security_context: Option<&SecurityContext>,
    ) -> Result<ResolvedGetQuery, RestError> {
        let resolved = self
            .route_table
            .resolve(relative_path, HttpMethod::Get)
            .ok_or_else(|| RestError::not_found("Route not found"))?;

        let query_name = match &resolved.route.source {
            RouteSource::Query { name } => name.as_str(),
            RouteSource::Mutation { .. } => {
                return Err(RestError::internal("GET route backed by mutation"));
            },
        };

        let query_def = self
            .schema
            .find_query(query_name)
            .ok_or_else(|| RestError::not_found(format!("Query not found: {query_name}")))?;

        // Check requires_role
        if let Some(ref required_role) = query_def.requires_role {
            match security_context {
                Some(ctx) if ctx.scopes.contains(required_role) => {},
                _ => return Err(RestError::forbidden()),
            }
        }

        let type_def = self.schema.find_type(&query_def.return_type);

        let extractor = RestParamExtractor::new(self.config, query_def, type_def);
        let path_pairs: Vec<(&str, &str)> =
            resolved.path_params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let params = extractor.extract(&path_pairs, query_pairs)?;

        // Build field names from RestFieldSpec
        let field_names = match &params.field_selection {
            RestFieldSpec::All => Vec::new(),
            RestFieldSpec::Fields(fields) => fields.clone(),
        };

        // Build arguments for QueryMatch
        let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();

        // Path params
        for (key, value) in &params.path_params {
            arguments.insert(key.clone(), value.clone());
        }

        // WHERE clause — merge regular filters with full-text search if present.
        let fts_where = params
            .search_query
            .as_deref()
            .and_then(|query| build_fts_where_clause(query, type_def));

        match (&params.where_clause, &fts_where) {
            (Some(regular), Some(fts)) => {
                // AND the regular filters with the FTS clause.
                arguments.insert("where".to_string(), json!({ "_and": [regular, fts] }));
            },
            (Some(regular), None) => {
                arguments.insert("where".to_string(), regular.clone());
            },
            (None, Some(fts)) => {
                arguments.insert("where".to_string(), fts.clone());
            },
            (None, None) => {},
        }

        // ORDER BY — use ts_rank relevance ordering when search is active
        // and no explicit sort was provided.
        if let Some(ref order_by) = params.order_by {
            arguments.insert("orderBy".to_string(), order_by.clone());
        } else if fts_where.is_some() {
            // Implicit relevance ordering: `ts_rank DESC` is signalled to the
            // executor as a special `_relevance` sort key.
            arguments.insert("orderBy".to_string(), json!([{ "_relevance": "desc" }]));
        }

        // Offset pagination into arguments (non-relay)
        if let PaginationParams::Offset { limit, offset } = &params.pagination {
            arguments.insert("limit".to_string(), json!(limit));
            if *offset > 0 {
                arguments.insert("offset".to_string(), json!(offset));
            }
        }

        // Build variables JSON (needed for relay pagination args)
        let mut variables = serde_json::Map::new();
        for (k, v) in &arguments {
            variables.insert(k.clone(), v.clone());
        }

        // Relay cursor params go into variables (not arguments)
        if let PaginationParams::Cursor {
            first,
            after,
            last,
            before,
        } = &params.pagination
        {
            if let Some(f) = first {
                variables.insert("first".to_string(), json!(f));
            }
            if let Some(ref a) = after {
                variables.insert("after".to_string(), json!(a));
            }
            if let Some(l) = last {
                variables.insert("last".to_string(), json!(l));
            }
            if let Some(ref b) = before {
                variables.insert("before".to_string(), json!(b));
            }
        }

        let variables_json = serde_json::Value::Object(variables);

        // Build QueryMatch
        let query_match =
            QueryMatch::from_operation(query_def.clone(), field_names, arguments, type_def)?;

        Ok(ResolvedGetQuery {
            query_name: query_name.to_string(),
            query_match,
            variables: variables_json,
            params,
        })
    }

    /// Handle a GET request (query execution).
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found, parameter validation failure,
    /// or query execution error.
    pub async fn handle_get(
        &self,
        relative_path: &str,
        query_pairs: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        let resolved_query =
            self.resolve_get_query(relative_path, query_pairs, security_context)?;
        let query_match = &resolved_query.query_match;
        let variables_json = &resolved_query.variables;
        let params = &resolved_query.params;

        // Parse Prefer header
        let prefer = PreferHeader::from_headers(headers);

        // Execute query (and optional count) in parallel
        let vars_ref = if variables_json.as_object().is_none_or(|m| m.is_empty()) {
            None
        } else {
            Some(variables_json)
        };

        let (result, total, count_applied) = match prefer.count_preference() {
            Some(CountPreference::Exact) => {
                let (r, c) = tokio::join!(
                    self.executor.execute_query_direct(query_match, vars_ref, security_context),
                    self.executor.count_rows(query_match, vars_ref, security_context),
                );
                (r?, Some(c?), Some("count=exact"))
            },
            Some(CountPreference::Planned) => {
                // count=planned falls back to count=exact on non-PostgreSQL
                let (r, c) = tokio::join!(
                    self.executor.execute_query_direct(query_match, vars_ref, security_context),
                    self.executor.count_rows(query_match, vars_ref, security_context),
                );
                (r?, Some(c?), Some("count=exact"))
            },
            Some(CountPreference::Estimated) => {
                // count=estimated falls back to count=exact on non-PostgreSQL
                let (r, c) = tokio::join!(
                    self.executor.execute_query_direct(query_match, vars_ref, security_context),
                    self.executor.count_rows(query_match, vars_ref, security_context),
                );
                (r?, Some(c?), Some("count=exact"))
            },
            None => {
                let r = self
                    .executor
                    .execute_query_direct(query_match, vars_ref, security_context)
                    .await?;
                (r, None, None)
            },
        };

        // Build response
        let mut response_headers = HeaderMap::new();

        // X-Request-Id
        set_request_id(headers, &mut response_headers);

        // Preference-Applied for count mode
        if let Some(count_pref) = count_applied {
            set_preference_applied(&mut response_headers, &[count_pref]);
        }

        // X-Preference-Fallback when planned/estimated fell back to exact
        if (prefer.count_planned || prefer.count_estimated) && count_applied == Some("count=exact")
        {
            response_headers
                .insert("x-preference-fallback", HeaderValue::from_static("count=exact"));
        }

        // Cache-Control headers
        let has_auth = headers.get("authorization").is_some();
        super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::cache_control::CacheContext {
                is_get: true,
                has_auth,
                query_ttl: query_match.query_def.cache_ttl_seconds,
                default_ttl: self.config.default_cache_ttl,
                cdn_max_age: self.config.cdn_max_age,
            },
        );

        let mut body = build_query_response(&result, total, &params.pagination)?;

        // Execute embedded resource sub-queries.
        let has_embeddings = !params.embeddings.is_empty() || !params.embedding_counts.is_empty();
        if has_embeddings {
            if let Some(data) = body.get_mut("data") {
                let embed_req = super::embedding::EmbeddingRequest {
                    executor: self.executor,
                    schema: self.schema,
                    config: self.config,
                    parent_type_name: &query_match.query_def.return_type,
                    security_context,
                };

                super::embedding::execute_embeddings(
                    &embed_req,
                    data,
                    &params.embeddings,
                    &params.embedding_filters,
                )
                .await?;

                super::embedding::execute_embedding_counts(
                    &embed_req,
                    data,
                    &params.embedding_counts,
                )
                .await?;
            }
        }

        Ok(RestResponse {
            status:  StatusCode::OK,
            headers: response_headers,
            body:    Some(body),
        })
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
            let bulk_handler = super::bulk::BulkHandler::new(
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
            let body_hash = super::idempotency::hash_body(body);
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
        super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::cache_control::CacheContext {
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
            let body_hash = super::idempotency::hash_body(body);
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
        super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::cache_control::CacheContext {
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

                super::cache_control::apply_cache_headers(
                    &mut response_headers,
                    &super::cache_control::CacheContext {
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
                let bulk_handler = super::bulk::BulkHandler::new(
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

                super::cache_control::apply_cache_headers(
                    &mut response_headers,
                    &super::cache_control::CacheContext {
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
                let bulk_handler = super::bulk::BulkHandler::new(
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
#[allow(clippy::missing_panics_doc)] // Reason: test code
#[allow(clippy::missing_errors_doc)] // Reason: test code
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Route resolution tests
    // -----------------------------------------------------------------------

    fn make_test_route_table() -> RestRouteTable {
        RestRouteTable {
            base_path:   "/rest/v1".to_string(),
            resources:   vec![RestResource {
                name:      "users".to_string(),
                type_name: "User".to_string(),
                id_arg:    Some("id".to_string()),
                routes:    vec![
                    RestRoute {
                        method:          HttpMethod::Get,
                        path:            "/users".to_string(),
                        source:          RouteSource::Query {
                            name: "users".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Get,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Query {
                            name: "user".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Post,
                        path:            "/users".to_string(),
                        source:          RouteSource::Mutation {
                            name: "createUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  201,
                    },
                    RestRoute {
                        method:          HttpMethod::Put,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Patch,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Patch,
                        path:            "/users/{id}/update-email".to_string(),
                        source:          RouteSource::Mutation {
                            name: "updateUserEmail".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                    RestRoute {
                        method:          HttpMethod::Delete,
                        path:            "/users/{id}".to_string(),
                        source:          RouteSource::Mutation {
                            name: "deleteUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  204,
                    },
                    RestRoute {
                        method:          HttpMethod::Post,
                        path:            "/users/{id}/archive".to_string(),
                        source:          RouteSource::Mutation {
                            name: "archiveUser".to_string(),
                        },
                        update_coverage: None,
                        success_status:  200,
                    },
                ],
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn resolve_collection_get() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users", HttpMethod::Get).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Query {
                name: "users".to_string(),
            }
        );
        assert!(resolved.path_params.is_empty());
    }

    #[test]
    fn resolve_single_get() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Get).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Query {
                name: "user".to_string(),
            }
        );
        assert_eq!(resolved.path_params.len(), 1);
        assert_eq!(resolved.path_params[0], ("id".to_string(), "42".to_string()));
    }

    #[test]
    fn resolve_post_collection() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users", HttpMethod::Post).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "createUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_put_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Put).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_patch_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Patch).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_patch_nested() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42/update-email", HttpMethod::Patch).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUserEmail".to_string(),
            }
        );
        assert_eq!(resolved.path_params.len(), 1);
    }

    #[test]
    fn resolve_delete_single() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42", HttpMethod::Delete).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "deleteUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_post_action() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/42/archive", HttpMethod::Post).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "archiveUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_not_found() {
        let table = make_test_route_table();
        assert!(table.resolve("/nonexistent", HttpMethod::Get).is_none());
    }

    #[test]
    fn resolve_wrong_method() {
        let table = make_test_route_table();
        assert!(table.resolve("/users", HttpMethod::Delete).is_none());
    }

    #[test]
    fn match_route_path_static() {
        let path_params = match_route_path("/users", &["users"]);
        assert!(path_params.is_some());
        assert!(path_params.unwrap().is_empty());
    }

    #[test]
    fn match_route_path_dynamic() {
        let path_params = match_route_path("/users/{id}", &["users", "42"]);
        assert!(path_params.is_some());
        let params = path_params.unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], ("id".to_string(), "42".to_string()));
    }

    #[test]
    fn match_route_path_multiple_params() {
        let path_params = match_route_path("/users/{uid}/posts/{pid}", &["users", "1", "posts", "2"]);
        assert!(path_params.is_some());
        let params = path_params.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "uid");
        assert_eq!(params[1].0, "pid");
    }

    #[test]
    fn match_route_path_mismatch() {
        let path_params = match_route_path("/users/{id}", &["posts", "42"]);
        assert!(path_params.is_none());
    }

    #[test]
    fn match_route_path_wrong_segment_count() {
        let path_params = match_route_path("/users/{id}", &["users"]);
        assert!(path_params.is_none());
    }
}
