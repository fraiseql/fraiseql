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
    params::{ExtractedParams, PaginationParams, RestFieldSpec, RestParamExtractor},
    resource::{HttpMethod, RouteSource},
    response::helpers::{check_if_none_match, compute_etag},
};

/// Refuse a request carrying a parameter no export representation can honour.
///
/// The three export representations — NDJSON, CSV, XLSX — offer a strict subset of the
/// JSON envelope's request surface, and this is the whole of what they leave out. It is
/// one function, called from
/// [`RestHandler::resolve_streaming_get_query`](RestHandler::resolve_streaming_get_query),
/// for the same reason the `rest_stream` opt-in lives there (#958): a fourth
/// representation inherits every rule by resolving through the only function that fits
/// it, rather than by someone remembering to copy three checks.
///
/// It replaces `validate_ndjson_request` / `validate_csv_request` / `validate_xlsx_request`,
/// which were three hand-copied bodies of the count and pagination rules — already drifted
/// in wording ("streaming responses" against "export responses") and, more to the point,
/// exactly the shape that let a fourth rule be forgotten in all three at once (#1268).
///
/// # What is refused, and why
///
/// * **`Prefer: count=…`** and **pagination** — an export reads the whole filtered relation in one
///   pass; there is no page to be on and no total to report alongside a body that has already
///   begun.
/// * **`?select=` embedded relationships and counts** — an export is *one statement over one
///   database portal*: a single snapshot, `O(N)` in row scans, holding one pooled connection from
///   the first row to the last. An embed, as [`super::super::embedding`] resolves one, is a
///   sub-query per parent row on a second connection (`embed_into_rows` loops `embed_into_single`;
///   `execute_embedding_counts` loops `count_related`). Executing embeds here would break every one
///   of those properties, unbounded — `export_rows` removes `limit` because an export means the
///   whole table (#811), where the JSON path's parent rows are bounded by `max_page_size`.
///
/// Refusing is a change of answer, not a loss of capability: before #1268 all three
/// representations accepted the selection, validated it, and emitted rows without it. CSV
/// and XLSX build their header from the raw `?select=`, so an export carried a column named
/// after the relationship that was empty on every row — indistinguishable from a table
/// where nothing is related.
///
/// # Errors
///
/// Returns `RestError::BadRequest` naming the offending parameter. Each branch states its
/// own diagnosis: a client cannot act on "something in your query string".
pub fn refuse_unstreamable_request(
    prefer: &PreferHeader,
    params: &ExtractedParams,
) -> Result<(), RestError> {
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request("count not available for export responses"));
    }

    if let PaginationParams::Offset { offset, .. } = params.pagination {
        if offset > 0 {
            return Err(RestError::bad_request(
                "pagination not available for export; use filters to narrow results",
            ));
        }
    }
    if matches!(params.pagination, PaginationParams::Cursor { .. }) {
        return Err(RestError::bad_request(
            "pagination not available for export; use filters to narrow results",
        ));
    }

    if !params.embeddings.is_empty() {
        let named = quoted_list(params.embeddings.iter().map(|spec| spec.relationship.as_str()));
        return Err(RestError::bad_request(format!(
            "embedded relationships are not available for export responses: {named}. An export \
             is one statement over one snapshot; resolving an embed issues a sub-query per row. \
             Request `Accept: application/json` to embed, or project the related data into the \
             exported view."
        )));
    }

    if !params.embedding_counts.is_empty() {
        let named = quoted_list(params.embedding_counts.iter().map(|name| format!("{name}.count")));
        return Err(RestError::bad_request(format!(
            "embedded counts are not available for export responses: {named}. An export is one \
             statement over one snapshot; a count issues a sub-query per row. Request \
             `Accept: application/json` for counts."
        )));
    }

    Ok(())
}

/// `` `a`, `b` `` — the offending names, quoted, in selection order.
fn quoted_list<S: AsRef<str>>(names: impl Iterator<Item = S>) -> String {
    names.map(|n| format!("`{}`", n.as_ref())).collect::<Vec<_>>().join(", ")
}

