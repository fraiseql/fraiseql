//! Rate limiting for brute-force and abuse protection.
//!
//! Provides [`KeyedRateLimiter`] — a per-key sliding-window counter backed by
//! a [`DashMap`] — and [`RateLimiters`], a pre-built set of limiters for
//! each authentication endpoint.
// # Threading Model
//
// Per-key updates are **atomic** with respect to concurrent access:
// - check() holds a per-shard write reference through the entire
//   read-current-time → load-record → update-counter sequence
// - Different keys land on different shards and never contend
// - This prevents race conditions where multiple threads simultaneously exceed
//   the limit on the *same* key
// - Periodic sweeps and capacity eviction are best-effort and run without
//   holding any other shard's lock

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::{DashMap, mapref::entry::Entry};

use crate::error::{AuthError, Result};

/// Abstraction over the wall-clock used by [`KeyedRateLimiter`].
///
/// Implementations must be `Send + Sync` because the limiter is shared across
/// request-handler threads. The trait is generic over the rate-limiter type
/// parameter, so production builds inline `SystemClock` without any virtual
/// dispatch or heap allocation.
///
/// A blanket impl exists for `F: Fn() -> u64 + Send + Sync`, so test code can
/// pass closures and `fn` pointers (such as `|| u64::MAX`) directly.
pub trait Clock: Send + Sync {
    /// Return the current time as a Unix timestamp (seconds since the epoch).
    fn now_unix_secs(&self) -> u64;
}

impl<F> Clock for F
where
    F: Fn() -> u64 + Send + Sync,
{
    fn now_unix_secs(&self) -> u64 {
        self()
    }
}

/// Production wall-clock that reads `SystemTime::now()`.
///
/// On system time error, returns `0` (fail-closed): a timestamp of `0` is
/// before any real `window_start`, so existing windows will not expire and
/// rate limiting continues to be enforced with existing counters. New windows
/// started while the clock is broken will have `window_start = 0`; when the
/// clock recovers, those windows will immediately expire (since any real
/// timestamp ≥ `0 + window_secs`) and reset naturally.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_secs(&self) -> u64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "System time error in rate limiter — brute-force protection \
                     continues using frozen timestamps. System clock may have moved \
                     backward or time source is unavailable."
                );
                // Return 0 (not u64::MAX): existing windows will not expire,
                // so rate limiting remains enforced during the clock failure.
                0
            },
        }
    }
}

/// Rate limit configuration for authentication endpoints (sliding-window algorithm).
///
/// Uses a per-key sliding-window counter for brute-force protection on
/// authentication endpoints (login, token refresh, callback).
///
/// Distinct from `fraiseql_server::middleware::RateLimitConfig`, which uses
/// a token-bucket algorithm for general request rate limiting.
#[derive(Debug, Clone)]
pub struct AuthRateLimitConfig {
    /// Whether rate limiting is enabled for this endpoint
    pub enabled:      bool,
    /// Maximum number of requests allowed in the window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_secs:  u64,
}

impl AuthRateLimitConfig {
    /// IP-based rate limiting for public endpoints
    /// 100 requests per 60 seconds (typical for auth/start, auth/callback)
    #[must_use]
    pub const fn per_ip_standard() -> Self {
        Self {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        }
    }

    /// Stricter IP-based rate limiting for sensitive endpoints
    /// 50 requests per 60 seconds
    #[must_use]
    pub const fn per_ip_strict() -> Self {
        Self {
            enabled:      true,
            max_requests: 50,
            window_secs:  60,
        }
    }

    /// User-based rate limiting for authenticated endpoints
    /// 10 requests per 60 seconds
    #[must_use]
    pub const fn per_user_standard() -> Self {
        Self {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        }
    }

    /// Failed login attempt limiting
    /// 5 failed attempts per 3600 seconds (1 hour)
    #[must_use]
    pub const fn failed_login_attempts() -> Self {
        Self {
            enabled:      true,
            max_requests: 5,
            window_secs:  3600,
        }
    }
}

