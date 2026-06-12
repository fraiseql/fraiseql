//! Helper functions for REST router integration.
//!
//! Contains utility functions for URL path manipulation, query parsing,
//! and response formatting.

use axum::response::Response;
use serde_json::json;

use super::{RestError, RestResponse, StatusCode};
use crate::config::error_sanitization::ErrorSanitizer;

/// Join a base path and route path into an Axum-compatible path pattern.
///
/// Path parameters in `route_path` (axum 0.8 `{id}` syntax) are passed through
/// unchanged.
pub(super) fn to_axum_path(base_path: &str, route_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    format!("{base}{route_path}")
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
            let decoded_key = urlencoding::decode(key).unwrap_or_else(|_| key.into()).into_owned();
            let decoded_value =
                urlencoding::decode(value).unwrap_or_else(|_| value.into()).into_owned();
            pairs.push((decoded_key, decoded_value));
        } else if !part.is_empty() {
            let decoded_key =
                urlencoding::decode(part).unwrap_or_else(|_| part.into()).into_owned();
            pairs.push((decoded_key, String::new()));
        }
    }
    pairs
}

/// Convert a REST handler result into an Axum HTTP response.
///
/// On the error path, server faults (5xx) are sanitized when the operator enabled error
/// sanitization: the raw message — which for `FraiseQLError::Database` carries schema
/// names, constraint details, and SQL fragments — is replaced with the generic message
/// and logged server-side instead (H7). Client-facing 4xx messages (validation, auth,
/// not-found, SQLSTATE 22/23 client-input faults) are intentional and pass through.
pub(super) fn rest_result_to_response(
    result: Result<RestResponse, RestError>,
    sanitizer: &ErrorSanitizer,
) -> Response {
    match result {
        Ok(rest_resp) => {
            // A `None` body means "no content" (e.g. a 204 DELETE) — emit an empty body, not
            // `{}`. Writing `{}` gave 204 No Content a 2-byte body, violating the HTTP spec.
            let body = match rest_resp.body {
                Some(value) => axum::body::Body::from(value.to_string()),
                None => axum::body::Body::empty(),
            };

            let mut response = Response::builder()
                .status(rest_resp.status)
                .body(body)
                .expect("Unable to construct response");

            let headers = response.headers_mut();
            for (key, value) in rest_resp.headers {
                if let Some(key_name) = key {
                    headers.insert(key_name, value);
                }
            }

            response
        },
        Err(mut e) => {
            if e.status.is_server_error() && sanitizer.should_sanitize_internal() {
                tracing::error!(
                    status = %e.status,
                    code = e.code,
                    "REST internal error (message sanitized before client response): {}",
                    e.message
                );
                e.message = sanitizer.internal_error_message();
                // Drop any structured detail too, so internal specifics never leak.
                e.details = None;
            }
            // Render via `RestError::to_json` so structured `details` (e.g. a 422's
            // `missing_fields`) reach the client — `error_response` drops them.
            Response::builder()
                .status(e.status)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(e.to_json().to_string()))
                .expect("Unable to construct error response")
        },
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
mod sanitization_tests {
    //! H7: REST internal (5xx) error bodies must not leak raw DB/SQL text when error
    //! sanitization is enabled — previously the REST path wrote `err.to_string()`
    //! (schema names, constraint detail, SQL fragments) verbatim into the body.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use fraiseql_error::FraiseQLError;

    use super::{RestError, RestResponse, rest_result_to_response};
    use crate::config::error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer};

    fn enabled_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            ..ErrorSanitizationConfig::default()
        })
    }

    /// A realistic raw Postgres error for a non-22/23 SQLSTATE (undefined function),
    /// which the `From<FraiseQLError>` mapper routes to a 500.
    fn raw_db_error() -> FraiseQLError {
        FraiseQLError::Database {
            message:   "function app.fn_secret(integer) does not exist in SELECT app.fn_secret($1)"
                .into(),
            sql_state: Some("42883".into()),
        }
    }

    async fn body_message(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["error"]["message"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn internal_db_error_is_sanitized_when_enabled() {
        let err = RestError::from(raw_db_error());
        assert_eq!(err.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);

        let response = rest_result_to_response(Err(err), &enabled_sanitizer());
        let message = body_message(response).await;

        assert_eq!(message, "An internal error occurred");
        for leak in ["fn_secret", "does not exist", "SELECT", "app."] {
            assert!(!message.contains(leak), "client body must not leak `{leak}`: {message}");
        }
    }

    #[tokio::test]
    async fn internal_db_error_passes_through_when_sanitization_disabled() {
        // Without sanitization, behaviour is unchanged (matches the GraphQL surface):
        // the raw message is still rendered — this is the leak the gate now closes.
        let err = RestError::from(raw_db_error());
        let response = rest_result_to_response(Err(err), &ErrorSanitizer::disabled());
        let message = body_message(response).await;
        assert!(
            message.contains("fn_secret") && message.contains("does not exist"),
            "disabled sanitizer must render the raw message: {message}"
        );
    }

    #[tokio::test]
    async fn client_input_4xx_message_is_preserved_even_when_enabled() {
        // SQLSTATE 22 (client-input data exception) maps to a 400 whose message tells the
        // caller what they did wrong — it must NOT be clobbered by internal sanitization.
        let err = RestError::from(FraiseQLError::Database {
            message:   "invalid input syntax for type uuid: \"not-a-uuid\"".into(),
            sql_state: Some("22P02".into()),
        });
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);

        let response = rest_result_to_response(Err(err), &enabled_sanitizer());
        let message = body_message(response).await;
        assert!(message.contains("not-a-uuid"), "client-input 4xx message preserved: {message}");
    }

    #[tokio::test]
    async fn ok_responses_are_unaffected() {
        let response = rest_result_to_response(
            Ok(RestResponse {
                status:  axum::http::StatusCode::OK,
                headers: axum::http::HeaderMap::new(),
                body:    Some(serde_json::json!({"data": {"id": 1}})),
            }),
            &enabled_sanitizer(),
        );
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
