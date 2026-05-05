//! HTTP header utilities: request ID echo and preference application.

use axum::http::{HeaderMap, HeaderValue};

/// Set `Preference-Applied` header from a list of applied preferences.
///
/// Joins all non-empty preferences into a single comma-separated header value
/// per RFC 7240 §3.  Does nothing if the list is empty.
pub(crate) fn set_preference_applied(headers: &mut HeaderMap, prefs: &[&str]) {
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
pub(crate) fn set_request_id(request_headers: &HeaderMap, response_headers: &mut HeaderMap) {
    let request_id = request_headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |s| s.to_string());

    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response_headers.insert("x-request-id", val);
    }
}
