//! GET query handler and query response building.

use std::collections::HashMap;

use axum::http::{HeaderMap, HeaderValue};
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::QueryMatch, security::SecurityContext};
use serde_json::json;

use super::{
    RestHandler,
    headers::{set_preference_applied, set_request_id},
    prefer::{CountPreference, PreferHeader},
    response::{RestError, RestResponse},
    routing::ResolvedGetQuery,
    search::build_fts_where_clause,
};
use crate::routes::rest::{
    params::{PaginationParams, RestFieldSpec, RestParamExtractor},
    resource::{HttpMethod, RouteSource},
};

impl<A: DatabaseAdapter> RestHandler<'_, A> {
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
        super::super::cache_control::apply_cache_headers(
            &mut response_headers,
            &super::super::cache_control::CacheContext {
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
                let embed_req = super::super::embedding::EmbeddingRequest {
                    executor: self.executor,
                    schema: self.schema,
                    config: self.config,
                    parent_type_name: &query_match.query_def.return_type,
                    security_context,
                };

                super::super::embedding::execute_embeddings(
                    &embed_req,
                    data,
                    &params.embeddings,
                    &params.embedding_filters,
                )
                .await?;

                super::super::embedding::execute_embedding_counts(
                    &embed_req,
                    data,
                    &params.embedding_counts,
                )
                .await?;
            }
        }

        Ok(RestResponse {
            status: axum::http::StatusCode::OK,
            headers: response_headers,
            body: Some(body),
        })
    }
}

/// Build a query response JSON with optional total count and pagination metadata.
pub(super) fn build_query_response(
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
pub(super) fn extract_relay_page_info(data: &serde_json::Value) -> Option<&serde_json::Value> {
    data.get("pageInfo")
}
