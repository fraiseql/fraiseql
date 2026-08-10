//! HTTP surface for local email + password authentication (#367).
//!
//! [`LocalPasswordAuthenticator`] shipped signup, login and the full password-reset
//! flow as **library methods with no route**: nothing in the server mounted them, so
//! an operator who configured local passwords got a schema and no way to use it.
//! These handlers close that gap.
//!
//! Mounted routes:
//!
//! - `POST /auth/v1/password/signup` — create an account, return session tokens.
//! - `POST /auth/v1/password/login` — verify credentials, return session tokens.
//! - `POST /auth/v1/password/reset` — start a reset. Always 202, never enumerable.
//! - `POST /auth/v1/password/reset/confirm` — redeem the token and set a new password.
//! - `POST /auth/v1/email/verify/start` — mail a verification link to the caller's own address.
//! - `POST /auth/v1/email/verify/confirm` — redeem it, promoting the account to verified (#945).
//!
//! The two verification routes form their own group
//! ([`email_verification_routes`]) mounted by `[auth.local] email_verification`, and
//! are the only routes here that require an authenticated caller: both act on the
//! account the bearer token was minted for, resolved through
//! [`SessionBearerAuthenticator`] rather than read from the request body.
//!
//! # Security
//!
//! - The reset-start response is a constant `202 Accepted` whether or not the address has an
//!   account (the underlying call is already non-enumerable and constant-cost).
//! - Signup and login are per-IP rate-limited through the shared [`RateLimiters`] `auth_start`
//!   bucket: password verification is deliberately expensive (Argon2id), so an unthrottled endpoint
//!   is both a credential-stuffing surface and a `CPU` `DoS`.
//! - Failed logins return one generic message; the specific reason is audit-logged only.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;

use super::LocalPasswordAuthenticator;
use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::AuthError,
    rate_limiting::RateLimiters,
    session::{SessionStore, unix_now},
    session_bearer::SessionBearerAuthenticator,
};

/// Session lifetime granted by a successful password signup/login (1 hour), matching
/// the OTP and MFA flows.
const SESSION_TTL_SECS: u64 = 3_600;

/// Axum state for the local-password routes.
#[derive(Clone)]
pub struct LocalPasswordRouteState {
    /// The authenticator (owns the credential store and the reset flow).
    pub authenticator: Arc<LocalPasswordAuthenticator>,
    /// Session store issuing tokens after a successful signup/login.
    pub session_store: Arc<dyn SessionStore>,
    /// Per-IP limiters; the `auth_start` bucket governs signup and login.
    pub rate_limiters: Arc<RateLimiters>,
}

/// Body of `POST /auth/v1/password/signup` and `/login`.
#[derive(Debug, Deserialize)]
pub struct CredentialsRequest {
    /// Email address (the account identity).
    pub email:    String,
    /// Plaintext password. Never logged, never persisted — only its Argon2id hash is.
    pub password: String,
}

/// Body of `POST /auth/v1/password/reset`.
#[derive(Debug, Deserialize)]
pub struct ResetStartRequest {
    /// Address to send the reset link to.
    pub email: String,
}

/// Body of `POST /auth/v1/password/reset/confirm`.
#[derive(Debug, Deserialize)]
pub struct ResetConfirmRequest {
    /// The opaque token from the reset link.
    pub token:        String,
    /// The new password.
    pub new_password: String,
}

/// Build the local-password route group.
#[allow(clippy::missing_panics_doc)] // Reason: infallible — no path capture syntax to reject
pub fn local_password_routes(state: Arc<LocalPasswordRouteState>) -> Router {
    Router::new()
        .route("/auth/v1/password/signup", post(password_signup))
        .route("/auth/v1/password/login", post(password_login))
        .route("/auth/v1/password/reset", post(password_reset_start))
        .route("/auth/v1/password/reset/confirm", post(password_reset_confirm))
        .with_state(state)
}

fn json_error(status: StatusCode, error: &str, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error, "message": message }))).into_response()
}

/// Enforce the per-IP `auth_start` budget, keyed on the transport peer only —
/// never an attacker-spoofable forwarded header.
fn rate_limit(state: &LocalPasswordRouteState, addr: SocketAddr, op: &str) -> Option<Response> {
    let client_ip = addr.ip().to_string();
    if state.rate_limiters.auth_start.check(&client_ip).is_ok() {
        return None;
    }
    let retry_after = state.rate_limiters.auth_start.clone_config().window_secs;
    get_audit_logger().log_failure(
        AuditEventType::AuthFailure,
        SecretType::SessionToken,
        None,
        op,
        "rate limited",
    );
    Some(
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
            Json(serde_json::json!({
                "error":   "rate_limited",
                "message": "Too many attempts; please retry later"
            })),
        )
            .into_response(),
    )
}

