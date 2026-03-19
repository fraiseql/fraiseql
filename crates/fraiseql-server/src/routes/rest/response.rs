//! REST response formatting and HTTP semantics.
//!
//! [`RestResponseFormatter`] transforms raw execution results into proper HTTP
//! responses: collection envelopes with pagination metadata and links, `201`
//! with `Location` for creates, `204` for deletes with `Prefer` negotiation,
//! structured error responses, `ETag` via xxHash64, `If-None-Match` → `304`,
//! `X-Request-Id` on all responses, and `Allow` header on `405`.

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::json;
use xxhash_rust::xxh3::xxh3_64;

use super::handler::{PreferHeader, RestError, RestResponse};
use super::params::PaginationParams;
use super::resource::HttpMethod;
use fraiseql_core::schema::{DeleteResponse, RestConfig};

// ---------------------------------------------------------------------------
// RestResponseFormatter
// ---------------------------------------------------------------------------

/// Formats raw execution results into HTTP responses with correct status codes,
/// headers, and envelope structure.
pub struct RestResponseFormatter<'a> {
    config: &'a RestConfig,
    base_path: &'a str,
}

impl<'a> RestResponseFormatter<'a> {
    /// Create a new response formatter.
    #[must_use]
    pub const fn new(config: &'a RestConfig, base_path: &'a str) -> Self {
        Self { config, base_path }
    }

    /// Format a single-resource GET response.
    ///
    /// Returns 200 with `{ "data": ... }` envelope, ETag, and `X-Request-Id`.
    /// If `If-None-Match` matches the computed ETag, returns 304 Not Modified.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_single(
        &self,
        result: &str,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_single_data(result)?;
        let body = json!({ "data": data });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| RestError::internal(format!("Failed to serialize response: {e}")))?;

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // ETag
        if self.config.etag {
            let etag = compute_etag(&body_bytes);
            if check_if_none_match(request_headers, &etag) == Some(true) {
                headers.insert("etag", header_value(&etag));
                return Ok(RestResponse {
                    status: StatusCode::NOT_MODIFIED,
                    headers,
                    body: None,
                });
            }
            headers.insert("etag", header_value(&etag));
        }

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(body),
        })
    }

    /// Format a collection GET response with pagination metadata and links.
    ///
    /// Uses `PaginationParams` to decide link style (offset vs cursor).
    /// Includes `Preference-Applied: count=exact` when total was requested.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_collection(
        &self,
        result: &str,
        total: Option<u64>,
        pagination: &PaginationParams,
        resource_path: &str,
        request_headers: &HeaderMap,
        prefer: &PreferHeader,
    ) -> Result<RestResponse, RestError> {
        let data = extract_collection_data(result)?;
        let mut response = json!({ "data": data });

        // Meta
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

                // Links
                let base = format!("{}{}", self.base_path, resource_path);
                let links = build_offset_links(&base, *limit, *offset, total);
                response["links"] = links;
            }
            PaginationParams::Cursor {
                first,
                after,
                last,
                before,
            } => {
                let mut meta = serde_json::Map::new();
                // Extract Relay pageInfo from the data envelope
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

                // Links
                let base = format!("{}{}", self.base_path, resource_path);
                let links =
                    build_cursor_links(&base, *first, after.as_deref(), &data);
                response["links"] = links;
            }
            PaginationParams::None => {
                // Single resource via collection endpoint (shouldn't happen, but handle gracefully)
            }
        }

        let body_bytes = serde_json::to_vec(&response)
            .map_err(|e| RestError::internal(format!("Failed to serialize response: {e}")))?;

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Preference-Applied for count=exact
        if prefer.count_exact && total.is_some() {
            headers.insert(
                "preference-applied",
                HeaderValue::from_static("count=exact"),
            );
        }

        // ETag
        if self.config.etag {
            let etag = compute_etag(&body_bytes);
            if check_if_none_match(request_headers, &etag) == Some(true) {
                headers.insert("etag", header_value(&etag));
                return Ok(RestResponse {
                    status: StatusCode::NOT_MODIFIED,
                    headers,
                    body: None,
                });
            }
            headers.insert("etag", header_value(&etag));
        }

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(response),
        })
    }

    /// Format a 201 Created response with `Location` header.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the mutation result cannot be parsed.
    pub fn format_created(
        &self,
        result: &str,
        resource_path: &str,
        id: Option<&serde_json::Value>,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_mutation_data(result)?;
        let body = json!({ "data": data });

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Location header
        if let Some(id_val) = id.or_else(|| extract_id_from_data(&data)) {
            let id_str = format_id_for_url(id_val);
            let location = format!("{}{}/{}", self.base_path, resource_path, id_str);
            if let Ok(val) = HeaderValue::from_str(&location) {
                headers.insert("location", val);
            }
        }

        Ok(RestResponse {
            status: StatusCode::CREATED,
            headers,
            body: Some(body),
        })
    }

    /// Format a mutation response (PUT, PATCH, custom action — 200 OK).
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the mutation result cannot be parsed.
    pub fn format_mutation(
        &self,
        result: &str,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_mutation_data(result)?;
        let body = json!({ "data": data });

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(body),
        })
    }

    /// Format a DELETE response based on config and `Prefer` header.
    ///
    /// Gracefully degrades when entity data is unavailable: returns 204 with
    /// `X-Preference-Fallback: entity-unavailable` instead of 200.
    pub fn format_deleted(
        &self,
        result: &str,
        mutation_name: &str,
        prefer: &PreferHeader,
        request_headers: &HeaderMap,
    ) -> RestResponse {
        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Determine return behavior: Prefer header overrides config
        let want_entity = if prefer.return_representation {
            true
        } else if prefer.return_minimal {
            false
        } else {
            matches!(self.config.delete_response, DeleteResponse::Entity)
        };

        if want_entity {
            let entity = extract_delete_entity(result, mutation_name);

            match entity {
                Some(entity_value) => {
                    if prefer.return_representation {
                        headers.insert(
                            "preference-applied",
                            HeaderValue::from_static("return=representation"),
                        );
                    }
                    RestResponse {
                        status: StatusCode::OK,
                        headers,
                        body: Some(json!({ "data": entity_value })),
                    }
                }
                None => {
                    // Graceful degradation: entity unavailable
                    if prefer.return_representation {
                        headers.insert(
                            "preference-applied",
                            HeaderValue::from_static("return=minimal"),
                        );
                        headers.insert(
                            "x-preference-fallback",
                            HeaderValue::from_static("entity-unavailable"),
                        );
                    }
                    RestResponse {
                        status: StatusCode::NO_CONTENT,
                        headers,
                        body: None,
                    }
                }
            }
        } else {
            if prefer.return_minimal {
                headers.insert(
                    "preference-applied",
                    HeaderValue::from_static("return=minimal"),
                );
            }
            RestResponse {
                status: StatusCode::NO_CONTENT,
                headers,
                body: None,
            }
        }
    }

    /// Format an error response with appropriate status code and structured body.
    ///
    /// Includes `Allow` header on 405 Method Not Allowed.
    #[must_use]
    pub fn format_error(
        error: &RestError,
        request_headers: &HeaderMap,
        allowed_methods: Option<&[HttpMethod]>,
    ) -> RestResponse {
        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Allow header on 405
        if error.status == StatusCode::METHOD_NOT_ALLOWED {
            if let Some(methods) = allowed_methods {
                let allow_value: String = methods
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                if let Ok(val) = HeaderValue::from_str(&allow_value) {
                    headers.insert("allow", val);
                }
            }
        }

        RestResponse {
            status: error.status,
            headers,
            body: Some(error.to_json()),
        }
    }
}

