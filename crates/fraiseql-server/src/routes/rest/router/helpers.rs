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
    let pattern = route_path.replace("{", ":").replace("}", "");
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
            for (key, value) in rest_resp.headers.into_iter() {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn to_axum_path_simple() {
        assert_eq!(to_axum_path("/rest/v1", "/users"), "/rest/v1/users");
    }

    #[test]
    fn to_axum_path_with_param() {
        assert_eq!(to_axum_path("/rest/v1", "/users/{id}"), "/rest/v1/users/:id");
    }

    #[test]
    fn to_axum_path_multiple_params() {
        assert_eq!(
            to_axum_path("/rest/v1", "/users/{uid}/posts/{pid}"),
            "/rest/v1/users/:uid/posts/:pid"
        );
    }

    #[test]
    fn strip_base_path_exact() {
        assert_eq!(strip_base_path("/rest/v1", "/rest/v1"), "/");
    }

    #[test]
    fn strip_base_path_with_route() {
        assert_eq!(strip_base_path("/rest/v1", "/rest/v1/users"), "/users");
    }

    #[test]
    fn strip_base_path_no_match() {
        assert_eq!(strip_base_path("/rest/v1", "/api/users"), "/api/users");
    }

    #[test]
    fn parse_query_pairs_single() {
        let pairs = parse_query_pairs("key=value");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("key".to_string(), "value".to_string()));
    }

    #[test]
    fn parse_query_pairs_multiple() {
        let pairs = parse_query_pairs("key1=value1&key2=value2");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("key1".to_string(), "value1".to_string()));
        assert_eq!(pairs[1], ("key2".to_string(), "value2".to_string()));
    }

    #[test]
    fn parse_query_pairs_url_encoded() {
        let pairs = parse_query_pairs("name=John%20Doe");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("name".to_string(), "John Doe".to_string()));
    }

    #[test]
    fn parse_query_pairs_no_value() {
        let pairs = parse_query_pairs("flag");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("flag".to_string(), String::new()));
    }

    #[test]
    fn error_response_structure() {
        let resp = error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", "Invalid input");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(resp.headers().get("content-type").unwrap(), "application/json");
    }
}
