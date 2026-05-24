//! In-memory token-bucket rate limiter backend.

use std::sync::Arc;

use dashmap::DashMap;
use tracing::debug;

use super::{
    config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig},
    key::{PathRateLimit, path_matches_rule},
    token_bucket::TokenBucket,
};

/// In-memory token-bucket rate limiter.
///
/// Each bucket map is a [`DashMap`]: lookups/refills on the request hot path
/// take only a per-shard write reference, never an async lock, so unrelated
/// keys (different IPs / users / paths / tenants) never contend.  Capacity
/// checks against `max_buckets` are best-effort under heavy concurrent
/// insertion — total entries may oscillate around the cap by a small amount.
pub struct InMemoryRateLimiter {
    pub(super) config:          RateLimitConfig,
    // IP -> TokenBucket (global limit)
    pub(super) ip_buckets:      Arc<DashMap<String, TokenBucket>>,
    // User ID -> TokenBucket
    pub(super) user_buckets:    Arc<DashMap<String, TokenBucket>>,
    // Per-path rules (from [security.rate_limiting] auth endpoint fields)
    pub(super) path_rules:      Vec<PathRateLimit>,
    // (path_prefix, ip) -> TokenBucket
    pub(super) path_ip_buckets: Arc<DashMap<(String, String), TokenBucket>>,
    // tenant_key -> TokenBucket (per-tenant rate limit)
    pub(super) tenant_buckets:  Arc<DashMap<String, TokenBucket>>,
}