// ---------------------------------------------------------------------------
// ETag helpers
// ---------------------------------------------------------------------------

/// Compute a weak ETag from response body bytes using xxHash64.
fn compute_etag(body: &[u8]) -> String {
    let hash = xxh3_64(body);
    format!("W/\"{hash:016x}\"")
}

/// Check `If-None-Match` header against computed ETag.
///
/// Returns `Some(true)` if the ETag matches (304 should be returned),
/// `Some(false)` if it doesn't match, `None` if no `If-None-Match` header.
fn check_if_none_match(headers: &HeaderMap, etag: &str) -> Option<bool> {
    let inm = headers.get("if-none-match")?.to_str().ok()?;
    // Handle wildcard
    if inm.trim() == "*" {
        return Some(true);
    }
    // Compare ETags (may be comma-separated)
    Some(inm.split(',').any(|tag| tag.trim() == etag))
}

// ---------------------------------------------------------------------------
// Data extraction helpers
// ---------------------------------------------------------------------------

/// Extract single resource data from executor result envelope.
///
/// The executor returns `{ "data": { "queryName": { ... } } }`.
/// Extracts the inner value (first field of the data object).
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
fn extract_single_data(result: &str) -> Result<serde_json::Value, RestError> {
    let parsed: serde_json::Value = serde_json::from_str(result)
        .map_err(|e| RestError::internal(format!("Failed to parse query result: {e}")))?;

    if let Some(data_obj) = parsed.get("data") {
        if let serde_json::Value::Object(map) = data_obj {
            Ok(map.values().next().cloned().unwrap_or(serde_json::Value::Null))
        } else {
            Ok(data_obj.clone())
        }
    } else {
        Ok(parsed)
    }
}

/// Extract collection data from executor result envelope.
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
fn extract_collection_data(result: &str) -> Result<serde_json::Value, RestError> {
    extract_single_data(result)
}

