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
//! - Rate-limited to `ANON_RATE_MAX` signups per client per `ANON_RATE_WINDOW_SECS`, unless built
//!   with [`AnonSignupState::new_without_limiter`] behind a router that limits the route itself. A
//!   client is an `IPv4` address or an `IPv6` /64, the block one subscriber is routinely given; the
//!   counters are capped at `ANON_MAX_TRACKED`, closed windows going first.
//! - Each anonymous `user_id` is a `UUIDv4` with an `anon_` prefix (unpredictable).

use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

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

/// Maximum anonymous signups per client per rate-limit window.
const ANON_RATE_MAX: u32 = 10;

/// Most per-client counters held at once.
const ANON_MAX_TRACKED: usize = 100_000;

/// Counters examined to choose each live one to evict once the closed windows are gone.
const EVICTION_SAMPLE: usize = 32;

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
    /// Per-client signup counters; `None` when the router limits the route instead.
    rate_counters:     Option<Arc<DashMap<String, RateRecord>>>,
    /// Most counters held at once.
    max_tracked:       usize,
}

impl AnonSignupState {
    /// Create a new signup state wrapping the given session store, with its own
    /// per-client limiter.
    #[must_use]
    pub fn new(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            rate_counters: Some(Arc::new(DashMap::new())),
            max_tracked: ANON_MAX_TRACKED,
        }
    }

    /// A state with no limiter of its own, for a router that already limits
    /// `POST /auth/v1/signup` per client. Two limiters on one route would leave the
    /// configured budget unenforceable above the tighter one.
    #[must_use]
    pub const fn new_without_limiter(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            rate_counters: None,
            max_tracked: ANON_MAX_TRACKED,
        }
    }

    /// Cap the number of counters held at once.
    #[must_use]
    pub const fn with_max_tracked(mut self, max_tracked: usize) -> Self {
        self.max_tracked = max_tracked;
        self
    }

    /// How many per-client counters the state holds.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.rate_counters.as_ref().map_or(0, |counters| counters.len())
    }

    /// Check whether the client at `ip` is within the rate limit.
    ///
    /// Increments the counter if allowed; returns `false` if the limit is exceeded.
    /// Always `true` for a state without a limiter.
    fn check_rate_limit(&self, ip: &str, now: u64) -> bool {
        let Some(counters) = &self.rate_counters else {
            return true;
        };
        let key = client_key(ip);
        if !counters.contains_key(&key) && counters.len() >= self.max_tracked {
            self.make_room(counters, now);
        }
        let mut record = counters.entry(key).or_insert(RateRecord {
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

    /// Free room for a new counter: drop every closed window, which enforces nothing,
    /// then, if the map is still full, the oldest live counters from a sample. Evicting
    /// down to `15/16` of the cap spreads the full scan over many inserts.
    fn make_room(&self, counters: &DashMap<String, RateRecord>, now: u64) {
        counters.retain(|_, r| now.saturating_sub(r.window_start) < ANON_RATE_WINDOW_SECS);
        let target = self.max_tracked - (self.max_tracked / 16).max(1);
        while counters.len() > target {
            let oldest = counters
                .iter()
                .take(EVICTION_SAMPLE)
                .min_by_key(|r| r.window_start)
                .map(|r| r.key().clone());
            match oldest {
                Some(key) => {
                    counters.remove(&key);
                },
                None => break,
            }
        }
    }
}

/// The counter key for a client: an `IPv4` address (mapped ones included) as is, an
/// `IPv6` address by its /64.
/// An address the handler did not produce (it never fails to parse) keys as written.
fn client_key(ip: &str) -> String {
    match ip.parse::<IpAddr>().map(|addr| addr.to_canonical()) {
        Ok(IpAddr::V6(v6)) => {
            let prefix = u128::from(v6) & !u128::from(u64::MAX);
            format!("{}/64", Ipv6Addr::from(prefix))
        },
        Ok(v4) => v4.to_string(),
        Err(_) => ip.to_string(),
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
/// Returns `429 Too Many Requests` if the per-client rate limit is exceeded.
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
mod tests;
