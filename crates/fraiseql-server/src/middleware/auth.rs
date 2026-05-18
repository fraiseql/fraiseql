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
    // Derive a best-effort peer key for rate limiting.
    // ConnectInfo is only available when the server was started with
    // `into_make_service_with_connect_info`; fall back to a header-based key.
    use std::net::SocketAddr;

    use axum::extract::ConnectInfo;
    let peer_key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

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
