//! HTTP Authentication Middleware
//!
//! This module provides a wrapper layer for extracting and validating JWT tokens
//! from HTTP Authorization headers. It integrates with the existing auth module
//! to validate tokens and convert them to GraphQL `UserContext`.
//!
//! # Architecture
//!
//! ```text
//! HTTP Request (Authorization: Bearer <JWT>)
//!     ↓
//! ┌─────────────────────────────────────┐
//! │ HTTP Auth Middleware (THIS)         │
//! │ - Extract Authorization header      │
//! │ - Parse "Bearer <token>" format      │
//! │ - Call existing JWTValidator        │
//! │ - Convert Claims → UserContext      │
//! │ - Handle auth errors                │
//! └─────────────────────────────────────┘
//!     ↓
//! ├─ Calls auth::jwt::`JWTValidator`
//! ├─ Uses auth::jwt::`Claims`
//! └─ Uses auth::errors::`AuthError`
//!     ↓
//! GraphQL `UserContext` (with authenticated user)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use crate::http::auth_middleware;
//! use axum::http::HeaderMap;
//!
//! // In HTTP handler:
//! let auth_header = headers.get("Authorization").and_then(|h| h.to_str().ok());
//! match auth_middleware::extract_and_validate_jwt(auth_header, &jwt_validator).await {
//!     Ok(user_context) => { /* execute GraphQL with user */ }
//!     Err(http_err) => return (http_err.status_code(), http_err.json_response()),
//! }
//! ```

use axum::http::StatusCode;
use serde_json::json;

use crate::auth::{AuthError, Claims, JWTValidator};
use crate::pipeline::unified::UserContext;

/// HTTP Authentication Error
///
/// Wraps authentication errors and provides HTTP-specific information
#[derive(Debug, Clone)]
pub struct HttpAuthError {
    /// HTTP status code to return
    pub status_code: StatusCode,
    /// Human-readable error message
    pub message: String,
    /// Error code for client classification
    pub code: String,
}

impl HttpAuthError {
    /// Create an unauthorized error (401)
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::UNAUTHORIZED,
            message: message.into(),
            code: "UNAUTHORIZED".to_string(),
        }
    }

    /// Create a forbidden error (403)
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::FORBIDDEN,
            message: message.into(),
            code: "FORBIDDEN".to_string(),
        }
    }

    /// Create a bad request error (400)
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::BAD_REQUEST,
            message: message.into(),
            code: "BAD_REQUEST".to_string(),
        }
    }

    /// Create an internal server error (500)
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            code: "INTERNAL_SERVER_ERROR".to_string(),
        }
    }

    /// Get HTTP status code
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Convert to JSON response body
    #[must_use]
    pub fn json_response(&self) -> serde_json::Value {
        json!({
            "errors": [
                {
                    "message": self.message,
                    "extensions": {
                        "code": self.code
                    }
                }
            ]
        })
    }
}

/// Extract and validate JWT from Authorization header
///
/// This function orchestrates JWT extraction and validation by:
/// 1. Checking for Authorization header presence
/// 2. Parsing "Bearer <token>" format
/// 3. Calling `JWTValidator` to validate the token
/// 4. Converting validated `Claims` to GraphQL `UserContext`
/// 5. Mapping errors to appropriate HTTP status codes
///
/// # Arguments
///
/// * `auth_header` - The Authorization header value (e.g., "Bearer eyJ...")
/// * `jwt_validator` - The JWT validator instance for token validation
///
/// # Returns
///
/// Returns `Ok(UserContext)` if validation succeeds, or `Err(HttpAuthError)` if:
/// - Authorization header is missing or invalid format
/// - Token is expired or has invalid signature
/// - Token validation fails for any reason
/// - JWKS fetch fails
///
/// # Errors
///
/// Returns `Err(HttpAuthError)` if:
/// - Authorization header format is invalid
/// - JWT token validation fails
/// - JWKS fetch fails
/// - Bearer token is empty
///
/// # Behavior
///
/// If Authorization header is missing, this function returns an anonymous
/// `UserContext` to support public APIs. The GraphQL schema and permission
/// layer enforce actual access control.
pub async fn extract_and_validate_jwt(
    auth_header: Option<&str>,
    jwt_validator: &JWTValidator,
) -> Result<UserContext, HttpAuthError> {
    // If no Authorization header, create anonymous context
    let Some(auth_header_str) = auth_header else {
        return Ok(UserContext {
            user_id: None,
            permissions: vec!["public".to_string()],
            roles: vec![],
            exp: u64::MAX,
        });
    };

    // Parse "Bearer <token>" format
    let token = parse_bearer_token(auth_header_str)?;

    // Validate JWT token using existing validator
    let claims = jwt_validator
        .validate(token)
        .await
        .map_err(|auth_err| convert_auth_error_to_http(&auth_err))?;

    // Convert Claims to UserContext
    Ok(claims_to_user_context(claims))
}

