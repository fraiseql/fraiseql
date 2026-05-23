//! REST response formatting and HTTP semantics.
//!
//! [`RestResponseFormatter`] transforms raw execution results into proper HTTP
//! responses: collection envelopes with pagination metadata and links, `201`
//! with `Location` for creates, `204` for deletes with `Prefer` negotiation,
//! structured error responses, `ETag` via xxHash64, `If-None-Match` → `304`,
//! `X-Request-Id` on all responses, and `Allow` header on `405`.

pub mod helpers;

#[cfg(test)]
mod tests;

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use fraiseql_core::schema::{DeleteResponse, RestConfig};
use helpers::{
    build_cursor_links, build_offset_links, check_if_none_match, compute_etag,
    extract_collection_data, extract_delete_entity, extract_id_from_data, extract_mutation_data,
    extract_relay_page_info, extract_single_data, format_id_for_url, header_value,
};
use serde_json::json;

use super::{
    handler::{PreferHeader, RestError, RestResponse, set_request_id},
    params::PaginationParams,
    resource::HttpMethod,
};

// ---------------------------------------------------------------------------
// RestResponseFormatter
// ---------------------------------------------------------------------------

/// Formats raw execution results into HTTP responses with correct status codes,
/// headers, and envelope structure.
pub struct RestResponseFormatter<'a> {
    config:    &'a RestConfig,
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
    /// Returns 200 with `{ "data": ... }` envelope, `ETag`, and `X-Request-Id`.
    /// If `If-None-Match` matches the computed `ETag`, returns 304 Not Modified.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_single(
        &self,
        result: &serde_json::Value,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_single_data(result)?;
        let serialized = serde_json::to_vec(&data)
            .map_err(|e| RestError::internal(format!("Failed to serialize response: {e}")))?;

        let etag = if self.config.etag {
            Some(compute_etag(&serialized))
        } else {
            None
        };

        // Check If-None-Match
        if let Some(ref etag_val) = etag {
            if check_if_none_match(request_headers, etag_val).unwrap_or(false) {
                let mut headers = HeaderMap::new();
                headers.insert("etag", header_value(etag_val));
                set_request_id(request_headers, &mut headers);
                return Ok(RestResponse {
                    status: StatusCode::NOT_MODIFIED,
                    headers,
                    body: None,
                });
            }
        }

        let mut headers = HeaderMap::new();

        if let Some(etag_val) = etag {
            headers.insert("etag", header_value(&etag_val));
        }

        set_request_id(request_headers, &mut headers);

        let body = json!({
            "data": data,
        });

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(body),
        })
    }

    /// Format a collection GET response (possibly with pagination).
    ///
    /// Returns 200 with `{ "data": [...], "meta": {...}, "links": {...} }`
    /// envelope, `ETag`, and `X-Request-Id`.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_collection(
        &self,
        result: &serde_json::Value,
        pagination: &PaginationParams,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_collection_data(result)?;

        let serialized = serde_json::to_vec(&data)
            .map_err(|e| RestError::internal(format!("Failed to serialize response: {e}")))?;

        let etag = if self.config.etag {
            Some(compute_etag(&serialized))
        } else {
            None
        };

        // Check If-None-Match
        if let Some(ref etag_val) = etag {
            if check_if_none_match(request_headers, etag_val).unwrap_or(false) {
                let mut headers = HeaderMap::new();
                headers.insert("etag", header_value(etag_val));
                set_request_id(request_headers, &mut headers);
                return Ok(RestResponse {
                    status: StatusCode::NOT_MODIFIED,
                    headers,
                    body: None,
                });
            }
        }

        let mut headers = HeaderMap::new();

        if let Some(etag_val) = etag {
            headers.insert("etag", header_value(&etag_val));
        }

        set_request_id(request_headers, &mut headers);

        let mut body = json!({
            "data": data,
        });

        // Add pagination metadata and links based on type
        match pagination {
            PaginationParams::Offset { limit, offset } => {
                body["meta"] = json!({
                    "limit": limit,
                    "offset": offset,
                });
                let base = self.base_path;
                body["links"] = build_offset_links(base, *limit, *offset, None);
            },
            PaginationParams::Cursor {
                first,
                after,
                last: _,
                before: _,
            } => {
                let mut meta = serde_json::Map::new();

                if let Some(page_info) = extract_relay_page_info(&data) {
                    if let Some(has_next) = page_info.get("hasNextPage") {
                        meta.insert("hasNextPage".to_string(), has_next.clone());
                    }
                    if let Some(has_prev) = page_info.get("hasPreviousPage") {
                        meta.insert("hasPreviousPage".to_string(), has_prev.clone());
                    }
                }

                body["meta"] = serde_json::Value::Object(meta);

                let base = self.base_path;
                body["links"] = build_cursor_links(base, *first, after.as_deref(), &data);
            },
            PaginationParams::None => {
                // No pagination metadata for single results
            },
        }

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(body),
        })
    }

    /// Format a mutation POST response (create or custom action).
    ///
    /// Returns 201 with optional `Location` header and response body.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_mutation_post(
        &self,
        result: &serde_json::Value,
        resource_path: &str,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_mutation_data(result)?;

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Attempt to extract ID for Location header
        if let Some(id_val) = extract_id_from_data(&data) {
            let id_str = format_id_for_url(id_val);
            let location = format!("{resource_path}/{id_str}");
            if let Ok(loc_val) = HeaderValue::from_str(&location) {
                headers.insert("location", loc_val);
            }
        }

        let body = json!({
            "data": data,
        });

        Ok(RestResponse {
            status: StatusCode::CREATED,
            headers,
            body: Some(body),
        })
    }

    /// Format a mutation PUT/PATCH response (full or partial update).
    ///
    /// Returns 200 with response body.
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the execution result cannot be parsed as JSON.
    pub fn format_mutation_update(
        &self,
        result: &serde_json::Value,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let data = extract_mutation_data(result)?;

        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        let body = json!({
            "data": data,
        });

        Ok(RestResponse {
            status: StatusCode::OK,
            headers,
            body: Some(body),
        })
    }

    /// Format a DELETE response (single or bulk).
    ///
    /// For single-resource DELETE, may return 200 with entity (if requested via
    /// `Prefer: return=representation`), or 204 No Content.
    /// For bulk delete, returns 200 with `deleted` count array.
    ///
    /// # Errors
    ///
    /// Returns `RestError` on execution result parsing failure.
    pub fn format_delete(
        &self,
        result: &serde_json::Value,
        prefer: &PreferHeader,
        mutation_name: &str,
        request_headers: &HeaderMap,
    ) -> Result<RestResponse, RestError> {
        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        // Check if response should contain the deleted entity
        let want_entity = if prefer.return_representation {
            true
        } else if prefer.return_minimal {
            false
        } else {
            matches!(self.config.delete_response, DeleteResponse::Entity)
        };

        if want_entity {
            if let Some(entity) = extract_delete_entity(result, mutation_name) {
                if prefer.return_representation {
                    headers.insert(
                        "preference-applied",
                        HeaderValue::from_static("return=representation"),
                    );
                }
                let body = json!({
                    "data": entity,
                });
                return Ok(RestResponse {
                    status: StatusCode::OK,
                    headers,
                    body: Some(body),
                });
            }
        }

        // Return 204 No Content
        Ok(RestResponse {
            status: StatusCode::NO_CONTENT,
            headers,
            body: None,
        })
    }

    /// Format a method not allowed (405) error response.
    ///
    /// Returns 405 with `Allow` header listing valid methods.
    #[must_use]
    pub fn format_method_not_allowed(
        &self,
        allowed_methods: &[HttpMethod],
        request_headers: &HeaderMap,
    ) -> RestResponse {
        let mut headers = HeaderMap::new();
        set_request_id(request_headers, &mut headers);

        let method_strs: Vec<&str> = allowed_methods
            .iter()
            .map(|m| match m {
                HttpMethod::Get => "GET",
                HttpMethod::Post => "POST",
                HttpMethod::Put => "PUT",
                HttpMethod::Patch => "PATCH",
                HttpMethod::Delete => "DELETE",
            })
            .collect();

        if let Ok(allow_header) = HeaderValue::from_str(&method_strs.join(", ")) {
            headers.insert("allow", allow_header);
        }

        RestResponse {
            status: StatusCode::METHOD_NOT_ALLOWED,
            headers,
            body: Some(RestError::method_not_allowed().to_json()),
        }
    }
}

impl RestError {
    /// 405 Method Not Allowed.
    #[must_use]
    pub fn method_not_allowed() -> Self {
        Self {
            status:  StatusCode::METHOD_NOT_ALLOWED,
            code:    "METHOD_NOT_ALLOWED",
            message: "Method not allowed".to_string(),
            details: None,
        }
    }
}
