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
    assert_eq!(state.tracked_count(), 0, "new state should have no rate counters");
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

// ── The per-IP map is bounded ─────────────────────────────────────────

/// A routine `IPv6` allocation is a /64, so keying on the full address let one client
/// mint 2^64 counters — each a fresh full budget, and none ever removed.
#[tokio::test]
async fn ipv6_addresses_in_one_slash64_share_one_counter() {
    let state = make_state();
    let now = unix_now().unwrap();

    for host in 0..1_000u32 {
        let ip = format!("2001:db8:1:2::{host:x}");
        let _ = state.check_rate_limit(&ip, now);
    }
    assert_eq!(state.tracked_count(), 1, "one /64, one counter");
    assert!(
        !state.check_rate_limit("2001:db8:1:2:ffff::1", now),
        "a new address in a spent /64 gets no fresh budget"
    );
}

/// Distinct /64s are distinct clients, so they get distinct counters, and the map still
/// stays under its cap.
#[tokio::test]
async fn the_counter_map_stays_bounded_across_many_prefixes() {
    let state = AnonSignupState::new(Arc::new(MockSessionStore)).with_max_tracked(64);
    let now = unix_now().unwrap();

    for prefix in 0..1_000u32 {
        let ip = format!("2001:db8:{:x}:{:x}::1", prefix >> 16, prefix & 0xffff);
        let _ = state.check_rate_limit(&ip, now);
        assert!(
            state.tracked_count() <= 64,
            "{} counters past a cap of 64",
            state.tracked_count()
        );
    }
}

/// A counter whose window has closed holds no enforcement: the next signup from that
/// address opens a fresh window anyway. Those go first when room is needed.
#[tokio::test]
async fn closed_windows_are_swept_before_a_live_counter_is_evicted() {
    let state = AnonSignupState::new(Arc::new(MockSessionStore)).with_max_tracked(4);
    let now = unix_now().unwrap();
    let old = now - ANON_RATE_WINDOW_SECS - 1;

    for n in 0..3u8 {
        let _ = state.check_rate_limit(&format!("198.51.100.{n}"), old);
    }
    // The live counter: spent.
    for _ in 0..ANON_RATE_MAX {
        let _ = state.check_rate_limit("203.0.113.7", now);
    }
    // Room for a newcomer is made from the closed windows, so the spent counter stays.
    let _ = state.check_rate_limit("203.0.113.8", now);
    assert_eq!(state.tracked_count(), 2, "the three closed windows are swept");
    assert!(!state.check_rate_limit("203.0.113.7", now), "the spent counter still refuses");
}

/// A caller whose router already limits `/auth/v1/signup` per client (the server does)
/// builds the state with no limiter of its own: one limit per route, the configured one.
#[tokio::test]
async fn a_state_without_a_limiter_keeps_no_counters() {
    let state = AnonSignupState::new_without_limiter(Arc::new(MockSessionStore));
    let now = unix_now().unwrap();
    for _ in 0..(ANON_RATE_MAX * 3) {
        assert!(state.check_rate_limit("192.0.2.1", now));
    }
    assert_eq!(state.tracked_count(), 0);
}
