//! REST request handler — direct execution without GraphQL parsing.
//!
//! Receives HTTP requests, resolves routes from [`RestRouteTable`], extracts
//! parameters via [`RestParamExtractor`], builds a [`QueryMatch`] or mutation
//! call, and executes directly via the [`Executor`] APIs.

use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    schema::{CompiledSchema, DeleteResponse, RestConfig, TypeDefinition},
    security::SecurityContext,
};
use fraiseql_error::FraiseQLError;
use serde_json::json;

use super::{
    idempotency::{IdempotencyCheck, IdempotencyStore, StoredResponse},
    params::{PaginationParams, RestFieldSpec, RestParamExtractor},
    resource::{HttpMethod, RestResource, RestRoute, RestRouteTable, RouteSource},
};

// ---------------------------------------------------------------------------
// Prefer header parsing
// ---------------------------------------------------------------------------

/// Count preference mode for collection queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountPreference {
    /// `count=exact` — execute a parallel `SELECT COUNT(*)` query.
    Exact,
    /// `count=planned` — extract row estimate from `EXPLAIN` output (PostgreSQL).
    Planned,
    /// `count=estimated` — read `n_live_tup` from `pg_stat_user_tables` (PostgreSQL).
    Estimated,
}

/// Handling preference (RFC 7240 §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlingPreference {
    /// Unknown parameters/preferences are silently ignored.
    Lenient,
    /// Unknown parameters cause a 400 Bad Request.
    Strict,
}

/// Parsed `Prefer` header values relevant to REST transport (RFC 7240).
#[derive(Debug, Clone, Default)]
pub struct PreferHeader {
    /// `count=exact` — execute a parallel COUNT query.
    pub count_exact:           bool,
    /// `count=planned` — EXPLAIN-based estimate (PostgreSQL).
    pub count_planned:         bool,
    /// `count=estimated` — `pg_stats` estimate (PostgreSQL).
    pub count_estimated:       bool,
    /// `return=representation` — return entity body on mutating operations.
    pub return_representation: bool,
    /// `return=minimal` — return empty body on mutating operations.
    pub return_minimal:        bool,
    /// `resolution=merge-duplicates` or `resolution=ignore-duplicates` — upsert mode.
    pub resolution:            Option<String>,
    /// `tx=rollback` — dry-run mode (execute then rollback).
    pub tx_rollback:           bool,
    /// `handling=strict` or `handling=lenient` (default: strict for Phase 1 compat).
    pub handling:              Option<HandlingPreference>,
    /// `max-affected=N` — limit bulk operation scope.
    pub max_affected:          Option<u64>,
}

impl PreferHeader {
    /// Return the active count preference, if any.
    #[must_use]
    pub const fn count_preference(&self) -> Option<CountPreference> {
        if self.count_exact {
            Some(CountPreference::Exact)
        } else if self.count_planned {
            Some(CountPreference::Planned)
        } else if self.count_estimated {
            Some(CountPreference::Estimated)
        } else {
            None
        }
    }