/// Issue a session for `user_id`, or render the failure.
async fn issue_session(state: &LocalPasswordRouteState, user_id: &str) -> Response {
    let Ok(now) = unix_now() else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "internal error");
    };
    match state.session_store.create_session(user_id, now + SESSION_TTL_SECS).await {
        Ok(tokens) => {
            get_audit_logger().log_success(
                AuditEventType::AuthSuccess,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "local_password",
            );
            (StatusCode::OK, Json(tokens)).into_response()
        },
        Err(e) => {
            tracing::error!(error = %e, "session creation failed after local-password auth");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_failed",
                "session could not be created",
            )
        },
    }
}

/// `POST /auth/v1/password/signup`
///
/// Creates a local credential for `email` and returns session tokens.
///
/// # Errors
///
/// `409` when the email already has a local credential, `422` when the password
/// fails the policy, `429` when rate-limited.
pub async fn password_signup(
    State(state): State<Arc<LocalPasswordRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<CredentialsRequest>,
) -> Response {
    if let Some(limited) = rate_limit(&state, addr, "password_signup") {
        return limited;
    }
    match state.authenticator.signup(&req.email, &req.password).await {
        Ok(user_id) => issue_session(&state, &user_id).await,
        Err(e @ AuthError::InvalidRegistration { .. }) => {
            json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_registration", &e.to_string())
        },
        // A duplicate signup is the one case where the specific reason is safe
        // to return: the caller supplied the address, so it discloses nothing
        // they did not already know.
        Err(AuthError::EmailAlreadyRegistered) => json_error(
            StatusCode::CONFLICT,
            "already_registered",
            "that email already has a password credential",
        ),
        Err(e) => {
            tracing::error!(error = %e, "local-password signup failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "signup_failed", "signup failed")
        },
    }
}

/// `POST /auth/v1/password/login`
///
/// Verifies the credentials and returns session tokens.
///
/// # Errors
///
/// `401` for any authentication failure (one generic message — an unknown account
/// and a wrong password are indistinguishable), `429` when rate-limited.
pub async fn password_login(
    State(state): State<Arc<LocalPasswordRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<CredentialsRequest>,
) -> Response {
    if let Some(limited) = rate_limit(&state, addr, "password_login") {
        return limited;
    }
    match state.authenticator.login(&req.email, &req.password).await {
        Ok(user_id) => issue_session(&state, &user_id).await,
        Err(e) => {
            get_audit_logger().log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "password_login",
                &e.to_string(),
            );
            json_error(StatusCode::UNAUTHORIZED, "invalid_credentials", "invalid email or password")
        },
    }
}

/// `POST /auth/v1/password/reset`
///
/// Starts a password reset. **Always** `202 Accepted`: the response neither
/// confirms nor denies that an account exists.
///
/// # Errors
///
/// `429` when rate-limited. Infrastructure failures are logged and still return
/// `202` — surfacing them would make the endpoint enumerable by error shape.
pub async fn password_reset_start(
    State(state): State<Arc<LocalPasswordRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<ResetStartRequest>,
) -> Response {
    if let Some(limited) = rate_limit(&state, addr, "password_reset_start") {
        return limited;
    }
    if let Err(e) = state.authenticator.start_password_reset(&req.email).await {
        tracing::error!(error = %e, "password reset start failed");
    }
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "If that address has an account, a reset link has been sent."
        })),
    )
        .into_response()
}

/// `POST /auth/v1/password/reset/confirm`
///
/// Redeems a reset token and sets the new password, revoking outstanding sessions.
///
/// # Errors
///
/// `422` for an invalid, expired or already-used token, or a password that fails
/// the policy; `429` when rate-limited.
pub async fn password_reset_confirm(
    State(state): State<Arc<LocalPasswordRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<ResetConfirmRequest>,
) -> Response {
    if let Some(limited) = rate_limit(&state, addr, "password_reset_confirm") {
        return limited;
    }
    match state.authenticator.confirm_password_reset(&req.token, &req.new_password).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "message": "Password updated." })))
            .into_response(),
        Err(e @ AuthError::InvalidRegistration { .. }) => {
            json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_password", &e.to_string())
        },
        Err(e) => {
            get_audit_logger().log_failure(
                AuditEventType::AuthFailure,
                SecretType::CsrfToken,
                None,
                "password_reset_confirm",
                &e.to_string(),
            );
            json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_token",
                "invalid, expired, or already-used reset token",
            )
        },
    }
}

// ─── Email verification (#945) ────────────────────────────────────────────────

/// Axum state for the email-verification routes.
///
/// Separate from [`LocalPasswordRouteState`] because the group mounts on its own
/// `[auth.local] email_verification` switch and needs one thing the password routes
/// do not: a way to learn *who* is calling.
#[derive(Clone)]
pub struct EmailVerificationRouteState {
    /// The authenticator (owns the verification token store and the promotion gate).
    pub authenticator:  Arc<LocalPasswordAuthenticator>,
    /// Resolves the caller's `user_id` from the request's bearer session token.
    pub session_bearer: Arc<SessionBearerAuthenticator>,
    /// Per-IP limiters; the `auth_start` bucket governs both routes.
    pub rate_limiters:  Arc<RateLimiters>,
}

