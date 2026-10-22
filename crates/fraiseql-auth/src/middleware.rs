//! The HTTP response every `AuthError` maps to.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::error::AuthError;

impl AuthError {
    /// Map each error variant to its HTTP response parts.
    ///
    /// SECURITY: Sanitized messages never expose internal details.
    #[allow(clippy::cognitive_complexity)] // Reason: exhaustive 1:1 mapping of AuthError variants to HTTP response tuples
    fn response_parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::TokenExpired => {
                (StatusCode::UNAUTHORIZED, "token_expired", "Authentication failed".to_string())
            },
            Self::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "invalid_signature",
                "Authentication failed".to_string(),
            ),
            Self::InvalidToken { .. }
            | Self::MissingClaim { .. }
            | Self::InvalidClaimValue { .. }
            // OIDC replay-protection errors and JWT temporal guards: return 401 without
            // revealing which specific claim was invalid to avoid oracle attacks.
            | Self::MissingNonce
            | Self::NonceMismatch
            | Self::MissingAuthTime
            | Self::SessionTooOld { .. }
            | Self::TokenIssuedInFuture
            | Self::TokenTooOld
            | Self::TokenNotYetValid
            // Algorithm-substitution attacks: reject with 401 without revealing which algorithm
            // was rejected, to avoid giving an attacker information about the allowed set.
            | Self::ForbiddenAlgorithm { .. } => {
                (StatusCode::UNAUTHORIZED, "invalid_token", "Authentication failed".to_string())
            },
            Self::TokenNotFound => {
                (StatusCode::UNAUTHORIZED, "token_not_found", "Authentication failed".to_string())
            },
            Self::SessionRevoked => {
                (StatusCode::UNAUTHORIZED, "session_revoked", "Authentication failed".to_string())
            },
            Self::InvalidState => {
                (StatusCode::BAD_REQUEST, "invalid_state", "Authentication failed".to_string())
            },
            Self::Forbidden { .. } => {
                (StatusCode::FORBIDDEN, "forbidden", "Permission denied".to_string())
            },
            Self::OAuthError { .. } => {
                (StatusCode::UNAUTHORIZED, "oauth_error", "Authentication failed".to_string())
            },
            Self::SessionError { .. } => {
                (StatusCode::UNAUTHORIZED, "session_error", "Authentication failed".to_string())
            },
            Self::DatabaseError { .. }
            | Self::ConfigError { .. }
            | Self::OidcMetadataError { .. }
            | Self::Internal { .. }
            | Self::SystemTimeError { .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Service temporarily unavailable".to_string(),
            ),
            Self::PkceError { .. } => {
                (StatusCode::BAD_REQUEST, "pkce_error", "Authentication failed".to_string())
            },
            Self::RateLimited { retry_after_secs } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("Too many requests. Retry after {retry_after_secs} seconds"),
            ),
            // SECURITY: unknown-user and wrong-password collapse to one response so the
            // client cannot enumerate registered emails (no user-existence oracle).
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "Invalid email or password".to_string(),
            ),
            Self::AccountDisabled => {
                (StatusCode::FORBIDDEN, "account_disabled", "This account is disabled".to_string())
            },
            // Account-wide deactivation (#946), as opposed to `AccountDisabled`, which
            // suspends only the local password credential. Reached at session creation —
            // i.e. after some credential already succeeded — so naming the state discloses
            // nothing to a party that does not already hold a working credential.
            Self::AccountDeactivated => (
                StatusCode::FORBIDDEN,
                "account_deactivated",
                "This account has been deactivated".to_string(),
            ),
            Self::EmailAlreadyRegistered => (
                StatusCode::CONFLICT,
                "email_already_registered",
                "An account already exists for this email".to_string(),
            ),
            // Safe to state plainly: reaching this requires having already proved
            // control of the address, so it is not an existence oracle (#945).
            Self::EmailClaimedByAnotherAccount => (
                StatusCode::CONFLICT,
                "email_claimed_by_another_account",
                "That email address is already verified on another account".to_string(),
            ),
            // The validation reason stays server-side; the client gets a generic message.
            Self::InvalidRegistration { .. } => (
                StatusCode::BAD_REQUEST,
                "invalid_registration",
                "Invalid registration details".to_string(),
            ),
        }
    }

    /// Log security-sensitive error details server-side before returning a sanitized response.
    #[allow(clippy::cognitive_complexity)] // Reason: exhaustive match logging security-sensitive details per AuthError variant
    fn log_security_details(&self) {
        use tracing::warn;

        match self {
            Self::InvalidToken { reason } => warn!("Invalid token error: {reason}"),
            Self::MissingClaim { claim } => warn!("Missing required claim: {claim}"),
            Self::InvalidClaimValue { claim, reason } => {
                warn!("Invalid claim value for '{claim}': {reason}");
            },
            Self::Forbidden { message } => warn!("Authorization denied: {message}"),
            Self::OAuthError { message } => warn!("OAuth provider error: {message}"),
            Self::SessionError { message } => warn!("Session error: {message}"),
            Self::DatabaseError { message } => {
                warn!("Database error (should not reach client): {message}");
            },
            Self::ConfigError { message } => {
                warn!("Configuration error (should not reach client): {message}");
            },
            Self::OidcMetadataError { message } => warn!("OIDC metadata error: {message}"),
            Self::PkceError { message } => warn!("PKCE error: {message}"),
            Self::Internal { message } => {
                warn!("Internal error (should not reach client): {message}");
            },
            Self::SystemTimeError { message } => {
                warn!("System time error (should not reach client): {message}");
            },
            Self::MissingNonce | Self::NonceMismatch => {
                warn!("OIDC nonce validation failed: {self}");
            },
            Self::MissingAuthTime | Self::SessionTooOld { .. } => {
                warn!("OIDC auth_time validation failed: {self}");
            },
            Self::TokenIssuedInFuture | Self::TokenTooOld | Self::TokenNotYetValid => {
                warn!("JWT temporal claim validation failed: {self}");
            },
            Self::ForbiddenAlgorithm { alg } => {
                warn!("OIDC algorithm-substitution attack rejected: forbidden algorithm '{alg}'");
            },
            Self::InvalidRegistration { reason } => warn!("Local signup rejected: {reason}"),
            // No additional logging here: the local-password flow audit-logs the precise
            // reason (unknown_user / wrong_password / disabled) at the point of failure.
            Self::TokenExpired
            | Self::InvalidSignature
            | Self::TokenNotFound
            | Self::SessionRevoked
            | Self::InvalidState
            | Self::RateLimited { .. }
            | Self::InvalidCredentials
            | Self::AccountDisabled
            | Self::AccountDeactivated
            | Self::EmailAlreadyRegistered
            | Self::EmailClaimedByAnotherAccount => {},
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        self.log_security_details();
        let (status, error_code, sanitized_message) = self.response_parts();

        let body = serde_json::json!({
            "errors": [{
                "message": sanitized_message,
                "extensions": {
                    "code": error_code
                }
            }]
        });

        (status, axum::Json(body)).into_response()
    }
}