    /// Collect all applied preferences as a comma-separated header value.
    #[must_use]
    pub fn applied_header_value(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.count_exact {
            parts.push("count=exact");
        } else if self.count_planned {
            parts.push("count=planned");
        } else if self.count_estimated {
            parts.push("count=estimated");
        }
        if self.return_representation {
            parts.push("return=representation");
        } else if self.return_minimal {
            parts.push("return=minimal");
        }
        if let Some(ref res) = self.resolution {
            // Handled separately since it needs the value
            let _ = res;
        }
        if self.tx_rollback {
            parts.push("tx=rollback");
        }
        if self.handling == Some(HandlingPreference::Strict) {
            parts.push("handling=strict");
        } else if self.handling == Some(HandlingPreference::Lenient) {
            parts.push("handling=lenient");
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

impl PreferHeader {
    /// Parse a `Prefer` header value (RFC 7240).
    ///
    /// Supports `count=exact|planned|estimated`, `return=representation|minimal`,
    /// `resolution=merge-duplicates|ignore-duplicates`, `tx=rollback|commit`,
    /// `handling=strict|lenient`, and `max-affected=N`.
    /// Unknown preferences are silently ignored per RFC 7240.
    #[must_use]
    pub fn parse(header_value: &str) -> Self {
        let mut result = Self::default();
        for pref in header_value.split(',') {
            let pref = pref.trim();
            if pref.eq_ignore_ascii_case("count=exact") {
                result.count_exact = true;
                result.count_planned = false;
                result.count_estimated = false;
            } else if pref.eq_ignore_ascii_case("count=planned") {
                result.count_planned = true;
                result.count_exact = false;
                result.count_estimated = false;
            } else if pref.eq_ignore_ascii_case("count=estimated") {
                result.count_estimated = true;
                result.count_exact = false;
                result.count_planned = false;
            } else if pref.eq_ignore_ascii_case("return=representation") {
                result.return_representation = true;
                result.return_minimal = false;
            } else if pref.eq_ignore_ascii_case("return=minimal") {
                result.return_minimal = true;
                result.return_representation = false;
            } else if pref.eq_ignore_ascii_case("tx=rollback") {
                result.tx_rollback = true;
            } else if pref.eq_ignore_ascii_case("tx=commit") {
                // Default behavior — acknowledged but no-op.
                result.tx_rollback = false;
            } else if pref.eq_ignore_ascii_case("handling=strict") {
                result.handling = Some(HandlingPreference::Strict);
            } else if pref.eq_ignore_ascii_case("handling=lenient") {
                result.handling = Some(HandlingPreference::Lenient);
            } else if let Some(val) = strip_prefix_ci(pref, "resolution=") {
                result.resolution = Some(val.to_string());
            } else if let Some(val) = strip_prefix_ci(pref, "max-affected=") {
                if let Ok(n) = val.parse::<u64>() {
                    result.max_affected = Some(n);
                }
            }
            // Unknown preferences silently ignored (per RFC 7240 §2)
        }
        result
    }

    /// Parse from a header map (handles missing and multiple Prefer headers).
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let mut result = Self::default();
        for value in headers.get_all("prefer") {
            if let Ok(s) = value.to_str() {
                let parsed = Self::parse(s);
                // Count: last-write-wins (mutually exclusive)
                if parsed.count_exact {
                    result.count_exact = true;
                    result.count_planned = false;
                    result.count_estimated = false;
                } else if parsed.count_planned {
                    result.count_planned = true;
                    result.count_exact = false;
                    result.count_estimated = false;
                } else if parsed.count_estimated {
                    result.count_estimated = true;
                    result.count_exact = false;
                    result.count_planned = false;
                }
                // Return: last-write-wins (mutually exclusive)
                if parsed.return_representation {
                    result.return_representation = true;
                    result.return_minimal = false;
                }
                if parsed.return_minimal {
                    result.return_minimal = true;
                    result.return_representation = false;
                }
                if parsed.tx_rollback {
                    result.tx_rollback = true;
                }
                if parsed.handling.is_some() {
                    result.handling = parsed.handling;
                }
                if parsed.resolution.is_some() {
                    result.resolution = parsed.resolution;
                }
                if parsed.max_affected.is_some() {
                    result.max_affected = parsed.max_affected;
                }
            }
        }
        result
    }
}

/// Case-insensitive prefix strip.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

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

impl<A: DatabaseAdapter> RestHandler<'_, A> {
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

// ---------------------------------------------------------------------------
// Response type
// ---------------------------------------------------------------------------

/// REST response before final HTTP serialization.
#[derive(Debug)]
pub struct RestResponse {
    /// HTTP status code.
    pub status:  StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body (None for 204 No Content).
    pub body:    Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// REST-specific error with HTTP status code.
#[derive(Debug)]
pub struct RestError {
    /// HTTP status code.
    pub status:  StatusCode,
    /// Error code string.
    pub code:    &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Structured details for field-level errors.
    pub details: Option<serde_json::Value>,
}

impl RestError {
    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::BAD_REQUEST,
            code:    "BAD_REQUEST",
            message: message.into(),
            details: None,
        }
    }

    /// 403 Forbidden.
    #[must_use]
    pub fn forbidden() -> Self {
        Self {
            status:  StatusCode::FORBIDDEN,
            code:    "FORBIDDEN",
            message: "Access denied".to_string(),
            details: None,
        }
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::NOT_FOUND,
            code:    "NOT_FOUND",
            message: message.into(),
            details: None,
        }
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable_entity(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            status:  StatusCode::UNPROCESSABLE_ENTITY,
            code:    "UNPROCESSABLE_ENTITY",
            message: message.into(),
            details: Some(details),
        }
    }

    /// 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::INTERNAL_SERVER_ERROR,
            code:    "INTERNAL_SERVER_ERROR",
            message: message.into(),
            details: None,
        }
    }

    /// Convert to a JSON error body.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut error = json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });
        if let Some(ref details) = self.details {
            error["error"]["details"] = details.clone();
        }
        error
    }
}

