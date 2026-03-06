/// Errors that arise during authentication and authorisation flows.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The supplied username/password (or API key) did not match any account.
    #[error("Invalid credentials")]
    InvalidCredentials,

    /// The access token has passed its expiry time and must be refreshed.
    #[error("Token expired")]
    TokenExpired,

    /// The access token is structurally invalid or has been tampered with.
    #[error("Invalid token: {reason}")]
    InvalidToken {
        /// Reason the token was rejected (kept server-side; not forwarded to clients).
        reason: String,
    },

    /// An upstream OAuth / OIDC provider returned an error during the flow.
    #[error("Provider error: {provider} - {message}")]
    ProviderError {
        /// Name of the provider (e.g. `"google"`, `"github"`).
        provider: String,
        /// Provider-supplied error message (kept server-side; not forwarded to clients).
        message:  String,
    },

    /// The OAuth `state` parameter did not match the stored value, indicating a
    /// possible CSRF attack or a stale/replayed authorisation request.
    #[error("Invalid OAuth state")]
    InvalidState,

    /// The resource owner explicitly declined the authorisation request at the
    /// provider's consent screen.
    #[error("User denied authorization")]
    UserDenied,

    /// No active session exists for the supplied session identifier.
    #[error("Session not found")]
    SessionNotFound,

    /// The session existed but has expired and can no longer be used.
    #[error("Session expired")]
    SessionExpired,

    /// The authenticated principal does not have the scopes or roles required
    /// to perform the requested operation.
    #[error("Insufficient permissions: requires {required}")]
    InsufficientPermissions {
        /// The permission or scope that was required but not granted.
        required: String,
    },

    /// The refresh token has been revoked, used more than once, or has expired.
    #[error("Refresh token invalid or expired")]
    RefreshTokenInvalid,

    /// The account has been administratively locked and cannot be used.
    #[error("Account locked: {reason}")]
    AccountLocked {
        /// Reason the account was locked.
        reason: String,
    },
}

impl AuthError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::TokenExpired => "token_expired",
            Self::InvalidToken { .. } => "invalid_token",
            Self::ProviderError { .. } => "auth_provider_error",
            Self::InvalidState => "invalid_oauth_state",
            Self::UserDenied => "user_denied",
            Self::SessionNotFound => "session_not_found",
            Self::SessionExpired => "session_expired",
            Self::InsufficientPermissions { .. } => "insufficient_permissions",
            Self::RefreshTokenInvalid => "refresh_token_invalid",
            Self::AccountLocked { .. } => "account_locked",
        }
    }
}
