//! Authentication middleware.
//!
//! Provides bearer token authentication for protected endpoints.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use subtle::ConstantTimeEq as _;

/// Window length (in seconds) for the admin brute-force rate limiter.
const ADMIN_AUTH_WINDOW_SECS: u64 = 60;

/// Per-IP failure record for the admin brute-force guard.
#[derive(Clone)]
struct FailureRecord {
    count:        u32,
    window_start: u64,
}

/// Per-IP sliding-window counter for failed bearer token attempts.
///
/// Shared inside `BearerAuthState` via an `Arc`-wrapped `DashMap` so that
/// the state can be `Clone`d cheaply across requests.
#[derive(Clone)]
pub(crate) struct FailureLimiter {
    records:      Arc<DashMap<String, FailureRecord>>,
    max_failures: u32,
}

impl FailureLimiter {
    /// The window length, exposed so tests age records by a real multiple of it.
    #[cfg(test)]
    pub(super) const WINDOW_SECS: u64 = ADMIN_AUTH_WINDOW_SECS;

    pub(crate) fn new(max_failures: u32) -> Self {
        Self {
            records: Arc::new(DashMap::new()),
            max_failures,
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }

    /// Drop records whose window expired long enough ago to be irrelevant.
    ///
    /// The map is keyed by client IP and only ever had entries removed on a
    /// *successful* auth, so a stream of failed attempts from changing source
    /// addresses grew it without bound — a slow memory leak reachable by any
    /// unauthenticated caller (#731). Sweeping is amortised: it runs on insert,
    /// and only once the map is large enough for the scan to be worth it.
    pub(super) fn evict_expired(&self, now: u64) {
        /// Only sweep once the map is big enough that a scan is worth its cost.
        const EVICTION_THRESHOLD: usize = 1024;
        /// Keep an expired window around for one further window, so a burst that
        /// straddles the boundary is still counted against the same IP.
        const RETENTION_WINDOWS: u64 = 2;

        if self.records.len() < EVICTION_THRESHOLD {
            return;
        }
        let cutoff = ADMIN_AUTH_WINDOW_SECS * RETENTION_WINDOWS;
        self.records
            .retain(|_, record| now.saturating_sub(record.window_start) < cutoff);
    }

    /// Record a failed attempt and return `true` if the IP is now rate-limited.
    pub(crate) fn record_failure(&self, ip: &str) -> bool {
        let now = Self::now_secs();
        // Before inserting a potentially new key, drop the dead ones.
        self.evict_expired(now);
        let mut entry = self.records.entry(ip.to_string()).or_insert_with(|| FailureRecord {
            count:        0,
            window_start: now,
        });

        if now >= entry.window_start + ADMIN_AUTH_WINDOW_SECS {
            // Window expired — start fresh
            entry.count = 1;
            entry.window_start = now;
            false
        } else {
            entry.count = entry.count.saturating_add(1);
            entry.count >= self.max_failures
        }
    }

    /// Return `true` if the IP is already rate-limited (without recording a new failure).
    pub(crate) fn is_blocked(&self, ip: &str) -> bool {
        let now = Self::now_secs();
        if let Some(entry) = self.records.get(ip) {
            if now < entry.window_start + ADMIN_AUTH_WINDOW_SECS {
                return entry.count >= self.max_failures;
            }
        }
        false
    }

    /// Reset the failure counter for an IP after a successful authentication.
    pub(crate) fn record_success(&self, ip: &str) {
        self.records.remove(ip);
    }

    /// Return the current failure count for an IP (used in tests).
    #[cfg(test)]
    pub(crate) fn failure_count(&self, ip: &str) -> u32 {
        self.records.get(ip).map_or(0, |e| e.count)
    }

    /// Number of per-IP records held (used in tests to pin eviction).
    #[cfg(test)]
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Record a failure with an explicit clock reading (used in tests to age
    /// records without sleeping).
    #[cfg(test)]
    pub(super) fn record_failure_at(&self, ip: &str, now: u64) {
        self.records.insert(
            ip.to_string(),
            FailureRecord {
                count:        1,
                window_start: now,
            },
        );
    }
}

/// Shared state for bearer token authentication.
#[derive(Clone)]
pub struct BearerAuthState {
    /// Expected bearer token.
    pub token:       Arc<String>,
    /// Per-IP brute-force guard.
    failure_limiter: FailureLimiter,
}

impl BearerAuthState {
    /// Create new bearer auth state with the default max-failures limit (10).
    #[must_use]
    pub fn new(token: String) -> Self {
        Self::with_max_failures(token, 10)
    }

    /// Create new bearer auth state with a custom max-failures limit.
    ///
    /// After `max_failures` failed attempts from the same IP within a 60-second
    /// window, further requests receive **429 Too Many Requests**.
    #[must_use]
    pub fn with_max_failures(token: String, max_failures: u32) -> Self {
        Self {
            token:           Arc::new(token),
            failure_limiter: FailureLimiter::new(max_failures),
        }
    }
}

/// Bearer token authentication middleware.
///
/// Validates that requests include a valid `Authorization: Bearer <token>` header.
///
/// # Response
///
/// - **401 Unauthorized**: Missing or malformed Authorization header
/// - **403 Forbidden**: Invalid token
///
/// # Example
///
/// ```text
/// // Requires: running Axum application with a route handler.
/// use axum::{Router, middleware};
/// use fraiseql_server::middleware::{bearer_auth_middleware, BearerAuthState};
///
/// let auth_state = BearerAuthState::new("my-secret-token".to_string());
///
/// let app = Router::new()
///     .route("/protected", get(handler))
///     .layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
/// ```
pub async fn bearer_auth_middleware(
    State(auth_state): State<BearerAuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Derive the peer key for the brute-force limiter from the validated transport peer
    // only. ConnectInfo is the real socket address (present in the shipped binary, which
    // starts with `into_make_service_with_connect_info`). We deliberately do NOT fall back
    // to `X-Forwarded-For`: that header is attacker-controlled, so keying on it would let a
    // caller rotate it to mint a fresh failure budget per value, defeating the limiter
    // (M-xff-limiter). When ConnectInfo is absent (some library embeddings), all callers
    // share the single `unknown` bucket — fail-closed (more restrictive), not bypassable.
    use std::net::SocketAddr;

    use axum::extract::ConnectInfo;
    let peer_key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_string(), |ci| ci.0.ip().to_string());

    // Reject immediately if already rate-limited (avoids any header work).
    if auth_state.failure_limiter.is_blocked(&peer_key) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many failed auth attempts").into_response();
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    match auth_header {
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                "Missing Authorization header",
            )
                .into_response();
        },
        Some(header_value) => {
            // Check for "Bearer " prefix
            if !header_value.starts_with("Bearer ") {
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "Bearer")],
                    "Invalid Authorization header format. Expected: Bearer <token>",
                )
                    .into_response();
            }

            // Extract token
            let token = &header_value[7..]; // Skip "Bearer "

            // Constant-time comparison to prevent timing attacks
            if !constant_time_compare(token, &auth_state.token) {
                // Record failure; return 429 once the limit is crossed.
                if auth_state.failure_limiter.record_failure(&peer_key) {
                    return (StatusCode::TOO_MANY_REQUESTS, "Too many failed auth attempts")
                        .into_response();
                }
                return (StatusCode::FORBIDDEN, "Invalid token").into_response();
            }

            // Successful auth — reset the failure counter.
            auth_state.failure_limiter.record_success(&peer_key);
        },
    }

    // Token is valid, proceed with request
    next.run(request).await
}

