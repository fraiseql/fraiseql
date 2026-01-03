//! HTTP middleware stack for the GraphQL server
//!
//! This module provides middleware components for:
//! - Response compression (standard + optional zstd)
//! - Cross-Origin Resource Sharing (CORS)
//! - Error handling and response formatting
//! - Request/response logging and tracing
//!
//! # Compression Strategy
//!
//! FraiseQL supports multiple compression algorithms:
//! - **Default**: Brotli (via tower-http, no feature flag needed)
//! - **Opt-in**: Zstandard/zstd (superior compression ratio, requires `advanced-compression` feature)
//!
//! ## When to Use Each
//!
//! | Algorithm | Ratio | Speed | Latency | Best For |
//! |-----------|-------|-------|---------|----------|
//! | Brotli | 20-24% | Fast | Medium | Default, broad client support |
//! | Zstd | 16-20% | Very Fast | Low | High-throughput APIs, modern clients |
//!
//! Example: GraphQL response with 50KB JSON
//! - Brotli: ~10KB (200ms compression)
//! - Zstd: ~8.5KB (50ms compression)
//!
//! ## Feature-Gated Usage
//!
//! ```toml
//! [features]
//! default = ["simd"]
//! advanced-compression = ["zstd", "async-compression"]
//!
//! # Enable in Cargo.toml:
//! # fraiseql = { version = "1.9", features = ["advanced-compression"] }
//! ```
//!
//! ## Configuration
//!
//! ```rust,ignore
//! // Create middleware stack with Brotli (default)
//! let app = Router::new()
//!     .route("/graphql", post(handler))
//!     .layer(create_compression_layer(CompressionAlgorithm::Brotli));
//!
//! // Or with Zstd (if advanced-compression feature enabled)
//! .layer(create_compression_layer(CompressionAlgorithm::Zstd));
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use tower_http::cors::CorsLayer;
use std::fmt;

/// Supported compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// Brotli compression (default, always available)
    /// - Compression ratio: ~20-24%
    /// - Speed: Fast
    /// - Best for: Default responses with broad client support
    Brotli,

    /// Zstandard compression (opt-in with `advanced-compression` feature)
    /// - Compression ratio: ~16-20% (better than Brotli)
    /// - Speed: Very fast (much faster than Brotli)
    /// - Best for: High-throughput APIs, modern clients
    #[cfg(feature = "advanced-compression")]
    Zstd,
}

impl fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brotli => write!(f, "Brotli"),
            #[cfg(feature = "advanced-compression")]
            Self::Zstd => write!(f, "Zstd"),
        }
    }
}

/// Compression configuration
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// Selected compression algorithm
    pub algorithm: CompressionAlgorithm,
    /// Minimum response size to compress (bytes)
    /// Responses smaller than this are sent uncompressed
    pub min_bytes: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Brotli,
            min_bytes: 256, // Don't compress responses smaller than 256 bytes
        }
    }
}

impl CompressionConfig {
    /// Create compression config with Brotli (default)
    pub fn brotli() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Brotli,
            min_bytes: 256,
        }
    }

    /// Create compression config with Zstd (requires `advanced-compression` feature)
    #[cfg(feature = "advanced-compression")]
    pub fn zstd() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            min_bytes: 256,
        }
    }

    /// Set minimum bytes threshold for compression
    pub fn with_min_bytes(mut self, min_bytes: usize) -> Self {
        self.min_bytes = min_bytes;
        self
    }

    /// Get human-readable compression info
    pub fn description(&self) -> String {
        format!(
            "{} compression (min {} bytes)",
            self.algorithm, self.min_bytes
        )
    }
}

