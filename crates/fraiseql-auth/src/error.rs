//! Authentication error types.
use thiserror::Error;

/// All errors that can arise in the authentication and authorization layer.
///
/// Each variant maps to an appropriate HTTP status code via the [`axum::response::IntoResponse`]
/// implementation in `middleware.rs`. Internal details are never forwarded to API clients —
/// the `IntoResponse` impl always returns a generic user-facing message and logs the
/// internal reason via `tracing::warn!`.
#[derive(Debug, Error, Clone)]
pub enum AuthError {
    /// A supplied token could not be parsed or validated.
    /// The `reason` field contains internal diagnostic detail and must not be
    /// sent to API clients.
    #[error("Invalid token: {reason}")]
    InvalidToken {
        /// Internal description of why the token is invalid (not forwarded to callers).
        reason: String,
    },

    /// The token's `exp` claim is in the past.
    #[error("Token expired. Obtain a new token by re-authenticating.")]
    TokenExpired,

    /// The token's cryptographic signature did not verify against the expected key.
    #[error(
        "Token signature is invalid. Ensure the token was issued by the expected provider."
    )]
    InvalidSignature,

    /// A required JWT claim (`sub`, `iss`, `aud`, etc.) was absent from the token.
    #[error("Missing required claim: {claim}")]
    MissingClaim {
        /// Name of the missing claim (e.g., `"sub"`, `"aud"`).
        claim: String,
    },

    /// A claim was present but its value did not satisfy the validator's constraints.
    #[error("Invalid claim: {claim} - {reason}")]
    InvalidClaimValue {
        /// Name of the claim that failed validation.
        claim: String,
        /// Internal description of the validation failure (not forwarded to callers).
        reason: String,
    },

    /// An error was returned by the upstream OAuth provider (e.g., during code exchange).
    /// The `message` field must not be forwarded to API clients — it may contain
    /// provider-internal URLs, error codes, or rate-limit state.
    #[error("OAuth error: {message}")]
    OAuthError {
        /// Provider-internal error message (not forwarded to callers).
        message: String,
    },

    /// A session-store operation failed (creation, lookup, or revocation).
    #[error("Session error: {message}")]
    SessionError {
        /// Internal session error details (not forwarded to callers).
        message: String,
    },

    /// A database operation within the auth layer failed.
    /// Must never be forwarded to API clients — the message may reveal
    /// connection strings, query structure, or infrastructure topology.
    #[error("Database error: {message}")]
    DatabaseError {
        /// Internal database error message (not forwarded to callers).
        message: String,
    },

    /// The auth subsystem was misconfigured or a required configuration value was missing.
    /// Must never be forwarded to API clients — the message may reveal file paths,
    /// environment variable names, or key material.
    #[error("Configuration error: {message}")]
    ConfigError {
        /// Internal configuration error details (not forwarded to callers).
        message: String,
    },

    /// Fetching or parsing the OIDC discovery document failed.
    #[error("OIDC metadata error: {message}")]
    OidcMetadataError {
        /// Internal metadata fetch error details (not forwarded to callers).
        message: String,
    },

    /// A PKCE (Proof Key for Code Exchange, RFC 7636) operation failed.
    #[error("PKCE error: {message}")]
    PkceError {
        /// Internal PKCE error details (not forwarded to callers).
        message: String,
    },

    /// The OAuth `state` parameter did not match any stored CSRF token.
    /// This may indicate a replay attack or an expired authorization flow.
    #[error("State validation failed")]
    InvalidState,

    /// No `Authorization: Bearer <token>` header was present in the request.
    #[error(
        "No authentication token provided. Include a Bearer token in the Authorization header."
    )]
    TokenNotFound,

    /// The session associated with a refresh token has been explicitly revoked.
    #[error("Session revoked")]
    SessionRevoked,

    /// The authenticated user lacks the required permission for the requested operation.
    /// The `message` field contains the specific permission check detail and must not
    /// be forwarded to API clients in full (it reveals internal role/permission names).
    #[error("Forbidden: {message}")]
    Forbidden {
        /// Internal permission check details (not forwarded to callers).
        message: String,
    },

    /// An unexpected internal error occurred. Must never be forwarded to API clients.
    #[error("Internal error: {message}")]
    Internal {
        /// Internal error details (not forwarded to callers).
        message: String,
    },

    /// The system clock returned an unexpected value during a time-sensitive operation.
    /// This typically indicates a misconfigured system clock or clock rollback.
    #[error("System time error: {message}")]
    SystemTimeError {
        /// Internal system time error details (not forwarded to callers).
        message: String,
    },

    /// The client exceeded the configured rate limit for this endpoint.
    /// Unlike most other variants, the retry window is safe to forward to clients.
    #[error("Rate limited: retry after {retry_after_secs} seconds")]
    RateLimited {
        /// How many seconds the client must wait before retrying.
        retry_after_secs: u64,
    },

    /// The OIDC ID token is missing the required `nonce` claim.
    ///
    /// Returned when an expected nonce was provided for comparison but the token
    /// does not carry a `nonce` claim. May indicate a misconfigured provider or
    /// a token replay attempt using a stripped token.
    /// See RFC 6749 §10.12 / OpenID Connect Core §3.1.3.7.
    #[error("ID token is missing the required nonce claim")]
    MissingNonce,

    /// The `nonce` claim in the ID token does not match the expected value.
    ///
    /// Indicates a possible token replay or session fixation attack.
    /// See RFC 6749 §10.12 / OpenID Connect Core §3.1.3.7.
    #[error("ID token nonce mismatch — possible replay attack")]
    NonceMismatch,

    /// The OIDC ID token is missing the `auth_time` claim when `max_age` was requested.
    ///
    /// When `max_age` is sent in the authorization request, the provider MUST include
    /// `auth_time` in the ID token. Its absence indicates a non-conformant provider.
    /// See OpenID Connect Core §3.1.3.7.
    #[error("ID token is missing auth_time claim (required when max_age is used)")]
    MissingAuthTime,

    /// The session authentication time exceeds the allowed `max_age`.
    ///
    /// The provider authenticated the user too long ago for this request's `max_age`
    /// constraint. The user must re-authenticate to obtain a fresh session.
    /// See OpenID Connect Core §3.1.3.7.
    #[error("Session is too old: authenticated {age}s ago, max_age is {max_age_secs}s")]
    SessionTooOld {
        /// How many seconds ago the session was authenticated.
        age:          i64,
        /// Maximum allowed authentication age in seconds (from the authorization request).
        max_age_secs: u64,
    },
}

/// Convenience alias for `Result<T, AuthError>`.
pub type Result<T> = std::result::Result<T, AuthError>;