/// Extract the bearer token from an `Authorization` header value.
///
/// Returns `Some(token)` if the header has the `Bearer ` prefix (with trailing space),
/// `None` for all other formats (Basic, Digest, missing prefix, etc.).
///
/// Exposed as `pub` for property testing.
#[must_use]
pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

/// Constant-time string comparison to prevent timing attacks.
///
/// Uses [`subtle::ConstantTimeEq`] to compare the byte representations of
/// both strings, preventing the compiler from optimising the comparison into
/// an early-exit branch that would leak information about where the strings
/// differ (timing oracle, RFC 6749 §10.12).
///
/// Strings of different lengths return `false` without inspecting bytes;
/// token lengths are considered non-secret (administrators choose them).
pub(crate) fn constant_time_compare(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Which admin credential authenticated a request to the SQL console (#962).
///
/// Every other admin route is on one of two routers, each authenticated by one
/// token, so "which token" is answered by which router matched. The console is a
/// single route that both tokens may reach and that must behave *differently* for
/// each, so the answer has to travel from the middleware to the handler. It does
/// so as a request extension the middleware inserts and nothing else constructs
/// — a client cannot supply it, because it is a Rust type and not a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminPrivilege {
    /// Authenticated by `admin_readonly_token`. The transaction runs `READ ONLY`.
    ReadOnly,
    /// Authenticated by `admin_token`. Writes are permitted; committing is still
    /// opt-in per request.
    ReadWrite,
}

/// What [`admin_dual_auth_middleware`] establishes about a request.
///
/// Inserted as a request extension on success and constructed nowhere else, so a
/// handler reading it is reading the middleware's conclusion rather than
/// re-deriving one. The peer address travels with the privilege because the
/// middleware has already resolved it (from `ConnectInfo`, never from
/// `X-Forwarded-For`), and both belong in the same audit record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminCaller {
    /// Which admin credential authenticated.
    pub privilege: AdminPrivilege,
    /// The transport peer's IP, or `"unknown"` when there is no `ConnectInfo`.
    pub peer_ip:   String,
}

/// Shared state for the dual-token admin authentication used by the SQL console.
///
/// Holds both tokens and **one** failure limiter, deliberately: the limiter
/// counts failed attempts per peer, and two limiters would let a caller spend a
/// fresh budget by guessing against each token in turn.
#[derive(Clone)]
pub struct AdminDualAuthState {
    write_token:     Arc<String>,
    readonly_token:  Option<Arc<String>>,
    failure_limiter: FailureLimiter,
}

