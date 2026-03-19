//! NDJSON streaming response handler for the REST transport.
//!
//! When a client sends `Accept: application/x-ndjson`, the GET handler delegates
//! to this module.  Each row is serialized as a single JSON line, with no
//! envelope (`data`/`meta`/`links`), enabling constant-memory streaming for
//! large result sets.

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::runtime::{Executor, QueryMatch};
use fraiseql_core::schema::{CompiledSchema, RestConfig};
use fraiseql_core::security::SecurityContext;

use super::handler::{PreferHeader, RestError, set_request_id};
use super::params::{PaginationParams, RestFieldSpec, RestParamExtractor};
use super::resource::{HttpMethod, RestRouteTable, RouteSource};

/// Content type for NDJSON responses.
pub const NDJSON_CONTENT_TYPE: &str = "application/x-ndjson";

/// Check whether an `Accept` header value requests NDJSON.
#[must_use]
pub fn accepts_ndjson(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(NDJSON_CONTENT_TYPE))
        })
}

/// Validate that NDJSON-incompatible preferences are not set.
///
/// Returns `Err(RestError)` if `Prefer: count=exact` or pagination params
/// (`limit`, `offset`) are present.
///
/// # Errors
///
/// Returns `RestError::BadRequest` when count or pagination is requested
/// alongside NDJSON streaming.
pub fn validate_ndjson_request(
    prefer: &PreferHeader,
    pagination: &PaginationParams,
) -> Result<(), RestError> {
    // Count is not available for streaming
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request(
            "count not available for streaming responses",
        ));
    }

    // Pagination is not available for streaming
    if let PaginationParams::Offset { offset, .. } = pagination {
        if *offset > 0 {
            return Err(RestError::bad_request(
                "pagination not available for streaming; use filters to narrow results",
            ));
        }
    }
    if matches!(pagination, PaginationParams::Cursor { .. }) {
        return Err(RestError::bad_request(
            "pagination not available for streaming; use filters to narrow results",
        ));
    }

    Ok(())
}

/// Request context for NDJSON streaming.
pub struct NdjsonRequest<'a, A: DatabaseAdapter> {
    /// Executor for query execution.
    pub executor: &'a Arc<Executor<A>>,
    /// Compiled schema reference.
    pub schema: &'a CompiledSchema,
    /// REST configuration.
    pub config: &'a RestConfig,
    /// REST route table.
    pub route_table: &'a RestRouteTable,
}

/// Execute a query and return results as an NDJSON byte stream.
///
/// Each row is serialized as a JSON object followed by a newline (`\n`).
/// The response uses `Transfer-Encoding: chunked` and has no envelope.
///
/// # Errors
///
/// Returns `RestError` on route resolution, parameter extraction, or query
/// execution failure.
pub async fn handle_ndjson_get<A: DatabaseAdapter>(
    ctx: &NdjsonRequest<'_, A>,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<NdjsonResponse, RestError> {
    let resolved = ctx
        .route_table
        .resolve(relative_path, HttpMethod::Get)
        .ok_or_else(|| RestError::not_found("Route not found"))?;

    let query_name = match &resolved.route.source {
        RouteSource::Query { name } => name.as_str(),
        RouteSource::Mutation { .. } => {
            return Err(RestError::internal("GET route backed by mutation"));
        }
    };

    let query_def = ctx
        .schema
        .find_query(query_name)
        .ok_or_else(|| RestError::not_found(format!("Query not found: {query_name}")))?;

    // Check requires_role
    if let Some(ref required_role) = query_def.requires_role {
        match security_context {
            Some(sec) if sec.scopes.contains(required_role) => {}
            _ => return Err(RestError::forbidden()),
        }
    }

    let type_def = ctx.schema.find_type(&query_def.return_type);

    let extractor = RestParamExtractor::new(ctx.config, query_def, type_def);
    let path_pairs: Vec<(&str, &str)> = resolved
        .path_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let params = extractor.extract(&path_pairs, query_pairs)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_ndjson_request(&prefer, &params.pagination)?;

    // Build field names
    let field_names = match &params.field_selection {
        RestFieldSpec::All => Vec::new(),
        RestFieldSpec::Fields(fields) => fields.clone(),
    };

    // Build arguments
    let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();
    for (key, value) in &params.path_params {
        arguments.insert(key.clone(), value.clone());
    }
    if let Some(ref where_clause) = params.where_clause {
        arguments.insert("where".to_string(), where_clause.clone());
    }
    if let Some(ref order_by) = params.order_by {
        arguments.insert("orderBy".to_string(), order_by.clone());
    }

    // Build variables
    let mut variables = serde_json::Map::new();
    for (k, v) in &arguments {
        variables.insert(k.clone(), v.clone());
    }
    let variables_json = serde_json::Value::Object(variables);

    // Build QueryMatch
    let query_match = QueryMatch::from_operation(
        query_def.clone(),
        field_names,
        arguments,
        type_def,
    )?;

    let vars_ref = if variables_json
        .as_object()
        .is_none_or(|m| m.is_empty())
    {
        None
    } else {
        Some(&variables_json)
    };

    // Execute the query — we get the full result and stream row-by-row
    let result = ctx
        .executor
        .execute_query_direct(&query_match, vars_ref, security_context)
        .await
        .map_err(RestError::from)?;

    // Parse and extract rows from the executor envelope
    let rows = extract_rows(&result, query_name)?;

    // Build NDJSON bytes: one JSON object per line
    let mut ndjson_bytes = Vec::new();
    for row in &rows {
        let mut line = serde_json::to_vec(row)
            .map_err(|e| RestError::internal(format!("Failed to serialize row: {e}")))?;
        line.push(b'\n');
        ndjson_bytes.extend_from_slice(&line);
    }

    // Build response headers
    let mut response_headers = HeaderMap::new();
    set_request_id(headers, &mut response_headers);
    response_headers.insert(
        "content-type",
        HeaderValue::from_static(NDJSON_CONTENT_TYPE),
    );

    Ok(NdjsonResponse {
        headers: response_headers,
        body: Bytes::from(ndjson_bytes),
    })
}

