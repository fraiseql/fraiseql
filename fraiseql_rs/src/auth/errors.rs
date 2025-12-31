//! Authentication error types.

use thiserror::Error;

/// Authentication and JWT validation errors
#[derive(Error, Debug)]
pub enum AuthError {
    /// Token is malformed, has invalid signature, or fails validation
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    /// Token has passed its expiration time
    #[error("Token expired")]
    TokenExpired,

    /// Token audience claim does not match expected value
    #[error("Invalid audience")]
    InvalidAudience,

    /// Token issuer claim does not match expected value
    #[error("Invalid issuer")]
    InvalidIssuer,

    /// Failed to fetch JSON Web Key Set from authentication provider
    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    /// Signing key ID not found in JWKS
    #[error("Key not found in JWKS: {0}")]
    KeyNotFound(String),

    /// Error accessing or updating JWKS cache
    #[error("Cache error: {0}")]
    CacheError(String),

    /// HTTP request error when fetching JWKS
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// JSON parsing error for JWKS or token claims
    #[error("JSON error: {0}")]
    JsonError(String),
}

impl From<reqwest::Error> for AuthError {
    fn from(err: reqwest::Error) -> Self {
        Self::HttpError(err.to_string())
    }
}

impl From<serde_json::Error> for AuthError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonError(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::InvalidAudience => Self::InvalidAudience,
            ErrorKind::ExpiredSignature => Self::TokenExpired,
            ErrorKind::InvalidIssuer => Self::InvalidIssuer,
            _ => Self::InvalidToken(err.to_string()),
        }
    }
}