impl InMemoryRateLimiter {
    /// Create new in-memory rate limiter.
    pub(super) fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_buckets: Arc::new(DashMap::new()),
            user_buckets: Arc::new(DashMap::new()),
            path_rules: Vec::new(),
            path_ip_buckets: Arc::new(DashMap::new()),
            tenant_buckets: Arc::new(DashMap::new()),
        }
    }

    /// Attach per-path rules derived from `[security.rate_limiting]` auth endpoint fields.
    ///
    /// Converts max-requests-per-window into token-per-second refill rates.
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for rate-limit window calculations
    #[must_use]
    pub(super) fn with_path_rules_from_security(
        mut self,
        sec: &RateLimitingSecurityConfig,
    ) -> Self {
        let mut rules = Vec::new();

        if sec.auth_start_max_requests > 0 && sec.auth_start_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/start".to_string(),
                tokens_per_sec: f64::from(sec.auth_start_max_requests)
                    / sec.auth_start_window_secs as f64,
                burst:          f64::from(sec.auth_start_max_requests),
            });
        }
        if sec.auth_callback_max_requests > 0 && sec.auth_callback_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/callback".to_string(),
                tokens_per_sec: f64::from(sec.auth_callback_max_requests)
                    / sec.auth_callback_window_secs as f64,
                burst:          f64::from(sec.auth_callback_max_requests),
            });
        }
        if sec.auth_refresh_max_requests > 0 && sec.auth_refresh_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/refresh".to_string(),
                tokens_per_sec: f64::from(sec.auth_refresh_max_requests)
                    / sec.auth_refresh_window_secs as f64,
                burst:          f64::from(sec.auth_refresh_max_requests),
            });
        }

        self.path_rules = rules;
        self
    }

    /// Check if request to `path` from `ip` is within the per-path limit.
    ///
    /// Returns an allowed [`CheckResult`] when no rule governs the path.
    /// Returns a denied result only when a matching rule exists and the bucket
    /// is empty.  `CheckResult::retry_after_secs` is set to the path-window
    /// interval (`ceil(1 / tokens_per_sec)`).
    pub(super) async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let rule = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix));
        let Some(rule) = rule else {
            return CheckResult::allow(f64::from(self.config.burst_size));
        };

        let key = (rule.path_prefix.clone(), ip.to_string());
        let (tokens_per_sec, burst) = (rule.tokens_per_sec, rule.burst);

        // Best-effort capacity check: a parallel inserter racing past this point
        // may push us slightly above max_buckets, but never unboundedly.
        if !self.path_ip_buckets.contains_key(&key)
            && self.path_ip_buckets.len() >= self.config.max_buckets
        {
            debug!(
                ip = ip,
                path = path,
                "Path-IP bucket capacity reached — denying unseen combination"
            );
            let retry = if tokens_per_sec > 0.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: ceil(1/tokens_per_sec) is always a small positive integer
                ((1.0_f64 / tokens_per_sec).ceil() as u32).max(1)
            } else {
                1
            };
            return CheckResult::deny(retry);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self
                .path_ip_buckets
                .entry(key)
                .or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, path = path, "Per-path rate limit exceeded");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/tokens_per_sec) is always a small positive integer
            let retry = if tokens_per_sec > 0.0 {
                ((1.0_f64 / tokens_per_sec).ceil() as u32).max(1)
            } else {
                1
            };
            CheckResult::deny(retry)
        }
    }

    /// Get rate limiter configuration.
    pub(super) const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed for given IP.
    pub(super) async fn check_ip_limit(&self, ip: &str, tenant_id: Option<&str>) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let key = tenant_id.map_or_else(|| ip.to_string(), |tid| format!("{}:{}", tid, ip));

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.ip_buckets.contains_key(&key)
            && self.ip_buckets.len() >= self.config.max_buckets
        {
            debug!(ip = ip, tenant_id = ?tenant_id, "IP bucket capacity reached — denying unseen IP");
            return CheckResult::deny(1);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self.ip_buckets.entry(key).or_insert_with(|| {
                TokenBucket::new(
                    f64::from(self.config.burst_size),
                    f64::from(self.config.rps_per_ip),
                )
            });
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, "Rate limit exceeded for IP");
            let rps = self.config.rps_per_ip;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Check if request is allowed for given user.
    pub(super) async fn check_user_limit(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
    ) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let key =
            tenant_id.map_or_else(|| user_id.to_string(), |tid| format!("{}:{}", tid, user_id));

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.user_buckets.contains_key(&key)
            && self.user_buckets.len() >= self.config.max_buckets
        {
            debug!(user_id = user_id, tenant_id = ?tenant_id, "User bucket capacity reached — denying unseen user");
            return CheckResult::deny(1);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self.user_buckets.entry(key).or_insert_with(|| {
                TokenBucket::new(
                    f64::from(self.config.burst_size),
                    f64::from(self.config.rps_per_user),
                )
            });
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(user_id = user_id, "Rate limit exceeded for user");
            let rps = self.config.rps_per_user;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Check if a request is allowed for a given tenant key.
    ///
    /// Each tenant gets its own token bucket keyed by `tenant:{key}`.
    /// The `rps` and `burst` values come from the tenant's quota configuration
    /// (not the global rate-limit config).
    ///
    /// Returns an allowed [`CheckResult`] when the tenant bucket has tokens.
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for rate-limit token calculations
    pub(super) async fn check_tenant_limit(
        &self,
        tenant_key: &str,
        rps: u32,
        burst: u32,
    ) -> CheckResult {
        let bucket_key = format!("tenant:{tenant_key}");

        // Best-effort capacity check; see `check_path_limit` for why this races safely.
        if !self.tenant_buckets.contains_key(&bucket_key)
            && self.tenant_buckets.len() >= self.config.max_buckets
        {
            debug!(
                tenant_key = tenant_key,
                "Tenant bucket capacity reached — denying unseen tenant"
            );
            return CheckResult::deny(1);
        }

        let (allowed, remaining) = {
            let mut bucket_ref = self
                .tenant_buckets
                .entry(bucket_key)
                .or_insert_with(|| TokenBucket::new(f64::from(burst), f64::from(rps)));
            let bucket = bucket_ref.value_mut();
            let allowed = bucket.try_consume(1.0);
            let remaining = bucket.token_count();
            (allowed, remaining)
        };

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(tenant_key = tenant_key, "Per-tenant rate limit exceeded");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Reason: ceil(1/rps) is always a small positive integer
            let retry = if rps == 0 {
                1
            } else {
                ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
            };
            CheckResult::deny(retry)
        }
    }

    /// Evict stale in-memory buckets (called by background cleanup task).
    ///
    /// A bucket is stale once it has been idle for longer than the time required
    /// to fully refill from empty (`burst_size / rps_per_ip`).  At that point the
    /// next request would start a fresh full bucket anyway, so the entry is safe
    /// to remove.
    #[allow(clippy::cast_precision_loss)] // Reason: precision loss is acceptable for rate-limit cleanup interval calculations
    pub(super) async fn cleanup(&self) {
        let ip_refill_secs = if self.config.rps_per_ip == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_ip)
        };
        let user_refill_secs = if self.config.rps_per_user == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_user)
        };

        let now = std::time::Instant::now();
        let ip_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(ip_refill_secs))
            .unwrap_or(now);
        let user_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(user_refill_secs))
            .unwrap_or(now);

        let before_ip = self.ip_buckets.len();
        self.ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        let evicted_ip = before_ip.saturating_sub(self.ip_buckets.len());

        let before_user = self.user_buckets.len();
        self.user_buckets.retain(|_, b| b.last_refill >= user_threshold);
        let evicted_user = before_user.saturating_sub(self.user_buckets.len());

        self.path_ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        self.tenant_buckets.retain(|_, b| b.last_refill >= ip_threshold);

        debug!(evicted_ip, evicted_user, "Rate limiter cleanup complete");
    }

    /// Number of per-path rate limit rules registered.
    pub(super) const fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    ///
    /// Returns `ceil(1 / tokens_per_sec)` for the rule matching `path`, or 1 if no rule
    /// matches (which shouldn't happen in practice — callers only invoke this after a
    /// rejection).
    pub(super) fn retry_after_for_path(&self, path: &str) -> u32 {
        if let Some(rule) = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix))
        {
            if rule.tokens_per_sec > 0.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: ceil(1/tokens_per_sec) is always a small positive integer
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }
}
