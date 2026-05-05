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

    // Prevent MIME type sniffing
    // Reason: "nosniff" is a valid static ASCII header value; parse() is infallible here.
    headers.insert("X-Content-Type-Options", "nosniff".parse().expect("valid header value"));

    // Prevent framing/clickjacking
    // Reason: "DENY" is a valid static ASCII header value; parse() is infallible here.
    headers.insert("X-Frame-Options", "DENY".parse().expect("valid header value"));

    // Enforce HTTPS (1 year with subdomains)
    // Reason: static ASCII header value; parse() is infallible here.
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().expect("valid header value"),
    );

    // Control referrer leakage
    // Reason: static ASCII header value; parse() is infallible here.
    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin".parse().expect("valid header value"),
    );

    // Content Security Policy - restrict resource loading
    // Note: 'unsafe-inline' is intentionally omitted; callers that need
    // inline styles for a GraphQL playground should set their own CSP.
    // Reason: static ASCII header value; parse() is infallible here.
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self'; style-src 'self'"
            .parse()
            .expect("valid header value"),
    );

    // Disable the legacy XSS auditor.  The `0` value is the modern best
    // practice: the auditor is absent from current browsers and its
    // "enabled" modes could introduce XSS on certain pages.
    // Reason: "0" is a valid static ASCII header value; parse() is infallible here.
    headers.insert("X-XSS-Protection", "0".parse().expect("valid header value"));

    response
}

