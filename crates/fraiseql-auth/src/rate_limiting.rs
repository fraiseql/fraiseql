//! Rate limiting for brute-force and abuse protection.
//!
//! Provides [`KeyedRateLimiter`] — a per-key sliding-window counter backed by
//! a `Mutex<HashMap>` — and [`RateLimiters`], a pre-built set of limiters for
//! each authentication endpoint.
// # Threading Model
//
// All rate limiting operations are **atomic** with respect to concurrent access:
// - Each call to check() holds a lock for its entire duration
// - Check-and-update operations cannot be interleaved with other threads
// - This prevents race conditions where multiple threads simultaneously exceed limits
// - The lock is held while reading current time, reading record, and updating counter
// - This ensures that the decision to allow/deny a request is consistent

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::{AuthError, Result};

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

/// Per-key sliding-window rate limiter backed by a `Mutex<HashMap>`.
///
/// Each unique key (IP address, user ID, etc.) gets its own independent counter.
/// The check-and-update sequence is atomic: no TOCTOU race can allow more requests
/// than `max_requests` in any single window, even under high concurrency.
///
/// The map is capped at `DEFAULT_MAX_ENTRIES` keys. When a new key arrives at
/// capacity the entry with the oldest `window_start` is evicted to make room,
/// bounding memory growth while still tracking new sources.
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
/// # Constructors
///
/// - [`KeyedRateLimiter::new`] — use the system wall clock (production).
/// - [`KeyedRateLimiter::with_clock`] — inject a custom clock (testing).
/// - [`KeyedRateLimiter::with_clock_and_max_entries`] — custom clock + cap (testing).
pub struct KeyedRateLimiter {
    records:     Arc<Mutex<HashMap<String, RequestRecord>>>,
    config:      AuthRateLimitConfig,
    max_entries: usize,
    /// Monotonically increasing call counter for triggering periodic sweeps.
    check_count: AtomicU64,
    /// Time source — defaults to `SystemTime::now()` via [`system_clock`].
    /// Overridable via [`KeyedRateLimiter::with_clock`] for testing.
    clock:       Box<dyn Fn() -> u64 + Send + Sync>,
}

