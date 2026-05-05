//! Helper functions for REST router integration.
//!
//! Contains utility functions for URL path manipulation, query parsing,
//! and response formatting.

use axum::response::Response;
use serde_json::json;

use super::{RestError, RestResponse, StatusCode};

/// Convert a base path and route path to an Axum-compatible path pattern.
///
/// Converts `{id}` path parameters to Axum's `:id` syntax.
pub(super) fn to_axum_path(base_path: &str, route_path: &str) -> String {
    let pattern = route_path.replace('{', ":").replace('}', "");
    format!("{base_path}{pattern}")
}

/// Strip the base path from a request path to get the route-relative path.
pub(super) fn strip_base_path(base_path: &str, request_path: &str) -> String {
    if let Some(suffix) = request_path.strip_prefix(base_path) {
        if suffix.is_empty() {
            "/".to_string()
        } else {
            suffix.to_string()
        }
    } else {
        request_path.to_string()
    }
}

/// Parse a URL query string into key-value pairs.
pub(super) fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for part in query.split('&') {
        if let Some((key, value)) = part.split_once('=') {
            let decoded_key = urlencoding::decode(key)
                .unwrap_or_else(|_| key.into())
                .into_owned();
            let decoded_value = urlencoding::decode(value)
                .unwrap_or_else(|_| value.into())
                .into_owned();
            pairs.push((decoded_key, decoded_value));
        } else if !part.is_empty() {
            let decoded_key = urlencoding::decode(part)
                .unwrap_or_else(|_| part.into())
                .into_owned();
            pairs.push((decoded_key, String::new()));
        }
    }
    pairs
}

/// Convert a REST handler result into an Axum HTTP response.
pub(super) fn rest_result_to_response(result: Result<RestResponse, RestError>) -> Response {
    match result {
        Ok(rest_resp) => {
            let body = rest_resp.body.unwrap_or(json!({}));
            let body_str = body.to_string();

            let mut response = Response::builder()
                .status(rest_resp.status)
                .body(axum::body::Body::from(body_str))
                .expect("Unable to construct response");

            let headers = response.headers_mut();
            for (key, value) in rest_resp.headers {
                if let Some(key_name) = key {
                    headers.insert(key_name, value);
                }
            }

            response
        },
        Err(e) => error_response(e.status, e.code, &e.message),
    }
}

/// Build an error response with the given status, code, and message.
pub(super) fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
        }
    });

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .expect("Unable to construct error response")
}

