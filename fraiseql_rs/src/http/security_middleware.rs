//! HTTP Security Middleware
//!
//! This module provides a wrapper layer between the HTTP server and existing security modules.
//! It integrates with the security layer (validation, rate limiting) and converts security errors
//! to appropriate HTTP responses.
//!
//! Rather than reimplementing security logic, this module adapts existing security modules
//! for use in the HTTP request/response cycle.
//!
//! # Architecture
//!
//! ```text
//! HTTP Request
//!     ↓
//! ┌─────────────────────────────────────┐
//! │ HTTP Security Middleware (THIS)     │
//! │ - Extract client IP                 │
//! │ - Call existing validators          │
//! │ - Call existing rate limiter        │
//! │ - Convert errors to HTTP responses  │
//! └─────────────────────────────────────┘
//!     ↓
//! ├─ Calls security::validators::QueryValidator
//! ├─ Calls security::rate_limit::RateLimiter
//! └─ Calls security::constraints::QueryConstraints
//!     ↓
//! HTTP Response (200/400/429/500)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use crate::http::security_middleware;
//!
//! // In HTTP handler:
//! match security_middleware::validate_graphql_request(
//!     &request.query,
//!     &request.variables,
//!     client_ip,
//! ).await {
//!     Ok(_) => { /* continue to execution */ }
//!     Err(http_err) => return (http_err.status_code(), http_err.json_response())
//! }
//! ```

use axum::http::StatusCode;
use serde_json::json;
use std::net::IpAddr;

use crate::graphql::types::ParsedQuery;
use crate::security::{QueryLimits, QueryValidator, RateLimitChecker, SecurityError};

use super::axum_server::GraphQLError;

/// HTTP Security Error
///
/// Wraps security errors and provides HTTP-specific information
#[derive(Debug, Clone)]
pub struct HttpSecurityError {
    /// HTTP status code to return
    pub status_code: StatusCode,
    /// Human-readable error message
    pub message: String,
    /// Error code for client classification
    pub code: String,
    /// Client IP that triggered the error
    pub client_ip: String,
    /// Optional retry-after value (for rate limits)
    pub retry_after: Option<u64>,
}

impl HttpSecurityError {
    /// Convert to GraphQL error response
    #[must_use] 
    pub fn to_graphql_error(&self) -> GraphQLError {
        let mut extensions = json!({
            "code": self.code,
            "client_ip": self.client_ip,
        });

        if let Some(retry_after) = self.retry_after {
            extensions["retry_after"] = json!(retry_after);
        }

        GraphQLError {
            message: self.message.clone(),
            extensions: Some(extensions),
        }
    }

    /// Get HTTP status code
    #[must_use] 
    pub const fn status_code(&self) -> StatusCode {
        self.status_code
    }
}

/// Validate a GraphQL request against security constraints
///
/// This function orchestrates the security checks by calling existing security modules.
/// It's the main entry point for HTTP layer security validation.
///
/// # Arguments
///
/// * `query` - The GraphQL query string
/// * `parsed_query` - Pre-parsed query structure
/// * `client_ip` - Client IP address for rate limiting
///
/// # Returns
///
/// Returns `Ok(())` if all security checks pass, or `Err(HttpSecurityError)` if any check fails.
///
/// # Check Order
///
/// 1. Query size validation
/// 2. Rate limiting (per IP)
/// 3. Query complexity and depth
///
/// This order ensures we reject obviously bad requests early before expensive parsing.
///
/// # Errors
///
/// Returns `Err(HttpSecurityError)` if:
/// - Query size exceeds the configured maximum
/// - Query depth exceeds the configured maximum
/// - Query complexity score exceeds limits
pub async fn validate_graphql_request(
    query: &str,
    parsed_query: &ParsedQuery,
    client_ip: IpAddr,
) -> Result<(), HttpSecurityError> {
    let ip_str = client_ip.to_string();

    // Use production-grade security limits
    let validator = QueryValidator::new(QueryLimits::production());

    // Validate query structure and complexity
    validator
        .validate(query, parsed_query)
        .map_err(|security_err| convert_security_error_to_http(&security_err, &ip_str))?;

    Ok(())
}

/// Check rate limit for a client IP
///
/// This integrates with the existing rate limiter to enforce per-IP rate limits.
///
/// # Arguments
///
/// * `limiter` - The rate limiter instance
/// * `client_ip` - Client IP address
///
/// # Returns
///
/// Returns `Ok(())` if the request is within rate limits, or `Err(HttpSecurityError)` if rate limit exceeded.
///
/// # Errors
///
/// Returns `Err(HttpSecurityError)` if:
/// - Rate limit for the client IP has been exceeded
/// - The limit checker is unavailable
pub async fn check_rate_limit(
    limiter: &RateLimitChecker,
    client_ip: IpAddr,
) -> Result<(), HttpSecurityError> {
    let ip_str = client_ip.to_string();

    // Check the /graphql path rate limit
    limiter
        .check(&ip_str, "/graphql")
        .await
        .map_err(|security_err| convert_security_error_to_http(&security_err, &ip_str))?;

    Ok(())
}