/// Request record for tracking
#[derive(Debug, Clone)]
struct RequestRecord {
    /// Number of requests in current window
    count:        u32,
    /// Unix timestamp of window start
    window_start: u64,
}

/// How often (in number of `check()` calls) expired entries are purged from the map.
///
/// Stale entries accumulate when keys stop sending requests.  Every
/// `PURGE_INTERVAL` calls the limiter performs a full sweep and removes entries
/// whose window has elapsed, bounding the HashMap's memory footprint.
const PURGE_INTERVAL: u64 = 1_000;

/// Default maximum number of unique keys the limiter will track simultaneously.
///
/// When the cap is reached, new keys are denied immediately and a warning is logged.
/// This prevents an attacker from exhausting memory by sending requests from millions
/// of unique IP addresses. The cap is conservative: 100k entries × ~100 bytes ≈ 10 MB.
const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// Per-key sliding-window rate limiter backed by a [`DashMap`].
///
/// Each unique key (IP address, user ID, etc.) gets its own independent counter.
/// The check-and-update sequence for a given key is atomic: no TOCTOU race can
/// allow more requests than `max_requests` in any single window, even under
/// high concurrency.  Distinct keys live on different shards and never block
/// each other.
///
/// The map is capped at `DEFAULT_MAX_ENTRIES` keys. When a new key arrives at
/// capacity the entry with the oldest `window_start` is evicted to make room,
/// bounding memory growth while still tracking new sources.  Because the
/// eviction step is performed outside the per-key lock, the LRU choice is
/// best-effort under heavy concurrent insertion — total entries may oscillate
/// around the cap by a small amount.  The current LRU-by-window-start was
/// approximate anyway (`min_by_key` over a snapshot).
///
/// # Deployment note
///
/// This rate limiter is **per-process**. In a multi-replica deployment, each
/// replica enforces the limit independently — the effective limit across *N*
/// replicas is *N × limit*. For true distributed enforcement, configure a
/// Redis-backed rate limiter via the `redis-rate-limiting` Cargo feature (see
/// the fraiseql-observers queue feature for the integration pattern). Call
/// [`warn_if_single_node_rate_limiting`] during server startup to emit a
/// reminder when no distributed backend is detected.
///
/// # Type parameter
///
/// `C: Clock` selects the time source. Production code uses the default
/// [`SystemClock`] (a zero-sized type) so the clock is inlined and no virtual
/// dispatch or heap allocation occurs. Tests can substitute any closure or
/// custom clock via [`KeyedRateLimiter::with_clock`].
///
/// # Constructors
///
/// - [`KeyedRateLimiter::new`] — use the system wall clock (production).
/// - [`KeyedRateLimiter::with_clock`] — inject a custom clock (testing).
/// - [`KeyedRateLimiter::with_clock_and_max_entries`] — custom clock + cap (testing).
pub struct KeyedRateLimiter<C: Clock = SystemClock> {
    records:     Arc<DashMap<String, RequestRecord>>,
    config:      AuthRateLimitConfig,
    max_entries: usize,
    /// Monotonically increasing call counter for triggering periodic sweeps.
    check_count: AtomicU64,
    /// Time source — defaults to [`SystemClock`].
    clock:       C,
}

impl<C: Clock + Clone> Clone for KeyedRateLimiter<C> {
    fn clone(&self) -> Self {
        Self {
            records:     Arc::clone(&self.records),
            config:      self.config.clone(),
            max_entries: self.max_entries,
            check_count: AtomicU64::new(self.check_count.load(Ordering::Relaxed)),
            clock:       self.clock.clone(),
        }
    }
}

impl KeyedRateLimiter<SystemClock> {
    /// Create a new keyed rate limiter using wall-clock time.
    #[must_use]
    pub fn new(config: AuthRateLimitConfig) -> Self {
        Self::with_parts(config, DEFAULT_MAX_ENTRIES, SystemClock)
    }

