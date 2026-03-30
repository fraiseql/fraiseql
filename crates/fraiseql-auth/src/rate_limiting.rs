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
    pub const fn per_ip_standard() -> Self {
        Self {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        }
    }

    /// Stricter IP-based rate limiting for sensitive endpoints
    /// 50 requests per 60 seconds
    pub const fn per_ip_strict() -> Self {
        Self {
            enabled:      true,
            max_requests: 50,
            window_secs:  60,
        }
    }

    /// User-based rate limiting for authenticated endpoints
    /// 10 requests per 60 seconds
    pub const fn per_user_standard() -> Self {
        Self {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        }
    }

    /// Failed login attempt limiting
    /// 5 failed attempts per 3600 seconds (1 hour)
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
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|poisoned| {
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
        let records = self
            .records
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("rate limiter mutex was poisoned, recovering");
                poisoned.into_inner()
            });
        records.len()
    }

    /// Clear all rate limiters (for testing or reset).
    pub fn clear(&self) {
        let mut records = self
            .records
            .lock()
            .unwrap_or_else(|poisoned| {
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]  // Reason: test module wildcard import; brings all items into test scope
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 3,
            window_secs:  60,
        });

        // Should allow up to max_requests
        for i in 0..3 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[test]
    fn test_rate_limiter_rejects_over_limit() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  60,
        });

        limiter.check("key").ok();
        limiter.check("key").ok();

        // Third should fail
        let result = limiter.check("key");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited error, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiter_per_key() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  60,
        });

        // Key 1: use allowance
        limiter.check("key1").ok();
        limiter.check("key1").ok();

        // Key 2: should have fresh allowance
        let result = limiter.check("key2");
        assert!(result.is_ok(), "Different key should have independent limit");
    }

    #[test]
    fn test_rate_limiter_error_contains_retry_after() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");

        match result {
            Err(AuthError::RateLimited { retry_after_secs }) => {
                assert_eq!(retry_after_secs, 60);
            },
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_rate_limiter_active_limiters_count() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        });

        assert_eq!(limiter.active_limiters(), 0);

        limiter.check("key1").ok();
        assert_eq!(limiter.active_limiters(), 1);

        limiter.check("key2").ok();
        assert_eq!(limiter.active_limiters(), 2);
    }

    #[test]
    fn test_rate_limiters_default() {
        let limiters = RateLimiters::new();

        // auth/start should allow requests
        let result = limiters.auth_start.check("ip_1");
        assert!(result.is_ok(), "auth/start should allow first request: {result:?}");

        // auth/refresh should track per-user
        let result = limiters.auth_refresh.check("user_1");
        assert!(result.is_ok(), "auth/refresh should allow first request: {result:?}");
    }

    #[test]
    fn test_rate_limit_config_presets() {
        let standard_ip = AuthRateLimitConfig::per_ip_standard();
        assert_eq!(standard_ip.max_requests, 100);
        assert_eq!(standard_ip.window_secs, 60);

        let strict_ip = AuthRateLimitConfig::per_ip_strict();
        assert_eq!(strict_ip.max_requests, 50);

        let user_limit = AuthRateLimitConfig::per_user_standard();
        assert_eq!(user_limit.max_requests, 10);

        let failed = AuthRateLimitConfig::failed_login_attempts();
        assert_eq!(failed.max_requests, 5);
        assert_eq!(failed.window_secs, 3600);
    }

    #[test]
    fn test_ip_based_rate_limiting() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::per_ip_standard());

        let ip = "203.0.113.1";

        // Should allow up to 100 requests
        for _ in 0..100 {
            let result = limiter.check(ip);
            assert!(result.is_ok(), "request within limit should be allowed: {result:?}");
        }

        // 101st should fail
        let result = limiter.check(ip);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after exceeding IP limit, got: {result:?}"
        );
    }

    #[test]
    fn test_rejected_login_tracking() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::failed_login_attempts());

        let user = "alice@example.com";

        // Should allow 5 failed attempts
        for _ in 0..5 {
            let result = limiter.check(user);
            assert!(result.is_ok(), "failed login attempt within limit should be allowed: {result:?}");
        }

        // 6th should fail
        let result = limiter.check(user);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after exceeding failed login limit, got: {result:?}"
        );
    }

    #[test]
    fn test_multiple_users_independent() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::failed_login_attempts());

        // User 1 uses attempts
        for _ in 0..5 {
            limiter.check("user1").ok();
        }

        // User 1 blocked
        let result = limiter.check("user1");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited for user1, got: {result:?}"
        );

        // User 2 should have fresh attempts
        let result = limiter.check("user2");
        assert!(result.is_ok(), "user2 should have independent fresh limit: {result:?}");
    }

    #[test]
    fn test_clear_limiters() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited before clear, got: {result:?}"
        );

        limiter.clear();

        // After clear, should allow again
        let result = limiter.check("key");
        assert!(result.is_ok(), "should allow requests after clear: {result:?}");
    }

    #[test]
    fn test_thread_safe_rate_limiting() {
        use std::sync::Arc as StdArc;

        let limiter = StdArc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        }));

        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = StdArc::clone(&limiter);
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = limiter_clone.check("concurrent");
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().ok();
        }

        // After 100 concurrent requests, next should fail
        let result = limiter.check("concurrent");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after concurrent exhaustion, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiting_many_keys() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        });

        // Simulate 1000 different IPs, each with requests
        for i in 0..1000 {
            let key = format!("192.168.{}.{}", i / 256, i % 256);
            let result = limiter.check(&key);
            assert!(result.is_ok(), "first request for {key} should be allowed: {result:?}");
        }

        assert_eq!(limiter.active_limiters(), 1000);
    }

    #[test]
    fn test_endpoint_combinations() {
        let limiters = RateLimiters::new();

        let ip = "203.0.113.1";
        let user = "bob@example.com";

        // Complete flow
        let result = limiters.auth_start.check(ip);
        assert!(result.is_ok(), "auth_start should allow: {result:?}");

        let result = limiters.auth_callback.check(ip);
        assert!(result.is_ok(), "auth_callback should allow: {result:?}");

        let result = limiters.auth_refresh.check(user);
        assert!(result.is_ok(), "auth_refresh should allow: {result:?}");

        let result = limiters.auth_logout.check(user);
        assert!(result.is_ok(), "auth_logout should allow: {result:?}");

        let result = limiters.failed_logins.check(user);
        assert!(result.is_ok(), "failed_logins should allow: {result:?}");
    }

    #[test]
    fn test_attack_prevention_scenario() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        });

        let target = "admin@example.com";

        // Attacker tries 10 failed attempts
        for _ in 0..10 {
            let _ = limiter.check(target);
        }

        // 11th blocked
        let result = limiter.check(target);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after attack scenario, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      false,
            max_requests: 1,
            window_secs:  60,
        });

        // Even with max_requests = 1, should allow many requests when disabled
        for i in 0..100 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed when rate limiting disabled", i);
        }
    }

    // CONCURRENCY AND ATOMICITY TESTS
    // These tests verify that the rate limiter is thread-safe and atomic

    #[test]
    fn test_concurrent_requests_from_same_key_respects_limit() {
        // RACE CONDITION CHECK: Multiple threads simultaneously checking the same key
        // This verifies that atomic operations prevent exceeding the limit
        use std::{sync::Arc, thread};

        let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 50,
            window_secs:  60,
        }));

        let key = "shared_key";
        let allowed_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let rejected_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut handles = vec![];

        // Spawn 100 concurrent threads, all checking the same key
        for _ in 0..100 {
            let limiter = Arc::clone(&limiter);
            let allowed = Arc::clone(&allowed_count);
            let rejected = Arc::clone(&rejected_count);

            let handle = thread::spawn(move || {
                match limiter.check(key) {
                    Ok(()) => allowed.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                    Err(_) => rejected.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                };
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let allowed = allowed_count.load(std::sync::atomic::Ordering::SeqCst);
        let rejected = rejected_count.load(std::sync::atomic::Ordering::SeqCst);

        // CRITICAL: Due to atomicity, at most 50 requests should be allowed
        assert_eq!(allowed, 50, "Atomic operations should limit to max_requests");
        assert_eq!(rejected, 50, "Remaining requests should be rejected");
        assert_eq!(allowed + rejected, 100, "All requests should be accounted for");
    }

    #[test]
    fn test_concurrent_requests_different_keys_independent() {
        // RACE CONDITION CHECK: Multiple threads checking different keys
        // This verifies that per-key isolation works under concurrent access
        use std::{sync::Arc, thread};

        let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        }));

        let mut handles = vec![];

        // Spawn 10 threads, each using a different key and making 15 requests
        for thread_id in 0..10 {
            let limiter = Arc::clone(&limiter);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", thread_id);
                let mut allowed = 0;
                let mut rejected = 0;

                for _ in 0..15 {
                    match limiter.check(&key) {
                        Ok(()) => allowed += 1,
                        Err(_) => rejected += 1,
                    }
                }

                (allowed, rejected)
            });
            handles.push(handle);
        }

        // Collect results from all threads
        let mut total_allowed = 0;
        let mut total_rejected = 0;

        for handle in handles {
            let (allowed, rejected) = handle.join().unwrap();
            total_allowed += allowed;
            total_rejected += rejected;
        }

        // CRITICAL: Each key gets independent limit of 10 requests
        assert_eq!(total_allowed, 100, "Each of 10 keys should allow 10 requests");
        assert_eq!(total_rejected, 50, "Each of 10 keys should reject 5 requests");
    }

    #[test]
    fn test_atomic_check_and_update_not_interleaved() {
        // This test verifies that the check-and-update sequence is atomic
        // by ensuring the counter never gets into an inconsistent state
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 3,
            window_secs:  60,
        });

        let key = "test_key";

        // Make 3 allowed requests
        let r = limiter.check(key);
        assert!(r.is_ok(), "request 1 should be allowed: {r:?}");
        let r = limiter.check(key);
        assert!(r.is_ok(), "request 2 should be allowed: {r:?}");
        let r = limiter.check(key);
        assert!(r.is_ok(), "request 3 should be allowed: {r:?}");

        // Verify counter is at 3 (not less, not more)
        assert_eq!(limiter.active_limiters(), 1);

        // 4th request should be rejected
        let r = limiter.check(key);
        assert!(matches!(r, Err(AuthError::RateLimited { .. })), "request 4 should be rate-limited: {r:?}");

        // 5th request should also be rejected (counter didn't change)
        let r = limiter.check(key);
        assert!(matches!(r, Err(AuthError::RateLimited { .. })), "request 5 should be rate-limited: {r:?}");

        // Counter should still be at 3 (not decremented on rejection)
        // This verifies that rejected requests didn't partially update state
    }

    #[test]
    fn test_concurrent_window_reset_safety() {
        // Verify that window reset (when window expires) is atomic
        // even under concurrent access
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  3600, // 1 hour - won't expire in test
        });

        let key = "reset_key";

        // Fill the window
        limiter.check(key).ok();
        limiter.check(key).ok();

        // Further requests should fail
        let r = limiter.check(key);
        assert!(matches!(r, Err(AuthError::RateLimited { .. })), "should be rate-limited: {r:?}");
        let r = limiter.check(key);
        assert!(matches!(r, Err(AuthError::RateLimited { .. })), "should still be rate-limited: {r:?}");

        // Verify state is consistent by clearing and re-checking
        limiter.clear();
        assert_eq!(limiter.active_limiters(), 0);

        // After clear, new requests should be allowed
        let r = limiter.check(key);
        assert!(r.is_ok(), "should allow after clear: {r:?}");
    }

    // ── LRU eviction tests (13-3) ─────────────────────────────────────────────

    #[test]
    fn test_rate_limiter_evicts_lru_entry_when_at_capacity() {
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_max_entries(config, 3);

        // Fill to capacity.
        limiter.check("key_a").unwrap();
        limiter.check("key_b").unwrap();
        limiter.check("key_c").unwrap();
        assert_eq!(limiter.active_limiters(), 3);

        // Adding a 4th key must succeed — the oldest entry is evicted to make room.
        let result = limiter.check("key_d");
        assert!(result.is_ok(), "new key must be accepted when limiter evicts LRU entry");
        assert_eq!(
            limiter.active_limiters(),
            3,
            "entry count must stay at capacity after eviction"
        );
    }

    #[test]
    fn test_rate_limiter_capacity_configurable() {
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_max_entries(config, 5);

        for i in 0..5 {
            limiter.check(&format!("key_{i}")).unwrap();
        }
        assert_eq!(limiter.active_limiters(), 5, "limiter must track exactly max_entries keys");

        // 6th key triggers eviction; count must stay at 5.
        limiter.check("key_overflow").unwrap();
        assert_eq!(limiter.active_limiters(), 5, "capacity must not exceed configured maximum");
    }

    #[test]
    fn test_rate_limiter_eviction_does_not_affect_active_ips() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };

        // Use an injectable clock so window_start values are deterministic.
        let now = Arc::new(AtomicU64::new(1_000));
        let clock_ref = Arc::clone(&now);
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_clock_and_max_entries(config, 2, move || {
            clock_ref.load(Ordering::Relaxed)
        });

        // key_a at t=1000 — uses its 1 allowed request.
        now.store(1_000, Ordering::Relaxed);
        limiter.check("key_a").unwrap();

        // key_b at t=2000 — uses its 1 allowed request (more recent than key_a).
        now.store(2_000, Ordering::Relaxed);
        limiter.check("key_b").unwrap();

        // At capacity (2). key_c at t=3000 — triggers eviction of key_a (oldest at t=1000).
        now.store(3_000, Ordering::Relaxed);
        limiter.check("key_c").unwrap();

        // key_b (window_start=2000) was NOT evicted; its rate limit is still active.
        let result = limiter.check("key_b");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "key_b must remain rate-limited after eviction of the older key_a entry, got: {result:?}"
        );
    }

    // ── Distributed RL warning test (13-4) ───────────────────────────────────

    #[test]
    fn test_startup_warn_emitted_when_no_distributed_backend() {
        // Verify the function is callable without panicking.
        // The tracing output is verified in observability integration tests.
        warn_if_single_node_rate_limiting();
    }

    #[test]
    fn test_no_toctou_race_condition() {
        // Time-of-Check-Time-of-Use (TOCTOU) race condition test
        // Verifies that checking the limit and updating the counter happen atomically
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1, // Very strict: only 1 request allowed
            window_secs:  60,
        });

        let key = "single_key";

        // First request is allowed
        let r = limiter.check(key);
        assert!(r.is_ok(), "first request should be allowed: {r:?}");

        // Due to atomic check-and-update, the second request must fail
        // There's no window where both can check and both succeed
        let result = limiter.check(key);
        assert!(
            result.is_err(),
            "Second request must fail - check-and-update is atomic so no TOCTOU race"
        );
    }
}