/// Extract mutation data from executor result envelope.
///
/// Mutation results have `{ "data": { "mutationName": { ... } } }` structure.
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
fn extract_mutation_data(result: &str) -> Result<serde_json::Value, RestError> {
    let parsed: serde_json::Value = serde_json::from_str(result)
        .map_err(|e| RestError::internal(format!("Failed to parse mutation result: {e}")))?;

    if let Some(data_obj) = parsed.get("data") {
        if let serde_json::Value::Object(map) = data_obj {
            // For mutations, extract the entity from mutation_response
            if let Some(mutation_result) = map.values().next() {
                // Try to extract entity from mutation_response structure
                if let Some(entity) = mutation_result.get("entity") {
                    if !entity.is_null() {
                        return Ok(entity.clone());
                    }
                }
                return Ok(mutation_result.clone());
            }
        }
        Ok(data_obj.clone())
    } else {
        Ok(parsed)
    }
}

/// Extract entity data from a DELETE mutation response.
///
/// Parses `data.{mutation_name}.entity` from the mutation result.
fn extract_delete_entity(result: &str, mutation_name: &str) -> Option<serde_json::Value> {
    let parsed: serde_json::Value = serde_json::from_str(result).ok()?;
    let entity = parsed.get("data")?.get(mutation_name)?.get("entity")?;

    if entity.is_null() {
        None
    } else {
        Some(entity.clone())
    }
}

/// Extract `pageInfo` from a Relay connection response.
fn extract_relay_page_info(data: &serde_json::Value) -> Option<&serde_json::Value> {
    data.get("pageInfo")
}

/// Try to extract an `id` field from mutation response data.
fn extract_id_from_data(data: &serde_json::Value) -> Option<&serde_json::Value> {
    data.get("id")
}