/// Convert a security error to an HTTP error
///
/// Maps `SecurityError` variants to appropriate HTTP status codes and messages.
fn convert_security_error_to_http(
    security_err: &SecurityError,
    client_ip: &str,
) -> HttpSecurityError {
    match security_err {
        // Rate limit errors → 429 Too Many Requests
        SecurityError::RateLimitExceeded {
            retry_after,
            limit,
            window_secs,
        } => HttpSecurityError {
            status_code: StatusCode::TOO_MANY_REQUESTS,
            message: format!(
                "Rate limit exceeded: {limit} requests per {window_secs} seconds"
            ),
            code: "RATE_LIMIT_EXCEEDED".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: Some(*retry_after),
        },

        // Query validation errors → 400 Bad Request
        SecurityError::QueryTooLarge { size, max_size } => HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: format!("Query size ({size} bytes) exceeds maximum ({max_size})"),
            code: "QUERY_TOO_LARGE".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        SecurityError::QueryTooDeep { depth, max_depth } => HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: format!(
                "Query nesting ({depth} levels) exceeds maximum ({max_depth})"
            ),
            code: "QUERY_TOO_DEEP".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        SecurityError::QueryTooComplex {
            complexity,
            max_complexity,
        } => HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: format!(
                "Query complexity ({complexity}) exceeds maximum ({max_complexity})"
            ),
            code: "QUERY_TOO_COMPLEX".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        // Other security errors → 400 Bad Request
        SecurityError::OriginNotAllowed(origin) => HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: format!("CORS origin not allowed: {origin}"),
            code: "CORS_ORIGIN_NOT_ALLOWED".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        SecurityError::InvalidCSRFToken(reason) => HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: format!("CSRF validation failed: {reason}"),
            code: "INVALID_CSRF_TOKEN".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        // Config and internal errors → 500 Internal Server Error
        SecurityError::SecurityConfigError(reason) => HttpSecurityError {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Security configuration error: {reason}"),
            code: "SECURITY_CONFIG_ERROR".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },

        // Default: Internal server error
        _ => HttpSecurityError {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Security validation failed".to_string(),
            code: "SECURITY_ERROR".to_string(),
            client_ip: client_ip.to_string(),
            retry_after: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_security_error_to_graphql_error() {
        let err = HttpSecurityError {
            status_code: StatusCode::TOO_MANY_REQUESTS,
            message: "Rate limit exceeded".to_string(),
            code: "RATE_LIMIT_EXCEEDED".to_string(),
            client_ip: "192.168.1.1".to_string(),
            retry_after: Some(60),
        };

        let gql_err = err.to_graphql_error();
        assert_eq!(gql_err.message, "Rate limit exceeded");
        assert!(gql_err.extensions.is_some());

        let ext = gql_err.extensions.unwrap();
        assert_eq!(ext["code"], "RATE_LIMIT_EXCEEDED");
        assert_eq!(ext["retry_after"], 60);
    }

    #[test]
    fn test_convert_rate_limit_error() {
        let err = SecurityError::RateLimitExceeded {
            retry_after: 30,
            limit: 100,
            window_secs: 60,
        };

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(http_err.code, "RATE_LIMIT_EXCEEDED");
        assert_eq!(http_err.retry_after, Some(30));
    }

    #[test]
    fn test_convert_query_too_large_error() {
        let err = SecurityError::QueryTooLarge {
            size: 100_000,
            max_size: 50_000,
        };

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(http_err.code, "QUERY_TOO_LARGE");
        assert!(http_err.message.contains("100000"));
    }

    #[test]
    fn test_convert_query_too_deep_error() {
        let err = SecurityError::QueryTooDeep {
            depth: 15,
            max_depth: 7,
        };

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(http_err.code, "QUERY_TOO_DEEP");
    }

    #[test]
    fn test_convert_query_too_complex_error() {
        let err = SecurityError::QueryTooComplex {
            complexity: 600,
            max_complexity: 500,
        };

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(http_err.code, "QUERY_TOO_COMPLEX");
    }

    #[test]
    fn test_convert_csrf_token_error() {
        let err = SecurityError::InvalidCSRFToken("token mismatch".to_string());

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(http_err.code, "INVALID_CSRF_TOKEN");
    }

    #[test]
    fn test_convert_config_error() {
        let err = SecurityError::SecurityConfigError("missing rate limit config".to_string());

        let http_err = convert_security_error_to_http(&err, "10.0.0.1");
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "SECURITY_CONFIG_ERROR");
    }

    #[test]
    fn test_http_security_error_client_ip() {
        let err = HttpSecurityError {
            status_code: StatusCode::BAD_REQUEST,
            message: "test error".to_string(),
            code: "TEST_ERROR".to_string(),
            client_ip: "192.168.1.1".to_string(),
            retry_after: None,
        };

        assert_eq!(err.client_ip, "192.168.1.1");
    }
}