impl<A: DatabaseAdapter> RestHandler<'_, A> {
    /// Resolve a GET request path for a **streaming representation**, refusing a
    /// route that has not opted in (#958).
    ///
    /// The three export handlers (NDJSON, CSV, XLSX) resolve through here rather
    /// than calling [`resolve_get_query`](Self::resolve_get_query) and checking the
    /// flag themselves. That is the point: a fourth representation added later gets
    /// the opt-in by using the only resolution function that fits it, instead of by
    /// someone remembering.
    ///
    /// The same argument covers every *other* rule an export representation does not
    /// share with the JSON envelope, so they all live in
    /// [`refuse_unstreamable_request`] and are applied here — count, pagination, and
    /// the `?select=` embeds and counts of #1268. The three handlers used to hold a
    /// hand-copied validator each, which is how the fourth rule came to be missing
    /// from all three.
    ///
    /// # Why the opt-in exists
    ///
    /// A streamed export is not a bigger page. It reads the whole filtered relation,
    /// is not bounded by `max_page_size` (an export total is not a page), and holds
    /// a pooled database connection until the client finishes reading. Those are the
    /// properties a route meant to hand over a dataset should have and the ones most
    /// routes should not, so the decision is the schema author's, per route.
    ///
    /// # Errors
    ///
    /// Returns `406 Not Acceptable` when the resolved query has `rest_stream = false`
    /// — the representation the client asked for is one this route does not offer —
    /// then `400 Bad Request` for anything [`refuse_unstreamable_request`] rejects,
    /// plus everything [`resolve_get_query`](Self::resolve_get_query) returns.
    ///
    /// The `406` deliberately comes first. "This route offers no export at all" is the
    /// more fundamental refusal; answering `400` first would tell a client to fix a
    /// `?select=` on a route where no `?select=` would have helped.
    pub fn resolve_streaming_get_query(
        &self,
        relative_path: &str,
        query_pairs: &[(&str, &str)],
        headers: &http::HeaderMap,
    ) -> Result<ResolvedGetQuery, RestError> {
        let resolved = self.resolve_get_query(relative_path, query_pairs, headers)?;

        let streamable = self
            .schema
            .find_query(&resolved.query_name)
            .is_some_and(|query_def| query_def.rest_stream);
        if !streamable {
            return Err(RestError::not_acceptable(format!(
                "`{}` is not exported as a stream; set `rest_stream = true` on the query to \
                 offer NDJSON, CSV and XLSX on this route",
                resolved.query_name
            )));
        }

        refuse_unstreamable_request(&PreferHeader::from_headers(headers), &resolved.params)?;

        Ok(resolved)
    }

    /// Resolve a GET request path into a prepared query match and extracted params.
    ///
    /// Route resolution, role checking, parameter extraction, and the `QueryMatch`
    /// with its variables. The streaming representations resolve through
    /// [`resolve_streaming_get_query`](Self::resolve_streaming_get_query) instead.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on route not found or parameter extraction error.
    ///
    /// Takes no security context: authorization on this path runs at the executor
    /// chokepoints, not here (#1122).
    pub fn resolve_get_query(
        &self,
        relative_path: &str,
        query_pairs: &[(&str, &str)],
        headers: &http::HeaderMap,
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

        // `requires_role` is NOT checked here (#1122). It used to be, against
        // `ctx.scopes` — the wrong field — and answering `403`, which the field's
        // own contract forbids. It now runs in `resolve_direct_read` and
        // `count_rows`, the two chokepoints every REST read passes through,
        // alongside `requires_actor` (#966) and field RBAC (#423). A gate in this
        // resolver would be a fourth place for the rule to drift, and would still
        // miss the embedding path, which does not come through here.

        let type_def = self.schema.find_type(&query_def.return_type);

        // #873.1: `Prefer: handling=lenient` reaches the extractor. Every GET path —
        // JSON, NDJSON, CSV, XLSX — resolves through here, so the preference cannot be
        // honoured on one representation and ignored on another.
        let lenient = PreferHeader::from_headers(headers).handling
            == Some(super::prefer::HandlingPreference::Lenient);
        let extractor = RestParamExtractor::new(self.config, query_def, type_def)
            .with_lenient_handling(lenient);
        let path_pairs: Vec<(&str, &str)> =
            resolved.path_params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let params = extractor.extract(&path_pairs, query_pairs)?;

        // Build field names from RestFieldSpec.
        //
        // `All` must expand to the type's declared fields, not to an empty vector.
        // The executor reads the requested field set twice — as the projection *and*
        // as the input to the `#423` field-authorization gate — so an empty vector
        // reads as "project nothing, gate nothing" to both. That is `#886`: a read
        // with no `?select=` returned `{"data":[{},{}]}`, and the gate that should
        // have refused a policy-gated field was handed nothing to inspect.
        //
        // Expanding here rather than teaching the projector to treat empty as "all"
        // is deliberate: the projector fix alone would leave the gate inert, because
        // the gate reads the same list.
        let field_names = match &params.field_selection {
            RestFieldSpec::All => type_def
                .map(|t| t.fields.iter().map(|f| f.name.to_string()).collect())
                .unwrap_or_default(),
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
            // Widened by `with_embed_join_keys` on the JSON path only; see
            // `ResolvedGetQuery::server_projected_keys`.
            server_projected_keys: Vec::new(),
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
        // #1230: an embed is resolved by reading a join key off the already-projected
        // parent row, so the projection has to carry that key even when the client did
        // not select it — and give it back afterwards. Applied here rather than inside
        // `resolve_get_query` because this is the representation that executes
        // embeddings; the streaming exports resolve through the same function and
        // would emit the extra column.
        let resolved_query = self
            .resolve_get_query(relative_path, query_pairs, headers)?
            .with_embed_join_keys(self.schema)?;
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

        // Preference-Applied for count mode and lenient handling.
        //
        // Both are echoed only when the server genuinely applied them: the extractor ran
        // in lenient mode for this request, and `count_applied` names the count strategy
        // that actually executed. #914 is the standing counter-example — `tx=rollback`
        // was pushed into this header while the transaction committed.
        let mut applied: Vec<&str> = Vec::new();
        if let Some(count_pref) = count_applied {
            applied.push(count_pref);
        }
        if prefer.handling == Some(super::prefer::HandlingPreference::Lenient) {
            applied.push("handling=lenient");
        }
        if !applied.is_empty() {
            set_preference_applied(&mut response_headers, &applied);
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

                // Both embed passes have read the join keys; the client asked for
                // neither, so neither survives into the response (#1230).
                super::super::embedding::strip_projected_keys(
                    data,
                    &resolved_query.server_projected_keys,
                );
            }
        }

        // #873.3: `RestConfig::etag` finally has a consumer on the live path.
        //
        // It defaults to `true` and is documented as enabling "`ETag` / `If-None-Match`
        // conditional response support", and the served OpenAPI documents a `304` on
        // single-resource GET — but `RestResponseFormatter`, which implements all of it,
        // had no production caller. No `ETag` was ever emitted, so a client following
        // the published contract had nothing to store and re-transferred the full body
        // on every poll; and an operator setting `etag = false` to turn the feature off
        // observed no change, because it had never been on.
        //
        // Computed over the final body — after embeddings — so two responses share an
        // `ETag` only when they are byte-identical.
        if self.config.etag {
            let serialized = serde_json::to_vec(&body).map_err(|e| {
                RestError::internal(format!("Failed to serialize response for ETag: {e}"))
            })?;
            let etag = compute_etag(&serialized);

            if check_if_none_match(headers, &etag).unwrap_or(false) {
                let mut not_modified = response_headers.clone();
                not_modified.insert(
                    "etag",
                    HeaderValue::from_str(&etag).map_err(|e| {
                        RestError::internal(format!("Computed ETag is not a valid header: {e}"))
                    })?,
                );
                return Ok(RestResponse {
                    status:  axum::http::StatusCode::NOT_MODIFIED,
                    headers: not_modified,
                    body:    None,
                });
            }

            response_headers.insert(
                "etag",
                HeaderValue::from_str(&etag).map_err(|e| {
                    RestError::internal(format!("Computed ETag is not a valid header: {e}"))
                })?,
            );
        }

        Ok(RestResponse {
            status:  axum::http::StatusCode::OK,
            headers: response_headers,
            body:    Some(body),
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