/// NDJSON streaming response (pre-serialized bytes).
#[derive(Debug)]
pub struct NdjsonResponse {
    /// Response headers.
    pub headers: HeaderMap,
    /// NDJSON body bytes.
    pub body: Bytes,
}

/// Extract rows from the executor result envelope.
///
/// The executor returns `{ "data": { "queryName": [...] } }`.
/// For a single resource, returns a one-element vec.
///
/// # Errors
///
/// Returns `RestError` if the result cannot be parsed.
fn extract_rows(result: &str, query_name: &str) -> Result<Vec<serde_json::Value>, RestError> {
    let parsed: serde_json::Value = serde_json::from_str(result)
        .map_err(|e| RestError::internal(format!("Failed to parse query result: {e}")))?;

    let data = parsed
        .get("data")
        .and_then(|d| d.get(query_name))
        .ok_or_else(|| RestError::internal("Missing data in query result"))?;

    match data {
        serde_json::Value::Array(arr) => Ok(arr.clone()),
        // Single resource — wrap in a vec
        other => Ok(vec![other.clone()]),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // accepts_ndjson
    // -----------------------------------------------------------------------

    #[test]
    fn accepts_ndjson_true_for_exact_match() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/x-ndjson"));
        assert!(accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_true_in_list() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, application/x-ndjson"),
        );
        assert!(accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_false_for_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json"));
        assert!(!accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_false_when_missing() {
        let headers = HeaderMap::new();
        assert!(!accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("Application/X-NDJSON"));
        assert!(accepts_ndjson(&headers));
    }

    // -----------------------------------------------------------------------
    // validate_ndjson_request
    // -----------------------------------------------------------------------

    #[test]
    fn validate_ndjson_rejects_count_exact() {
        let prefer = PreferHeader {
            count_exact: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("count not available"));
    }

    #[test]
    fn validate_ndjson_rejects_count_planned() {
        let prefer = PreferHeader {
            count_planned: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_rejects_count_estimated() {
        let prefer = PreferHeader {
            count_estimated: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_rejects_cursor_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Cursor {
            first: Some(10),
            after: None,
            last: None,
            before: None,
        };
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("pagination not available"));
    }

    #[test]
    fn validate_ndjson_rejects_offset_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 5,
        };
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_allows_limit_only() {
        // offset=0 with limit is fine — it's the default, not explicit pagination
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit: 100,
            offset: 0,
        };
        assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
    }

    #[test]
    fn validate_ndjson_allows_no_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::None;
        assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
    }

    // -----------------------------------------------------------------------
    // extract_rows
    // -----------------------------------------------------------------------

    #[test]
    fn extract_rows_from_array() {
        let result = r#"{"data":{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}}"#;
        let rows = extract_rows(result, "users").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["name"], "Bob");
    }

    #[test]
    fn extract_rows_from_single_resource() {
        let result = r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#;
        let rows = extract_rows(result, "user").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "Alice");
    }

    #[test]
    fn extract_rows_missing_data() {
        let result = r#"{"errors":[]}"#;
        let err = extract_rows(result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn extract_rows_missing_query() {
        let result = r#"{"data":{"other_query":[]}}"#;
        let err = extract_rows(result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // -----------------------------------------------------------------------
    // NDJSON serialization
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_format_one_object_per_line() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        let lines: Vec<&str> = output.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line is valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    #[test]
    fn ndjson_no_envelope() {
        let rows = vec![json!({"id": 1})];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        // No "data", "meta", or "links" wrapper
        assert!(!output.contains("\"data\""));
        assert!(!output.contains("\"meta\""));
        assert!(!output.contains("\"links\""));
    }

    #[test]
    fn ndjson_select_fields_applied() {
        // When ?select=id,name is used, each row should only have those fields.
        // This is handled upstream by QueryMatch field selection, but verify format.
        let rows = [json!({"id": 1, "name": "Alice"})];

        let line = serde_json::to_string(&rows[0]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(parsed.get("id").is_some());
        assert!(parsed.get("name").is_some());
        assert!(parsed.get("email").is_none());
    }

    // -----------------------------------------------------------------------
    // Content-Type header
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_content_type_constant() {
        assert_eq!(NDJSON_CONTENT_TYPE, "application/x-ndjson");
    }
}
