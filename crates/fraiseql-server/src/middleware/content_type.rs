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
pub async fn require_json_content_type(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    if req.method() != Method::POST {
        return Ok(next.run(req).await);
    }

    let content_type = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

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
            serde_json::to_string(&body).unwrap_or_default(),
        )
            .into_response());
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use axum::{Router, routing::post};
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header::CONTENT_TYPE};
    use axum::middleware;
    use tower::ServiceExt;

    use super::require_json_content_type;

    async fn echo_handler() -> &'static str {
        "ok"
    }

    fn app() -> Router {
        Router::new()
            .route("/graphql", post(echo_handler))
            .layer(middleware::from_fn(require_json_content_type))
    }

    #[tokio::test]
    async fn text_plain_rejected_with_415() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "text/plain")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn form_urlencoded_rejected_with_415() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("query=%7B+__typename+%7D"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn application_json_passes() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn application_json_with_charset_passes() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/json; charset=utf-8")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_request_passes_without_content_type() {
        let app = Router::new()
            .route("/graphql", axum::routing::get(echo_handler))
            .layer(middleware::from_fn(require_json_content_type));

        let res = app
            .oneshot(
                Request::get("/graphql")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_content_type_rejected() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
