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
    search::plan_search,
};
use crate::routes::rest::{
    params::{ExtractedParams, PaginationParams, RestFieldSpec, RestParamExtractor},
    resource::{HttpMethod, RouteSource},
    response::helpers::{check_if_none_match, compute_etag},
};

/// Refuse a request carrying a parameter no export representation can honour.
///
/// The three export representations — NDJSON, CSV, XLSX — offer a strict subset of the
/// JSON envelope's request surface, and this is where every rule about that subset lives.
/// It is one function, called from
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
/// * **`?rel.field=value` embedding filters** — the filters on those embeds, arriving by a
///   different syntax and a different producer. An export carries no embed for them to narrow, and
///   their only consumer belongs to the JSON path (#1275).
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
///
/// # Completeness
///
/// The read is an **exhaustive destructure** — no `..` — so this is a complete statement of
/// the rules over the fields that exist, and stays one: adding a field to
/// [`ExtractedParams`] does not compile until this function says what an export does with it
/// (`error[E0027]: pattern does not mention field ...`, a hard error rather than a lint).
///
/// That guard is what the four accept-validate-discard defects on this path had in common —
/// #1268, #1273, #1274 and #1275 were each a field this function held no opinion about, found
/// by a reader rather than by the compiler. It is the same forcing function the wildcard-arm
/// class needed twice, three releases apart (#864, #1267): enumerate, and let the new case
/// fail to build.
///
/// It forces a decision at this site; it does not check that the decision is right.
/// Exhaustiveness fires on a field's *absence from the pattern*, so a field mentioned and
/// mis-disposed is still a defect this cannot see (#1282).
///
/// # Known gaps
///
/// None recorded. #1278 — a relay route unable to bound its export total — was the last one, and
/// is closed: `?first=` now bounds the total exactly as `?limit=` does on an offset route, which
/// is the count/position rule stated above rather than a special case for relay.
pub fn refuse_unstreamable_request(
    prefer: &PreferHeader,
    params: &ExtractedParams,
) -> Result<(), RestError> {
    // Exhaustive by construction: no `..`. An eleventh field on `ExtractedParams` does not
    // compile until this function states what an export does with it (E0027, a hard error
    // rather than a lint), which is what turns the prose above into a rule. Each binding
    // carries its disposition; the four already-fixed defects on this path were each a
    // field nothing here had an opinion about.
    let ExtractedParams {
        // Honoured: the route's own identity, folded into the `QueryMatch` arguments.
        path_params: _,
        // Honoured: `resolve_get_query` builds it into `arguments["where"]`.
        where_clause: _,
        // Honoured: `arguments["orderBy"]`.
        order_by: _,
        // Deliberately NOT read. This is the plan — the page the JSON representation would
        // have served — and reading it here in place of the request is #1273 exactly. The
        // binding exists to be seen and skipped, not to be used.
        pagination: _,
        // Read below, as sent: its positions and directions are refused, its counts
        // (`limit`, `first`) bound the export total via `export_total` (#811, #1278).
        requested_pagination,
        // Honoured: the projection, which is also the column list CSV and XLSX write their
        // header from (`export_columns`, #1274).
        field_selection: _,
        // Honoured. `resolve_get_query` turns it into a full-text `WHERE` clause
        // (`build_fts_where_clause`) and merges it into `arguments["where"]`; `export_rows`
        // streams that same `QueryMatch`, removing only `limit`. So `?search=` narrows an
        // export exactly as it narrows a JSON read, and the extractor has already refused it
        // where the type declares no searchable field. Pinned by
        // `a_search_is_not_refused_because_an_export_honours_it` (this gate's answer) and
        // `a_search_narrows_an_export` (that the clause reaches the rows, #1282).
        search_query: _,
        // Refused below (#1268).
        embeddings,
        // Refused below (#1275).
        embedding_filters,
        // Refused below (#1268).
        embedding_counts,
    } = params;

    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request("count not available for export responses"));
    }

    // Read from what the client **sent**, not from the plan. The plan answers a different
    // question — it is the page the JSON representation would have served — and this path
    // discards it anyway: `export_rows` removes `limit` from the query's arguments because
    // an export total is not a page (#811).
    //
    // Reading the plan is what made a `relay = true` route unexportable (#1273): resolving
    // a request with no cursor parameter still yields `Cursor { first: Some(default_page_size) }`,
    // so `matches!(.., Cursor { .. })` refused every export the route was ever offered,
    // naming a parameter no request had carried. The offset branch beside it never had the
    // defect because `offset > 0` happens to be unreachable by a default — it was already
    // asking the right question, by luck of the value rather than by construction.
    //
    // `?limit=` and `?first=` are deliberately absent from this list, and the reason is the
    // rule rather than an exception to it (#1278).
    //
    // What an export refuses is not "pagination". It is a **position** or a **direction**: an
    // export starts at the beginning of the relation and reads to the end, so there is nothing
    // for `?offset=`, `?after=`, `?before=` or `?last=` to modify. A **count** is different —
    // it bounds how much of that relation is emitted, which an export can honour exactly, and
    // `export_rows` applies it to the stream rather than to the query so `max_page_size` never
    // clamps it (#811).
    //
    // Each pagination family has one count. The offset family's is `?limit=`, permitted here
    // since #811. The cursor family's is `?first=`, and refusing it left a relay export
    // bounded by nothing at all: `?limit=` is refused on a relay route by the cross-pagination
    // guard in `RestParamExtractor::extract`, as the wrong vocabulary for the route, so the two
    // route shapes differed in capability and not merely in spelling.
    //
    // That guard stays as it is. Accepting `?limit=` on a relay route for exports only would
    // make it representation-dependent, and it lives in the extractor, which does not know the
    // representation — the same reason #1285 is not fixed there.
    if matches!(requested_pagination.offset, Some(offset) if offset > 0)
        || requested_pagination.after.is_some()
        || requested_pagination.last.is_some()
        || requested_pagination.before.is_some()
    {
        return Err(RestError::bad_request(
            "pagination not available for export; use filters to narrow results",
        ));
    }

    if !embeddings.is_empty() {
        let named = quoted_list(embeddings.iter().map(|spec| spec.relationship.as_str()));
        return Err(RestError::bad_request(format!(
            "embedded relationships are not available for export responses: {named}. An export \
             is one statement over one snapshot; resolving an embed issues a sub-query per row. \
             Request `Accept: application/json` to embed, or project the related data into the \
             exported view."
        )));
    }

    if !embedding_counts.is_empty() {
        let named = quoted_list(embedding_counts.iter().map(|name| format!("{name}.count")));
        return Err(RestError::bad_request(format!(
            "embedded counts are not available for export responses: {named}. An export is one \
             statement over one snapshot; a count issues a sub-query per row. Request \
             `Accept: application/json` for counts."
        )));
    }

    // #1275: the filters on those embeds, which arrive by a different syntax and a different
    // producer, and which no export could honour even before the two branches above existed.
    //
    // The field is *reachable* — `extract_embedding_filters` reads every query pair
    // unconditionally, whether or not `?select=` named an embed, so a bare `?author.name=alice`
    // fills it. What is unreachable is its effect: the only consumer is
    // `embedding::execute_embeddings`, which the export path never calls and which, since the
    // embed branch above, could not be reached from here in principle. Accepted, never honoured,
    // and answered `200` with the whole unfiltered relation — the accept / validate / discard
    // shape #1268 removed from this path, left behind on it.
    //
    // `bulk/mod.rs` refuses the same parameter, and this is not that rule copied: it refuses
    // because a filter contributing no `WHERE` clause would mutate rows the caller did not
    // select, and this refuses because an export carries no embed to filter. The JSON path
    // honours these filters and must keep accepting them, so there is no third site the two
    // could collapse into.
    if !embedding_filters.is_empty() {
        let named = quoted_list(embedding_filter_parameters(embedding_filters));
        return Err(RestError::bad_request(format!(
            "embedded-relationship filters are not available for export responses: {named}. A \
             dotted parameter filters an embedded relationship, and an export carries no embed \
             to filter. Narrow the exported rows themselves with `?field=value`, or request \
             `Accept: application/json` to embed and filter."
        )));
    }

    Ok(())
}