impl AdminDualAuthState {
    /// Build the state from the two configured tokens.
    ///
    /// `readonly_token` is `None` in single-token mode, where `admin_token`
    /// grants everything — the console then has no read-only credential at all,
    /// which is the honest reading of "one token, all operations" and matches how
    /// the rest of the admin API already behaves.
    #[must_use]
    pub fn new(write_token: String, readonly_token: Option<String>, max_failures: u32) -> Self {
        Self {
            write_token:     Arc::new(write_token),
            readonly_token:  readonly_token.map(Arc::new),
            failure_limiter: FailureLimiter::new(max_failures),
        }
    }

    /// Classify a presented token.
    ///
    /// Both comparisons always run: returning early on the write-token match
    /// would make the response time depend on which token was presented, and the
    /// whole point of [`constant_time_compare`] is that it does not.
    fn classify(&self, presented: &str) -> Option<AdminPrivilege> {
        let is_write = constant_time_compare(presented, &self.write_token);
        let is_readonly = self
            .readonly_token
            .as_ref()
            .is_some_and(|t| constant_time_compare(presented, t));
        match (is_write, is_readonly) {
            (true, _) => Some(AdminPrivilege::ReadWrite),
            (false, true) => Some(AdminPrivilege::ReadOnly),
            (false, false) => None,
        }
    }
}

/// Bearer authentication that reports *which* admin token authenticated (#962).
///
/// Same refusals and the same per-peer brute-force guard as
/// [`bearer_auth_middleware`]; the difference is that a success inserts an
/// [`AdminPrivilege`] into the request extensions instead of discarding the
/// distinction. Used only by the SQL console, whose behaviour depends on it.
///
/// # Response
///
/// - **401 Unauthorized**: missing or malformed `Authorization` header
/// - **403 Forbidden**: the token matches neither admin credential
/// - **429 Too Many Requests**: too many failures from this peer
pub async fn admin_dual_auth_middleware(
    State(auth_state): State<AdminDualAuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    use std::net::SocketAddr;

    use axum::extract::ConnectInfo;

    // Keyed on the transport peer only, never on `X-Forwarded-For` — see
    // `bearer_auth_middleware` for why (M-xff-limiter).
    let peer_key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_string(), |ci| ci.0.ip().to_string());

    if auth_state.failure_limiter.is_blocked(&peer_key) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many failed auth attempts").into_response();
    }

    let Some(header_value) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "Missing Authorization header",
        )
            .into_response();
    };

    let Some(token) = extract_bearer_token(header_value) else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "Invalid Authorization header format. Expected: Bearer <token>",
        )
            .into_response();
    };

    let Some(privilege) = auth_state.classify(token) else {
        if auth_state.failure_limiter.record_failure(&peer_key) {
            return (StatusCode::TOO_MANY_REQUESTS, "Too many failed auth attempts")
                .into_response();
        }
        return (StatusCode::FORBIDDEN, "Invalid token").into_response();
    };

    auth_state.failure_limiter.record_success(&peer_key);
    request.extensions_mut().insert(AdminCaller {
        privilege,
        peer_ip: peer_key,
    });
    next.run(request).await
}

#[cfg(test)]
mod xff_tests {
    //! M-xff-limiter: the brute-force limiter must not key on the attacker-controlled
    //! `X-Forwarded-For` header — rotating it must not grant a fresh failure budget.
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt as _;

    use super::{BearerAuthState, bearer_auth_middleware};

    async fn protected() -> &'static str {
        "ok"
    }

    fn wrong_token_request(xff: &str) -> Request<Body> {
        Request::builder()
            .uri("/")
            .header("authorization", "Bearer wrong-token")
            .header("x-forwarded-for", xff)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn rotating_x_forwarded_for_does_not_refresh_the_failure_budget() {
        let state = BearerAuthState::with_max_failures("correct-token".to_string(), 2);
        let app = Router::new()
            .route("/", get(protected))
            .layer(middleware::from_fn_with_state(state, bearer_auth_middleware));

        // A oneshot sets no ConnectInfo, so the peer key falls back to "unknown" for every
        // request. Each failed attempt carries a DIFFERENT X-Forwarded-For: if the limiter
        // keyed on it, each value would get its own budget and never block. With the XFF
        // fallback removed they share the single "unknown" bucket, so the limit is reached.
        let mut statuses = Vec::new();
        for i in 0..5 {
            let resp = app
                .clone()
                .oneshot(wrong_token_request(&format!("203.0.113.{i}")))
                .await
                .unwrap();
            statuses.push(resp.status());
        }

        assert!(
            statuses.contains(&StatusCode::TOO_MANY_REQUESTS),
            "rotating X-Forwarded-For must still hit the shared rate limit, got {statuses:?}"
        );
    }
}