/// Body of `POST /auth/v1/email/verify/confirm`.
#[derive(Debug, Deserialize)]
pub struct EmailVerifyConfirmRequest {
    /// The opaque token from the verification link.
    pub token: String,
}

/// Build the email-verification route group.
#[allow(clippy::missing_panics_doc)] // Reason: infallible — no path capture syntax to reject
pub fn email_verification_routes(state: Arc<EmailVerificationRouteState>) -> Router {
    Router::new()
        .route("/auth/v1/email/verify/start", post(email_verify_start))
        .route("/auth/v1/email/verify/confirm", post(email_verify_confirm))
        .with_state(state)
}

/// Enforce the per-IP `auth_start` budget for the verification group.
fn verify_rate_limit(
    state: &EmailVerificationRouteState,
    addr: SocketAddr,
    op: &str,
) -> Option<Response> {
    let client_ip = addr.ip().to_string();
    if state.rate_limiters.auth_start.check(&client_ip).is_ok() {
        return None;
    }
    let retry_after = state.rate_limiters.auth_start.clone_config().window_secs;
    get_audit_logger().log_failure(
        AuditEventType::AuthFailure,
        SecretType::SessionToken,
        None,
        op,
        "rate limited",
    );
    Some(
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
            Json(serde_json::json!({
                "error":   "rate_limited",
                "message": "Too many attempts; please retry later"
            })),
        )
            .into_response(),
    )
}

/// Resolve the caller, or render the `401`.
// Reason: the boxed `Response` keeps the Err variant small (clippy::result_large_err).
fn caller(
    state: &EmailVerificationRouteState,
    headers: &HeaderMap,
    op: &str,
) -> Result<String, Box<Response>> {
    state.session_bearer.subject(headers).map_err(|e| {
        get_audit_logger().log_failure(
            AuditEventType::AuthFailure,
            SecretType::SessionToken,
            None,
            op,
            &e.to_string(),
        );
        Box::new(
            (
                StatusCode::UNAUTHORIZED,
                [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
                Json(serde_json::json!({
                    "error":   "unauthenticated",
                    "message": "a valid session is required"
                })),
            )
                .into_response(),
        )
    })
}

/// `POST /auth/v1/email/verify/start`
///
/// Mails a verification link to the address the caller's own local identity claims.
/// The address is never taken from the request, so this cannot aim a mail at an
/// address the account does not already claim.
///
/// **Always** `202 Accepted` for an authenticated caller: an account with no local
/// identity, one already verified, and one that was just sent a link are
/// indistinguishable.
///
/// # Errors
///
/// `401` without a valid session, `429` when rate-limited. Infrastructure failures
/// are logged and still return `202`.
pub async fn email_verify_start(
    State(state): State<Arc<EmailVerificationRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(limited) = verify_rate_limit(&state, addr, "email_verify_start") {
        return limited;
    }
    let user_id = match caller(&state, &headers, "email_verify_start") {
        Ok(user_id) => user_id,
        Err(unauthorized) => return *unauthorized,
    };
    if let Err(e) = state.authenticator.start_email_verification(&user_id).await {
        tracing::error!(error = %e, "email verification start failed");
    }
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "If this account has an unverified address, a verification link has been \
                        sent to it."
        })),
    )
        .into_response()
}

/// `POST /auth/v1/email/verify/confirm`
///
/// Redeems a verification token **for the calling account**. Both halves are
/// required: the token proves control of the mailbox, the session proves ownership
/// of the account. A token issued to a different account is rejected exactly like a
/// forged one.
///
/// # Errors
///
/// `401` without a valid session; `409` when the address is already verified on
/// another account (the merge refusal, #945); `422` for an invalid, expired or
/// already-used token; `429` when rate-limited.
pub async fn email_verify_confirm(
    State(state): State<Arc<EmailVerificationRouteState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<EmailVerifyConfirmRequest>,
) -> Response {
    if let Some(limited) = verify_rate_limit(&state, addr, "email_verify_confirm") {
        return limited;
    }
    let user_id = match caller(&state, &headers, "email_verify_confirm") {
        Ok(user_id) => user_id,
        Err(unauthorized) => return *unauthorized,
    };
    match state.authenticator.confirm_email_verification(&user_id, &req.token).await {
        Ok(verified) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "verified": true,
                "email":    verified.email,
                "user_id":  verified.user_id,
            })),
        )
            .into_response(),
        // Reporting this plainly is safe: reaching it required proving control of the
        // address, so it discloses nothing the caller had not already established.
        Err(e @ AuthError::EmailClaimedByAnotherAccount) => {
            json_error(StatusCode::CONFLICT, "email_claimed_by_another_account", &e.to_string())
        },
        Err(e) => {
            get_audit_logger().log_failure(
                AuditEventType::AuthFailure,
                SecretType::CsrfToken,
                Some(user_id),
                "email_verify_confirm",
                &e.to_string(),
            );
            json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_token",
                "invalid, expired, or already-used verification token",
            )
        },
    }
}

#[cfg(test)]
mod routes_tests;
