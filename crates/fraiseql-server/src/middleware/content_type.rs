//! CSRF protection via Content-Type enforcement.
//!
//! Rejects POST requests that do not carry `Content-Type: application/json`.
//! This prevents cross-site request forgery via `text/plain` or
//! `application/x-www-form-urlencoded` form submissions.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Middleware that rejects POST requests without a JSON Content-Type.
///
/// Non-POST methods pass through unconditionally.
/// POST requests must have `Content-Type` starting with `application/json`
/// (e.g. `application/json` or `application/json; charset=utf-8`).
///
/// # Errors
///
/// Returns a `415 Unsupported Media Type` response if the POST request does not carry a JSON
/// `Content-Type`.
pub async fn require_json_content_type(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    if req.method() != Method::POST {
        return Ok(next.run(req).await);
    }

    let content_type = req.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");

    if !content_type.starts_with("application/json") {
        let body = serde_json::json!({
            "errors": [{
                "message": "Content-Type must be application/json",
                "extensions": { "code": "UNSUPPORTED_MEDIA_TYPE" }
            }]
        });
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            [(CONTENT_TYPE, "application/json")],
            serde_json::to_string(&body).unwrap_or_else(|_| {
                r#"{"errors":[{"message":"Unsupported Media Type"}]}"#.to_owned()
            }),
        )
            .into_response());
    }

    Ok(next.run(req).await)
}