    /// Create a rate limiter with a custom entry cap.
    ///
    /// Use this when the deployment context calls for a tighter or looser bound
    /// than `DEFAULT_MAX_ENTRIES`.  Setting `max_entries = 0` disables the cap
    /// (unbounded — not recommended in production).
    #[must_use]
    pub fn with_max_entries(config: AuthRateLimitConfig, max_entries: usize) -> Self {
        Self::with_parts(config, max_entries, SystemClock)
    }
}

impl<C: Clock> KeyedRateLimiter<C> {
    /// Create a rate limiter with an injectable clock (for testing).
    ///
    /// The `clock`'s `now_unix_secs` method is called on every `check()` to
    /// obtain the current Unix timestamp. Pass `|| u64::MAX` to simulate a
    /// broken system clock and verify fail-open behavior.
    pub fn with_clock(config: AuthRateLimitConfig, clock: C) -> Self {
        Self::with_parts(config, DEFAULT_MAX_ENTRIES, clock)
    }

    /// Create a rate limiter with both a custom clock and a custom entry cap (for testing).
    ///
    /// Combines the benefits of [`KeyedRateLimiter::with_clock`] and
    /// [`KeyedRateLimiter::with_max_entries`] for deterministic eviction tests.
    pub fn with_clock_and_max_entries(
        config: AuthRateLimitConfig,
        max_entries: usize,
        clock: C,
    ) -> Self {
        Self::with_parts(config, max_entries, clock)
    }

    fn with_parts(config: AuthRateLimitConfig, max_entries: usize, clock: C) -> Self {
        Self {
            records: Arc::new(DashMap::new()),
            config,
            max_entries,
            check_count: AtomicU64::new(0),
            clock,
        }
    }

    /// Check if a request should be allowed for the given key
    ///
    /// # Atomicity
    ///
    /// The check-and-update step for a given key is **atomic**: while
    /// inspecting and mutating the `RequestRecord` for `key`, this function
    /// holds the per-shard write reference for that key.  No concurrent thread
    /// can observe a partial state for the same key, which prevents the
    /// classic TOCTOU race where multiple threads simultaneously exceed the
    /// rate limit.
    ///
    /// The periodic sweep and the capacity-eviction step are best-effort
    /// (they run outside the per-key lock); they may overshoot the entry cap
    /// by a small amount under heavy concurrent insertion.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the request is allowed and the counter has been incremented.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::RateLimited`] if the key has exceeded the configured
    /// rate limit within the sliding window.
    pub fn check(&self, key: &str) -> Result<()> {
        // If rate limiting is disabled, always allow the request.
        if !self.config.enabled {
            return Ok(());
        }

        let now = self.clock.now_unix_secs();

        // Periodic expiry sweep to bound DashMap growth.
        // Runs every PURGE_INTERVAL calls; overflow wraps silently which is fine.
        let count = self.check_count.fetch_add(1, Ordering::Relaxed);
        if count.is_multiple_of(PURGE_INTERVAL) {
            self.records
                .retain(|_, r| now < r.window_start.saturating_add(self.config.window_secs));
        }

        // Enforce max-entries cap to prevent unbounded memory growth under distributed attacks.
        // A cap of 0 disables the limit (opt-in unbounded mode).
        // When at capacity, evict the entry with the oldest window_start (best-effort LRU)
        // so new sources can always be tracked without permanently blocking new IPs.
        if self.max_entries > 0
            && !self.records.contains_key(key)
            && self.records.len() >= self.max_entries
        {
            if let Some(oldest_key) =
                self.records.iter().min_by_key(|r| r.value().window_start).map(|r| r.key().clone())
            {
                self.records.remove(&oldest_key);
                tracing::debug!(
                    max_entries = self.max_entries,
                    "Rate limiter at capacity — evicted oldest entry to make room for new key"
                );
            }
        }

        // Get or create record for this key.  `Entry` holds a per-shard write
        // lock through the closure / match arms, giving us per-key atomicity.
        match self.records.entry(key.to_string()) {
            Entry::Occupied(mut occ) => {
                let record = occ.get_mut();
                if now >= record.window_start.saturating_add(self.config.window_secs) {
                    // CASE 1: Window has expired - start a new window
                    record.count = 1;
                    record.window_start = now;
                    Ok(())
                } else if record.count < self.config.max_requests {
                    // CASE 2: Window is active and we haven't exceeded the limit
                    record.count += 1;
                    Ok(())
                } else {
                    // CASE 3: Window is active and we've reached the limit
                    Err(AuthError::RateLimited {
                        retry_after_secs: self.config.window_secs,
                    })
                }
            },
            Entry::Vacant(vac) => {
                // First request from this key — start a fresh window.
                vac.insert(RequestRecord {
                    count:        1,
                    window_start: now,
                });
                Ok(())
            },
        }
    }