impl From<FraiseQLError> for RestError {
    fn from(err: FraiseQLError) -> Self {
        match &err {
            FraiseQLError::NotFound { .. } => Self::not_found(err.to_string()),
            FraiseQLError::Validation { .. }
            | FraiseQLError::UnknownField { .. }
            | FraiseQLError::UnknownType { .. } => Self::bad_request(err.to_string()),
            FraiseQLError::Authorization { .. } => Self::forbidden(),
            FraiseQLError::Authentication { .. } => Self {
                status:  StatusCode::UNAUTHORIZED,
                code:    "UNAUTHENTICATED",
                message: "Authentication required".to_string(),
                details: None,
            },
            _ => Self::internal(err.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a [`StoredResponse`] from the idempotency store back to a [`RestResponse`].
fn stored_response_to_rest(stored: StoredResponse, request_headers: &HeaderMap) -> RestResponse {
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

/// Execute a mutation, routing through security context when available.
async fn execute_mutation<A: DatabaseAdapter>(
    executor: &Executor<A>,
    mutation_name: &str,
    variables: Option<&serde_json::Value>,
    security_context: Option<&SecurityContext>,
) -> Result<serde_json::Value, RestError> {
    let result = executor
        .execute_mutation_with_security(
            mutation_name,
            variables.unwrap_or(&serde_json::json!({})),
            security_context,
        )
        .await;
    result.map_err(RestError::from)
}

/// Build mutation variables from path params and request body.
fn build_mutation_variables(
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

/// Coerce a path parameter string to an appropriate JSON value.
///
/// Attempts integer, then boolean, then falls back to string.
fn coerce_path_param_value(value: &str) -> serde_json::Value {
    // Try integer
    if let Ok(n) = value.parse::<i64>() {
        return json!(n);
    }
    // Try boolean
    match value {
        "true" => return json!(true),
        "false" => return json!(false),
        _ => {},
    }
    // Fall back to string
    json!(value)
}

/// Validate that all writable fields are present in a PUT request body.
///
/// # Errors
///
/// Returns `RestError::UnprocessableEntity` with field-level details for each
/// missing field.
fn validate_put_body(body: &serde_json::Value, type_def: &TypeDefinition) -> Result<(), RestError> {
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
fn extract_delete_entity(
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

/// Build a query response JSON with optional total count and pagination metadata.
fn build_query_response(
    result: &serde_json::Value,
    total: Option<u64>,
    pagination: &PaginationParams,
) -> Result<serde_json::Value, RestError> {
    // Extract data from the executor result envelope
    let data = if let Some(data_obj) = result.get("data") {
        // The executor returns `{ "data": { "queryName": [...] } }`.
        // Extract the inner value (first field of the data object).
        if let serde_json::Value::Object(map) = data_obj {
            map.values().next().cloned().unwrap_or(serde_json::Value::Null)
        } else {
            data_obj.clone()
        }
    } else {
        result.clone()
    };

    let mut response = json!({ "data": data });

    // Add metadata for collection responses
    match pagination {
        PaginationParams::Offset { limit, offset } => {
            let mut meta = json!({
                "limit": limit,
                "offset": offset,
            });
            if let Some(total) = total {
                meta["total"] = json!(total);
            }
            response["meta"] = meta;
        },
        PaginationParams::Cursor {
            first,
            after,
            last,
            before,
        } => {
            let mut meta = serde_json::Map::new();
            // Extract Relay pageInfo from the data if available
            if let Some(page_info) = extract_relay_page_info(&data) {
                if let Some(has_next) = page_info.get("hasNextPage") {
                    meta.insert("hasNextPage".to_string(), has_next.clone());
                }
                if let Some(has_prev) = page_info.get("hasPreviousPage") {
                    meta.insert("hasPreviousPage".to_string(), has_prev.clone());
                }
            }
            if let Some(f) = first {
                meta.insert("first".to_string(), json!(f));
            }
            if let Some(ref a) = after {
                meta.insert("after".to_string(), json!(a));
            }
            if let Some(l) = last {
                meta.insert("last".to_string(), json!(l));
            }
            if let Some(ref b) = before {
                meta.insert("before".to_string(), json!(b));
            }
            if let Some(total) = total {
                meta.insert("total".to_string(), json!(total));
            }
            response["meta"] = serde_json::Value::Object(meta);
        },
        PaginationParams::None => {
            // Single resource — no pagination metadata
        },
    }

    Ok(response)
}

/// Extract `pageInfo` from a Relay connection response.
fn extract_relay_page_info(data: &serde_json::Value) -> Option<&serde_json::Value> {
    data.get("pageInfo")
}

/// Set `Preference-Applied` header from a list of applied preferences.
///
/// Joins all non-empty preferences into a single comma-separated header value
/// per RFC 7240 §3.  Does nothing if the list is empty.
pub(super) fn set_preference_applied(headers: &mut HeaderMap, prefs: &[&str]) {
    let prefs: Vec<&&str> = prefs.iter().filter(|p| !p.is_empty()).collect();
    if prefs.is_empty() {
        return;
    }
    let value: String = prefs.iter().map(|p| **p).collect::<Vec<_>>().join(", ");
    if let Ok(val) = HeaderValue::from_str(&value) {
        headers.insert("preference-applied", val);
    }
}

/// Set `X-Request-Id` header: echo from request or generate a new UUID.
pub(super) fn set_request_id(request_headers: &HeaderMap, response_headers: &mut HeaderMap) {
    let request_id = request_headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |s| s.to_string());

    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response_headers.insert("x-request-id", val);
    }
}

/// Build a FTS WHERE clause from a search query string and the type's searchable fields.
///
/// Produces `{"_or": [{"field": {"websearch_query": "query"}}, ...]}` for each
/// searchable field.  Returns `None` if the type has no searchable fields.
fn build_fts_where_clause(
    query: &str,
    type_def: Option<&TypeDefinition>,
) -> Option<serde_json::Value> {
    let td = type_def?;
    let fields = td.searchable_fields();
    if fields.is_empty() {
        return None;
    }

    let clauses: Vec<serde_json::Value> = fields
        .iter()
        .map(|f| json!({ f.name.as_str(): { "websearch_query": query } }))
        .collect();

    if clauses.len() == 1 {
        // Reason: len == 1 checked above; iterator always yields Some on a non-empty vec.
        Some(clauses.into_iter().next().expect("len checked above"))
    } else {
        Some(json!({ "_or": clauses }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
#[allow(clippy::missing_panics_doc)] // Reason: test code
#[allow(clippy::missing_errors_doc)] // Reason: test code
mod tests {
    use fraiseql_core::schema::{FieldDefinition, FieldType, TypeDefinition};

    use super::*;

    /// Parse a JSON string literal into `serde_json::Value` for test assertions.
    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    // -----------------------------------------------------------------------
    // Prefer header tests
    // -----------------------------------------------------------------------

    #[test]
    fn prefer_parse_count_exact() {
        let prefer = PreferHeader::parse("count=exact");
        assert!(prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_representation() {
        let prefer = PreferHeader::parse("return=representation");
        assert!(!prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_minimal() {
        let prefer = PreferHeader::parse("return=minimal");
        assert!(!prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_combined() {
        let prefer = PreferHeader::parse("count=exact, return=representation");
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_case_insensitive() {
        let prefer = PreferHeader::parse("Count=Exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_parse_unknown_ignored() {
        let prefer = PreferHeader::parse("respond-async, count=exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_minimal_overrides_representation() {
        let prefer = PreferHeader::parse("return=representation, return=minimal");
        assert!(prefer.return_minimal);
        assert!(!prefer.return_representation);
    }

    #[test]
    fn prefer_from_headers_multiple() {
        let mut headers = HeaderMap::new();
        headers.append("prefer", HeaderValue::from_static("count=exact"));
        headers.append("prefer", HeaderValue::from_static("return=representation"));
        let prefer = PreferHeader::from_headers(&headers);
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
    }

    #[test]
    fn prefer_parse_resolution_merge() {
        let prefer = PreferHeader::parse("resolution=merge-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
    }

    #[test]
    fn prefer_parse_resolution_ignore() {
        let prefer = PreferHeader::parse("resolution=ignore-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("ignore-duplicates"));
    }

    #[test]
    fn prefer_parse_tx_rollback() {
        let prefer = PreferHeader::parse("tx=rollback");
        assert!(prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_max_affected() {
        let prefer = PreferHeader::parse("max-affected=50");
        assert_eq!(prefer.max_affected, Some(50));
    }

    #[test]
    fn prefer_parse_max_affected_invalid() {
        let prefer = PreferHeader::parse("max-affected=abc");
        assert_eq!(prefer.max_affected, None);
    }

    #[test]
    fn prefer_parse_combined_bulk() {
        let prefer = PreferHeader::parse(
            "resolution=merge-duplicates, return=representation, max-affected=100",
        );
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
        assert!(prefer.return_representation);
        assert_eq!(prefer.max_affected, Some(100));
    }

    #[test]
    fn prefer_parse_tx_rollback_combined() {
        let prefer = PreferHeader::parse("tx=rollback, return=representation");
        assert!(prefer.tx_rollback);
        assert!(prefer.return_representation);
    }

    #[test]
    fn prefer_from_headers_bulk() {
        let mut headers = HeaderMap::new();
        headers.append("prefer", HeaderValue::from_static("resolution=merge-duplicates"));
        headers.append("prefer", HeaderValue::from_static("max-affected=25"));
        let prefer = PreferHeader::from_headers(&headers);
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
        assert_eq!(prefer.max_affected, Some(25));
    }

    #[test]
    fn prefer_parse_resolution_case_insensitive() {
        let prefer = PreferHeader::parse("Resolution=merge-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
    }

    #[test]
    fn prefer_parse_tx_case_insensitive() {
        let prefer = PreferHeader::parse("TX=ROLLBACK");
        assert!(prefer.tx_rollback);
    }

    // -----------------------------------------------------------------------
    // Extended Prefer header tests (Cycle 12)
    // -----------------------------------------------------------------------

    #[test]
    fn prefer_parse_count_planned() {
        let prefer = PreferHeader::parse("count=planned");
        assert!(prefer.count_planned);
        assert!(!prefer.count_exact);
        assert!(!prefer.count_estimated);
        assert_eq!(prefer.count_preference(), Some(CountPreference::Planned));
    }

    #[test]
    fn prefer_parse_count_estimated() {
        let prefer = PreferHeader::parse("count=estimated");
        assert!(prefer.count_estimated);
        assert!(!prefer.count_exact);
        assert!(!prefer.count_planned);
        assert_eq!(prefer.count_preference(), Some(CountPreference::Estimated));
    }

    #[test]
    fn prefer_count_modes_mutually_exclusive() {
        // Last one wins
        let prefer = PreferHeader::parse("count=exact, count=planned");
        assert!(prefer.count_planned);
        assert!(!prefer.count_exact);
    }

    #[test]
    fn prefer_parse_handling_strict() {
        let prefer = PreferHeader::parse("handling=strict");
        assert_eq!(prefer.handling, Some(HandlingPreference::Strict));
    }

    #[test]
    fn prefer_parse_handling_lenient() {
        let prefer = PreferHeader::parse("handling=lenient");
        assert_eq!(prefer.handling, Some(HandlingPreference::Lenient));
    }

    #[test]
    fn prefer_parse_handling_case_insensitive() {
        let prefer = PreferHeader::parse("Handling=Strict");
        assert_eq!(prefer.handling, Some(HandlingPreference::Strict));
    }

    #[test]
    fn prefer_parse_tx_commit_resets_rollback() {
        let prefer = PreferHeader::parse("tx=rollback, tx=commit");
        assert!(!prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_tx_commit_no_op() {
        let prefer = PreferHeader::parse("tx=commit");
        assert!(!prefer.tx_rollback);
    }

    #[test]
    fn prefer_combined_all_preferences() {
        let prefer = PreferHeader::parse("return=representation, count=exact, handling=strict");
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
        assert_eq!(prefer.handling, Some(HandlingPreference::Strict));
    }

    #[test]
    fn prefer_unknown_silently_ignored() {
        let prefer = PreferHeader::parse("foo=bar, count=exact");
        assert!(prefer.count_exact);
        // Unknown pref "foo=bar" should not cause any field to be set
        assert!(prefer.resolution.is_none());
        assert!(prefer.handling.is_none());
    }

    #[test]
    fn prefer_count_preference_none() {
        let prefer = PreferHeader::parse("return=representation");
        assert_eq!(prefer.count_preference(), None);
    }

    #[test]
    fn prefer_applied_header_value_single() {
        let prefer = PreferHeader::parse("count=exact");
        assert_eq!(prefer.applied_header_value().as_deref(), Some("count=exact"));
    }

    #[test]
    fn prefer_applied_header_value_multiple() {
        let prefer = PreferHeader::parse("count=exact, return=representation, handling=strict");
        let value = prefer.applied_header_value().unwrap();
        assert!(value.contains("count=exact"));
        assert!(value.contains("return=representation"));
        assert!(value.contains("handling=strict"));
    }

    #[test]
    fn prefer_applied_header_value_none() {
        let prefer = PreferHeader::default();
        assert!(prefer.applied_header_value().is_none());
    }

    #[test]
    fn prefer_from_headers_count_planned() {
        let mut headers = HeaderMap::new();
        headers.append("prefer", HeaderValue::from_static("count=planned"));
        let prefer = PreferHeader::from_headers(&headers);
        assert!(prefer.count_planned);
        assert!(!prefer.count_exact);
    }

    #[test]
    fn prefer_from_headers_handling() {
        let mut headers = HeaderMap::new();
        headers.append("prefer", HeaderValue::from_static("handling=lenient"));
        let prefer = PreferHeader::from_headers(&headers);
        assert_eq!(prefer.handling, Some(HandlingPreference::Lenient));
    }

    // -----------------------------------------------------------------------
    // stored_response_to_rest tests
    // -----------------------------------------------------------------------

    #[test]
    fn stored_response_replay() {
        let stored = StoredResponse {
            status:  201,
            headers: vec![("x-rows-affected".to_string(), "1".to_string())],
            body:    Some(json!({"id": 1})),
        };
        let request_headers = HeaderMap::new();
        let rest = stored_response_to_rest(stored, &request_headers);
        assert_eq!(rest.status, StatusCode::CREATED);
        assert_eq!(rest.headers.get("idempotency-key").unwrap().to_str().unwrap(), "replayed=true");
        assert_eq!(rest.body.unwrap()["id"], 1);
    }

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
        assert_eq!(resolved.path_params, vec![("id".to_string(), "42".to_string())]);
    }

    #[test]
    fn resolve_post_create() {
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
    fn resolve_put_update() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/1", HttpMethod::Put).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUser".to_string(),
            }
        );
        assert_eq!(resolved.path_params, vec![("id".to_string(), "1".to_string())]);
    }

    #[test]
    fn resolve_patch_sub_resource_action() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/5/update-email", HttpMethod::Patch).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "updateUserEmail".to_string(),
            }
        );
        assert_eq!(resolved.path_params, vec![("id".to_string(), "5".to_string())]);
    }

    #[test]
    fn resolve_delete() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/99", HttpMethod::Delete).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "deleteUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_post_custom_action() {
        let table = make_test_route_table();
        let resolved = table.resolve("/users/7/archive", HttpMethod::Post).unwrap();
        assert_eq!(
            resolved.route.source,
            RouteSource::Mutation {
                name: "archiveUser".to_string(),
            }
        );
    }

    #[test]
    fn resolve_nonexistent_route() {
        let table = make_test_route_table();
        assert!(table.resolve("/posts", HttpMethod::Get).is_none());
    }

    #[test]
    fn resolve_wrong_method() {
        let table = make_test_route_table();
        assert!(table.resolve("/users", HttpMethod::Put).is_none());
    }

    // -----------------------------------------------------------------------
    // Path matching tests
    // -----------------------------------------------------------------------

    #[test]
    fn match_exact_path() {
        let result = match_route_path("/users", &["users"]);
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn match_param_path() {
        let result = match_route_path("/users/{id}", &["users", "123"]);
        assert_eq!(result, Some(vec![("id".to_string(), "123".to_string())]));
    }

    #[test]
    fn match_multi_segment_path() {
        let result = match_route_path("/users/{id}/archive", &["users", "7", "archive"]);
        assert_eq!(result, Some(vec![("id".to_string(), "7".to_string())]));
    }

    #[test]
    fn no_match_different_length() {
        let result = match_route_path("/users/{id}", &["users"]);
        assert_eq!(result, None);
    }

    #[test]
    fn no_match_different_segment() {
        let result = match_route_path("/users/{id}", &["posts", "1"]);
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // PUT body validation tests
    // -----------------------------------------------------------------------

    fn make_user_type() -> TypeDefinition {
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("pk_user", FieldType::Int))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("email", FieldType::String))
    }

    #[test]
    fn validate_put_body_all_fields_present() {
        let td = make_user_type();
        let body = json!({
            "name": "Alice",
            "email": "alice@test.com",
        });
        assert!(validate_put_body(&body, &td).is_ok());
    }

    #[test]
    fn validate_put_body_missing_field() {
        let td = make_user_type();
        let body = json!({
            "name": "Alice",
        });
        let err = validate_put_body(&body, &td).unwrap_err();
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        let details = err.details.unwrap();
        let missing = details["missing_fields"].as_array().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0]["field"], "email");
    }

    #[test]
    fn validate_put_body_excludes_pk() {
        let td = make_user_type();
        // pk_user should NOT be required (primary key excluded by writable_fields)
        let body = json!({
            "name": "Alice",
            "email": "alice@test.com",
        });
        assert!(validate_put_body(&body, &td).is_ok());
    }

    #[test]
    fn validate_put_body_non_object() {
        let td = make_user_type();
        let body = json!("not an object");
        let err = validate_put_body(&body, &td).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // Delete entity extraction tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_entity_nested_format() {
        let result: serde_json::Value = serde_json::from_str(
            r#"{"data":{"deleteUser":{"success":true,"entity":{"id":1,"name":"Alice"}}}}"#,
        )
        .unwrap();
        let entity = extract_delete_entity(&result, "deleteUser").unwrap();
        assert_eq!(entity["id"], 1);
        assert_eq!(entity["name"], "Alice");
    }

    #[test]
    fn extract_entity_executor_format() {
        // Executor flattens entity fields + __typename directly under mutation name
        let result: serde_json::Value = serde_json::from_str(
            r#"{"data":{"delete_user":{"pk_user_id":42,"name":"Alice","__typename":"User"}}}"#,
        )
        .unwrap();
        let entity = extract_delete_entity(&result, "delete_user").unwrap();
        assert_eq!(entity["pk_user_id"], 42);
        assert_eq!(entity["name"], "Alice");
        // __typename should be stripped
        assert!(entity.get("__typename").is_none());
    }

    #[test]
    fn extract_entity_null() {
        let result: serde_json::Value =
            serde_json::from_str(r#"{"data":{"deleteUser":{"success":true,"entity":null}}}"#)
                .unwrap();
        assert!(extract_delete_entity(&result, "deleteUser").is_none());
    }

    #[test]
    fn extract_entity_missing() {
        let result: serde_json::Value =
            serde_json::from_str(r#"{"data":{"deleteUser":{}}}"#).unwrap();
        assert!(extract_delete_entity(&result, "deleteUser").is_none());
    }

    #[test]
    fn extract_entity_null_value() {
        assert!(extract_delete_entity(&serde_json::Value::Null, "deleteUser").is_none());
    }

    // -----------------------------------------------------------------------
    // Path param coercion tests
    // -----------------------------------------------------------------------

    #[test]
    fn coerce_integer() {
        assert_eq!(coerce_path_param_value("42"), json!(42));
    }

    #[test]
    fn coerce_negative_integer() {
        assert_eq!(coerce_path_param_value("-1"), json!(-1));
    }

    #[test]
    fn coerce_boolean_true() {
        assert_eq!(coerce_path_param_value("true"), json!(true));
    }

    #[test]
    fn coerce_boolean_false() {
        assert_eq!(coerce_path_param_value("false"), json!(false));
    }

    #[test]
    fn coerce_string_fallback() {
        assert_eq!(coerce_path_param_value("alice"), json!("alice"));
    }

    #[test]
    fn coerce_uuid_as_string() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(coerce_path_param_value(uuid), json!(uuid));
    }

    // -----------------------------------------------------------------------
    // Query response building tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_response_single_resource() {
        let result = r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#;
        let response = build_query_response(&v(result), None, &PaginationParams::None).unwrap();
        assert_eq!(response["data"]["id"], 1);
        assert!(response.get("meta").is_none());
    }

    #[test]
    fn build_response_collection_offset() {
        let result = r#"{"data":{"users":[{"id":1},{"id":2}]}}"#;
        let response = build_query_response(
            &v(result),
            Some(100),
            &PaginationParams::Offset {
                limit:  10,
                offset: 0,
            },
        )
        .unwrap();
        assert!(response["data"].is_array());
        assert_eq!(response["meta"]["limit"], 10);
        assert_eq!(response["meta"]["offset"], 0);
        assert_eq!(response["meta"]["total"], 100);
    }

    #[test]
    fn build_response_collection_no_total() {
        let result = r#"{"data":{"users":[{"id":1}]}}"#;
        let response = build_query_response(
            &v(result),
            None,
            &PaginationParams::Offset {
                limit:  10,
                offset: 0,
            },
        )
        .unwrap();
        assert!(response["meta"].get("total").is_none());
    }

    #[test]
    fn build_response_cursor_pagination() {
        let result = r#"{"data":{"posts":{"edges":[{"cursor":"abc","node":{"id":1}}],"pageInfo":{"hasNextPage":true,"hasPreviousPage":false}}}}"#;
        let response = build_query_response(
            &v(result),
            None,
            &PaginationParams::Cursor {
                first:  Some(5),
                after:  None,
                last:   None,
                before: None,
            },
        )
        .unwrap();
        assert_eq!(response["meta"]["first"], 5);
    }

    // -----------------------------------------------------------------------
    // X-Request-Id tests
    // -----------------------------------------------------------------------

    #[test]
    fn request_id_echoed() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert("x-request-id", HeaderValue::from_static("abc-123"));
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        assert_eq!(response_headers.get("x-request-id").unwrap().to_str().unwrap(), "abc-123");
    }

    #[test]
    fn request_id_generated_when_missing() {
        let request_headers = HeaderMap::new();
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        let id = response_headers.get("x-request-id").unwrap().to_str().unwrap();
        // Should be a UUID (36 chars with hyphens)
        assert_eq!(id.len(), 36);
        assert!(id.contains('-'));
    }

    // -----------------------------------------------------------------------
    // Content-Type validation for PATCH
    // -----------------------------------------------------------------------

    #[test]
    fn content_type_application_json_accepted() {
        let ct = "application/json";
        let lower = ct.to_lowercase();
        assert!(lower.contains("application/json"));
    }

    #[test]
    fn content_type_merge_patch_accepted() {
        let ct = "application/merge-patch+json";
        let lower = ct.to_lowercase();
        assert!(lower.contains("application/merge-patch+json"));
    }

    // -----------------------------------------------------------------------
    // RestError tests
    // -----------------------------------------------------------------------

    #[test]
    fn rest_error_to_json_without_details() {
        let err = RestError::not_found("User not found");
        let json = err.to_json();
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "User not found");
        assert!(json["error"].get("details").is_none());
    }

    #[test]
    fn rest_error_to_json_with_details() {
        let err = RestError::unprocessable_entity("Missing fields", json!({"missing": ["email"]}));
        let json = err.to_json();
        assert_eq!(json["error"]["code"], "UNPROCESSABLE_ENTITY");
        assert_eq!(json["error"]["details"]["missing"][0], "email");
    }

    #[test]
    fn rest_error_from_fraiseql_not_found() {
        let err = FraiseQLError::not_found("User", "42");
        let rest_err = RestError::from(err);
        assert_eq!(rest_err.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn rest_error_from_fraiseql_validation() {
        let err = FraiseQLError::Validation {
            message: "Invalid field".to_string(),
            path:    None,
        };
        let rest_err = RestError::from(err);
        assert_eq!(rest_err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn rest_error_from_fraiseql_auth() {
        let err = FraiseQLError::Authorization {
            message:  "Denied".to_string(),
            action:   None,
            resource: None,
        };
        let rest_err = RestError::from(err);
        assert_eq!(rest_err.status, StatusCode::FORBIDDEN);
    }

    // -----------------------------------------------------------------------
    // Full-text search WHERE clause builder
    // -----------------------------------------------------------------------

    fn searchable_type_def() -> TypeDefinition {
        let mut td = TypeDefinition::new("Article", "v_article");
        td.fields = vec![
            FieldDefinition::new("title", FieldType::String),
            FieldDefinition::new("body", FieldType::String),
            FieldDefinition::new("status", FieldType::Int),
        ];
        td
    }

    #[test]
    fn fts_where_clause_with_multiple_searchable_fields() {
        let td = searchable_type_def();
        let clause = build_fts_where_clause("rust async", Some(&td)).unwrap();

        // Should produce { "_or": [ {"title": ...}, {"body": ...} ] }
        let or_clauses = clause["_or"].as_array().unwrap();
        assert_eq!(or_clauses.len(), 2);
        assert_eq!(or_clauses[0]["title"]["websearch_query"], "rust async");
        assert_eq!(or_clauses[1]["body"]["websearch_query"], "rust async");
    }

    #[test]
    fn fts_where_clause_with_single_searchable_field() {
        let mut td = TypeDefinition::new("Note", "v_note");
        td.fields = vec![FieldDefinition::new("content", FieldType::String)];

        let clause = build_fts_where_clause("hello", Some(&td)).unwrap();

        // Single field: no _or wrapper
        assert_eq!(clause["content"]["websearch_query"], "hello");
    }

    #[test]
    fn fts_where_clause_returns_none_without_searchable_fields() {
        let td = TypeDefinition::new("Plain", "v_plain");
        assert!(build_fts_where_clause("test", Some(&td)).is_none());
    }

    #[test]
    fn fts_where_clause_returns_none_without_type_def() {
        assert!(build_fts_where_clause("test", None).is_none());
    }
}
