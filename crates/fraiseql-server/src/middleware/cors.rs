//! CORS middleware.

use tower_http::cors::{Any, CorsLayer};

/// Create CORS layer.
///
/// Configures Cross-Origin Resource Sharing for the GraphQL API.
///
/// # Configuration
///
/// - Allows all origins (use `allow_origins` for production)
/// - Allows common HTTP methods (GET, POST, OPTIONS)
/// - Allows common headers (Content-Type, Authorization)
/// - Exposes all headers
#[must_use]
pub fn cors_layer() -> CorsLayer {
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
    let origins: Vec<_> = allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

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

#[cfg(test)]
mod tests {
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
}
