//! Rate limit configuration types.

use serde::{Deserialize, Serialize};

/// Minimal mirror of the `[security.rate_limiting]` TOML section, deserialized
/// from the compiled schema's `security.rate_limiting` JSON key.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RateLimitingSecurityConfig {
    /// Enable rate limiting.
    pub enabled: bool,
    /// Global request rate cap (requests per second, per IP).
    pub requests_per_second: u32,
    /// Burst allowance above the steady-state rate.
    pub burst_size: u32,
    /// Auth initiation endpoint — max requests per window.
    pub auth_start_max_requests: u32,
    /// Auth initiation window in seconds.
    pub auth_start_window_secs: u64,
    /// OAuth callback endpoint — max requests per window.
    pub auth_callback_max_requests: u32,
    /// OAuth callback window in seconds.
    pub auth_callback_window_secs: u64,
    /// Token refresh endpoint — max requests per window.
    pub auth_refresh_max_requests: u32,
    /// Token refresh window in seconds.
    pub auth_refresh_window_secs: u64,
    /// Per-authenticated-user request rate in requests/second.
    /// Defaults to 10× `requests_per_second` if not set.
    #[serde(default)]
    pub requests_per_second_per_user: Option<u32>,
    /// Redis URL for distributed rate limiting (not yet implemented).
    pub redis_url: Option<String>,
    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for the client IP.
    ///
    /// Enable only when FraiseQL is deployed behind a trusted reverse proxy
    /// (e.g. nginx, Cloudflare, AWS ALB) that sets these headers.  Enabling
    /// without a trusted proxy allows clients to spoof their IP address.
    #[serde(default)]
    pub trust_proxy_headers: bool,

    /// CIDR ranges trusted as proxy IPs (e.g. `["10.0.0.0/8", "172.16.0.0/12"]`).
    ///
    /// When set and `trust_proxy_headers = true`, X-Forwarded-For is only honoured
    /// when the direct connection IP falls within one of these CIDR ranges.
    /// Requests arriving from outside these ranges use the connection IP directly,
    /// preventing clients from spoofing their address by setting X-Forwarded-For.
    ///
    /// When `None` and `trust_proxy_headers = true`, all proxy IPs are trusted
    /// (less secure — a startup warning is emitted).
    #[serde(default)]
    pub trusted_proxy_cidrs: Option<Vec<String>>,
}

/// Rate limiting configuration (token-bucket algorithm).
///
/// Enforces request-per-second limits per IP/user across all GraphQL
/// operations. This is the canonical rate limiter for request throttling.
///
/// Distinct from `fraiseql_auth::AuthRateLimitConfig`, which uses a
/// sliding-window algorithm for auth endpoint brute-force protection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Requests per second per IP
    pub rps_per_ip: u32,

    /// Requests per second per user (if authenticated)
    pub rps_per_user: u32,

    /// Burst capacity (maximum tokens to accumulate)
    pub burst_size: u32,

    /// Cleanup interval in seconds (remove stale entries)
    pub cleanup_interval_secs: u64,

    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for client IP extraction.
    ///
    /// Must only be enabled when behind a trusted reverse proxy.
    pub trust_proxy_headers: bool,

    /// Parsed CIDR ranges trusted as proxy IPs.
    ///
    /// When non-empty, X-Forwarded-For is only trusted if the direct connection IP
    /// falls within one of these ranges.  An empty `Vec` with `trust_proxy_headers = true`
    /// means all direct IPs are treated as trusted proxies (less secure).
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:               true,
            rps_per_ip:            100,  // 100 req/sec per IP
            rps_per_user:          1000, // 1000 req/sec per user
            burst_size:            500,  // Allow bursts up to 500 requests
            cleanup_interval_secs: 300,  // Clean up every 5 minutes
            trust_proxy_headers:   false,
            trusted_proxy_cidrs:   Vec::new(),
        }
    }
}

impl RateLimitConfig {
    /// Build from the `[security.rate_limiting]` config embedded in the compiled schema.
    ///
    /// Maps `requests_per_second` → `rps_per_ip` and `burst_size` directly.
    /// `rps_per_user` uses the explicit `requests_per_second_per_user` value when set,
    /// or defaults to 10× `requests_per_second`.
    ///
    /// The default 10× multiplier reflects that authenticated users are identifiable
    /// (abuse is traceable) and include service accounts with higher call rates.
    /// Operators can override with `requests_per_second_per_user` in `fraiseql.toml`.
    pub fn from_security_config(sec: &RateLimitingSecurityConfig) -> Self {
        let trusted_proxy_cidrs = sec
            .trusted_proxy_cidrs
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|s| {
                s.parse::<ipnet::IpNet>()
                    .map_err(|e| {
                        tracing::warn!(cidr = %s, error = %e, "Invalid trusted_proxy_cidr — skipping");
                    })
                    .ok()
            })
            .collect();

        Self {
            enabled: sec.enabled,
            rps_per_ip: sec.requests_per_second,
            rps_per_user: sec
                .requests_per_second_per_user
                .unwrap_or_else(|| sec.requests_per_second.saturating_mul(10)),
            burst_size: sec.burst_size,
            cleanup_interval_secs: 300,
            trust_proxy_headers: sec.trust_proxy_headers,
            trusted_proxy_cidrs,
        }
    }
}

/// Result returned by all `check_*` rate-limit methods.
///
/// Carries the allow/deny decision, the approximate remaining token count
/// (used for the `X-RateLimit-Remaining` response header), and the
/// recommended `Retry-After` interval in seconds (0 when the request was
/// allowed).
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Whether the request should be allowed.
    pub allowed:          bool,
    /// Tokens remaining in the bucket after this request (≥ 0).
    pub remaining:        f64,
    /// Seconds the client should wait before retrying (0 when allowed).
    pub retry_after_secs: u32,
}

impl CheckResult {
    pub(super) const fn allow(remaining: f64) -> Self {
        Self {
            allowed: true,
            remaining,
            retry_after_secs: 0,
        }
    }

    pub(super) const fn deny(retry_after_secs: u32) -> Self {
        Self {
            allowed: false,
            remaining: 0.0,
            retry_after_secs,
        }
    }
}
