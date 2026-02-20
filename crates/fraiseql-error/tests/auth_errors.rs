#![allow(missing_docs)]

use fraiseql_error::AuthError;

#[test]
fn invalid_credentials_error_code() {
    assert_eq!(AuthError::InvalidCredentials.error_code(), "invalid_credentials");
}

#[test]
fn invalid_credentials_display() {
    assert_eq!(AuthError::InvalidCredentials.to_string(), "Invalid credentials");
}

#[test]
fn token_expired_error_code() {
    assert_eq!(AuthError::TokenExpired.error_code(), "token_expired");
}

#[test]
fn token_expired_display() {
    assert_eq!(AuthError::TokenExpired.to_string(), "Token expired");
}

#[test]
fn invalid_token_error_code() {
    let err = AuthError::InvalidToken {
        reason: "malformed".into(),
    };
    assert_eq!(err.error_code(), "invalid_token");
}

#[test]
fn invalid_token_display_interpolates_reason() {
    let err = AuthError::InvalidToken {
        reason: "malformed JWT".into(),
    };
    assert_eq!(err.to_string(), "Invalid token: malformed JWT");
}

#[test]
fn provider_error_code_and_display() {
    let err = AuthError::ProviderError {
        provider: "google".into(),
        message:  "timeout".into(),
    };
    assert_eq!(err.error_code(), "auth_provider_error");
    assert_eq!(err.to_string(), "Provider error: google - timeout");
}

#[test]
fn invalid_state_error_code() {
    assert_eq!(AuthError::InvalidState.error_code(), "invalid_oauth_state");
}

#[test]
fn invalid_state_display() {
    assert_eq!(AuthError::InvalidState.to_string(), "Invalid OAuth state");
}

#[test]
fn user_denied_error_code_and_display() {
    assert_eq!(AuthError::UserDenied.error_code(), "user_denied");
    assert_eq!(AuthError::UserDenied.to_string(), "User denied authorization");
}

#[test]
fn session_not_found_error_code_and_display() {
    assert_eq!(AuthError::SessionNotFound.error_code(), "session_not_found");
    assert_eq!(AuthError::SessionNotFound.to_string(), "Session not found");
}

#[test]
fn session_expired_error_code_and_display() {
    assert_eq!(AuthError::SessionExpired.error_code(), "session_expired");
    assert_eq!(AuthError::SessionExpired.to_string(), "Session expired");
}

#[test]
fn insufficient_permissions_error_code_and_display() {
    let err = AuthError::InsufficientPermissions {
        required: "admin".into(),
    };
    assert_eq!(err.error_code(), "insufficient_permissions");
    assert_eq!(err.to_string(), "Insufficient permissions: requires admin");
}

#[test]
fn refresh_token_invalid_error_code_and_display() {
    assert_eq!(AuthError::RefreshTokenInvalid.error_code(), "refresh_token_invalid");
    assert_eq!(AuthError::RefreshTokenInvalid.to_string(), "Refresh token invalid or expired");
}

#[test]
fn account_locked_error_code_and_display() {
    let err = AuthError::AccountLocked {
        reason: "too many attempts".into(),
    };
    assert_eq!(err.error_code(), "account_locked");
    assert_eq!(err.to_string(), "Account locked: too many attempts");
}
