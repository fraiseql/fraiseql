//! Rate limit configuration types.

use serde::{Deserialize, Serialize};

/// Default failed-login attempt cap, mirroring the CLI `toml_schema` default so the
/// runtime can tell an operator-tuned value from an untouched one (#356).
pub const DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS: u32 = 10;
/// Default failed-login lockout window in seconds, mirroring the CLI default (#356).
pub const DEFAULT_FAILED_LOGIN_LOCKOUT_SECS: u64 = 900;

const fn default_failed_login_max_attempts() -> u32 {
    DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS
}
const fn default_failed_login_lockout_secs() -> u64 {
    DEFAULT_FAILED_LOGIN_LOCKOUT_SECS
}

/// Minimal mirror of the `[security.rate_limiting]` TOML section, deserialized
/// from the compiled schema's `security.rate_limiting` JSON key.
#[derive(Debug, Clone, Deserialize)]
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
    /// Logout endpoint — max requests per window.
    ///
    /// Declarable in `[security.rate_limiting]` since the CLI shipped the key, but
    /// with no field here it had nowhere to land and no path rule was ever built for
    /// `/auth/logout` (#893).
    pub auth_logout_max_requests: u32,
    /// Logout window in seconds.
    pub auth_logout_window_secs: u64,
    /// Maximum failed first-factor login attempts before lockout.
    ///
    /// Mirrors the CLI `[security.rate_limiting] failed_login_max_attempts` field.
    /// The off-the-shelf binary performs no first-factor login of its own (OIDC/JWT
    /// is validated cryptographically and first-factor auth is delegated to the
    /// provider; TOTP MFA is a library-only feature), so it cannot enforce this. A
    /// value tuned away from the default is rejected at startup in production (#356);
    /// see `failed_login_lockout_check`.
    #[serde(default = "default_failed_login_max_attempts")]
    pub failed_login_max_attempts: u32,
    /// Lockout window in seconds after `failed_login_max_attempts` is exceeded.
    /// Not enforced by the binary — see `failed_login_max_attempts`.
    #[serde(default = "default_failed_login_lockout_secs")]
    pub failed_login_lockout_secs: u64,
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

impl Default for RateLimitingSecurityConfig {
    /// Defaults with a **usable** budget, not `u32::default()`.
    ///
    /// The container is `#[serde(default)]`, so serde starts from this value and
    /// overwrites only the keys a producer actually emitted. Deriving `Default` gave
    /// `requests_per_second = 0` and `burst_size = 0` — a limiter with a budget of
    /// zero, which denies every request. Any producer that emitted a `rate_limiting`
    /// section without those two keys would therefore hand the deployment a server
    /// that 429s all traffic, which is the trap sitting under the compiler casing fix
    /// in #893. The values mirror `RateLimitConfig::default`.
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: 100,
            burst_size: 500,
            auth_start_max_requests: 0,
            auth_start_window_secs: 0,
            auth_callback_max_requests: 0,
            auth_callback_window_secs: 0,
            auth_refresh_max_requests: 0,
            auth_refresh_window_secs: 0,
            auth_logout_max_requests: 0,
            auth_logout_window_secs: 0,
            failed_login_max_attempts: DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS,
            failed_login_lockout_secs: DEFAULT_FAILED_LOGIN_LOCKOUT_SECS,
            requests_per_second_per_user: None,
            redis_url: None,
            trust_proxy_headers: false,
            trusted_proxy_cidrs: None,
        }
    }
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

    /// Maximum number of unique IP/user buckets to hold in memory at once.
    ///
    /// When any of the three tracking maps (`ip_buckets`, `user_buckets`,
    /// `path_ip_buckets`) reaches this limit, requests arriving from a
    /// previously-unseen key are **denied** until stale entries are evicted by
    /// the background cleanup task.  This prevents unbounded memory growth
    /// under a flood of spoofed or unique source IPs.
    ///
    /// Defaults to `100_000`.  At ~200 bytes per bucket, this cap allows up to
    /// ~20 `MiB` of tracking state per map before enforcement kicks in.
    #[serde(default = "default_max_buckets")]
    pub max_buckets: usize,
}

const fn default_max_buckets() -> usize {
    100_000
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
            max_buckets:           100_000,
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
            max_buckets: default_max_buckets(),
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
}

impl RateLimitOverrides {
    /// Whether any override was supplied.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.rps_per_ip.is_none()
            && self.rps_per_user.is_none()
            && self.burst_size.is_none()
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
