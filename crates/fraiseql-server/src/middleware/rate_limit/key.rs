//! Rate limit key construction and path matching helpers.

/// Build a namespaced rate-limiting key for use in both in-memory and Redis backends.
///
/// Format: `fraiseql:rl:{strategy}:{identifier}` for simple strategies, or
/// `fraiseql:rl:{strategy}:{prefix}:{identifier}` when an optional path prefix is supplied.
///
/// Exposed as `pub` for property testing.
pub fn build_rate_limit_key(strategy: &str, identifier: &str, prefix: Option<&str>) -> String {
    match prefix {
        Some(p) => format!("fraiseql:rl:{strategy}:{p}:{identifier}"),
        None => format!("fraiseql:rl:{strategy}:{identifier}"),
    }
}

/// Returns `true` if `ip` is a loopback or RFC 1918 private address.
///
/// Used to warn operators that rate limiting may be inoperative when running
/// behind a reverse proxy without `trust_proxy_headers = true`.
pub(super) const fn is_private_or_loopback(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Returns `true` if `path` is governed by the rule whose canonical prefix is
/// `prefix`.
///
/// Requires that `path` equals `prefix` exactly, or that it is followed
/// immediately by `/` or `?`. This prevents `/auth/start` from matching
/// `/auth/startover` (DoS vector: exhausting the `/auth/start` bucket via an
/// unrelated path).
pub(super) fn path_matches_rule(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    let rest = match path.strip_prefix(prefix) {
        Some(r) => r,
        None => return false,
    };
    rest.starts_with('/') || rest.starts_with('?')
}

/// A per-path rate limit rule, derived from `[security.rate_limiting]` auth endpoint fields.
#[derive(Debug, Clone)]
pub(super) struct PathRateLimit {
    /// Path prefix to match (exact prefix, e.g., `/auth/start`).
    pub(super) path_prefix:    String,
    /// Token refill rate (tokens per second = max_requests / window_secs).
    pub(super) tokens_per_sec: f64,
    /// Maximum burst (= max_requests).
    pub(super) burst:          f64,
}
