//! Helper functions for REST request handling and response building.

use std::collections::HashMap;

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::Executor,
    schema::TypeDefinition,
    security::SecurityContext,
};
use serde_json::json;

use super::response::{RestError, RestResponse};
use crate::routes::rest::{
    idempotency::StoredResponse,
    params::PaginationParams,
};

/// Convert a [`StoredResponse`] from the idempotency store back to a [`RestResponse`].
pub(super) fn stored_response_to_rest(stored: StoredResponse, request_headers: &HeaderMap) -> RestResponse {
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

/// Coerce a path parameter string to an appropriate JSON value.
///
/// Attempts integer, then boolean, then falls back to string.
pub(super) fn coerce_path_param_value(value: &str) -> serde_json::Value {
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
pub(super) fn validate_put_body(body: &serde_json::Value, type_def: &TypeDefinition) -> Result<(), RestError> {
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
pub(super) fn build_fts_where_clause(
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
    use super::*;

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

    #[test]
    fn coerce_path_param_value_integer() {
        let val = coerce_path_param_value("42");
        assert_eq!(val, json!(42i64));
    }

    #[test]
    fn coerce_path_param_value_boolean_true() {
        let val = coerce_path_param_value("true");
        assert_eq!(val, json!(true));
    }

    #[test]
    fn coerce_path_param_value_boolean_false() {
        let val = coerce_path_param_value("false");
        assert_eq!(val, json!(false));
    }

    #[test]
    fn coerce_path_param_value_string() {
        let val = coerce_path_param_value("hello");
        assert_eq!(val, json!("hello"));
    }

    #[test]
    fn build_query_response_single() {
        let result = json!({
            "data": {
                "user": {
                    "id": 1,
                    "name": "Alice"
                }
            }
        });
        let response = build_query_response(&result, None, &PaginationParams::None).unwrap();
        assert_eq!(response["data"]["id"], 1);
        assert!(!response.get("meta").is_some_and(|m| m.is_object()));
    }

    #[test]
    fn build_query_response_with_offset_pagination() {
        let result = json!({
            "data": {
                "users": [
                    {"id": 1},
                    {"id": 2}
                ]
            }
        });
        let pagination = PaginationParams::Offset {
            limit:  10,
            offset: 0,
        };
        let response = build_query_response(&result, Some(100), &pagination).unwrap();
        assert_eq!(response["meta"]["limit"], 10);
        assert_eq!(response["meta"]["offset"], 0);
        assert_eq!(response["meta"]["total"], 100);
    }

    #[test]
    fn extract_relay_page_info_present() {
        let data = json!({
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false
            }
        });
        let info = extract_relay_page_info(&data);
        assert!(info.is_some());
        assert_eq!(info.unwrap()["hasNextPage"], true);
    }

    #[test]
    fn extract_relay_page_info_missing() {
        let data = json!({"items": []});
        let info = extract_relay_page_info(&data);
        assert!(info.is_none());
    }

    #[test]
    fn set_preference_applied_single() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["count=exact"]);
        assert_eq!(headers.get("preference-applied").unwrap().to_str().unwrap(), "count=exact");
    }

    #[test]
    fn set_preference_applied_multiple() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["count=exact", "return=representation"]);
        let value = headers.get("preference-applied").unwrap().to_str().unwrap();
        assert!(value.contains("count=exact"));
        assert!(value.contains("return=representation"));
    }

    #[test]
    fn set_preference_applied_empty() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &[]);
        assert!(headers.get("preference-applied").is_none());
    }

    #[test]
    fn set_preference_applied_filters_empty() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["", "count=exact", ""]);
        let value = headers.get("preference-applied").unwrap().to_str().unwrap();
        assert_eq!(value, "count=exact");
    }

    #[test]
    fn set_request_id_from_request() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert("x-request-id", "test-id-123".parse().unwrap());
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        assert_eq!(response_headers.get("x-request-id").unwrap().to_str().unwrap(), "test-id-123");
    }

    #[test]
    fn set_request_id_generate_new() {
        let request_headers = HeaderMap::new();
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        let id = response_headers.get("x-request-id").unwrap().to_str().unwrap();
        // Should be a valid UUID
        assert!(uuid::Uuid::parse_str(id).is_ok());
    }
}
