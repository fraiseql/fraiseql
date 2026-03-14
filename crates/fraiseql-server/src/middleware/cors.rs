//! CORS and security headers middleware.

use axum::{middleware::Next, response::IntoResponse};
use tower_http::cors::{Any, CorsLayer};

/// Create CORS layer for development only.
///
/// ⚠️  **SECURITY WARNING**: This function allows all origins and methods.
/// **DO NOT USE IN PRODUCTION** - use `cors_layer_restricted()` instead.
///
/// Configures Cross-Origin Resource Sharing for the GraphQL API with permissive settings.
///
/// # Development Only
///
/// This configuration is suitable only for local development.
/// - Allows all origins (no origin validation)
/// - Allows all HTTP methods
/// - Allows all headers
/// - Exposes all headers
///
/// # Production
///
/// For production deployments, use `cors_layer_restricted()` with specific allowed origins.
/// See server configuration `cors_origins` setting.
#[must_use]
pub fn cors_layer() -> CorsLayer {
    tracing::warn!(
        "Using permissive CORS settings (allows all origins). \
         This is suitable for development only. \
         For production, configure cors_origins in server config and use cors_layer_restricted()."
    );
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
}

/// Create restricted CORS layer for production.
///
/// # Arguments
///
/// * `allowed_origins` - List of allowed origin URLs
#[must_use]
pub fn cors_layer_restricted(allowed_origins: Vec<String>) -> CorsLayer {
    let origins: Vec<_> = allowed_origins.iter().filter_map(|origin| origin.parse().ok()).collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
}

/// Security headers middleware.
///
/// Adds security-related HTTP response headers to protect against:
/// - Content-type sniffing attacks
/// - Clickjacking attacks
/// - XSS attacks
/// - Insecure HTTPS connections
/// - Information leakage via referrer headers
///
/// # Headers Added
///
/// * `X-Content-Type-Options: nosniff` - Prevents MIME type sniffing
/// * `X-Frame-Options: DENY` - Prevents embedding in iframes
/// * `Strict-Transport-Security` - Enforces HTTPS
/// * `Referrer-Policy: strict-origin-when-cross-origin` - Control referrer leakage
///
/// # Example
///
/// ```no_run
/// // Requires: running Axum application.
/// # use axum::Router;
/// # use fraiseql_server::middleware::security_headers_middleware;
/// let app = Router::new()
///     .layer(axum::middleware::from_fn(security_headers_middleware));
/// ```
pub async fn security_headers_middleware(
    req: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    let mut response = next.run(req).await;

    let headers = response.headers_mut();

    // Prevent MIME type sniffing
    headers.insert("X-Content-Type-Options", "nosniff".parse().expect("valid header value"));

    // Prevent framing/clickjacking
    headers.insert("X-Frame-Options", "DENY".parse().expect("valid header value"));

    // Enforce HTTPS (1 year with subdomains)
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().expect("valid header value"),
    );

    // Control referrer leakage
    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin".parse().expect("valid header value"),
    );

    // Content Security Policy - restrict resource loading
    // Note: 'unsafe-inline' is intentionally omitted; callers that need
    // inline styles for a GraphQL playground should set their own CSP.
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self'; style-src 'self'"
            .parse()
            .expect("valid header value"),
    );

    // Disable the legacy XSS auditor.  The `0` value is the modern best
    // practice: the auditor is absent from current browsers and its
    // "enabled" modes could introduce XSS on certain pages.
    headers.insert("X-XSS-Protection", "0".parse().expect("valid header value"));

    response
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    #[test]
    fn test_cors_layer_creation() {
        let _layer = cors_layer();
        // Basic test to ensure layer can be created
    }

    #[test]
    fn test_cors_layer_restricted() {
        let origins = vec!["https://example.com".to_string()];
        let _layer = cors_layer_restricted(origins);
    }

    #[test]
    fn test_cors_layer_restricted_empty_origins() {
        let origins = vec![];
        let _layer = cors_layer_restricted(origins);
    }

    #[test]
    fn test_cors_layer_restricted_invalid_origin() {
        let origins = vec![
            "not-a-valid-url".to_string(),
            "https://valid.com".to_string(),
        ];
        let layer = cors_layer_restricted(origins);
        // Layer creation should handle invalid origins gracefully
        let _ = layer;
    }

    // ── Security headers middleware tests (15-4) ────────────────────────────

    use axum::{Router, body::Body, http::Request, middleware, routing::get};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn sec_app() -> Router {
        Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn(security_headers_middleware))
    }

    #[tokio::test]
    async fn test_security_headers_nosniff_present() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.headers().get("x-content-type-options").unwrap(), "nosniff");
    }

    #[tokio::test]
    async fn test_security_headers_frame_options_deny() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn test_security_headers_xss_protection_zero() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("x-xss-protection").unwrap(),
            "0",
            "X-XSS-Protection must be 0 (legacy auditor disabled)"
        );
    }

    #[test]
    fn test_cors_layer_config_comprehensive() {
        let origins = vec![
            "https://example.com".to_string(),
            "https://app.example.com".to_string(),
        ];
        let _ = cors_layer_restricted(origins);
    }
}
