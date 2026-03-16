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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    // ─── build_rate_limit_key ───────────────────────────────────────────────

    #[test]
    fn key_without_prefix() {
        let key = build_rate_limit_key("ip", "1.2.3.4", None);
        assert_eq!(key, "fraiseql:rl:ip:1.2.3.4");
    }

    #[test]
    fn key_with_prefix() {
        let key = build_rate_limit_key("path", "1.2.3.4", Some("/auth/start"));
        assert_eq!(key, "fraiseql:rl:path:/auth/start:1.2.3.4");
    }

    // ─── is_private_or_loopback ─────────────────────────────────────────────

    #[test]
    fn loopback_ipv4_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn rfc1918_10_x_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn rfc1918_172_16_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn rfc1918_192_168_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn link_local_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn public_ipv4_is_not_private() {
        assert!(!is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn loopback_ipv6_is_private() {
        assert!(is_private_or_loopback(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn public_ipv6_is_not_private() {
        assert!(!is_private_or_loopback(IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    // ─── path_matches_rule ──────────────────────────────────────────────────

    #[test]
    fn exact_match() {
        assert!(path_matches_rule("/auth/start", "/auth/start"));
    }

    #[test]
    fn sub_path_matches() {
        assert!(path_matches_rule("/auth/start/extra", "/auth/start"));
    }

    #[test]
    fn query_string_matches() {
        assert!(path_matches_rule("/auth/start?code=abc", "/auth/start"));
    }

    #[test]
    fn superset_does_not_match() {
        assert!(!path_matches_rule("/auth/startover", "/auth/start"));
    }

    #[test]
    fn hyphenated_suffix_does_not_match() {
        assert!(!path_matches_rule("/auth/start-session", "/auth/start"));
    }

    #[test]
    fn completely_different_path_does_not_match() {
        assert!(!path_matches_rule("/graphql", "/auth/start"));
    }

    #[test]
    fn empty_path_does_not_match_prefix() {
        assert!(!path_matches_rule("", "/auth/start"));
    }
}