/// Format an ID value for use in a URL path segment.
fn format_id_for_url(id: &serde_json::Value) -> String {
    match id {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Link builders
// ---------------------------------------------------------------------------

/// Build pagination links for offset-based pagination.
fn build_offset_links(
    base: &str,
    limit: u64,
    offset: u64,
    total: Option<u64>,
) -> serde_json::Value {
    let mut links = serde_json::Map::new();

    // self
    links.insert(
        "self".to_string(),
        json!(format!("{base}?limit={limit}&offset={offset}")),
    );

    // first
    links.insert(
        "first".to_string(),
        json!(format!("{base}?limit={limit}&offset=0")),
    );

    // next (if there could be more items)
    let next_offset = offset + limit;
    let has_next = total.is_none_or(|t| next_offset < t);
    if has_next {
        links.insert(
            "next".to_string(),
            json!(format!("{base}?limit={limit}&offset={next_offset}")),
        );
    } else {
        links.insert("next".to_string(), serde_json::Value::Null);
    }

    // prev
    if offset > 0 {
        let prev_offset = offset.saturating_sub(limit);
        links.insert(
            "prev".to_string(),
            json!(format!("{base}?limit={limit}&offset={prev_offset}")),
        );
    } else {
        links.insert("prev".to_string(), serde_json::Value::Null);
    }

    // last (only if total is known)
    if let Some(total) = total {
        if total > 0 {
            let last_offset = ((total - 1) / limit) * limit;
            links.insert(
                "last".to_string(),
                json!(format!("{base}?limit={limit}&offset={last_offset}")),
            );
        } else {
            links.insert(
                "last".to_string(),
                json!(format!("{base}?limit={limit}&offset=0")),
            );
        }
    }
    // Omit `last` entirely when total is unknown (not null — absent)

    serde_json::Value::Object(links)
}

/// Build pagination links for cursor-based (Relay) pagination.
fn build_cursor_links(
    base: &str,
    first: Option<u64>,
    after: Option<&str>,
    data: &serde_json::Value,
) -> serde_json::Value {
    let mut links = serde_json::Map::new();

    // self
    let mut self_url = base.to_string();
    if let Some(f) = first {
        self_url = format!("{self_url}?first={f}");
        if let Some(a) = after {
            self_url = format!("{self_url}&after={a}");
        }
    }
    links.insert("self".to_string(), json!(self_url));

    // next: use last edge's cursor if hasNextPage
    let has_next = data
        .get("pageInfo")
        .and_then(|pi| pi.get("hasNextPage"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if has_next {
        if let Some(end_cursor) = extract_end_cursor(data) {
            let mut next_url = base.to_string();
            if let Some(f) = first {
                next_url = format!("{next_url}?first={f}&after={end_cursor}");
            } else {
                next_url = format!("{next_url}?after={end_cursor}");
            }
            links.insert("next".to_string(), json!(next_url));
        }
    }

    serde_json::Value::Object(links)
}

/// Extract the end cursor from a Relay connection response.
fn extract_end_cursor(data: &serde_json::Value) -> Option<&str> {
    data.get("pageInfo")?
        .get("endCursor")?
        .as_str()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Set `X-Request-Id` header: echo from request or generate a new UUID.
pub(crate) fn set_request_id(request_headers: &HeaderMap, response_headers: &mut HeaderMap) {
    let request_id = request_headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response_headers.insert("x-request-id", val);
    }
}

/// Create a `HeaderValue` from a string, panicking on invalid characters.
///
/// Only used for values known to be valid ASCII (ETag hex strings).
fn header_value(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap_or_else(|_| HeaderValue::from_static(""))
}

// ---------------------------------------------------------------------------
// Method Not Allowed helper
// ---------------------------------------------------------------------------

impl RestError {
    /// 405 Method Not Allowed.
    #[must_use]
    pub fn method_not_allowed() -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "METHOD_NOT_ALLOWED",
            message: "Method not allowed".to_string(),
            details: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
#[allow(clippy::missing_panics_doc)] // Reason: test code
mod tests {
    use super::*;

    fn default_config() -> RestConfig {
        RestConfig::default()
    }

    fn no_etag_config() -> RestConfig {
        RestConfig {
            etag: false,
            ..RestConfig::default()
        }
    }

    fn entity_delete_config() -> RestConfig {
        RestConfig {
            delete_response: DeleteResponse::Entity,
            ..RestConfig::default()
        }
    }

    fn empty_headers() -> HeaderMap {
        HeaderMap::new()
    }

    fn headers_with_request_id(id: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-request-id", HeaderValue::from_str(id).unwrap());
        h
    }

    fn headers_with_if_none_match(etag: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("if-none-match", HeaderValue::from_str(etag).unwrap());
        h
    }

    // -----------------------------------------------------------------------
    // ETag computation
    // -----------------------------------------------------------------------

    #[test]
    fn etag_format_is_weak_validator_hex() {
        let etag = compute_etag(b"hello world");
        assert!(etag.starts_with("W/\""));
        assert!(etag.ends_with('"'));
        // 16 hex chars between quotes
        let inner = &etag[3..etag.len() - 1];
        assert_eq!(inner.len(), 16);
        assert!(inner.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn etag_deterministic() {
        let a = compute_etag(b"same content");
        let b = compute_etag(b"same content");
        assert_eq!(a, b);
    }

    #[test]
    fn etag_changes_with_content() {
        let a = compute_etag(b"content A");
        let b = compute_etag(b"content B");
        assert_ne!(a, b);
    }

    // -----------------------------------------------------------------------
    // If-None-Match
    // -----------------------------------------------------------------------

    #[test]
    fn if_none_match_absent() {
        assert!(check_if_none_match(&empty_headers(), "W/\"abc\"").is_none());
    }

    #[test]
    fn if_none_match_matches() {
        let etag = "W/\"abc123\"";
        let headers = headers_with_if_none_match(etag);
        assert_eq!(check_if_none_match(&headers, etag), Some(true));
    }

    #[test]
    fn if_none_match_stale() {
        let headers = headers_with_if_none_match("W/\"old\"");
        assert_eq!(check_if_none_match(&headers, "W/\"new\""), Some(false));
    }

    #[test]
    fn if_none_match_wildcard() {
        let headers = headers_with_if_none_match("*");
        assert_eq!(check_if_none_match(&headers, "W/\"any\""), Some(true));
    }

    #[test]
    fn if_none_match_comma_separated() {
        let headers = headers_with_if_none_match("W/\"a\", W/\"b\", W/\"c\"");
        assert_eq!(check_if_none_match(&headers, "W/\"b\""), Some(true));
        assert_eq!(check_if_none_match(&headers, "W/\"d\""), Some(false));
    }

    // -----------------------------------------------------------------------
    // format_single
    // -----------------------------------------------------------------------

    #[test]
    fn single_resource_200_with_data() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#;
        let resp = formatter.format_single(result, &empty_headers()).unwrap();

        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.body.unwrap();
        assert_eq!(body["data"]["id"], 1);
        assert_eq!(body["data"]["name"], "Alice");
        assert!(body.get("meta").is_none());
    }

    #[test]
    fn single_resource_has_etag() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;
        let resp = formatter.format_single(result, &empty_headers()).unwrap();

        assert!(resp.headers.get("etag").is_some());
        let etag = resp.headers.get("etag").unwrap().to_str().unwrap();
        assert!(etag.starts_with("W/\""));
    }

    #[test]
    fn single_resource_no_etag_when_disabled() {
        let config = no_etag_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;
        let resp = formatter.format_single(result, &empty_headers()).unwrap();

        assert!(resp.headers.get("etag").is_none());
    }

    #[test]
    fn single_resource_304_on_matching_etag() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;

        // First request to get the ETag
        let resp1 = formatter.format_single(result, &empty_headers()).unwrap();
        let etag = resp1.headers.get("etag").unwrap().to_str().unwrap().to_string();

        // Second request with If-None-Match
        let headers = headers_with_if_none_match(&etag);
        let resp2 = formatter.format_single(result, &headers).unwrap();

        assert_eq!(resp2.status, StatusCode::NOT_MODIFIED);
        assert!(resp2.body.is_none());
        assert!(resp2.headers.get("etag").is_some());
    }

    #[test]
    fn single_resource_200_on_stale_etag() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;
        let headers = headers_with_if_none_match("W/\"stale\"");
        let resp = formatter.format_single(result, &headers).unwrap();

        assert_eq!(resp.status, StatusCode::OK);
        assert!(resp.body.is_some());
    }

    #[test]
    fn single_resource_has_request_id() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;
        let headers = headers_with_request_id("abc-123");
        let resp = formatter.format_single(result, &headers).unwrap();

        assert_eq!(
            resp.headers.get("x-request-id").unwrap().to_str().unwrap(),
            "abc-123"
        );
    }

    #[test]
    fn single_resource_generates_request_id_when_missing() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"user":{"id":1}}}"#;
        let resp = formatter.format_single(result, &empty_headers()).unwrap();

        let id = resp.headers.get("x-request-id").unwrap().to_str().unwrap();
        assert_eq!(id.len(), 36); // UUID format
    }

    // -----------------------------------------------------------------------
    // format_collection — offset pagination
    // -----------------------------------------------------------------------

    #[test]
    fn collection_offset_with_total() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[{"id":1},{"id":2}]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader {
            count_exact: true,
            ..Default::default()
        };

        let resp = formatter
            .format_collection(result, Some(42), &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.body.unwrap();
        assert!(body["data"].is_array());
        assert_eq!(body["meta"]["total"], 42);
        assert_eq!(body["meta"]["limit"], 10);
        assert_eq!(body["meta"]["offset"], 0);
        // Preference-Applied
        assert_eq!(
            resp.headers.get("preference-applied").unwrap().to_str().unwrap(),
            "count=exact"
        );
    }

    #[test]
    fn collection_offset_without_total_omits_meta_total() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[{"id":1}]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        assert!(body["meta"].get("total").is_none());
        // No Preference-Applied header
        assert!(resp.headers.get("preference-applied").is_none());
    }

    #[test]
    fn collection_offset_links_with_total() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[{"id":1}]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 10,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, Some(42), &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        let links = &body["links"];
        assert_eq!(links["self"], "/rest/v1/users?limit=10&offset=10");
        assert_eq!(links["first"], "/rest/v1/users?limit=10&offset=0");
        assert_eq!(links["next"], "/rest/v1/users?limit=10&offset=20");
        assert_eq!(links["prev"], "/rest/v1/users?limit=10&offset=0");
        assert_eq!(links["last"], "/rest/v1/users?limit=10&offset=40");
    }

    #[test]
    fn collection_offset_links_first_page() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, Some(42), &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        let links = &body["links"];
        assert!(links["prev"].is_null());
        assert_eq!(links["next"], "/rest/v1/users?limit=10&offset=10");
    }

    #[test]
    fn collection_offset_links_last_page() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 40,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, Some(42), &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        let links = &body["links"];
        assert!(links["next"].is_null()); // No more items
    }

    #[test]
    fn collection_offset_links_last_omitted_without_total() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        let links = &body["links"];
        assert!(links.get("last").is_none());
    }

    // -----------------------------------------------------------------------
    // format_collection — cursor pagination (Relay)
    // -----------------------------------------------------------------------

    #[test]
    fn collection_cursor_has_next_page_meta() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"posts":{"edges":[{"cursor":"c1","node":{"id":1}}],"pageInfo":{"hasNextPage":true,"hasPreviousPage":false,"endCursor":"c1"}}}}"#;
        let pagination = PaginationParams::Cursor {
            first: Some(5),
            after: None,
            last: None,
            before: None,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/posts", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        assert_eq!(body["meta"]["hasNextPage"], true);
        assert_eq!(body["meta"]["hasPreviousPage"], false);
        assert_eq!(body["meta"]["first"], 5);
    }

    #[test]
    fn collection_cursor_links_with_next() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"posts":{"edges":[{"cursor":"c1","node":{"id":1}}],"pageInfo":{"hasNextPage":true,"hasPreviousPage":false,"endCursor":"c1"}}}}"#;
        let pagination = PaginationParams::Cursor {
            first: Some(10),
            after: None,
            last: None,
            before: None,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/posts", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        let links = &body["links"];
        assert_eq!(links["self"], "/rest/v1/posts?first=10");
        assert_eq!(links["next"], "/rest/v1/posts?first=10&after=c1");
    }

    #[test]
    fn collection_cursor_no_next_link_when_no_next_page() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"posts":{"edges":[],"pageInfo":{"hasNextPage":false,"hasPreviousPage":false}}}}"#;
        let pagination = PaginationParams::Cursor {
            first: Some(10),
            after: None,
            last: None,
            before: None,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/posts", &empty_headers(), &prefer)
            .unwrap();

        let body = resp.body.unwrap();
        assert!(body["links"].get("next").is_none());
    }

    // -----------------------------------------------------------------------
    // format_collection — ETag / 304
    // -----------------------------------------------------------------------

    #[test]
    fn collection_has_etag() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[{"id":1}]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader::default();

        let resp = formatter
            .format_collection(result, None, &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();

        assert!(resp.headers.get("etag").is_some());
    }

    #[test]
    fn collection_304_on_matching_etag() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"users":[{"id":1}]}}"#;
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 0,
        };
        let prefer = PreferHeader::default();

        // First request to get the ETag
        let resp1 = formatter
            .format_collection(result, None, &pagination, "/users", &empty_headers(), &prefer)
            .unwrap();
        let etag = resp1.headers.get("etag").unwrap().to_str().unwrap().to_string();

        // Second request with If-None-Match
        let headers = headers_with_if_none_match(&etag);
        let resp2 = formatter
            .format_collection(result, None, &pagination, "/users", &headers, &prefer)
            .unwrap();

        assert_eq!(resp2.status, StatusCode::NOT_MODIFIED);
        assert!(resp2.body.is_none());
    }

    // -----------------------------------------------------------------------
    // format_created
    // -----------------------------------------------------------------------

    #[test]
    fn created_201_with_location() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"createUser":{"entity":{"id":3,"name":"Charlie"}}}}"#;

        let resp = formatter
            .format_created(result, "/users", None, &empty_headers())
            .unwrap();

        assert_eq!(resp.status, StatusCode::CREATED);
        let body = resp.body.unwrap();
        assert_eq!(body["data"]["id"], 3);
        assert_eq!(
            resp.headers.get("location").unwrap().to_str().unwrap(),
            "/rest/v1/users/3"
        );
    }

    #[test]
    fn created_201_with_explicit_id() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"createUser":{"entity":{"id":5}}}}"#;
        let id = json!(5);

        let resp = formatter
            .format_created(result, "/users", Some(&id), &empty_headers())
            .unwrap();

        assert_eq!(
            resp.headers.get("location").unwrap().to_str().unwrap(),
            "/rest/v1/users/5"
        );
    }

    #[test]
    fn created_201_with_uuid_id() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = format!(
            r#"{{"data":{{"createUser":{{"entity":{{"id":"{uuid}","name":"Alice"}}}}}}}}"#
        );

        let resp = formatter
            .format_created(&result, "/users", None, &empty_headers())
            .unwrap();

        let location = resp.headers.get("location").unwrap().to_str().unwrap();
        assert_eq!(location, format!("/rest/v1/users/{uuid}"));
    }

    #[test]
    fn created_201_has_request_id() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"createUser":{"entity":{"id":1}}}}"#;
        let headers = headers_with_request_id("req-42");

        let resp = formatter
            .format_created(result, "/users", None, &headers)
            .unwrap();

        assert_eq!(
            resp.headers.get("x-request-id").unwrap().to_str().unwrap(),
            "req-42"
        );
    }

    // -----------------------------------------------------------------------
    // format_mutation (PUT/PATCH/custom action)
    // -----------------------------------------------------------------------

    #[test]
    fn mutation_200_with_data() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"updateUser":{"entity":{"id":1,"name":"Updated"}}}}"#;

        let resp = formatter.format_mutation(result, &empty_headers()).unwrap();

        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.body.unwrap();
        assert_eq!(body["data"]["id"], 1);
    }

    // -----------------------------------------------------------------------
    // format_deleted — no_content config
    // -----------------------------------------------------------------------

    #[test]
    fn deleted_204_no_content_default() {
        let config = default_config(); // delete_response = NoContent
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"deleteUser":{"success":true,"entity":null}}}"#;
        let prefer = PreferHeader::default();

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &empty_headers());

        assert_eq!(resp.status, StatusCode::NO_CONTENT);
        assert!(resp.body.is_none());
    }

    #[test]
    fn deleted_200_entity_config() {
        let config = entity_delete_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result =
            r#"{"data":{"deleteUser":{"success":true,"entity":{"id":1,"name":"Alice"}}}}"#;
        let prefer = PreferHeader::default();

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &empty_headers());

        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.body.unwrap();
        assert_eq!(body["data"]["id"], 1);
    }

    #[test]
    fn deleted_prefer_return_representation_with_entity() {
        let config = default_config(); // NoContent default, but Prefer overrides
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result =
            r#"{"data":{"deleteUser":{"success":true,"entity":{"id":1,"name":"Alice"}}}}"#;
        let prefer = PreferHeader {
            return_representation: true,
            ..Default::default()
        };

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &empty_headers());

        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.body.unwrap();
        assert_eq!(body["data"]["id"], 1);
        assert_eq!(
            resp.headers
                .get("preference-applied")
                .unwrap()
                .to_str()
                .unwrap(),
            "return=representation"
        );
    }

    #[test]
    fn deleted_prefer_return_representation_entity_unavailable() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"deleteUser":{"success":true,"entity":null}}}"#;
        let prefer = PreferHeader {
            return_representation: true,
            ..Default::default()
        };

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &empty_headers());

        assert_eq!(resp.status, StatusCode::NO_CONTENT);
        assert!(resp.body.is_none());
        assert_eq!(
            resp.headers
                .get("preference-applied")
                .unwrap()
                .to_str()
                .unwrap(),
            "return=minimal"
        );
        assert_eq!(
            resp.headers
                .get("x-preference-fallback")
                .unwrap()
                .to_str()
                .unwrap(),
            "entity-unavailable"
        );
    }

    #[test]
    fn deleted_prefer_return_minimal_overrides_entity_config() {
        let config = entity_delete_config(); // Entity default, but Prefer overrides
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result =
            r#"{"data":{"deleteUser":{"success":true,"entity":{"id":1,"name":"Alice"}}}}"#;
        let prefer = PreferHeader {
            return_minimal: true,
            ..Default::default()
        };

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &empty_headers());

        assert_eq!(resp.status, StatusCode::NO_CONTENT);
        assert!(resp.body.is_none());
        assert_eq!(
            resp.headers
                .get("preference-applied")
                .unwrap()
                .to_str()
                .unwrap(),
            "return=minimal"
        );
    }

    #[test]
    fn deleted_has_request_id() {
        let config = default_config();
        let formatter = RestResponseFormatter::new(&config, "/rest/v1");
        let result = r#"{"data":{"deleteUser":{"success":true}}}"#;
        let prefer = PreferHeader::default();
        let headers = headers_with_request_id("del-99");

        let resp = formatter.format_deleted(result, "deleteUser", &prefer, &headers);

        assert_eq!(
            resp.headers.get("x-request-id").unwrap().to_str().unwrap(),
            "del-99"
        );
    }

    // -----------------------------------------------------------------------
    // format_error
    // -----------------------------------------------------------------------

    #[test]
    fn error_response_has_structured_body() {
        let err = RestError::not_found("User 42 not found");
        let resp = RestResponseFormatter::format_error(&err, &empty_headers(), None);

        assert_eq!(resp.status, StatusCode::NOT_FOUND);
        let body = resp.body.unwrap();
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert_eq!(body["error"]["message"], "User 42 not found");
    }

    #[test]
    fn error_response_has_request_id() {
        let err = RestError::bad_request("Invalid");
        let headers = headers_with_request_id("err-1");
        let resp = RestResponseFormatter::format_error(&err, &headers, None);

        assert_eq!(
            resp.headers.get("x-request-id").unwrap().to_str().unwrap(),
            "err-1"
        );
    }

    #[test]
    fn error_405_has_allow_header() {
        let err = RestError::method_not_allowed();
        let allowed = [HttpMethod::Get, HttpMethod::Post, HttpMethod::Delete];
        let resp = RestResponseFormatter::format_error(&err, &empty_headers(), Some(&allowed));

        assert_eq!(resp.status, StatusCode::METHOD_NOT_ALLOWED);
        let allow = resp.headers.get("allow").unwrap().to_str().unwrap();
        assert!(allow.contains("GET"));
        assert!(allow.contains("POST"));
        assert!(allow.contains("DELETE"));
    }

    #[test]
    fn error_405_no_allow_when_methods_not_provided() {
        let err = RestError::method_not_allowed();
        let resp = RestResponseFormatter::format_error(&err, &empty_headers(), None);

        assert!(resp.headers.get("allow").is_none());
    }

    #[test]
    fn error_non_405_no_allow_header() {
        let err = RestError::not_found("Not found");
        let allowed = [HttpMethod::Get];
        let resp = RestResponseFormatter::format_error(&err, &empty_headers(), Some(&allowed));

        assert!(resp.headers.get("allow").is_none());
    }

    #[test]
    fn error_with_details() {
        let err = RestError::unprocessable_entity(
            "Validation failed",
            json!({
                "fields": [
                    { "field": "email", "reason": "required for PUT" }
                ]
            }),
        );
        let resp = RestResponseFormatter::format_error(&err, &empty_headers(), None);

        let body = resp.body.unwrap();
        assert_eq!(body["error"]["details"]["fields"][0]["field"], "email");
    }

    // -----------------------------------------------------------------------
    // Link builder unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn offset_links_middle_page() {
        let links = build_offset_links("/rest/v1/users", 10, 20, Some(100));
        assert_eq!(links["self"], "/rest/v1/users?limit=10&offset=20");
        assert_eq!(links["first"], "/rest/v1/users?limit=10&offset=0");
        assert_eq!(links["next"], "/rest/v1/users?limit=10&offset=30");
        assert_eq!(links["prev"], "/rest/v1/users?limit=10&offset=10");
        assert_eq!(links["last"], "/rest/v1/users?limit=10&offset=90");
    }

    #[test]
    fn offset_links_first_page_null_prev() {
        let links = build_offset_links("/rest/v1/users", 10, 0, Some(50));
        assert!(links["prev"].is_null());
        assert_eq!(links["next"], "/rest/v1/users?limit=10&offset=10");
    }

    #[test]
    fn offset_links_last_page_null_next() {
        let links = build_offset_links("/rest/v1/users", 10, 40, Some(42));
        assert!(links["next"].is_null());
    }

    #[test]
    fn offset_links_no_total_omits_last() {
        let links = build_offset_links("/rest/v1/users", 10, 0, None);
        assert!(links.get("last").is_none());
        // next is still present (unknown total — assume more)
        assert_eq!(links["next"], "/rest/v1/users?limit=10&offset=10");
    }

    #[test]
    fn offset_links_empty_collection() {
        let links = build_offset_links("/rest/v1/users", 10, 0, Some(0));
        assert!(links["next"].is_null()); // 0 + 10 >= 0
        assert_eq!(links["last"], "/rest/v1/users?limit=10&offset=0");
    }

    #[test]
    fn cursor_links_with_end_cursor() {
        let data = json!({
            "edges": [{"cursor": "c1", "node": {"id": 1}}],
            "pageInfo": {"hasNextPage": true, "hasPreviousPage": false, "endCursor": "c1"}
        });
        let links = build_cursor_links("/rest/v1/posts", Some(10), None, &data);
        assert_eq!(links["self"], "/rest/v1/posts?first=10");
        assert_eq!(links["next"], "/rest/v1/posts?first=10&after=c1");
    }

    #[test]
    fn cursor_links_no_next_when_last_page() {
        let data = json!({
            "edges": [],
            "pageInfo": {"hasNextPage": false, "hasPreviousPage": true}
        });
        let links = build_cursor_links("/rest/v1/posts", Some(10), Some("prev_cursor"), &data);
        assert!(links.get("next").is_none());
        assert_eq!(links["self"], "/rest/v1/posts?first=10&after=prev_cursor");
    }

    // -----------------------------------------------------------------------
    // Data extraction helpers
    // -----------------------------------------------------------------------

    #[test]
    fn extract_single_data_from_envelope() {
        let result = r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#;
        let data = extract_single_data(result).unwrap();
        assert_eq!(data["id"], 1);
        assert_eq!(data["name"], "Alice");
    }

    #[test]
    fn extract_single_data_unwraps_first_field() {
        let result = r#"{"data":{"someQuery":{"value":42}}}"#;
        let data = extract_single_data(result).unwrap();
        assert_eq!(data["value"], 42);
    }

    #[test]
    fn extract_mutation_data_extracts_entity() {
        let result = r#"{"data":{"createUser":{"entity":{"id":3,"name":"Charlie"}}}}"#;
        let data = extract_mutation_data(result).unwrap();
        assert_eq!(data["id"], 3);
    }

    #[test]
    fn extract_mutation_data_null_entity_returns_full_response() {
        let result = r#"{"data":{"deleteUser":{"success":true,"entity":null}}}"#;
        let data = extract_mutation_data(result).unwrap();
        assert_eq!(data["success"], true);
    }

    #[test]
    fn extract_delete_entity_present() {
        let result =
            r#"{"data":{"deleteUser":{"success":true,"entity":{"id":1,"name":"Alice"}}}}"#;
        let entity = extract_delete_entity(result, "deleteUser").unwrap();
        assert_eq!(entity["id"], 1);
    }

    #[test]
    fn extract_delete_entity_null() {
        let result = r#"{"data":{"deleteUser":{"success":true,"entity":null}}}"#;
        assert!(extract_delete_entity(result, "deleteUser").is_none());
    }

    #[test]
    fn extract_delete_entity_missing() {
        let result = r#"{"data":{"deleteUser":{"success":true}}}"#;
        assert!(extract_delete_entity(result, "deleteUser").is_none());
    }

    #[test]
    fn format_id_integer() {
        assert_eq!(format_id_for_url(&json!(42)), "42");
    }

    #[test]
    fn format_id_string() {
        assert_eq!(
            format_id_for_url(&json!("550e8400-e29b-41d4-a716-446655440000")),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }
}
