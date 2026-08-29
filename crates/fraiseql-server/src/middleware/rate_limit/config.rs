//! Rate limit configuration types.

use serde::{Deserialize, Serialize};

/// Default failed-login attempt cap, mirroring the CLI `toml_schema` default so the
/// runtime can tell an operator-tuned value from an untouched one (#356).
pub const DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS: u32 = 10;
/// Default failed-login lockout window in seconds, mirroring the CLI default (#356).
pub const DEFAULT_FAILED_LOGIN_LOCKOUT_SECS: u64 = 900;

/// The compiled `[security.rate_limiting]` shape — the schema seam owns it
/// (#977), so the CLI, the compiled artefact and this server share one type
/// (before #977 this file kept its own copy, with different defaults than the
/// producer's).
pub use fraiseql_core::schema::RateLimitingSecurityConfig;

/// Rate limiting configuration (token-bucket algorithm).
///
/// Enforces request-per-second limits per IP/user across all GraphQL
/// operations. This is the canonical rate limiter for request throttling.
///
/// Distinct from `fraiseql_auth::AuthRateLimitConfig`, which uses a
/// sliding-window algorithm for auth endpoint brute-force protection.
///
/// The container is `#[serde(default)]` (#874): a partial `[rate_limiting]`
/// block — including the exact example in `ServerConfig`'s rustdoc — starts
/// from the documented [`Default`] and overwrites only the keys the operator
/// set. Without it, omitting `cleanup_interval_secs` (a key no documentation
/// mentions) refused to boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

    /// Maximum tracked buckets per map — a memory ceiling, not a security control.
    ///
    /// One token bucket per key in each map (`ip_buckets`, `user_buckets`,
    /// `path_ip_buckets`, `tenant_buckets`). At ~200 bytes a bucket the default
    /// `100_000` is about 20 `MiB` per map.
    ///
    /// Reaching the cap evicts the least-recently-used of a sample; it does **not**
    /// refuse the newcomer. It did until #1080 — permanently, for the life of the
    /// process, because the cleanup task the denial waited on did not exist — and the
    /// wording here outlived that fix. Setting the cap low therefore costs accuracy
    /// (an evicted client resumes with a full bucket) and never availability.
    ///
    /// Operator-settable as `[security.rate_limiting] max_buckets` since #1171; before
    /// that `assemble` hardcoded it, so it was the one number in this section a
    /// deployment could not size to its host.
    pub max_buckets: usize,
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
            max_buckets:           fraiseql_core::schema::DEFAULT_RATE_LIMIT_MAX_BUCKETS,
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
    #[must_use]
    pub fn from_security_config(sec: &RateLimitingSecurityConfig) -> Self {
        // Reason: the fallible parse is the supported path (`try_from_security_config`);
        // this infallible constructor keeps the previous skip-on-invalid behaviour only
        // for callers that have already validated, and `resolve_rate_limiter` is not one
        // of them.
        let trusted_proxy_cidrs = parse_trusted_proxy_cidrs(sec).unwrap_or_else(|_| Vec::new());

        Self::assemble(sec, trusted_proxy_cidrs)
    }

    /// Build from the compiled schema's security block, refusing an unparseable CIDR.
    ///
    /// # Errors
    ///
    /// Returns the offending entry when `trusted_proxy_cidrs` contains something that
    /// is not a CIDR.
    ///
    /// Silently skipping such an entry was a third way past the `#618` proxy-trust
    /// guard, independent of the two in `#837`: the guard inspects the *string* list,
    /// so `trusted_proxy_cidrs = ["10.0.0.0/8typo"]` is non-empty and passes, and the
    /// parsed list it produces is then empty — which `extract_real_ip` treats as
    /// "trust every peer's X-Forwarded-For", the exact posture the guard exists to
    /// refuse.
    pub fn try_from_security_config(
        sec: &RateLimitingSecurityConfig,
    ) -> std::result::Result<Self, String> {
        Ok(Self::assemble(sec, parse_trusted_proxy_cidrs(sec)?))
    }

    fn assemble(sec: &RateLimitingSecurityConfig, trusted_proxy_cidrs: Vec<ipnet::IpNet>) -> Self {
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
            max_buckets: sec.max_buckets,
        }
    }
}

/// Rate-limit settings supplied by a CLI flag or environment variable.
///
/// Kept as a distinct set of `Option`s rather than merged eagerly into
/// [`RateLimitConfig`], because the merge destroyed the one fact the resolver needs:
/// *which* fields the operator actually set. Without it, `rps_per_ip = 1000` from
/// `FRAISEQL_RATE_LIMIT_RPS_PER_IP` is indistinguishable from the struct default, so
/// the overrides could only be applied wholesale or not at all — and "not at all" is
/// what happened whenever the compiled schema also configured rate limiting (#774).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitOverrides {
    /// `--rate-limiting-enabled` / `FRAISEQL_RATE_LIMITING_ENABLED`.
    pub enabled:      Option<bool>,
    /// `--rate-limit-rps-per-ip` / `FRAISEQL_RATE_LIMIT_RPS_PER_IP`.
    pub rps_per_ip:   Option<u32>,
    /// `--rate-limit-rps-per-user` / `FRAISEQL_RATE_LIMIT_RPS_PER_USER`.
    pub rps_per_user: Option<u32>,
    /// `--rate-limit-burst-size` / `FRAISEQL_RATE_LIMIT_BURST_SIZE`.
    pub burst_size:   Option<u32>,
    /// `--rate-limit-max-buckets` / `FRAISEQL_RATE_LIMIT_MAX_BUCKETS` (#1171). Sizes the
    /// tracking maps to the host, which is a per-deployment decision the compiled schema
    /// cannot know.
    pub max_buckets:  Option<usize>,
}

impl RateLimitOverrides {
    /// Whether any override was supplied.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.rps_per_ip.is_none()
            && self.rps_per_user.is_none()
            && self.burst_size.is_none()
            && self.max_buckets.is_none()
    }

    /// Whether the overrides ask for rate limiting that no other source configured.
    #[must_use]
    pub fn enables(&self) -> bool {
        self.enabled == Some(true) || (self.enabled.is_none() && !self.is_empty())
    }

    /// Apply each supplied value over `config`, leaving the rest untouched.
    pub const fn apply_to(&self, config: &mut RateLimitConfig) {
        if let Some(v) = self.enabled {
            config.enabled = v;
        }
        if let Some(v) = self.rps_per_ip {
            config.rps_per_ip = v;
        }
        if let Some(v) = self.rps_per_user {
            config.rps_per_user = v;
        }
        if let Some(v) = self.burst_size {
            config.burst_size = v;
        }
        if let Some(v) = self.max_buckets {
            config.max_buckets = v;
        }
    }
}

/// Parse `trusted_proxy_cidrs`, naming the first entry that is not a CIDR.
fn parse_trusted_proxy_cidrs(
    sec: &RateLimitingSecurityConfig,
) -> std::result::Result<Vec<ipnet::IpNet>, String> {
    sec.trusted_proxy_cidrs
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| {
            s.parse::<ipnet::IpNet>().map_err(|e| {
                format!(
                    "[security.rate_limiting] trusted_proxy_cidrs contains {s:?}, which is not a \
                     CIDR range ({e}). Every entry must parse, because an entry that is dropped \
                     leaves the list shorter than it looks — and an empty list means every peer \
                     is trusted to set X-Forwarded-For."
                )
            })
        })
        .collect()
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
