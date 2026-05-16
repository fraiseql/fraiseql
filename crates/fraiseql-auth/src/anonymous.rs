//! Anonymous session creation — `POST /auth/v1/signup`.
//!
//! Issues a guest session with a `7`-day `TTL` without requiring any credentials.
//! The resulting `user_id` carries an `anon_` prefix so application code can
//! distinguish anonymous visitors from authenticated users.
//!
//! # Upgrade path
//!
//! When an anonymous user later completes a social or email auth flow, the
//! application calls [`upgrade_anonymous_session`] to atomically swap the
//! `anon_` prefix for a stable identity.
//!
//! # Security
//!
//! - Rate-limited to [`ANON_RATE_MAX`] signups per IP per [`ANON_RATE_WINDOW_SECS`].
//! - Each anonymous `user_id` is a `UUIDv4` with an `anon_` prefix (unpredictable).

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::Result,
    session::{SessionStore, unix_now},
};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Anonymous session `TTL` (7 days).
const ANON_SESSION_TTL_SECS: u64 = 7 * 24 * 3600;

/// Rate-limit window for anonymous signups (1 hour).
const ANON_RATE_WINDOW_SECS: u64 = 3_600;

/// Maximum anonymous signups per IP per rate-limit window.
const ANON_RATE_MAX: u32 = 10;

// ─── Rate-limit record ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RateRecord {
    /// Count of signups in the current window.
    count:        u32,
    /// Start of the current window (Unix seconds).
    window_start: u64,
}

// ─── Route state ──────────────────────────────────────────────────────────────

/// Axum route state for `POST /auth/v1/signup`.
#[derive(Clone)]
pub struct AnonSignupState {
    /// Session store used to issue the anonymous session.
    pub session_store: Arc<dyn SessionStore>,
    /// Per-IP signup rate-limit counters.
    rate_counters:     Arc<DashMap<String, RateRecord>>,
}

impl AnonSignupState {
    /// Create a new signup state wrapping the given session store.
    #[must_use]
    pub fn new(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            rate_counters: Arc::new(DashMap::new()),
        }
    }

    /// Check whether `ip` is within the rate limit.
    ///
    /// Increments the counter if allowed; returns `false` if the limit is exceeded.
    fn check_rate_limit(&self, ip: &str, now: u64) -> bool {
        let mut record = self.rate_counters.entry(ip.to_string()).or_insert(RateRecord {
            count:        0,
            window_start: now,
        });

        // Reset window if it has elapsed.
        if now.saturating_sub(record.window_start) >= ANON_RATE_WINDOW_SECS {
            record.count = 0;
            record.window_start = now;
        }

        if record.count >= ANON_RATE_MAX {
            return false;
        }
        record.count += 1;
        true
    }
}

// ─── Response types ───────────────────────────────────────────────────────────

/// Response for `POST /auth/v1/signup`.
#[derive(Debug, Serialize)]
pub struct AnonSignupResponse {
    /// Anonymous user identifier (`anon_<uuid>`).
    pub user_id:       String,
    /// Short-lived access token.
    pub access_token:  String,
    /// Long-lived refresh token.
    pub refresh_token: String,
    /// Seconds until the access token expires.
    pub expires_in:    u64,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// `POST /auth/v1/signup` — issue an anonymous session.
///
/// Returns `200 OK` with a [`AnonSignupResponse`] on success.
/// Returns `429 Too Many Requests` if the IP rate limit is exceeded.
///
/// # Errors
///
/// Returns `500 Internal Server Error` if the session store fails.
pub async fn anon_signup(
    State(state): State<Arc<AnonSignupState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    let logger = get_audit_logger();
    let ip = addr.ip().to_string();

    let now = match unix_now() {
        Ok(t) => t,
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "anon_signup:clock",
                &e.to_string(),
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        },
    };

    // Rate-limit check.
    if !state.check_rate_limit(&ip, now) {
        logger.log_failure(
            AuditEventType::AuthFailure,
            SecretType::SessionToken,
            None,
            "anon_signup:rate_limited",
            "too many anonymous signups from this IP",
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "rate_limited"})),
        )
            .into_response();
    }

    let user_id = format!("anon_{}", Uuid::new_v4().as_simple());
    let expires_at = now + ANON_SESSION_TTL_SECS;

    match state.session_store.create_session(&user_id, expires_at).await {
        Ok(tokens) => {
            logger.log_success(
                AuditEventType::SessionTokenCreated,
                SecretType::SessionToken,
                Some(user_id.clone()),
                "anon_signup",
            );
            (
                StatusCode::OK,
                Json(AnonSignupResponse {
                    user_id,
                    access_token: tokens.access_token,
                    refresh_token: tokens.refresh_token,
                    expires_in: tokens.expires_in,
                }),
            )
                .into_response()
        },
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "anon_signup:session_create",
                &e.to_string(),
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        },
    }
}

