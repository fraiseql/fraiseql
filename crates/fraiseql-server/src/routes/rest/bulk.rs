//! Bulk operation handler — array body insert, filter-based update/delete.
//!
//! CQRS constraint: all writes go through mutation functions.  The REST layer
//! never issues raw `INSERT`, `UPDATE`, or `DELETE` SQL.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::{Executor, QueryMatch},
    schema::{CompiledSchema, MutationOperation, RestConfig},
    security::SecurityContext,
};
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
    operation: &'a str,
    missing_filter_msg: &'a str,
}

// ---------------------------------------------------------------------------
// Bulk handler
// ---------------------------------------------------------------------------

/// Handles bulk operations for the REST transport.
pub struct BulkHandler<'a, A: DatabaseAdapter> {
    executor: &'a Arc<Executor<A>>,
    schema: &'a CompiledSchema,
    config: &'a RestConfig,
    route_table: &'a RestRouteTable,
}

impl<'a, A: DatabaseAdapter + SupportsMutations> BulkHandler<'a, A> {
    /// Create a new bulk handler.
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
        if prefer.tx_rollback {
            applied.push("tx=rollback".to_string());
        }

        // Return representation or minimal
        if prefer.return_minimal {
            applied.push("return=minimal".to_string());
            let refs: Vec<&str> = applied.iter().map(String::as_str).collect();
            set_preference_applied(&mut response_headers, &refs);
            Ok(RestResponse {
                status: StatusCode::CREATED,
                headers: response_headers,
                body: None,
            })
        } else {
            // Parse and collect entity data from results
            let entities: Vec<serde_json::Value> = results
                .entities
                .unwrap_or_default()
                .iter()
                .filter_map(|r| {
                    if let serde_json::Value::String(s) = r {
                        extract_entity_from_result(s)
                    } else {
                        Some(r.clone())
                    }
                })
                .collect();

            if prefer.return_representation {
                applied.push("return=representation".to_string());
            }
            let refs: Vec<&str> = applied.iter().map(String::as_str).collect();
            set_preference_applied(&mut response_headers, &refs);

            Ok(RestResponse {
                status: StatusCode::CREATED,
                headers: response_headers,
                body: Some(json!(entities)),
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
                operation: "update",
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
                operation: "delete",
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

        if !has_filter_params(query_params) {
            return Err(RestError::bad_request(op.missing_filter_msg));
        }

        let operation = op.operation;

        let (resource, mutation_name, list_query_name) =
            self.resolve_bulk_mutation(relative_path, operation)?;

        let id_field = resource.id_arg.as_deref().unwrap_or("id");

        let query_match =
            self.build_filter_query_match(list_query_name, query_params, &resource.type_name)?;

        let max_affected = prefer.max_affected.unwrap_or(self.config.max_bulk_affected);

        let bulk_result = self
            .executor
            .execute_bulk_by_filter(
                &query_match,
                mutation_name,
                Some(body),
                id_field,
                max_affected,
                security_context,
            )
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
    fn build_filter_query_match(
        &self,
        query_name: &str,
        query_params: &[(&str, &str)],
        type_name: &str,
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

        let mut arguments = std::collections::HashMap::new();
        if let Some(ref where_clause) = params.where_clause {
            arguments.insert("where".to_string(), where_clause.clone());
        }

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
        if prefer.tx_rollback {
            applied.push("tx=rollback");
        }

        if prefer.return_representation {
            let entities: Vec<serde_json::Value> = bulk_result
                .entities
                .unwrap_or_default()
                .iter()
                .filter_map(|r| {
                    if let serde_json::Value::String(s) = r {
                        extract_entity_from_result(s)
                    } else {
                        Some(r.clone())
                    }
                })
                .collect();

            applied.push("return=representation");
            set_preference_applied(&mut response_headers, &applied);

            Ok(RestResponse {
                status: StatusCode::OK,
                headers: response_headers,
                body: Some(json!(entities)),
            })
        } else if prefer.return_minimal || bulk_result.affected_rows == 0 {
            if prefer.return_minimal {
                applied.push("return=minimal");
            }
            set_preference_applied(&mut response_headers, &applied);
            Ok(RestResponse {
                status: StatusCode::NO_CONTENT,
                headers: response_headers,
                body: None,
            })
        } else {
            set_preference_applied(&mut response_headers, &applied);
            Ok(RestResponse {
                status: StatusCode::OK,
                headers: response_headers,
                body: None,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Check if query parameters contain at least one filter.
fn has_filter_params(query_params: &[(&str, &str)]) -> bool {
    // Reserved non-filter params
    const NON_FILTER: &[&str] = &[
        "select", "sort", "limit", "offset", "first", "after", "last", "before", "filter",
    ];

    query_params.iter().any(|(key, _)| {
        let base_key = key.split('[').next().unwrap_or(key);
        // "filter" IS a filter param (JSON DSL), others with brackets are bracket operators
        if *key == "filter" {
            return true;
        }
        // Bracket operators like name[eq]=foo are filters
        if key.contains('[') {
            return true;
        }
        // Simple value params that aren't reserved are implicit eq filters
        !NON_FILTER.contains(&base_key)
    })
}

/// Extract entity data from a mutation result JSON string.
fn extract_entity_from_result(result: &str) -> Option<serde_json::Value> {
    let parsed: serde_json::Value = serde_json::from_str(result).ok()?;
    let data = parsed.get("data")?;

    // Get the first field in the data object (mutation name)
    let mutation_result = data.as_object()?.values().next()?;

    // Try nested entity format first
    if let Some(entity) = mutation_result.get("entity") {
        if entity.is_null() {
            return None;
        }
        let mut cleaned = entity.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            obj.remove("__typename");
        }
        return Some(cleaned);
    }

    // Executor format: fields + __typename at top level
    if mutation_result.is_object() && !mutation_result.as_object()?.is_empty() {
        let mut cleaned = mutation_result.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            obj.remove("__typename");
        }
        if cleaned.as_object().is_some_and(serde_json::Map::is_empty) {
            return None;
        }
        return Some(cleaned);
    }

    None
}

/// Set `X-Rows-Affected` header.
fn set_rows_affected(headers: &mut HeaderMap, count: u64) {
    if let Ok(val) = HeaderValue::from_str(&count.to_string()) {
        headers.insert("x-rows-affected", val);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // has_filter_params tests
    // -----------------------------------------------------------------------

    #[test]
    fn no_filter_params_empty() {
        assert!(!has_filter_params(&[]));
    }

    #[test]
    fn no_filter_only_reserved() {
        let params = vec![
            ("select", "id,name"),
            ("sort", "-name"),
            ("limit", "10"),
            ("offset", "0"),
        ];
        assert!(!has_filter_params(&params));
    }

    #[test]
    fn filter_bracket_operator() {
        let params = vec![("status[eq]", "inactive")];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_json_dsl() {
        let params = vec![("filter", r#"{"status":{"eq":"inactive"}}"#)];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_simple_value() {
        // Simple value param that isn't reserved → implicit eq
        let params = vec![("status", "inactive")];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_mixed_with_reserved() {
        let params = vec![("limit", "10"), ("status[eq]", "inactive")];
        assert!(has_filter_params(&params));
    }

    // -----------------------------------------------------------------------
    // extract_entity_from_result tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_entity_nested_format() {
        let result = r#"{"data":{"createUser":{"entity":{"id":1,"name":"Alice"}}}}"#;
        let entity = extract_entity_from_result(result).unwrap();
        assert_eq!(entity["id"], 1);
        assert_eq!(entity["name"], "Alice");
    }

    #[test]
    fn extract_entity_executor_format() {
        let result =
            r#"{"data":{"createUser":{"pk_user_id":1,"name":"Alice","__typename":"User"}}}"#;
        let entity = extract_entity_from_result(result).unwrap();
        assert_eq!(entity["pk_user_id"], 1);
        assert!(entity.get("__typename").is_none());
    }

    #[test]
    fn extract_entity_null() {
        let result = r#"{"data":{"createUser":{"entity":null}}}"#;
        assert!(extract_entity_from_result(result).is_none());
    }

    #[test]
    fn extract_entity_invalid_json() {
        assert!(extract_entity_from_result("not json").is_none());
    }

    // -----------------------------------------------------------------------
    // X-Rows-Affected header tests
    // -----------------------------------------------------------------------

    #[test]
    fn rows_affected_header() {
        let mut headers = HeaderMap::new();
        set_rows_affected(&mut headers, 42);
        assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "42");
    }

    #[test]
    fn rows_affected_zero() {
        let mut headers = HeaderMap::new();
        set_rows_affected(&mut headers, 0);
        assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "0");
    }
}
