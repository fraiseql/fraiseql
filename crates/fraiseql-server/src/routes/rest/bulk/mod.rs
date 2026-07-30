//! Bulk operation handler — array body insert, filter-based update/delete.
//!
//! CQRS constraint: all writes go through mutation functions.  The REST layer
//! never issues raw `INSERT`, `UPDATE`, or `DELETE` SQL.

pub mod helpers;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::{Executor, QueryMatch},
    schema::{CompiledSchema, MutationOperation, RestConfig},
    security::SecurityContext,
};
use helpers::{extract_entity_from_result, extract_ids, set_rows_affected};
use serde_json::json;

use super::{
    handler::{PreferHeader, RestError, RestResponse, set_preference_applied, set_request_id},
    params::RestParamExtractor,
    resource::{RestRouteTable, RouteSource},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of items in a single bulk insert request.
const MAX_BULK_INSERT_ITEMS: usize = 1_000;

/// Operation-specific parameters for filter-based bulk operations.
struct BulkFilterOp<'a> {
    operation:          &'a str,
    missing_filter_msg: &'a str,
}

// ---------------------------------------------------------------------------
// Bulk handler
// ---------------------------------------------------------------------------

/// Handles bulk operations for the REST transport.
pub struct BulkHandler<'a, A: DatabaseAdapter> {
    executor:    &'a Arc<Executor<A>>,
    schema:      &'a CompiledSchema,
    config:      &'a RestConfig,
    route_table: &'a RestRouteTable,
}