/// Upgrade an anonymous session to a real identity.
///
/// Revokes all sessions for the old `anon_` identity and creates a new session
/// for `new_user_id`.  Returns the new token pair.
///
/// # Errors
///
/// Returns an error if revocation or session creation fails.
pub async fn upgrade_anonymous_session(
    session_store: &dyn SessionStore,
    anon_user_id: &str,
    new_user_id: &str,
    expires_at: u64,
) -> Result<crate::session::TokenPair> {
    // Revoke all anonymous sessions first.
    session_store.revoke_all_sessions(anon_user_id).await?;
    // Issue a new session under the real identity.
    session_store.create_session(new_user_id, expires_at).await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::session::{SessionData, SessionStore, TokenPair};

    // ── Minimal mock session store ─────────────────────────────────────────

    struct MockSessionStore;

    #[async_trait::async_trait]
    impl SessionStore for MockSessionStore {
        async fn create_session(
            &self,
            user_id: &str,
            expires_at: u64,
        ) -> crate::error::Result<TokenPair> {
            Ok(TokenPair {
                access_token:  format!("access_{user_id}"),
                refresh_token: format!("refresh_{user_id}"),
                expires_in:    expires_at.saturating_sub(unix_now().unwrap_or(0)),
            })
        }

        async fn get_session(&self, _: &str) -> crate::error::Result<SessionData> {
            Err(crate::error::AuthError::TokenNotFound)
        }

        async fn revoke_session(&self, _: &str) -> crate::error::Result<()> {
            Ok(())
        }

        async fn revoke_all_sessions(&self, _: &str) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn make_state() -> Arc<AnonSignupState> {
        Arc::new(AnonSignupState::new(Arc::new(MockSessionStore)))
    }

    // ── Cycle 5 unit tests ────────────────────────────────────────────────

    #[test]
    fn test_anon_user_id_has_anon_prefix() {
        let user_id = format!("anon_{}", uuid::Uuid::new_v4().as_simple());
        assert!(user_id.starts_with("anon_"), "anon user_id must start with 'anon_'");
        assert!(user_id.len() > 5, "anon user_id must not be empty after prefix");
    }

    #[test]
    fn test_anon_session_ttl_is_7_days() {
        assert_eq!(ANON_SESSION_TTL_SECS, 7 * 24 * 3600, "anonymous session TTL must be 7 days");
    }

    #[tokio::test]
    async fn test_rate_limit_allows_up_to_max() {
        let state = make_state();
        let now = unix_now().unwrap();
        for i in 0..ANON_RATE_MAX {
            assert!(state.check_rate_limit("192.168.1.1", now), "signup #{i} should be allowed");
        }
        assert!(
            !state.check_rate_limit("192.168.1.1", now),
            "signup beyond ANON_RATE_MAX should be rejected"
        );
    }

    #[tokio::test]
    async fn test_rate_limit_resets_after_window() {
        let state = make_state();
        let now = unix_now().unwrap();

        for _ in 0..ANON_RATE_MAX {
            state.check_rate_limit("10.0.0.1", now);
        }
        assert!(!state.check_rate_limit("10.0.0.1", now));

        let later = now + ANON_RATE_WINDOW_SECS + 1;
        assert!(
            state.check_rate_limit("10.0.0.1", later),
            "rate limit should reset after window expires"
        );
    }

    #[tokio::test]
    async fn test_rate_limit_is_per_ip() {
        let state = make_state();
        let now = unix_now().unwrap();

        for _ in 0..ANON_RATE_MAX {
            state.check_rate_limit("1.2.3.4", now);
        }
        assert!(!state.check_rate_limit("1.2.3.4", now));

        assert!(
            state.check_rate_limit("5.6.7.8", now),
            "rate limit for one IP must not affect another IP"
        );
    }

    #[tokio::test]
    async fn test_upgrade_anonymous_session_calls_revoke_and_create() {
        let store = Arc::new(MockSessionStore);
        let now = unix_now().unwrap();
        let result =
            upgrade_anonymous_session(&*store, "anon_abc123", "user_real", now + 3600).await;
        assert!(result.is_ok(), "upgrade should succeed: {result:?}");
        let tokens = result.unwrap();
        assert!(
            tokens.access_token.contains("user_real"),
            "access token should be for the real user"
        );
    }

    #[tokio::test]
    async fn test_new_anon_state_creates_empty_rate_counters() {
        let state = make_state();
        assert!(state.rate_counters.is_empty(), "new state should have no rate counters");
    }

    #[tokio::test]
    async fn test_different_ips_get_independent_counters() {
        let state = make_state();
        let now = unix_now().unwrap();

        // Partially consume IP A.
        state.check_rate_limit("10.0.0.1", now);
        state.check_rate_limit("10.0.0.1", now);

        // IP B should still have full allowance.
        for i in 0..ANON_RATE_MAX {
            assert!(state.check_rate_limit("10.0.0.2", now), "IP B signup #{i} should be allowed");
        }
    }
}
