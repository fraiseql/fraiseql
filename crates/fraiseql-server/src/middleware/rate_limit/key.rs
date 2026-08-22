//! Rate limit key construction and path matching helpers.

/// Build a namespaced rate-limiting key for use in both in-memory and Redis backends.
///
/// Format: `fraiseql:rl:{strategy}:{identifier}` for simple strategies, or
/// `fraiseql:rl:{strategy}:{prefix}:{identifier}` when an optional path prefix is supplied.
///
/// Exposed as `pub` for property testing.
#[must_use]
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
/// `/auth/startover` (`DoS` vector: exhausting the `/auth/start` bucket via an
/// unrelated path).
pub(super) fn path_matches_rule(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    let Some(rest) = path.strip_prefix(prefix) else {
        return false;
    };
    rest.starts_with('/') || rest.starts_with('?')
}

/// A per-path rate limit rule, derived from `[security.rate_limiting]` auth endpoint fields.
#[derive(Debug, Clone)]
pub(super) struct PathRateLimit {
    /// Path prefix to match (exact prefix, e.g., `/auth/start`).
    pub(super) path_prefix:    String,
    /// Token refill rate (tokens per second = `max_requests` / `window_secs`).
    pub(super) tokens_per_sec: f64,
    /// Maximum burst (= `max_requests`).
    pub(super) burst:          f64,
}

impl PathRateLimit {
    /// Build one rule when the `(max_requests, window_secs)` pair is enabled.
    #[allow(clippy::cast_precision_loss)] // Reason: window_secs is a small config value; no meaningful precision loss
    fn rule(prefix: &str, max_requests: u32, window_secs: u64) -> Option<Self> {
        (max_requests > 0 && window_secs > 0).then(|| Self {
            path_prefix:    prefix.to_string(),
            tokens_per_sec: f64::from(max_requests) / window_secs as f64,
            burst:          f64::from(max_requests),
        })
    }

    /// Derive every per-path rule from `[security.rate_limiting]`'s auth
    /// endpoint fields — the **single** builder both the in-memory and Redis
    /// backends attach, so the two rule tables cannot drift.
    ///
    /// The social endpoints (#368) share the PKCE settings: an
    /// `/auth/v1/authorize` flood fills the same bounded CSRF state store the
    /// `/auth/start` budget protects (#788/H25), and `/auth/v1/callback`
    /// performs the same provider round-trips as `/auth/callback`.
    pub(super) fn rules_from_security(
        sec: &super::config::RateLimitingSecurityConfig,
    ) -> Vec<Self> {
        [
            Self::rule("/auth/start", sec.auth_start_max_requests, sec.auth_start_window_secs),
            Self::rule(
                "/auth/v1/authorize",
                sec.auth_start_max_requests,
                sec.auth_start_window_secs,
            ),
            Self::rule(
                "/auth/callback",
                sec.auth_callback_max_requests,
                sec.auth_callback_window_secs,
            ),
            Self::rule(
                "/auth/v1/callback",
                sec.auth_callback_max_requests,
                sec.auth_callback_window_secs,
            ),
            Self::rule(
                "/auth/refresh",
                sec.auth_refresh_max_requests,
                sec.auth_refresh_window_secs,
            ),
            Self::rule("/auth/logout", sec.auth_logout_max_requests, sec.auth_logout_window_secs),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Normalise a candidate client address into a rate-limit bucket key.
///
/// Returns `None` when `candidate` does not parse as an IP address, so the caller can
/// fall back to the peer address rather than keying on it.
///
/// Two properties, both required for the key space to be bounded by something the
/// caller cannot inflate (#1143):
///
/// 1. **A proxy header is not an address until it parses as one.** `X-Real-IP` and
///    `X-Forwarded-For` are client-supplied strings; a trusted proxy forwards them, it does not
///    validate them. Returning the raw string made every distinct value a distinct bucket — the
///    `X-Tenant-ID` amplification again, one header over.
/// 2. **`IPv6` collapses to its /64 prefix.** A single routine `IPv6` allocation *is* a /64, so
///    keying on the full /128 would let one ordinary customer mint 2^64 buckets. Bounding the
///    tenant and subject while leaving this open would close the front door and leave the side door
///    ajar. `IPv4` is keyed whole: a /32 is one host.
pub(super) fn normalise_ip_key(candidate: &str) -> Option<String> {
    use std::net::IpAddr;

    match candidate.trim().parse::<IpAddr>().ok()? {
        IpAddr::V4(v4) => Some(v4.to_string()),
        IpAddr::V6(v6) => {
            let mut octets = v6.octets();
            octets[8..].fill(0);
            Some(format!("{}/64", std::net::Ipv6Addr::from(octets)))
        },
    }
}