impl<'a, A: DatabaseAdapter + SupportsMutations> BulkHandler<'a, A> {
    /// Create a new bulk handler.
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
        }
    }

    /// Handle a bulk POST (array body → batch insert or upsert).
    ///
    /// # Errors
    ///
    /// Returns `RestError` on validation failure or mutation error.
    pub async fn handle_bulk_insert(
        &self,
        items: &[serde_json::Value],
        mutation_name: &str,
        prefer: &PreferHeader,
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        // Validate array is non-empty
        if items.is_empty() {
            return Err(RestError::bad_request("Bulk insert requires at least one item"));
        }

        // Validate size limit
        if items.len() > MAX_BULK_INSERT_ITEMS {
            return Err(RestError::bad_request(format!(
                "Bulk insert limited to {MAX_BULK_INSERT_ITEMS} items, got {}",
                items.len()
            )));
        }

        // Check upsert mode
        let effective_mutation = if let Some(ref resolution) = prefer.resolution {
            let mutation_def = self.schema.find_mutation(mutation_name).ok_or_else(|| {
                RestError::bad_request(format!("Mutation '{mutation_name}' not found"))
            })?;

            match resolution.as_str() {
                "merge-duplicates" | "ignore-duplicates" => match &mutation_def.upsert_function {
                    Some(upsert_fn) => upsert_fn.as_str(),
                    None => {
                        return Err(RestError::bad_request(
                            "Upsert not available — no compiler-generated upsert function exists",
                        ));
                    },
                },
                _ => mutation_name,
            }
        } else {
            mutation_name
        };

        // Execute batch
        let results = self
            .executor
            .execute_mutation_batch(effective_mutation, items, security_context)
            .await
            .map_err(RestError::from)?;

        let mut response_headers = HeaderMap::new();
        set_request_id(headers, &mut response_headers);
        set_rows_affected(&mut response_headers, results.affected_rows);

        // Collect all applied preferences into a single header
        let mut applied: Vec<String> = Vec::new();
        if let Some(ref res) = prefer.resolution {
            applied.push(format!("resolution={res}"));
        }

        // Return representation or minimal
        if prefer.return_minimal {
            applied.push("return=minimal".to_string());
            let refs: Vec<&str> = applied.iter().map(String::as_str).collect();
            set_preference_applied(&mut response_headers, &refs);
            Ok(RestResponse {
                status:  StatusCode::CREATED,
                headers: response_headers,
                body:    None,
            })
        } else {
            // Parse and collect entity data from results
            let entities: Vec<serde_json::Value> = results
                .entities
                .unwrap_or_default()
                .iter()
                .filter_map(|r| {
                    if r.is_string() {
                        // Legacy: string-wrapped JSON result — parse and extract
                        let parsed: serde_json::Value = serde_json::from_str(r.as_str()?).ok()?;
                        extract_entity_from_result(&parsed)
                    } else {
                        extract_entity_from_result(r)
                    }
                })
                .collect();

            if prefer.return_representation {
                applied.push("return=representation".to_string());
            }
            let refs: Vec<&str> = applied.iter().map(String::as_str).collect();
            set_preference_applied(&mut response_headers, &refs);

            Ok(RestResponse {
                status:  StatusCode::CREATED,
                headers: response_headers,
                body:    Some(json!(entities)),
            })
        }
    }

    /// Handle a bulk PATCH (collection-level update with filter).
    ///
    /// CQRS flow: query view → get matching IDs → count guard → mutate per row.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on missing filter, max-affected exceeded, or mutation error.
    pub async fn handle_bulk_update(
        &self,
        relative_path: &str,
        body: &serde_json::Value,
        query_params: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        self.handle_bulk_filter_operation(
            relative_path,
            body,
            query_params,
            headers,
            security_context,
            BulkFilterOp {
                operation:          "update",
                missing_filter_msg: "Bulk update requires at least one filter parameter",
            },
        )
        .await
    }

    /// Handle a bulk DELETE (collection-level delete with filter).
    ///
    /// CQRS flow: query view → get matching IDs → count guard → delete per row.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on missing filter, max-affected exceeded, or mutation error.
    pub async fn handle_bulk_delete(
        &self,
        relative_path: &str,
        query_params: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
    ) -> Result<RestResponse, RestError> {
        let empty_body = json!({});
        self.handle_bulk_filter_operation(
            relative_path,
            &empty_body,
            query_params,
            headers,
            security_context,
            BulkFilterOp {
                operation:          "delete",
                missing_filter_msg: "Bulk delete requires at least one filter parameter",
            },
        )
        .await
    }

    /// Shared CQRS filter-based bulk operation (update or delete).
    ///
    /// Flow: validate filter → resolve mutation → query view for IDs →
    /// count guard → mutate per row → build response.
    async fn handle_bulk_filter_operation(
        &self,
        relative_path: &str,
        body: &serde_json::Value,
        query_params: &[(&str, &str)],
        headers: &HeaderMap,
        security_context: Option<&SecurityContext>,
        op: BulkFilterOp<'_>,
    ) -> Result<RestResponse, RestError> {
        let prefer = PreferHeader::from_headers(headers);

        // #914: `tx=rollback` was parsed, echoed in `Preference-Applied`, and never
        // honoured — a dry-run bulk DELETE committed while the response asserted the
        // rollback. Refusing is the honest answer until the preference is implemented:
        // the adapter has `execute_function_call_dry_run` (run-in-transaction-then-
        // rollback), but reaching it per request needs an execution mode threaded
        // through `Executor::execute`, whose `RuntimeConfig` is shared across requests.
        if prefer.tx_rollback {
            return Err(RestError::bad_request(
                "Prefer: tx=rollback is not supported on bulk operations. Omit the \
                 preference to execute, or use `fraiseql query --dry-run` to validate a \
                 mutation without committing.",
            ));
        }

        let operation = op.operation;

        let (resource, mutation_name, list_query_name) =
            self.resolve_bulk_mutation(relative_path, operation)?;

        let id_field = resource.id_arg.as_deref().unwrap_or("id");

        // #916: a client-supplied `max-affected` may only make the request *more*
        // conservative. This used to be `unwrap_or`, so a client replaced the
        // operator's cap outright and could raise it to any `u64`.
        let max_affected = prefer
            .max_affected
            .map_or(self.config.max_bulk_affected, |n| n.min(self.config.max_bulk_affected));

        let query_match = self.build_filter_query_match(
            list_query_name,
            query_params,
            &resource.type_name,
            op.missing_filter_msg,
            max_affected,
        )?;

        // Select the rows the filter matched. `build_filter_query_match` bounded this
        // query at `max_affected + 1`, so an over-cap request is detectable without
        // scanning the whole view (#862).
        let filter_result = self
            .executor
            .execute_query_direct(&query_match, None, security_context)
            .await
            .map_err(RestError::from)?;

        let ids = extract_ids(&filter_result, id_field);

        if u64::try_from(ids.len()).unwrap_or(u64::MAX) > max_affected {
            return Err(RestError {
                status:  StatusCode::PAYLOAD_TOO_LARGE,
                code:    "TOO_MANY_AFFECTED",
                message: format!(
                    "Bulk {operation} matches more than the maximum of {max_affected} rows. \
                     Narrow the filter, or lower `Prefer: max-affected`."
                ),
                details: None,
            });
        }

        // #913: mutate each matched row. The previous implementation called the mutation
        // once with no row identity and reported the filter's row count as
        // `affected_rows` — a count for work it had not done.
        let bulk_result = self
            .executor
            .execute_bulk_by_ids(mutation_name, id_field, &ids, Some(body), security_context)
            .await
            .map_err(RestError::from)?;

        self.build_bulk_response(bulk_result, &prefer, headers)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Resolve a collection-level path to the appropriate bulk mutation.
    ///
    /// Returns `(resource, mutation_name, list_query_name)`.
    fn resolve_bulk_mutation(
        &self,
        relative_path: &str,
        operation: &str,
    ) -> Result<(&super::resource::RestResource, &str, &str), RestError> {
        // Find the resource matching this collection path
        let path_segments: Vec<&str> = relative_path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let resource_name = path_segments.first().copied().unwrap_or("");

        let resource =
            self.route_table.resources.iter().find(|r| r.name == resource_name).ok_or_else(
                || RestError::not_found(format!("Resource '{resource_name}' not found")),
            )?;

        // Find the appropriate mutation (update or delete) for this resource
        let mutation_name = resource
            .routes
            .iter()
            .find_map(|route| match &route.source {
                RouteSource::Mutation { name } => {
                    let mutation_def = self.schema.find_mutation(name)?;
                    let op_matches = matches!(
                        (&mutation_def.operation, operation),
                        (MutationOperation::Update { .. }, "update")
                            | (MutationOperation::Delete { .. }, "delete")
                    );
                    if op_matches {
                        Some(name.as_str())
                    } else {
                        None
                    }
                },
                RouteSource::Query { .. } => None,
            })
            .ok_or_else(|| {
                RestError::bad_request(format!(
                    "No {operation} mutation found for resource '{resource_name}'"
                ))
            })?;

        // Find the list query for this resource
        let list_query_name = resource
            .routes
            .iter()
            .find_map(|route| match &route.source {
                RouteSource::Query { name } if route.path == format!("/{resource_name}") => {
                    Some(name.as_str())
                },
                _ => None,
            })
            .ok_or_else(|| {
                RestError::internal(format!("No list query found for resource '{resource_name}'"))
            })?;

        Ok((resource, mutation_name, list_query_name))
    }

    /// Build a `QueryMatch` from query parameters for filter-based queries.
    /// Build the row-selection query for a bulk operation.
    ///
    /// The filter guard lives **here**, after extraction, not in a syntactic pre-check
    /// over the raw query string. `has_filter_params` used to answer "does this look
    /// like it has a filter?" while this function forwarded only `params.where_clause` —
    /// so `?filter={}`, `?search=x` and any dotted key satisfied the guard and produced
    /// no `WHERE` clause at all (`#862`). Two functions answering the same question in
    /// different ways is the defect; there is now one answer, taken where the truth is
    /// knowable.
    fn build_filter_query_match(
        &self,
        query_name: &str,
        query_params: &[(&str, &str)],
        type_name: &str,
        missing_filter_msg: &str,
        max_affected: u64,
    ) -> Result<QueryMatch, RestError> {
        let query_def = self
            .schema
            .find_query(query_name)
            .ok_or_else(|| RestError::internal(format!("Query '{query_name}' not found")))?
            .clone();

        let type_def = self.schema.find_type(type_name);

        let extractor = RestParamExtractor::new(self.config, &query_def, type_def);

        let params = extractor.extract(&[], query_params).map_err(RestError::from)?;

        // Build QueryMatch with only the ID field for bulk operations
        let id_fields = type_def
            .map(|td| {
                td.fields
                    .iter()
                    .filter(|f| f.is_primary_key())
                    .map(|f| f.output_name().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let fields = if id_fields.is_empty() {
            vec!["id".to_string()]
        } else {
            id_fields
        };

        // A filter the row-selection query cannot apply is worse than no filter: the
        // executor reads `arguments["where"]` only when the query declares
        // `auto_params.has_where`, so on a query without it the WHERE clause is dropped
        // and the selection becomes the first `max_affected` rows of the *whole view* —
        // rows the caller never asked for, mutated under a filter it believes applied.
        if !query_def.auto_params.has_where {
            return Err(RestError::bad_request(format!(
                "Query '{query_name}' does not accept a `where` argument, so a bulk filter \
                 cannot be applied to it. Bulk operations require a filterable list query."
            )));
        }

        // The guard: a filter that contributes no WHERE clause is not a filter. Without
        // this, a bulk mutation runs against every row of the view.
        let Some(where_clause) = params.where_clause.clone() else {
            return Err(RestError::bad_request(missing_filter_msg));
        };

        // Parameters the extractor accepted but this path cannot honour. Silently
        // dropping them is what let `?search=x` and `?rel.field=v` pass for filters.
        if params.search_query.is_some() {
            return Err(RestError::bad_request(
                "`search` is not supported on bulk operations — it does not contribute a \
                 WHERE clause. Use an explicit field filter.",
            ));
        }
        if !params.embedding_filters.is_empty() {
            return Err(RestError::bad_request(
                "Embedded-relationship filters (`rel.field=value`) are not supported on \
                 bulk operations — they do not contribute a WHERE clause. Use an explicit \
                 field filter.",
            ));
        }

        let mut arguments = std::collections::HashMap::new();
        arguments.insert("where".to_string(), where_clause);
        // Always bound the selection. `enforce_max_page_size(None, max)` returns
        // `Ok(None)`, so an absent limit meant an unbounded scan of the whole view
        // materialised into JSON (#862). `+ 1` makes "more than the cap" detectable
        // without fetching every row.
        arguments.insert("limit".to_string(), serde_json::json!(max_affected.saturating_add(1)));

        QueryMatch::from_operation(query_def, fields, arguments, type_def).map_err(RestError::from)
    }

    /// Build the HTTP response for a bulk operation result.
    fn build_bulk_response(
        &self,
        bulk_result: fraiseql_core::runtime::BulkResult,
        prefer: &PreferHeader,
        headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let mut response_headers = HeaderMap::new();
        set_request_id(headers, &mut response_headers);
        set_rows_affected(&mut response_headers, bulk_result.affected_rows);

        let mut applied: Vec<&str> = Vec::new();

        if prefer.return_representation {
            let entities: Vec<serde_json::Value> = bulk_result
                .entities
                .unwrap_or_default()
                .iter()
                .filter_map(|r| {
                    if r.is_string() {
                        // Legacy: string-wrapped JSON result — parse and extract
                        let parsed: serde_json::Value = serde_json::from_str(r.as_str()?).ok()?;
                        extract_entity_from_result(&parsed)
                    } else {
                        extract_entity_from_result(r)
                    }
                })
                .collect();

            applied.push("return=representation");
            set_preference_applied(&mut response_headers, &applied);

            Ok(RestResponse {
                status:  StatusCode::OK,
                headers: response_headers,
                body:    Some(json!(entities)),
            })
        } else if prefer.return_minimal || bulk_result.affected_rows == 0 {
            if prefer.return_minimal {
                applied.push("return=minimal");
            }
            set_preference_applied(&mut response_headers, &applied);
            Ok(RestResponse {
                status:  StatusCode::NO_CONTENT,
                headers: response_headers,
                body:    None,
            })
        } else {
            set_preference_applied(&mut response_headers, &applied);
            Ok(RestResponse {
                status:  StatusCode::OK,
                headers: response_headers,
                body:    None,
            })
        }
    }
}
