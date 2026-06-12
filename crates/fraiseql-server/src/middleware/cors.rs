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
/// Only the origins listed in `allowed_origins` will be accepted.
/// An empty list blocks all cross-origin requests.
///
/// # Arguments
///
/// * `allowed_origins` - Exact origin URLs that are permitted, e.g.
///   `["https://app.example.com", "https://admin.example.com"]`.
///
/// # Example
///
/// ```text
/// // Typically called from ServerConfig::cors_origins during server startup.
/// let layer = cors_layer_restricted(&["https://app.example.com".to_string()]);
/// ```
pub fn cors_layer_restricted(allowed_origins: &[String]) -> CorsLayer {
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
/// ```text
/// // Requires: running Axum application.
/// use axum::Router;
/// use fraiseql_server::middleware::security_headers_middleware;
/// let app = Router::new()
///     .layer(axum::middleware::from_fn(security_headers_middleware));
/// ```
///
/// # Panics
///
/// Cannot panic in practice — the `expect` calls parse static header value
/// string literals that are always valid.
pub async fn security_headers_middleware(
    req: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    let mut response = next.run(req).await;

    let headers = response.headers_mut();

    // All headers are set with `entry().or_insert()` (set-if-absent) rather than `insert()`
    // (overwrite) so a handler can opt into a different policy for its own response. This
    // matters for the GraphQL playground, which serves HTML that loads CDN scripts/iframes
    // and therefore sets its own relaxed `Content-Security-Policy`; a clobbering global CSP
    // of `script-src 'self'` would break it. Reason: each value is a valid static ASCII
    // header value, so `parse()` is infallible here.

    // Prevent MIME type sniffing.
    headers
        .entry("X-Content-Type-Options")
        .or_insert_with(|| "nosniff".parse().expect("valid header value"));

    // Prevent framing/clickjacking.
    headers
        .entry("X-Frame-Options")
        .or_insert_with(|| "DENY".parse().expect("valid header value"));

    // Enforce HTTPS (1 year with subdomains).
    headers.entry("Strict-Transport-Security").or_insert_with(|| {
        "max-age=31536000; includeSubDomains".parse().expect("valid header value")
    });

    // Control referrer leakage.
    headers
        .entry("Referrer-Policy")
        .or_insert_with(|| "strict-origin-when-cross-origin".parse().expect("valid header value"));

    // Content Security Policy — restrict resource loading. A handler serving HTML that needs
    // CDN/inline resources (e.g. the playground) sets its own CSP, which this preserves.
    headers.entry("Content-Security-Policy").or_insert_with(|| {
        "default-src 'self'; script-src 'self'; style-src 'self'"
            .parse()
            .expect("valid header value")
    });

    // Disable the legacy XSS auditor. The `0` value is the modern best practice: the auditor
    // is absent from current browsers and its "enabled" modes could introduce XSS.
    headers
        .entry("X-XSS-Protection")
        .or_insert_with(|| "0".parse().expect("valid header value"));

    response
}

#[cfg(test)]
mod security_headers_tests {
    //! M-sec-headers: the security-headers middleware must actually set the headers (it was
    //! never layered), and must not clobber a handler that sets its own policy.
    #![allow(clippy::unwrap_used)]

    use axum::{
        Router,
        body::Body,
        http::{Request, header},
        middleware,
        response::IntoResponse,
        routing::get,
    };
    use tower::ServiceExt as _;

    use super::security_headers_middleware;

    #[tokio::test]
    async fn sets_all_security_headers() {
        async fn ok() -> &'static str {
            "ok"
        }
        let app = Router::new()
            .route("/", get(ok))
            .layer(middleware::from_fn(security_headers_middleware));

        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let h = resp.headers();
        assert_eq!(h.get("X-Content-Type-Options").unwrap(), "nosniff");
        assert_eq!(h.get("X-Frame-Options").unwrap(), "DENY");
        assert!(h.contains_key("Strict-Transport-Security"));
        assert!(h.contains_key("Referrer-Policy"));
        assert!(h.contains_key("Content-Security-Policy"));
        assert_eq!(h.get("X-XSS-Protection").unwrap(), "0");
    }

    #[tokio::test]
    async fn preserves_handler_set_csp() {
        async fn csp_handler() -> impl IntoResponse {
            (
                [(header::CONTENT_SECURITY_POLICY, "script-src https://cdn.example.com")],
                "html",
            )
        }
        let app = Router::new()
            .route("/", get(csp_handler))
            .layer(middleware::from_fn(security_headers_middleware));

        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // The handler's CSP is preserved (set-if-absent), while the other security headers
        // are still added — so the playground's relaxed CSP survives the global middleware.
        assert_eq!(
            resp.headers().get("Content-Security-Policy").unwrap(),
            "script-src https://cdn.example.com"
        );
        assert_eq!(resp.headers().get("X-Frame-Options").unwrap(), "DENY");
    }
}