/// The `rel.field` parameters behind an `embedding_filters` map, in a stable order.
///
/// The map is a `HashMap` keyed by relationship, so it carries no order of its own and this has
/// to impose one: an iteration-order message would name the same request differently on
/// consecutive runs, which is not a contract a client or a test can hold. Sorted, therefore —
/// not "as sent", which the map cannot answer.
///
/// The operator is deliberately not echoed. `?author.name=alice` and `?author.name[eq]=alice`
/// are stored identically (`{"name": {"eq": "alice"}}`), so reconstructing the bracket would
/// mean guessing which form the client wrote. The field path identifies the parameter under
/// either.
///
/// Nothing here has been checked against the schema. `extract_embedding_filters` classifies on
/// the dot alone — `?nonsense.field=x` is stored just as quietly as `?author.name=x` — so this
/// echoes what was sent rather than describing a relationship the type is known to have.
fn embedding_filter_parameters(filters: &HashMap<String, serde_json::Value>) -> Vec<String> {
    let mut named: Vec<String> = filters
        .iter()
        .flat_map(|(relationship, fields)| match fields.as_object() {
            Some(obj) if !obj.is_empty() => {
                obj.keys().map(|field| format!("{relationship}.{field}")).collect::<Vec<_>>()
            },
            // The producer only ever inserts a non-empty object. Any other shape would
            // otherwise vanish from the refusal that exists to name it, leaving a `400` whose
            // message names no parameter at all.
            _ => vec![relationship.clone()],
        })
        .collect();
    named.sort();
    named
}

/// `` `a`, `b` `` — the offending names, quoted, in the order the caller supplies.
///
/// That order is the caller's statement, not this function's: the two `?select=` branches pass
/// selection order, and the filter branch passes a sort, because its source is a `HashMap` with
/// no order to preserve.
fn quoted_list<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> String {
    names
        .into_iter()
        .map(|n| format!("`{}`", n.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
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
        //
        // The plan carries the ORDER BY too (#1284): the rows a search matches
        // and the order they come back in are one decision over one field list.
        let search = params.search_query.as_deref().and_then(|query| plan_search(query, type_def));
        let fts_where = search.as_ref().map(|plan| plan.where_clause.clone());

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

        // ORDER BY — the client's sort, or the relevance ranking a search implies.
        //
        // The ranking does NOT go into `arguments`. It is not a value a client
        // can spell, and the argument map is the client's surface — the same
        // separation `scope_where` makes for server-composed predicates (#1170).
        // Writing it there as `[{"_relevance": "desc"}]` is exactly what #1284
        // was: a shape no consumer parses, type-checked by every layer that
        // touched it, so the documented default path of `?search=` — the one
        // this server's own OpenAPI document promises is "ranked by relevance
        // unless `sort` is specified" — answered 400 on every representation.
        let relevance = if let Some(ref order_by) = params.order_by {
            arguments.insert("orderBy".to_string(), order_by.clone());
            // The client named a sort, so it wins, exactly as documented.
            None
        } else {
            search.map(|plan| plan.relevance)
        };

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
        let mut query_match =
            QueryMatch::from_operation(query_def.clone(), field_names, arguments, type_def)?;
        if let Some(relevance) = relevance {
            query_match = query_match.with_search_relevance(relevance);
        }

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
