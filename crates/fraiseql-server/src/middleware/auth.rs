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
    pub(crate) fn new(max_failures: u32) -> Self {
        Self {
            records: Arc::new(DashMap::new()),
            max_failures,
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }

    /// Record a failed attempt and return `true` if the IP is now rate-limited.
    pub(crate) fn record_failure(&self, ip: &str) -> bool {
        let now = Self::now_secs();
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

#[cfg(test)]
mod xff_tests {
    //! M-xff-limiter: the brute-force limiter must not key on the attacker-controlled
    //! `X-Forwarded-For` header — rotating it must not grant a fresh failure budget.
    #![allow(clippy::unwrap_used)]

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
