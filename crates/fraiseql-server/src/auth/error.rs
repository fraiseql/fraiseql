// Authentication error types
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum AuthError {
    #[error("Invalid token: {reason}")]
    InvalidToken { reason: String },

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Missing required claim: {claim}")]
    MissingClaim { claim: String },

    #[error("Invalid claim: {claim} - {reason}")]
    InvalidClaimValue { claim: String, reason: String },

    #[error("OAuth error: {message}")]
    OAuthError { message: String },

    #[error("Session error: {message}")]
    SessionError { message: String },

    #[error("Database error: {message}")]
    DatabaseError { message: String },

    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    #[error("OIDC metadata error: {message}")]
    OidcMetadataError { message: String },

    #[error("PKCE error: {message}")]
    PkceError { message: String },

    #[error("State validation failed")]
    InvalidState,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Session revoked")]
    SessionRevoked,

    #[error("Forbidden: {message}")]
    Forbidden { message: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("System time error: {message}")]
    SystemTimeError { message: String },
}

pub type Result<T> = std::result::Result<T, AuthError>;