/// Parse a Bearer token from Authorization header value
///
/// Expects format: "Bearer <token>"
///
/// # Errors
///
/// Returns `Err(HttpAuthError)` if:
/// - Header doesn't start with "Bearer " (case-insensitive)
/// - Token is empty after "Bearer "
fn parse_bearer_token(header_value: &str) -> Result<&str, HttpAuthError> {
    // Trim whitespace
    let header_value = header_value.trim();

    // Check for Bearer prefix (case-insensitive)
    if !header_value.to_lowercase().starts_with("bearer ") {
        return Err(HttpAuthError::bad_request(
            "Authorization header must be in format: Bearer <token>",
        ));
    }

    // Extract token after "Bearer "
    let token = &header_value[7..].trim(); // Skip "Bearer " (7 chars) and trim

    if token.is_empty() {
        return Err(HttpAuthError::bad_request(
            "Authorization header contains empty token",
        ));
    }

    Ok(token)
}

/// Convert validated JWT Claims to GraphQL `UserContext`
///
/// Maps JWT claims to `UserContext` fields:
/// - `sub` (subject) → `user_id`
/// - `exp` (expiration) → `exp`
/// - `permissions` custom claim → `permissions` (if present)
/// - `roles` custom claim → `roles` (if present)
///
/// # Arguments
///
/// * `claims` - The validated JWT claims
///
/// # Returns
///
/// A `UserContext` with:
/// - `user_id`: From `claims.sub`
/// - `permissions`: From `claims.custom["permissions"]` or empty
/// - `roles`: From `claims.custom["roles"]` or empty
/// - `exp`: From `claims.exp`
///
/// # Example
///
/// ```ignore
/// let claims = Claims {
///     sub: "user-123".to_string(),
///     exp: 1234567890,
///     ..Default::default()
/// };
/// let ctx = claims_to_user_context(claims);
/// assert_eq!(ctx.user_id, Some("user-123".to_string()));
/// ```
#[must_use]
pub fn claims_to_user_context(claims: Claims) -> UserContext {
    // Extract permissions from custom claims
    let permissions = claims
        .custom
        .get("permissions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    // Extract roles from custom claims
    let roles = claims
        .custom
        .get("roles")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    UserContext {
        user_id: Some(claims.sub),
        permissions,
        roles,
        exp: claims.exp,
    }
}

/// Convert `AuthError` to `HttpAuthError`
///
/// Maps auth module errors to appropriate HTTP status codes and specific error codes:
/// - `InvalidToken` → 401 Unauthorized (code: TOKEN_INVALID)
/// - `TokenExpired` → 401 Unauthorized (code: TOKEN_EXPIRED)
/// - `InvalidAudience` → 401 Unauthorized (code: INVALID_AUDIENCE)
/// - `InvalidIssuer` → 401 Unauthorized (code: INVALID_ISSUER)
/// - `KeyNotFound`, `JwksFetchFailed`, `HttpError`, `CacheError`, `JsonError` → 500 Internal Server Error
///
/// Specific error codes allow clients to distinguish between different authentication
/// failure modes and adjust their behavior accordingly (e.g., refresh token on expiration).
///
/// # Arguments
///
/// * `auth_err` - The authentication error to convert
///
/// # Returns
///
/// An `HttpAuthError` with appropriate status code, specific error code, and message
fn convert_auth_error_to_http(auth_err: &AuthError) -> HttpAuthError {
    match auth_err {
        // Token validation failures → 401 Unauthorized with specific codes
        AuthError::InvalidToken(reason) => {
            let mut err = HttpAuthError::unauthorized(format!("Invalid token: {reason}"));
            err.code = "TOKEN_INVALID".to_string();
            err
        }
        AuthError::TokenExpired => {
            let mut err = HttpAuthError::unauthorized("Token has expired");
            err.code = "TOKEN_EXPIRED".to_string();
            err
        }
        AuthError::InvalidAudience => {
            let mut err = HttpAuthError::unauthorized("Token audience is invalid");
            err.code = "INVALID_AUDIENCE".to_string();
            err
        }
        AuthError::InvalidIssuer => {
            let mut err = HttpAuthError::unauthorized("Token issuer is invalid");
            err.code = "INVALID_ISSUER".to_string();
            err
        }

        // Key management errors → 500 Internal Server Error
        AuthError::KeyNotFound(kid) => {
            let mut err = HttpAuthError::internal_error(format!("Key not found in JWKS: {kid}"));
            err.code = "KEY_NOT_FOUND".to_string();
            err
        }

        // JWKS fetching errors → 500 Internal Server Error
        AuthError::JwksFetchFailed(reason) => {
            let mut err = HttpAuthError::internal_error(format!("Failed to fetch JWKS: {reason}"));
            err.code = "JWKS_FETCH_FAILED".to_string();
            err
        }

        // HTTP transport errors → 500 Internal Server Error
        AuthError::HttpError(reason) => {
            let mut err = HttpAuthError::internal_error(format!("HTTP error: {reason}"));
            err.code = "HTTP_ERROR".to_string();
            err
        }

        // Cache errors → 500 Internal Server Error
        AuthError::CacheError(reason) => {
            let mut err = HttpAuthError::internal_error(format!("Cache error: {reason}"));
            err.code = "CACHE_ERROR".to_string();
            err
        }

        // JSON parsing errors → 500 Internal Server Error
        AuthError::JsonError(reason) => {
            let mut err = HttpAuthError::internal_error(format!("JSON error: {reason}"));
            err.code = "JSON_PARSE_ERROR".to_string();
            err
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bearer_token_valid() {
        let header = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let token = parse_bearer_token(header).unwrap();
        assert_eq!(
            token,
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
        );
    }

    #[test]
    fn test_parse_bearer_token_with_whitespace() {
        let header = "  Bearer   eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature  ";
        let token = parse_bearer_token(header).unwrap();
        assert_eq!(
            token,
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
        );
    }

    #[test]
    fn test_parse_bearer_token_case_insensitive() {
        let header = "bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let token = parse_bearer_token(header).unwrap();
        assert_eq!(
            token,
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
        );
    }

    #[test]
    fn test_parse_bearer_token_uppercase() {
        let header = "BEARER eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let token = parse_bearer_token(header).unwrap();
        assert_eq!(
            token,
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
        );
    }

    #[test]
    fn test_parse_bearer_token_missing_bearer() {
        let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let result = parse_bearer_token(header);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("Bearer"));
    }

    #[test]
    fn test_parse_bearer_token_invalid_scheme() {
        let header = "Basic dXNlcm5hbWU6cGFzc3dvcmQ=";
        let result = parse_bearer_token(header);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parse_bearer_token_empty_token() {
        let header = "Bearer ";
        let result = parse_bearer_token(header);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("empty"));
    }

    #[test]
    fn test_claims_to_user_context_basic() {
        let claims = Claims {
            sub: "user-123".to_string(),
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            custom: Default::default(),
        };

        let ctx = claims_to_user_context(claims);
        assert_eq!(ctx.user_id, Some("user-123".to_string()));
        assert!(ctx.permissions.is_empty());
        assert!(ctx.roles.is_empty());
        assert_eq!(ctx.exp, 1_234_567_890);
    }

    #[test]
    fn test_claims_to_user_context_with_permissions() {
        let _custom: std::collections::HashMap<String, serde_json::Value> = Default::default();
        let permissions_json = serde_json::json!(["read", "write"]);
        // We need to manually construct this since Claims has HashMap<String, Value>
        let claims = Claims {
            sub: "user-123".to_string(),
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            custom: {
                let mut m = std::collections::HashMap::new();
                m.insert("permissions".to_string(), permissions_json);
                m
            },
        };

        let ctx = claims_to_user_context(claims);
        assert_eq!(ctx.user_id, Some("user-123".to_string()));
        assert_eq!(ctx.permissions, vec!["read", "write"]);
        assert!(ctx.roles.is_empty());
    }

    #[test]
    fn test_claims_to_user_context_with_roles() {
        let claims = Claims {
            sub: "user-123".to_string(),
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            custom: {
                let mut m = std::collections::HashMap::new();
                m.insert("roles".to_string(), serde_json::json!(["admin", "user"]));
                m
            },
        };

        let ctx = claims_to_user_context(claims);
        assert_eq!(ctx.user_id, Some("user-123".to_string()));
        assert!(ctx.permissions.is_empty());
        assert_eq!(ctx.roles, vec!["admin", "user"]);
    }

    #[test]
    fn test_claims_to_user_context_with_both() {
        let claims = Claims {
            sub: "user-456".to_string(),
            iss: "https://example.com".to_string(),
            aud: vec!["api".to_string()],
            exp: 9_999_999_999,
            iat: 1_234_567_800,
            custom: {
                let mut m = std::collections::HashMap::new();
                m.insert("permissions".to_string(), serde_json::json!(["read"]));
                m.insert("roles".to_string(), serde_json::json!(["viewer"]));
                m
            },
        };

        let ctx = claims_to_user_context(claims);
        assert_eq!(ctx.user_id, Some("user-456".to_string()));
        assert_eq!(ctx.permissions, vec!["read"]);
        assert_eq!(ctx.roles, vec!["viewer"]);
        assert_eq!(ctx.exp, 9_999_999_999);
    }

    #[test]
    fn test_http_auth_error_unauthorized() {
        let err = HttpAuthError::unauthorized("Token expired");
        assert_eq!(err.status_code, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "Token expired");
        assert_eq!(err.code, "UNAUTHORIZED");
    }

    #[test]
    fn test_http_auth_error_bad_request() {
        let err = HttpAuthError::bad_request("Invalid format");
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "Invalid format");
        assert_eq!(err.code, "BAD_REQUEST");
    }

    #[test]
    fn test_http_auth_error_internal_error() {
        let err = HttpAuthError::internal_error("JWKS fetch failed");
        assert_eq!(err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message, "JWKS fetch failed");
        assert_eq!(err.code, "INTERNAL_SERVER_ERROR");
    }

    #[test]
    fn test_http_auth_error_json_response() {
        let err = HttpAuthError::unauthorized("Invalid token");
        let json = err.json_response();
        assert!(json.get("errors").is_some());
        let errors = json.get("errors").unwrap().as_array().unwrap();
        assert_eq!(errors[0]["message"], "Invalid token");
        assert_eq!(errors[0]["extensions"]["code"], "UNAUTHORIZED");
    }

    #[test]
    fn test_convert_auth_error_invalid_token() {
        let auth_err = AuthError::InvalidToken("bad sig".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::UNAUTHORIZED);
        assert_eq!(http_err.code, "TOKEN_INVALID");
        assert!(http_err.message.contains("Invalid token"));
    }

    #[test]
    fn test_convert_auth_error_token_expired() {
        let auth_err = AuthError::TokenExpired;
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::UNAUTHORIZED);
        assert_eq!(http_err.code, "TOKEN_EXPIRED");
        assert!(http_err.message.contains("expired"));
    }

    #[test]
    fn test_convert_auth_error_invalid_audience() {
        let auth_err = AuthError::InvalidAudience;
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::UNAUTHORIZED);
        assert_eq!(http_err.code, "INVALID_AUDIENCE");
    }

    #[test]
    fn test_convert_auth_error_invalid_issuer() {
        let auth_err = AuthError::InvalidIssuer;
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::UNAUTHORIZED);
        assert_eq!(http_err.code, "INVALID_ISSUER");
    }

    #[test]
    fn test_convert_auth_error_key_not_found() {
        let auth_err = AuthError::KeyNotFound("kid-123".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "KEY_NOT_FOUND");
        assert!(http_err.message.contains("kid-123"));
    }

    #[test]
    fn test_convert_auth_error_jwks_fetch_failed() {
        let auth_err = AuthError::JwksFetchFailed("connection timeout".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "JWKS_FETCH_FAILED");
        assert!(http_err.message.contains("JWKS"));
    }

    #[test]
    fn test_convert_auth_error_http_error() {
        let auth_err = AuthError::HttpError("connection refused".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "HTTP_ERROR");
    }

    #[test]
    fn test_convert_auth_error_cache_error() {
        let auth_err = AuthError::CacheError("eviction failed".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "CACHE_ERROR");
    }

    #[test]
    fn test_convert_auth_error_json_error() {
        let auth_err = AuthError::JsonError("unexpected field".to_string());
        let http_err = convert_auth_error_to_http(&auth_err);
        assert_eq!(http_err.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(http_err.code, "JSON_PARSE_ERROR");
    }
}
