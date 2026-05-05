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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
#[allow(clippy::missing_panics_doc)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn set_preference_applied_single() {
        let mut headers = HeaderMap::new();
        set_preference_applied(&mut headers, &["count=exact"]);
        assert_eq!(
            headers.get("preference-applied").unwrap().to_str().unwrap(),
            "count=exact"
        );
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
        assert_eq!(
            response_headers.get("x-request-id").unwrap().to_str().unwrap(),
            "test-id-123"
        );
    }

    #[test]
    fn set_request_id_generate_new() {
        let request_headers = HeaderMap::new();
        let mut response_headers = HeaderMap::new();
        set_request_id(&request_headers, &mut response_headers);
        let id = response_headers.get("x-request-id").unwrap().to_str().unwrap();
        assert!(uuid::Uuid::parse_str(id).is_ok());
    }
}
