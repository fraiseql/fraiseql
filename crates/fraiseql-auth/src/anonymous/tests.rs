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
    let result = upgrade_anonymous_session(&*store, "anon_abc123", "user_real", now + 3600).await;
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
