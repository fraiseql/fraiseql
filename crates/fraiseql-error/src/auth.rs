#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {reason}")]
    InvalidToken { reason: String },

    #[error("Provider error: {provider} - {message}")]
    ProviderError { provider: String, message: String },

    #[error("Invalid OAuth state")]
    InvalidState,

    #[error("User denied authorization")]
    UserDenied,

    #[error("Session not found")]
    SessionNotFound,

    #[error("Session expired")]
    SessionExpired,

    #[error("Insufficient permissions: requires {required}")]
    InsufficientPermissions { required: String },

    #[error("Refresh token invalid or expired")]
    RefreshTokenInvalid,

    #[error("Account locked: {reason}")]
    AccountLocked { reason: String },
}

impl AuthError {
    pub fn error_code(&self) -> &'static str {
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