    /// Get the number of active rate limiters (for monitoring).
    pub fn active_limiters(&self) -> usize {
        self.records.len()
    }

    /// Clear all rate limiters (for testing or reset).
    pub fn clear(&self) {
        self.records.clear();
    }

    /// Create a copy for independent testing
    pub fn clone_config(&self) -> AuthRateLimitConfig {
        self.config.clone()
    }
}

/// Emit a startup warning when no distributed rate-limiting backend is configured.
///
/// Call once during server startup. If the `FRAISEQL_RATE_LIMIT_WARN_SINGLE_NODE`
/// environment variable is set to `true` or `1` (case-insensitive) and the
/// `FRAISEQL_RATE_LIMIT_BACKEND` variable is unset, a `warn!` is emitted reminding
/// operators that each replica enforces limits independently — the effective limit
/// across *N* replicas is *N × limit*.
///
/// This is a documentation-only reminder; it does not change runtime behaviour.
pub fn warn_if_single_node_rate_limiting() {
    let should_warn = std::env::var("FRAISEQL_RATE_LIMIT_WARN_SINGLE_NODE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    let has_backend = std::env::var("FRAISEQL_RATE_LIMIT_BACKEND").is_ok();
    if should_warn && !has_backend {
        tracing::warn!(
            "Rate limiter is per-process; multi-replica deployments are not protected against \
             distributed brute-force. Configure a Redis-backed rate limiter via the \
             `redis-rate-limiting` feature for distributed enforcement."
        );
    }
}

/// Global rate limiters for different endpoints
pub struct RateLimiters {
    /// auth/start: per-IP, 100 req/min
    pub auth_start:    KeyedRateLimiter,
    /// auth/callback: per-IP, 50 req/min
    pub auth_callback: KeyedRateLimiter,
    /// auth/refresh: per-user, 10 req/min
    pub auth_refresh:  KeyedRateLimiter,
    /// auth/logout: per-user, 20 req/min
    pub auth_logout:   KeyedRateLimiter,
    /// Failed login tracking: per-user, 5 attempts/hour
    pub failed_logins: KeyedRateLimiter,
}

impl RateLimiters {
    /// Create default rate limiters for all endpoints
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth_start:    KeyedRateLimiter::new(AuthRateLimitConfig::per_ip_standard()),
            auth_callback: KeyedRateLimiter::new(AuthRateLimitConfig::per_ip_strict()),
            auth_refresh:  KeyedRateLimiter::new(AuthRateLimitConfig::per_user_standard()),
            auth_logout:   KeyedRateLimiter::new(AuthRateLimitConfig::per_user_standard()),
            failed_logins: KeyedRateLimiter::new(AuthRateLimitConfig::failed_login_attempts()),
        }
    }

    /// Create with custom configurations
    #[must_use]
    pub fn with_configs(
        start_cfg: AuthRateLimitConfig,
        callback_cfg: AuthRateLimitConfig,
        refresh_cfg: AuthRateLimitConfig,
        logout_cfg: AuthRateLimitConfig,
        failed_cfg: AuthRateLimitConfig,
    ) -> Self {
        Self {
            auth_start:    KeyedRateLimiter::new(start_cfg),
            auth_callback: KeyedRateLimiter::new(callback_cfg),
            auth_refresh:  KeyedRateLimiter::new(refresh_cfg),
            auth_logout:   KeyedRateLimiter::new(logout_cfg),
            failed_logins: KeyedRateLimiter::new(failed_cfg),
        }
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}
