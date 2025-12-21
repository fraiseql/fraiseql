//! Authentication error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid audience")]
    InvalidAudience,

    #[error("Invalid issuer")]
    InvalidIssuer,

    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    #[error("Key not found in JWKS: {0}")]
    KeyNotFound(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("JSON error: {0}")]
    JsonError(String),
}

impl From<reqwest::Error> for AuthError {
    fn from(err: reqwest::Error) -> Self {
        AuthError::HttpError(err.to_string())
    }
}

impl From<serde_json::Error> for AuthError {
    fn from(err: serde_json::Error) -> Self {
        AuthError::JsonError(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::InvalidAudience => AuthError::InvalidAudience,
            ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            ErrorKind::InvalidIssuer => AuthError::InvalidIssuer,
            _ => AuthError::InvalidToken(err.to_string()),
        }
    }
}