/// Default clock that reads wall-clock time.
///
/// On system time error, returns `0` (fail-closed): a timestamp of `0` is
/// before any real `window_start`, so existing windows will not expire and
/// rate limiting continues to be enforced with existing counters. New windows
/// started while the clock is broken will have `window_start = 0`; when the
/// clock recovers, those windows will immediately expire (since any real
/// timestamp ≥ 0 + `window_secs`) and reset naturally.
fn system_clock() -> u64 {
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

impl KeyedRateLimiter {
    /// Create a new keyed rate limiter using wall-clock time.
    pub fn new(config: AuthRateLimitConfig) -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            config,
            max_entries: DEFAULT_MAX_ENTRIES,
            check_count: AtomicU64::new(0),
            clock: Box::new(system_clock),
        }
    }

    /// Create a rate limiter with a custom entry cap.
    ///
    /// Use this when the deployment context calls for a tighter or looser bound
    /// than `DEFAULT_MAX_ENTRIES`.  Setting `max_entries = 0` disables the cap
    /// (unbounded — not recommended in production).
    pub fn with_max_entries(config: AuthRateLimitConfig, max_entries: usize) -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            config,
            max_entries,
            check_count: AtomicU64::new(0),
            clock: Box::new(system_clock),
        }
    }

    /// Create a rate limiter with an injectable clock (for testing).
    ///
    /// The `clock` function is called on every `check()` to obtain the current Unix timestamp.
    /// Pass `|| u64::MAX` to simulate a broken system clock and verify fail-open behavior.
    pub fn with_clock<F>(config: AuthRateLimitConfig, clock: F) -> Self
    where
        F: Fn() -> u64 + Send + Sync + 'static,
    {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            config,
            max_entries: DEFAULT_MAX_ENTRIES,
            check_count: AtomicU64::new(0),
            clock: Box::new(clock),
        }
    }

    /// Create a rate limiter with both a custom clock and a custom entry cap (for testing).
    ///
    /// Combines the benefits of [`KeyedRateLimiter::with_clock`] and
    /// [`KeyedRateLimiter::with_max_entries`] for deterministic eviction tests.
    pub fn with_clock_and_max_entries<F>(
        config: AuthRateLimitConfig,
        max_entries: usize,
        clock: F,
    ) -> Self
    where
        F: Fn() -> u64 + Send + Sync + 'static,
    {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            config,
            max_entries,
            check_count: AtomicU64::new(0),
            clock: Box::new(clock),
        }
    }

    /// Check if a request should be allowed for the given key
    ///
    /// # Atomicity
    ///
    /// This operation is **atomic** - the entire check-and-update sequence happens atomically:
    /// 1. Acquires exclusive lock on rate limit records
    /// 2. Gets current timestamp
    /// 3. Loads or creates request record for this key
    /// 4. Decides: allow, reset window, or deny
    /// 5. Updates counter/window only if request is allowed
    /// 6. Releases lock
    ///
    /// No concurrent thread can observe a partial state. This prevents classic
    /// time-of-check-time-of-use (TOCTOU) race conditions where multiple threads
    /// simultaneously exceed the rate limit.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the request is allowed and the counter has been incremented.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::RateLimited`] if the key has exceeded the configured
    /// rate limit within the sliding window.
    ///
    /// # Panics
    ///
    /// Panics if the Mutex is poisoned (another thread panicked while holding the lock).
    /// This is acceptable because a poisoned lock indicates a thread panic, suggesting
    /// the system is already in an inconsistent state and should be restarted.
    pub fn check(&self, key: &str) -> Result<()> {
        // If rate limiting is disabled, always allow the request
        // Note: This check is outside the lock for efficiency, but there's a benign race:
        // if another thread changes config.enabled between this check and acquiring the lock,
        // we still proceed to update the counter. This is safe because we only update counters
        // and don't depend on the enabled flag for correctness (counter updates are idempotent).
        if !self.config.enabled {
            return Ok(());
        }

        // CRITICAL: Acquire lock - this ensures all operations below are atomic.
        // On poison, recover the inner data — the HashMap is still valid even if the
        // thread that held the lock panicked mid-update (worst case: a stale entry).
        let mut records = self.records.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("rate limiter mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        let now = (self.clock)();

        // Periodic expiry sweep to bound HashMap growth.
        // Runs every PURGE_INTERVAL calls; overflow wraps silently which is fine.
        let count = self.check_count.fetch_add(1, Ordering::Relaxed);
        if count.is_multiple_of(PURGE_INTERVAL) {
            records.retain(|_, r| now < r.window_start.saturating_add(self.config.window_secs));
        }

        // Enforce max-entries cap to prevent unbounded memory growth under distributed attacks.
        // A cap of 0 disables the limit (opt-in unbounded mode).
        // When at capacity, evict the entry with the oldest window_start (LRU by activity)
        // so new sources can always be tracked without permanently blocking new IPs.
        if self.max_entries > 0 && !records.contains_key(key) && records.len() >= self.max_entries {
            if let Some(oldest_key) =
                records.iter().min_by_key(|(_, r)| r.window_start).map(|(k, _)| k.clone())
            {
                records.remove(&oldest_key);
                tracing::debug!(
                    max_entries = self.max_entries,
                    "Rate limiter at capacity — evicted oldest entry to make room for new key"
                );
            }
        }

        // Get or create record for this key (first request from this key)
        let record = records.entry(key.to_string()).or_insert_with(|| RequestRecord {
            count:        0,
            window_start: now,
        });

        // Thread-safe decision: all branches update state atomically while holding the lock
        if now >= record.window_start.saturating_add(self.config.window_secs) {
            // CASE 1: Window has expired - start a new window
            // This request is the first in the new window, so it's allowed
            record.count = 1;
            record.window_start = now;
            Ok(())
        } else if record.count < self.config.max_requests {
            // CASE 2: Window is active and we haven't exceeded the limit
            // This request is allowed - increment the counter atomically
            record.count += 1;
            Ok(())
        } else {
            // CASE 3: Window is active and we've reached the limit
            // This request is NOT allowed - counter is not incremented
            // Subsequent requests will also fail until the window expires
            Err(AuthError::RateLimited {
                retry_after_secs: self.config.window_secs,
            })
        }
    }

    /// Get the number of active rate limiters (for monitoring).
    pub fn active_limiters(&self) -> usize {
        let records = self.records.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("rate limiter mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        records.len()
    }

    /// Clear all rate limiters (for testing or reset).
    pub fn clear(&self) {
        let mut records = self.records.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("rate limiter mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        records.clear();
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