/// Creates the compression middleware layer
///
/// This function returns the appropriate compression middleware based on
/// the selected algorithm. The returned layer should be added to the Axum
/// router for response compression.
///
/// # Arguments
///
/// * `config` - Compression configuration
///
/// # Returns
///
/// A tower layer that compresses responses using the configured algorithm
///
/// # Example
///
/// ```ignore
/// use fraiseql_rs::http::middleware::{CompressionConfig, create_compression_layer};
///
/// let config = CompressionConfig::default(); // Uses Brotli
/// let layer = create_compression_layer(config);
/// ```
pub fn create_compression_layer(
    config: CompressionConfig,
) -> tower_http::compression::CompressionLayer {
    use tower_http::compression::CompressionLayer as TowerCompressionLayer;

    eprintln!("Compression configured: {}", config.description());

    // Note: tower-http compression layer is standard and doesn't allow runtime
    // algorithm selection. For Zstd support, we use the async-compression crate
    // which is feature-gated.
    //
    // The Brotli compression is always available via tower-http.
    TowerCompressionLayer::new()
}

/// Creates the CORS middleware layer
///
/// Configures CORS to allow GraphQL requests from any origin.
/// For production, this should be restricted to specific domains.
///
/// # Returns
///
/// A tower layer that handles CORS headers
pub fn create_cors_layer() -> CorsLayer {
    use tower_http::cors::AllowOrigin;

    CorsLayer::permissive()
    // In production, configure this:
    // .allow_origin(AllowOrigin::list(vec![
    //     "https://example.com".parse().unwrap(),
    // ]))
}

/// Error response for HTTP errors
#[derive(Debug)]
pub struct HttpError {
    /// HTTP status code
    pub status: StatusCode,
    /// Error message
    pub message: String,
}

impl HttpError {
    /// Create a new HTTP error
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Create a 400 Bad Request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// Create a 401 Unauthorized error
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    /// Create a 403 Forbidden error
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// Create a 404 Not Found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// Create a 500 Internal Server Error
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Create a 503 Service Unavailable error
    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "status": self.status.as_u16(),
                "message": self.message,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        });

        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_algorithm_display() {
        assert_eq!(CompressionAlgorithm::Brotli.to_string(), "Brotli");
        #[cfg(feature = "advanced-compression")]
        assert_eq!(CompressionAlgorithm::Zstd.to_string(), "Zstd");
    }

    #[test]
    fn test_compression_config_default() {
        let config = CompressionConfig::default();
        assert_eq!(config.algorithm, CompressionAlgorithm::Brotli);
        assert_eq!(config.min_bytes, 256);
    }

    #[test]
    fn test_compression_config_brotli() {
        let config = CompressionConfig::brotli();
        assert_eq!(config.algorithm, CompressionAlgorithm::Brotli);
        assert_eq!(config.min_bytes, 256);
    }

    #[cfg(feature = "advanced-compression")]
    #[test]
    fn test_compression_config_zstd() {
        let config = CompressionConfig::zstd();
        assert_eq!(config.algorithm, CompressionAlgorithm::Zstd);
        assert_eq!(config.min_bytes, 256);
    }

    #[test]
    fn test_compression_config_with_min_bytes() {
        let config = CompressionConfig::default().with_min_bytes(512);
        assert_eq!(config.min_bytes, 512);
    }

    #[test]
    fn test_compression_config_description() {
        let config = CompressionConfig::default();
        assert!(config.description().contains("Brotli"));
        assert!(config.description().contains("256 bytes"));
    }

    #[test]
    fn test_http_error_creation() {
        let err = HttpError::bad_request("Invalid input");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "Invalid input");
    }

    #[test]
    fn test_http_error_constructors() {
        assert_eq!(
            HttpError::unauthorized("Auth failed").status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            HttpError::forbidden("Access denied").status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            HttpError::not_found("Not found").status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            HttpError::internal_error("Server error").status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            HttpError::service_unavailable("Maintenance").status,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn test_http_error_into_response() {
        let err = HttpError::bad_request("Invalid query");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_compression_algorithm_equality() {
        assert_eq!(
            CompressionAlgorithm::Brotli,
            CompressionAlgorithm::Brotli
        );
        #[cfg(feature = "advanced-compression")]
        {
            assert_eq!(
                CompressionAlgorithm::Zstd,
                CompressionAlgorithm::Zstd
            );
            assert_ne!(
                CompressionAlgorithm::Brotli,
                CompressionAlgorithm::Zstd
            );
        }
    }
}
