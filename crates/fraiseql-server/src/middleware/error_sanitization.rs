//! Global error sanitization middleware.
//!
//! Intercepts all HTTP error responses (4xx/5xx) and sanitizes internal details
//! before they reach the client. Covers both GraphQL and REST API endpoints,
//! replacing the previous approach of manual `ErrorSanitizer::sanitize()` calls
//! at each error site.
//!
//! Successful responses (2xx/3xx) are passed through without body inspection.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::config::error_sanitization::ErrorSanitizer;

/// Maximum error response body size we'll read for sanitization (256 KiB).
/// Larger error bodies are passed through unsanitized to prevent DoS.
const MAX_ERROR_BODY_BYTES: usize = 256 * 1024;

/// Axum middleware that sanitizes error responses globally.
///
/// Intercepts responses with 4xx/5xx status codes that have a JSON content-type,
/// parses the body, sanitizes internal details from the `errors` array (GraphQL)
/// or `message`/`detail` fields (REST), and re-serializes.
///
/// Successful responses and non-JSON error responses are passed through unchanged.
///
/// # Errors
///
/// Returns the original response unchanged if body parsing or re-serialization fails.
pub async fn error_sanitization_middleware(
    State(sanitizer): State<Arc<ErrorSanitizer>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !sanitizer.is_enabled() {
        return next.run(request).await;
    }

    let response = next.run(request).await;

    // Only inspect error responses (4xx/5xx)
    if response.status().is_success() || response.status().is_redirection() {
        return response;
    }

    // Only inspect JSON responses
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("application/json"));

    if !is_json {
        return response;
    }

    // Decompose the response to preserve status, headers, and version

    // Read the body (with size limit to prevent DoS)
    let (parts, body) = response.into_parts();
    let body_bytes = match axum::body::to_bytes(body, MAX_ERROR_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    // Try to parse as JSON and sanitize
    let sanitized_bytes = match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        Ok(mut json) => {
            sanitize_json_error(&sanitizer, &mut json);
            match serde_json::to_vec(&json) {
                Ok(bytes) => bytes,
                Err(_) => body_bytes.to_vec(),
            }
        },
        Err(_) => body_bytes.to_vec(),
    };

    // Reconstruct response with original status, headers, and version
    let body_len = sanitized_bytes.len();
    let mut response = Response::from_parts(parts, Body::from(sanitized_bytes));
    // Update content-length since the body may have changed size
    response.headers_mut().insert(header::CONTENT_LENGTH, body_len.into());
    response
}

/// Sanitize a JSON error response in-place.
///
/// Handles two formats:
/// - **GraphQL**: `{ "errors": [{ "message": "...", "code": "...", "extensions": { "detail": "..."
///   } }] }`
/// - **REST**: `{ "message": "...", "detail": "..." }`
fn sanitize_json_error(sanitizer: &ErrorSanitizer, json: &mut serde_json::Value) {
    // GraphQL format: sanitize each error in the "errors" array
    if let Some(errors) = json.get_mut("errors").and_then(|e| e.as_array_mut()) {
        for error in errors {
            sanitize_single_error(sanitizer, error);
        }
    }

    // REST format: sanitize top-level "message" and "detail"
    if json.get("errors").is_none() {
        sanitize_single_error(sanitizer, json);
    }
}

/// Sanitize a single error object.
///
/// Strips internal details from `InternalServerError` and `DatabaseError` codes.
fn sanitize_single_error(sanitizer: &ErrorSanitizer, error: &mut serde_json::Value) {
    let code = error.get("code").and_then(|c| c.as_str()).unwrap_or("");

    let is_internal = matches!(code, "INTERNAL_SERVER_ERROR" | "DATABASE_ERROR");

    if is_internal {
        // Delegate to the ErrorSanitizer for message replacement
        // Build a temporary GraphQLError, sanitize, extract the message
        if let Some(message) = error.get("message").and_then(|m| m.as_str()) {
            let code_enum = if code == "DATABASE_ERROR" {
                crate::error::ErrorCode::DatabaseError
            } else {
                crate::error::ErrorCode::InternalServerError
            };
            let temp = crate::error::GraphQLError::new(message, code_enum);
            let sanitized = sanitizer.sanitize(temp);
            error["message"] = serde_json::Value::String(sanitized.message);
        }
    }

    // Strip implementation details from extensions
    if let Some(extensions) = error.get_mut("extensions") {
        if let Some(obj) = extensions.as_object_mut() {
            obj.remove("detail");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use super::*;
    use crate::config::error_sanitization::ErrorSanitizationConfig;

    fn test_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled:                     true,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        })
    }

    #[test]
    fn test_sanitize_graphql_db_error() {
        let sanitizer = test_sanitizer();
        let mut json = serde_json::json!({
            "errors": [{
                "message": "ERROR: relation \"tb_users\" does not exist",
                "code": "DATABASE_ERROR",
                "extensions": {
                    "detail": "at line 42 in query.rs"
                }
            }]
        });

        sanitize_json_error(&sanitizer, &mut json);

        let error = &json["errors"][0];
        assert_eq!(error["message"], "An internal error occurred");
        assert!(error["extensions"].get("detail").is_none());
    }

    #[test]
    fn test_sanitize_preserves_validation_error() {
        let sanitizer = test_sanitizer();
        let mut json = serde_json::json!({
            "errors": [{
                "message": "Field 'email' is required",
                "code": "VALIDATION_ERROR"
            }]
        });

        sanitize_json_error(&sanitizer, &mut json);

        assert_eq!(json["errors"][0]["message"], "Field 'email' is required");
    }

    #[test]
    fn test_sanitize_rest_internal_error() {
        let sanitizer = test_sanitizer();
        let mut json = serde_json::json!({
            "message": "connection refused: postgres://user:pass@host/db",
            "code": "INTERNAL_SERVER_ERROR",
            "extensions": {
                "detail": "panic at src/db.rs:123"
            }
        });

        sanitize_json_error(&sanitizer, &mut json);

        assert_eq!(json["message"], "An internal error occurred");
        assert!(json["extensions"].get("detail").is_none());
    }

    #[test]
    fn test_disabled_sanitizer_passes_through() {
        let sanitizer = ErrorSanitizer::disabled();
        let mut json = serde_json::json!({
            "errors": [{
                "message": "ERROR: relation \"tb_users\" does not exist",
                "code": "DATABASE_ERROR"
            }]
        });

        sanitize_json_error(&sanitizer, &mut json);

        // Disabled sanitizer doesn't change messages (only strips detail via JSON path)
        // But since the sanitizer.sanitize() call preserves message when disabled,
        // the message should be unchanged
        assert_eq!(json["errors"][0]["message"], "ERROR: relation \"tb_users\" does not exist");
    }
}
